//! Local rule for senders the user has unsubscribed from or blocked: whatever
//! they still send afterwards is archived (or trashed) automatically instead of
//! landing in the inbox, because many senders ignore or only slowly honor
//! unsubscribe requests. The rule can also just mark their mail as read.

use rusqlite::{params, Connection};

use crate::commands::mail::flags::push_read_status;
use crate::commands::mail::moves::{archive_email_for, trash_email_for};
use crate::commands::subscriptions::{sender_address, sender_name};
use crate::state::DbState;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Remembers the sender of `email_id` as unsubscribed and flags the mail we
/// already have from them (without moving it).
pub fn block_sender_of(conn: &Connection, account_id: i64, email_id: &str) -> rusqlite::Result<()> {
    let sender: String = conn.query_row(
        "SELECT sender FROM emails WHERE id=?1 AND account_id=?2",
        params![email_id, account_id],
        |r| r.get(0),
    )?;
    let key = sender_address(&sender);
    if key.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT OR IGNORE INTO unsubscribed_senders (account_id, sender_key, created_at) VALUES (?1,?2,?3)",
        params![account_id, key, now()],
    )?;

    let ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id, sender FROM emails WHERE account_id=?1 AND unsubscribed=0")?;
        let rows = stmt.query_map(params![account_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.filter_map(Result::ok)
            .filter(|(_, s)| sender_address(s) == key)
            .map(|(id, _)| id)
            .collect()
    };
    for id in ids {
        conn.execute(
            "UPDATE emails SET unsubscribed=1 WHERE id=?1 AND account_id=?2",
            params![id, account_id],
        )?;
    }
    Ok(())
}

/// What happens to future mail from a sender with a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleAction {
    Archive,
    Trash,
    /// Leave it in the inbox (only `mark_read` applies).
    Keep,
}

impl RuleAction {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleAction::Archive => "archive",
            RuleAction::Trash => "trash",
            RuleAction::Keep => "none",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "trash" => RuleAction::Trash,
            "none" => RuleAction::Keep,
            _ => RuleAction::Archive,
        }
    }
}

/// Creates or replaces the rule for the sender of `email_id`, and flags the
/// mail we already have from them so the rule only affects mail that arrives later.
pub fn set_sender_rule(
    conn: &Connection,
    account_id: i64,
    email_id: &str,
    action: RuleAction,
    mark_read: bool,
) -> rusqlite::Result<()> {
    block_sender_of(conn, account_id, email_id)?;
    let sender: String = conn.query_row(
        "SELECT sender FROM emails WHERE id=?1 AND account_id=?2",
        params![email_id, account_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE unsubscribed_senders SET action=?1, mark_read=?2 WHERE account_id=?3 AND sender_key=?4",
        params![action.as_str(), mark_read as i32, account_id, sender_address(&sender)],
    )?;
    Ok(())
}

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct BlockedSender {
    pub sender_key: String,
    pub name: String,
    pub created_at: i64,
    pub action: String,
    pub mark_read: bool,
}

pub fn list_blocked(conn: &Connection, account_id: i64) -> rusqlite::Result<Vec<BlockedSender>> {
    let mut stmt = conn.prepare(
        "SELECT sender_key, created_at, action, mark_read FROM unsubscribed_senders WHERE account_id=?1 ORDER BY created_at DESC, sender_key",
    )?;
    let keys: Vec<(String, i64, String, bool)> = stmt
        .query_map(params![account_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i32>(3)? != 0)))?
        .filter_map(Result::ok)
        .collect();
    let mut names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut stmt = conn.prepare("SELECT sender FROM emails WHERE account_id=?1 ORDER BY internal_ts DESC")?;
    for sender in stmt.query_map(params![account_id], |r| r.get::<_, String>(0))?.filter_map(Result::ok) {
        names.entry(sender_address(&sender)).or_insert(sender);
    }
    Ok(keys
        .into_iter()
        .map(|(key, created_at, action, mark_read)| {
            let name = names.get(&key).map(|s| sender_name(s, &key)).unwrap_or_else(|| key.clone());
            BlockedSender { sender_key: key, name, created_at, action, mark_read }
        })
        .collect())
}

/// Stops auto-archiving a sender. Their mail is un-flagged too, so the
/// startup backfill (which rebuilds the list from flagged mail) can't re-add them.
pub fn unblock(conn: &Connection, account_id: i64, sender_key: &str) -> rusqlite::Result<()> {
    let key = sender_key.trim().to_lowercase();
    conn.execute(
        "DELETE FROM unsubscribed_senders WHERE account_id=?1 AND sender_key=?2",
        params![account_id, key],
    )?;
    let ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id, sender FROM emails WHERE account_id=?1 AND unsubscribed=1")?;
        let rows = stmt.query_map(params![account_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.filter_map(Result::ok)
            .filter(|(_, s)| sender_address(s) == key)
            .map(|(id, _)| id)
            .collect()
    };
    for id in ids {
        conn.execute("UPDATE emails SET unsubscribed=0 WHERE id=?1 AND account_id=?2", params![id, account_id])?;
    }
    Ok(())
}

/// Inbox mail not yet handled (`unsubscribed=0`) from a sender with a rule,
/// together with that rule. Mail the user later moves back to the inbox keeps
/// `unsubscribed=1`, so it is not handled a second time.
fn pending(conn: &Connection, account_id: i64) -> rusqlite::Result<Vec<(String, RuleAction, bool)>> {
    let rules: std::collections::HashMap<String, (RuleAction, bool)> = {
        let mut stmt =
            conn.prepare("SELECT sender_key, action, mark_read FROM unsubscribed_senders WHERE account_id=?1")?;
        let rows = stmt.query_map(params![account_id], |r| {
            Ok((r.get::<_, String>(0)?, RuleAction::parse(&r.get::<_, String>(1)?), r.get::<_, i32>(2)? != 0))
        })?;
        rows.filter_map(Result::ok).map(|(key, action, read)| (key, (action, read))).collect()
    };
    if rules.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT id, sender FROM emails WHERE account_id=?1 AND mailbox='INBOX' AND unsubscribed=0",
    )?;
    let rows = stmt.query_map(params![account_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows
        .filter_map(Result::ok)
        .filter_map(|(id, sender)| rules.get(&sender_address(&sender)).map(|&(action, read)| (id, action, read)))
        .collect())
}

pub async fn suppress_unsubscribed(state: &DbState, account_id: i64) {
    let todo = {
        let conn = state.conn.lock().await;
        match pending(&conn, account_id) {
            Ok(todo) => todo,
            Err(e) => {
                log::error!("Unsubscribed-sender lookup failed account={}: {}", account_id, e);
                return;
            }
        }
    };
    for (id, action, mark_read) in todo {
        if mark_read {
            let already = {
                let conn = state.conn.lock().await;
                conn.query_row(
                    "SELECT is_read FROM emails WHERE id=?1 AND account_id=?2",
                    params![id, account_id],
                    |r| r.get::<_, i32>(0),
                )
                .map(|v| v != 0)
                .unwrap_or(true)
            };
            if !already {
                push_read_status(state, account_id, &id, true).await;
            }
        }
        let moved = match action {
            RuleAction::Archive => archive_email_for(state, account_id, &id).await,
            RuleAction::Trash => trash_email_for(state, account_id, &id).await,
            RuleAction::Keep => Ok(()),
        };
        match moved {
            Ok(()) => {
                let conn = state.conn.lock().await;
                let _ = conn.execute(
                    "UPDATE emails SET unsubscribed=1, is_read=is_read OR ?3 WHERE id=?1 AND account_id=?2",
                    params![id, account_id, mark_read as i32],
                );
            }
            // Leave unsubscribed=0 so the next sync retries (e.g. offline / rate limit).
            Err(e) => log::warn!("Applying sender rule to {} failed: {}", id, e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE emails (id TEXT, account_id INTEGER, sender TEXT, mailbox TEXT, internal_ts INTEGER DEFAULT 0, unsubscribed INTEGER DEFAULT 0);
             CREATE TABLE unsubscribed_senders (account_id INTEGER NOT NULL, sender_key TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT 0, action TEXT NOT NULL DEFAULT 'archive', mark_read INTEGER NOT NULL DEFAULT 1, PRIMARY KEY (account_id, sender_key));",
        ).unwrap();
        conn
    }

    fn add(conn: &Connection, id: &str, sender: &str, mailbox: &str) {
        conn.execute("INSERT INTO emails (id, account_id, sender, mailbox) VALUES (?1,1,?2,?3)", params![id, sender, mailbox]).unwrap();
    }

    #[test]
    fn new_mail_from_blocked_sender_is_pending_but_old_mail_is_not() {
        let conn = setup();
        add(&conn, "old", "News <n@x.com>", "INBOX");
        add(&conn, "other", "Friend <f@y.com>", "INBOX");
        block_sender_of(&conn, 1, "old").unwrap();
        assert!(pending(&conn, 1).unwrap().is_empty());

        add(&conn, "new", "\"News\" <N@X.com>", "INBOX");
        add(&conn, "new-archived", "n@x.com", "ARCHIVE");
        assert_eq!(pending(&conn, 1).unwrap(), vec![("new".to_string(), RuleAction::Archive, true)]);
    }

    #[test]
    fn unblocking_stops_the_rule_and_unflags_mail() {
        let conn = setup();
        add(&conn, "old", "News <n@x.com>", "INBOX");
        block_sender_of(&conn, 1, "old").unwrap();
        let listed = list_blocked(&conn, 1).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].sender_key, "n@x.com");
        assert_eq!(listed[0].name, "News");

        unblock(&conn, 1, "N@x.com").unwrap();
        assert!(list_blocked(&conn, 1).unwrap().is_empty());
        add(&conn, "new", "News <n@x.com>", "INBOX");
        assert!(pending(&conn, 1).unwrap().is_empty());
        let flagged: i64 = conn.query_row("SELECT COUNT(*) FROM emails WHERE unsubscribed=1", [], |r| r.get(0)).unwrap();
        assert_eq!(flagged, 0);
    }

    #[test]
    fn rule_action_and_read_flag_are_stored_per_sender() {
        let conn = setup();
        add(&conn, "a", "News <n@x.com>", "INBOX");
        add(&conn, "b", "Promo <p@y.com>", "INBOX");
        set_sender_rule(&conn, 1, "a", RuleAction::Trash, false).unwrap();
        set_sender_rule(&conn, 1, "b", RuleAction::Keep, true).unwrap();

        add(&conn, "a2", "n@x.com", "INBOX");
        add(&conn, "b2", "p@y.com", "INBOX");
        let mut todo = pending(&conn, 1).unwrap();
        todo.sort();
        assert_eq!(
            todo,
            vec![
                ("a2".to_string(), RuleAction::Trash, false),
                ("b2".to_string(), RuleAction::Keep, true),
            ]
        );

        // Changing the rule replaces it rather than adding a second one.
        set_sender_rule(&conn, 1, "a", RuleAction::Archive, true).unwrap();
        let listed = list_blocked(&conn, 1).unwrap();
        assert_eq!(listed.len(), 2);
        let a = listed.iter().find(|s| s.sender_key == "n@x.com").unwrap();
        assert_eq!((a.action.as_str(), a.mark_read), ("archive", true));
    }
}
