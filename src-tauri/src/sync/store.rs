use crate::db::{set_mailbox_sync_state, MailboxSyncState};
use crate::imap_client::SyncResult;
use crate::state::DbState;

pub async fn apply_sync_result(state: &DbState, account_id: i64, result: SyncResult, mailbox: &str) {
    let SyncResult { emails, highest_uid, uidvalidity, uids_reset, window } = result;
    upsert_emails(state, account_id, emails, mailbox).await;

    let conn = state.conn.lock().await;
    if let Err(e) = apply_reconcile_window(&conn, account_id, mailbox, uids_reset, window.as_ref()) {
        log::error!("IMAP reconcile failed account={} mailbox={}: {}", account_id, mailbox, e);
    }

    if highest_uid > 0 || uidvalidity > 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let sync_state = MailboxSyncState {
            account_id,
            mailbox_name: mailbox.to_string(),
            highest_uid,
            uidvalidity,
            last_synced_at: now,
        };
        let _ = set_mailbox_sync_state(&conn, &sync_state);
    }

    if mailbox == "INBOX" {
        if let Err(e) = crate::smart_inbox::assign_unassigned(&conn, account_id) {
            log::error!("Smart Inbox assignment after IMAP reconcile failed: {}", e);
        }
    }
}

pub(crate) fn apply_reconcile_window(
    conn: &rusqlite::Connection,
    account_id: i64,
    mailbox: &str,
    uids_reset: bool,
    window: Option<&crate::imap_client::ReconcileWindow>,
) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    if uids_reset {
        tx.execute(
            "UPDATE emails SET imap_uid=NULL, imap_uid_mailbox=NULL WHERE account_id=?1 AND imap_uid_mailbox=?2",
            rusqlite::params![account_id, mailbox],
        )?;
    }
    if let Some(window) = window {
        {
            let mut update = tx.prepare(
                "UPDATE emails SET is_read=?1, starred=?2, imap_uid=?3, imap_uid_mailbox=?4, mailbox=?4
                 WHERE id=?5 AND account_id=?6 AND (mailbox=?4 OR mailbox='OTHER')",
            )?;
            for entry in &window.entries {
                update.execute(rusqlite::params![
                    entry.is_read as i32, entry.starred as i32, entry.uid, mailbox, entry.id, account_id
                ])?;
            }
        }

        let present: std::collections::HashSet<u32> = window.entries.iter().map(|e| e.uid).collect();
        let stale: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT id, imap_uid FROM emails
                 WHERE account_id=?1 AND mailbox=?2 AND imap_uid_mailbox=?2 AND imap_uid >= ?3",
            )?;
            let rows = stmt.query_map(rusqlite::params![account_id, mailbox, window.min_uid], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?))
            })?;
            rows.filter_map(Result::ok)
                .filter(|(_, uid)| !present.contains(uid))
                .map(|(id, _)| id)
                .collect()
        };
        for id in stale {
            tx.execute(
                "UPDATE emails SET mailbox='OTHER', imap_uid=NULL, imap_uid_mailbox=NULL WHERE id=?1 AND account_id=?2",
                rusqlite::params![id, account_id],
            )?;
        }
    }
    tx.commit()
}

pub async fn upsert_emails(state: &DbState, account_id: i64, emails: Vec<crate::db::Email>, mailbox: &str) {
    let conn = state.conn.lock().await;

    for email in &emails {
        if let Err(e) = conn.execute(
            "INSERT INTO emails (id, account_id, draft_id, thread_id, subject, sender, to_recipients, cc_recipients,
                                 snippet, body_html, attachments_json, has_attachments, date, is_read, starred,
                                 mailbox, labels, internal_ts, list_unsubscribe)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
             ON CONFLICT(id, account_id) DO UPDATE SET
                thread_id=excluded.thread_id,
                subject=excluded.subject,
                sender=excluded.sender,
                to_recipients=excluded.to_recipients,
                cc_recipients=excluded.cc_recipients,
                snippet=excluded.snippet,
                body_html=excluded.body_html,
                attachments_json=excluded.attachments_json,
                has_attachments=excluded.has_attachments,
                date=excluded.date,
                is_read=excluded.is_read,
                starred=excluded.starred,
                mailbox=excluded.mailbox,
                labels=excluded.labels,
                internal_ts=excluded.internal_ts,
                list_unsubscribe=excluded.list_unsubscribe",
            rusqlite::params![
                email.id, email.account_id, email.draft_id, email.thread_id,
                email.subject, email.sender, email.to_recipients, email.cc_recipients,
                email.snippet, email.body_html, email.attachments_json,
                email.has_attachments as i32, email.date, email.is_read as i32,
                email.starred as i32, email.mailbox, email.labels, email.internal_ts,
                email.list_unsubscribe
            ],
        ) {
            log::error!("IMAP upsert email {} failed: {}", email.id, e);
        }
    }

    if mailbox == "INBOX" {
        if let Err(e) = crate::smart_inbox::assign_unassigned(&conn, account_id) {
            log::error!("Smart Inbox assignment after IMAP sync failed: {}", e);
        }
    }
}
