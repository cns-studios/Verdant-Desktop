//! Fallback for senders we can't unsubscribe from: the user picks what to do
//! with the mail they already have and what should happen to future mail.

use std::sync::Arc;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::mail::flags::push_read_status;
use crate::commands::mail::moves::{archive_email_for, trash_email_for};
use crate::commands::subscriptions::sender_address;
use crate::state::{get_active_id, DbState};
use crate::sync::{set_sender_rule, RuleAction};

#[derive(Debug, Default, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CleanupOptions {
    /// Move the sender's existing mail to the trash.
    pub trash_existing: bool,
    /// Archive the sender's existing inbox mail.
    pub archive_existing: bool,
    /// Mark the sender's existing mail as read.
    pub mark_read: bool,
    /// Send all future mail from the sender straight to the trash.
    pub block_sender: bool,
    /// Apply the trash / archive / mark-read choices above to future mail too.
    pub apply_to_future: bool,
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct CleanupResult {
    pub processed: usize,
    pub failed: usize,
}

impl CleanupOptions {
    /// The rule that should handle future mail, if any. Blocking wins over the
    /// per-action choices.
    fn future_rule(&self) -> Option<(RuleAction, bool)> {
        if self.block_sender {
            return Some((RuleAction::Trash, true));
        }
        if !self.apply_to_future {
            return None;
        }
        let action = if self.trash_existing {
            RuleAction::Trash
        } else if self.archive_existing {
            RuleAction::Archive
        } else {
            RuleAction::Keep
        };
        if action == RuleAction::Keep && !self.mark_read {
            return None;
        }
        Some((action, self.mark_read))
    }
}

struct Existing {
    id: String,
    mailbox: String,
    is_read: bool,
}

/// Mail we hold from the sender of `email_id` that is still in a normal folder.
fn existing_from_sender(conn: &Connection, account_id: i64, email_id: &str) -> rusqlite::Result<Vec<Existing>> {
    let sender: String = conn.query_row(
        "SELECT sender FROM emails WHERE id=?1 AND account_id=?2",
        params![email_id, account_id],
        |r| r.get(0),
    )?;
    let key = sender_address(&sender);
    if key.is_empty() {
        return Ok(Vec::new());
    }
    // LIKE only narrows the scan; the exact address is compared below.
    let mut stmt = conn.prepare(
        "SELECT id, sender, mailbox, is_read FROM emails
         WHERE account_id=?1 AND mailbox IN ('INBOX','ARCHIVE','OTHER') AND sender LIKE '%'||?2||'%'",
    )?;
    let rows = stmt.query_map(params![account_id, key], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i32>(3)? != 0))
    })?;
    Ok(rows
        .filter_map(Result::ok)
        .filter(|(_, sender, _, _)| sender_address(sender) == key)
        .map(|(id, _, mailbox, is_read)| Existing { id, mailbox, is_read })
        .collect())
}

#[tauri::command]
pub async fn clean_up_sender(
    state: State<'_, Arc<DbState>>,
    email_id: String,
    options: CleanupOptions,
) -> Result<CleanupResult, String> {
    let account_id = get_active_id(&state).await;
    clean_up_sender_for(&state, account_id, &email_id, options).await
}

pub(crate) async fn clean_up_sender_for(
    state: &DbState,
    account_id: i64,
    email_id: &str,
    options: CleanupOptions,
) -> Result<CleanupResult, String> {
    let (existing, rule) = {
        let conn = state.conn.lock().await;
        let existing = existing_from_sender(&conn, account_id, email_id).map_err(|e| e.to_string())?;
        // Record the rule (and flag current mail as handled) before moving anything, so
        // a sync running meanwhile can't apply it to the mail we're about to process.
        let rule = options.future_rule();
        if let Some((action, mark_read)) = rule {
            set_sender_rule(&conn, account_id, email_id, action, mark_read).map_err(|e| e.to_string())?;
        }
        (existing, rule)
    };
    if rule.is_none() && !(options.trash_existing || options.archive_existing || options.mark_read) {
        return Ok(CleanupResult::default());
    }

    let mut result = CleanupResult::default();
    for mail in existing {
        let mut ok = true;
        // Read state first: the server-side flag needs the mail's current folder.
        if options.mark_read && !mail.is_read {
            push_read_status(state, account_id, &mail.id, true).await;
            let conn = state.conn.lock().await;
            ok &= conn
                .execute("UPDATE emails SET is_read=1 WHERE id=?1 AND account_id=?2", params![mail.id, account_id])
                .is_ok();
        }
        let moved = if options.trash_existing {
            Some(trash_email_for(state, account_id, &mail.id).await)
        } else if options.archive_existing && mail.mailbox == "INBOX" {
            Some(archive_email_for(state, account_id, &mail.id).await)
        } else {
            None
        };
        if let Some(Err(e)) = moved {
            log::warn!("Cleaning up {} failed: {}", mail.id, e);
            ok = false;
        }
        if ok { result.processed += 1 } else { result.failed += 1 }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> CleanupOptions {
        CleanupOptions::default()
    }

    #[test]
    fn no_future_rule_unless_asked() {
        assert_eq!(opts().future_rule(), None);
        assert_eq!(CleanupOptions { trash_existing: true, ..opts() }.future_rule(), None);
    }

    #[test]
    fn blocking_trashes_future_mail_regardless_of_other_choices() {
        let blocked = CleanupOptions { block_sender: true, archive_existing: true, apply_to_future: true, ..opts() };
        assert_eq!(blocked.future_rule(), Some((RuleAction::Trash, true)));
    }

    #[test]
    fn apply_to_future_mirrors_the_chosen_actions() {
        let archive = CleanupOptions { archive_existing: true, mark_read: true, apply_to_future: true, ..opts() };
        assert_eq!(archive.future_rule(), Some((RuleAction::Archive, true)));
        let trash = CleanupOptions { trash_existing: true, apply_to_future: true, ..opts() };
        assert_eq!(trash.future_rule(), Some((RuleAction::Trash, false)));
        let read_only = CleanupOptions { mark_read: true, apply_to_future: true, ..opts() };
        assert_eq!(read_only.future_rule(), Some((RuleAction::Keep, true)));
        // Nothing chosen means there is nothing to apply.
        assert_eq!(CleanupOptions { apply_to_future: true, ..opts() }.future_rule(), None);
    }

    #[test]
    fn finds_only_the_senders_own_mail() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE emails (id TEXT, account_id INTEGER, sender TEXT, mailbox TEXT, is_read INTEGER DEFAULT 0);
             INSERT INTO emails VALUES ('a',1,'News <n@x.com>','INBOX',0);
             INSERT INTO emails VALUES ('b',1,'\"News\" <N@X.com>','ARCHIVE',1);
             INSERT INTO emails VALUES ('c',1,'n@x.com.evil.org','INBOX',0);
             INSERT INTO emails VALUES ('d',1,'n@x.com','TRASH',0);
             INSERT INTO emails VALUES ('e',2,'n@x.com','INBOX',0);",
        )
        .unwrap();
        let mut ids: Vec<String> = existing_from_sender(&conn, 1, "a").unwrap().into_iter().map(|m| m.id).collect();
        ids.sort();
        assert_eq!(ids, vec!["a", "b"]);
    }
}
