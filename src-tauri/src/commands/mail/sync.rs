use std::sync::Arc;

use tauri::State;

use crate::gmail::sync_mailbox_page_internal_for;
use crate::state::{get_active_id, DbState};
use crate::sync::sync_mailbox_internal_for;

#[tauri::command]
pub async fn sync_emails(state: State<'_, Arc<DbState>>) -> Result<(), String> {
    let id = get_active_id(&state).await;
    sync_mailbox_internal_for(&state, id, "INBOX").await
}

#[tauri::command]
pub async fn sync_mailbox(state: State<'_, Arc<DbState>>, mailbox: String) -> Result<(), String> {
    let id = get_active_id(&state).await;
    sync_mailbox_internal_for(&state, id, mailbox.as_str()).await
}

#[tauri::command]
pub async fn sync_mailbox_page(
    state: State<'_, Arc<DbState>>,
    mailbox: String,
    page_token: Option<String>,
) -> Result<Option<String>, String> {
    let id = get_active_id(&state).await;
    sync_mailbox_page_internal_for(&state, id, mailbox.as_str(), page_token).await
}

#[tauri::command]
pub async fn sync_imap_mailbox_page(
    state: State<'_, Arc<DbState>>,
    mailbox: String,
    offset: u32,
) -> Result<bool, String> {
    let account_id = get_active_id(&state).await;

    let account = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id)
            .ok().flatten()
            .ok_or_else(|| "Account not found".to_string())?
    };

    if account.provider != "imap" {
        return Err("Not an IMAP account".to_string());
    }

    let acc_clone = account.clone();
    let mb = mailbox.clone();
    let result = tokio::task::spawn_blocking(move || {
        crate::imap_client::sync_imap_mailbox_page(&acc_clone, &mb, offset, 50)
    }).await;

    match result {
        Ok(Ok(emails)) => {
            let has_more = !emails.is_empty();
            let conn = state.conn.lock().await;
            for email in emails {
                if let Err(e) = conn.execute(
                    "INSERT INTO emails (id, account_id, draft_id, thread_id, subject, sender, to_recipients, cc_recipients, bcc_recipients,
                                         snippet, body_html, attachments_json, has_attachments, date, is_read, starred,
                                         mailbox, labels, internal_ts, list_unsubscribe)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)
                    ON CONFLICT(id, account_id) DO UPDATE SET
                        snippet = excluded.snippet,
                        body_html = excluded.body_html,
                        is_read = excluded.is_read,
                        mailbox = excluded.mailbox,
                        labels = excluded.labels,
                        internal_ts = excluded.internal_ts,
                        list_unsubscribe = excluded.list_unsubscribe",
                    rusqlite::params![
                        email.id, email.account_id, email.draft_id, email.thread_id,
                        email.subject, email.sender, email.to_recipients, email.cc_recipients, email.bcc_recipients,
                        email.snippet, email.body_html, email.attachments_json,
                        email.has_attachments as i32, email.date, email.is_read as i32,
                        email.starred as i32, email.mailbox, email.labels, email.internal_ts,
                        email.list_unsubscribe
                    ],
                ) {
                    log::error!("sync_imap_mailbox_page upsert failed: {}", e);
                }
            }
            if mailbox == "INBOX" {
                if let Err(e) = crate::smart_inbox::assign_unassigned(&conn, account_id) {
                    log::error!("Smart Inbox assignment after Gmail page sync failed: {}", e);
                }
            }
            Ok(has_more)
        }
        Ok(Err(e)) => Err(e),
        Err(e) => Err(format!("Task error: {}", e)),
    }
}
