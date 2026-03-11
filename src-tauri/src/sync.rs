use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde_json::json;
use tauri::{AppHandle, Manager};
use tokio::time;

use crate::db::Db;
use crate::imap;
use crate::models::Account;

const SYNC_INTERVAL_SECS: u64 = 300;

pub fn spawn_sync_loop(app: AppHandle, db: Arc<Db>) {
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(SYNC_INTERVAL_SECS));
        interval.tick().await;

        loop {
            interval.tick().await;
            log::info!("Background sync: starting");

            if let Err(e) = sync_all(&app, &db).await {
                log::error!("Background sync error: {e}");
            }
        }
    });
}

pub async fn sync_all(app: &AppHandle, db: &Db) -> Result<()> {
    let accounts: Vec<Account> = sqlx::query_as(
        "SELECT id, name, email, imap_host, imap_port, imap_tls,
                smtp_host, smtp_port, smtp_tls FROM accounts",
    )
    .fetch_all(db)
    .await?;

    for account in accounts {
        if let Err(e) = sync_account(app, db, &account).await {
            log::warn!("Sync failed for account {}: {e}", account.email);
        }
    }
    Ok(())
}

async fn sync_account(app: &AppHandle, db: &Db, account: &Account) -> Result<()> {
    let mut session = imap::login(
        &account.id,
        &account.imap_host,
        account.imap_port as u16,
        account.imap_tls,
        &account.email,
    )
    .await?;

    let mailbox: Option<(String, String)> = sqlx::query_as(
        "SELECT id, full_name FROM mailboxes WHERE account_id = ? AND (full_name = 'INBOX' OR full_name LIKE '%Inbox%') LIMIT 1",
    )
    .bind(&account.id)
    .fetch_optional(db)
    .await?;

    if let Some((mailbox_id, full_name)) = mailbox {
        let headers =
            imap::fetch_headers(&mut session, &account.id, &mailbox_id, &full_name, db).await?;

        let new_count = headers.len();
        log::info!("Sync: {} new/updated messages in {}", new_count, account.email);

        let _ = app.emit_all(
            "mail://sync-done",
            json!({
                "account_id": account.id,
                "mailbox_id": mailbox_id,
                "count": new_count
            }),
        );
    }

    match session {
        imap::ImapSession::Tls(mut s) => { let _ = s.logout().await; }
        imap::ImapSession::Plain(mut s) => { let _ = s.logout().await; }
    }

    Ok(())
}
