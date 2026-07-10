use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

use crate::db::{get_all_accounts, set_mailbox_sync_state, Account, MailboxSyncState};
use crate::imap_sync::{ImapCredentials, connect_with_timeout, SyncResult};
use crate::state::DbState;

const SYNC_INTERVAL_SECS: u64 = 45;
const IMAP_SYNC_INTERVAL_SECS: u64 = 20;
const IMAP_MAILBOXES: &[&str] = &["INBOX", "SENT", "DRAFT", "TRASH"];
const IDLE_TIMEOUT_SECS: u64 = 60 * 5;
const RETRY_BASE_MS: u64 = 1000;
const RETRY_MAX_MS: u64 = 30000;

pub async fn start_all_sync_tasks(app: tauri::AppHandle, state: Arc<DbState>) {
    let accounts = {
        let conn = state.conn.lock().await;
        get_all_accounts(&conn).unwrap_or_default()
    };

    for account in accounts {
        start_account_sync(app.clone(), state.clone(), account).await;
    }
}

pub async fn start_account_sync(app: tauri::AppHandle, state: Arc<DbState>, account: Account) {
    let account_id = account.id;
    stop_account_sync(&state, account_id).await;

    let (tx, rx) = oneshot::channel::<()>();

    {
        let mut handles = state.sync_handles.lock().await;
        handles.insert(account_id, tx);
    }

    let state_clone = state.clone();
    tokio::spawn(async move {
        if account.provider == "imap" {
            run_imap_sync_loop(app, state_clone, account, rx).await;
        } else {
            run_gmail_sync_loop(app, state_clone, account, rx).await;
        }
    });
}

pub async fn stop_account_sync(state: &DbState, account_id: i64) {
    let mut handles = state.sync_handles.lock().await;
    if let Some(tx) = handles.remove(&account_id) {
        let _ = tx.send(());
    }
}

async fn run_gmail_sync_loop(
    app: tauri::AppHandle,
    state: Arc<DbState>,
    account: Account,
    mut shutdown: oneshot::Receiver<()>,
) {
    use crate::commands::mail::sync_mailbox_internal_for;
    sync_mailbox_internal_for(&state, account.id, "INBOX").await.ok();

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(SYNC_INTERVAL_SECS)) => {
                let fresh = get_fresh_account(&state, account.id).await;
                if let Some(acc) = fresh {
                    sync_gmail_account(&app, &state, &acc).await;
                } else { break; }
            }
            _ = &mut shutdown => break,
        }
    }
}

async fn run_imap_sync_loop(
    app: tauri::AppHandle,
    state: Arc<DbState>,
    account: Account,
    mut shutdown: oneshot::Receiver<()>,
) {
    let account_id = account.id;
    let interval = Duration::from_secs(IMAP_SYNC_INTERVAL_SECS);

    sync_imap_account(&app, &state, &account).await;

    loop {
        let account = get_fresh_account(&state, account_id).await;
        let Some(ref acc) = account else { break; };

        let idle_result = try_imap_idle(acc, Duration::from_secs(IDLE_TIMEOUT_SECS)).await;

        if idle_result {
            sync_imap_account(&app, &state, acc).await;
            continue;
        }

        tokio::select! {
            _ = tokio::time::sleep(interval) => {
                let account = get_fresh_account(&state, account_id).await;
                if let Some(acc) = account {
                    sync_imap_account(&app, &state, &acc).await;
                } else { break; }
            }
            _ = &mut shutdown => break,
        }
    }
}

async fn get_fresh_account(state: &DbState, account_id: i64) -> Option<Account> {
    let conn = state.conn.lock().await;
    crate::db::get_account_by_id(&conn, account_id).ok().flatten()
}

async fn try_imap_idle(account: &Account, timeout: Duration) -> bool {
    let creds = match ImapCredentials::from_account(account) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let result = tokio::task::spawn_blocking(move || -> Result<bool, String> {
        let mut session = connect_with_timeout(&creds, 15)?;

        let folders: Vec<String> = session
            .list(None, Some("*"))
            .map_err(|e| format!("{}", e))?
            .iter().map(|n| n.name().to_string()).collect();

        let inbox = crate::imap_sync::imap_folder_for_mailbox("INBOX", &folders)
            .unwrap_or_else(|| "INBOX".to_string());

        session.select(&inbox)
            .map_err(|e| format!("{}", e))?;

        let handle = session.idle()
            .map_err(|e| format!("IDLE not supported: {}", e))?;

        let result = handle.wait_with_timeout(timeout);
        let got = result.is_ok();
        Ok(got)
    }).await;

    match result {
        Ok(Ok(notification)) => notification,
        _ => false,
    }
}

async fn sync_gmail_account(app: &tauri::AppHandle, state: &DbState, account: &Account) {
    use crate::commands::mail::sync_mailbox_internal_for;

    let mailboxes = ["INBOX", "SENT", "DRAFT", "TRASH"];

    for mailbox in &mailboxes {
        if let Err(e) = sync_mailbox_internal_for(state, account.id, mailbox).await {
            log::error!("Gmail sync error account={} mailbox={}: {}", account.id, mailbox, e);
        }
    }

    emit_notifications_and_event_gmail(app.clone(), state, account).await;
    use tauri::Emitter;
    let _ = app.emit("emails-synced", ());
}

async fn emit_notifications_and_event_gmail(app: tauri::AppHandle, state: &DbState, account: &Account) {
    let account_id = account.id;

    let unread_emails = {
        let conn = state.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, subject, sender FROM emails WHERE account_id=?1 AND mailbox='INBOX' AND is_read=0 AND notified=0 AND internal_ts > (strftime('%s','now') - 3600) LIMIT 50"
        ).unwrap();
        let rows = stmt.query_map([account_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        }).unwrap();
        rows.filter_map(Result::ok).collect::<Vec<_>>()
    };

    if !unread_emails.is_empty() {
        use tauri_plugin_notification::NotificationExt;
        let count = unread_emails.len();
        let acc_name = account.display_name.as_deref().unwrap_or(&account.email);

        let title = if count == 1 {
            format!("New Email - {}", acc_name)
        } else {
            format!("{} New Emails - {}", count, acc_name)
        };

        let body = if count == 1 {
            let (_, subj, send) = &unread_emails[0];
            format!("From: {}\n{}", send, subj)
        } else {
            format!("You have {} new messages in your inbox.", count)
        };

        let _ = app.notification().builder()
            .title(title)
            .body(body)
            .show();

        let conn = state.conn.lock().await;
        for (id, _, _) in unread_emails {
            let _ = conn.execute(
                "UPDATE emails SET notified=1 WHERE id=?1 AND account_id=?2",
                rusqlite::params![id, account_id],
            );
        }
    }
}

async fn sync_imap_account(app: &tauri::AppHandle, state: &DbState, account: &Account) {
    use crate::imap_sync::sync_imap_mailbox_incremental;

    let account_id = account.id;
    let app_clone = app.clone();
    let mut had_new_emails = false;

    for mailbox in IMAP_MAILBOXES {
        let acc = account.clone();
        let mb = mailbox.to_string();
        let mb_for_fallback = mb.clone();

        let stored_state = {
            let conn = state.conn.lock().await;
            crate::db::get_mailbox_sync_state(&conn, account_id, &mb)
                .ok()
                .flatten()
        };

        let (stored_uidvalidity, stored_highest_uid) = stored_state
            .map(|s| (Some(s.uidvalidity), Some(s.highest_uid)))
            .unwrap_or((None, None));

        let result = tokio::task::spawn_blocking(move || {
            sync_imap_mailbox_incremental(&acc, &mb, stored_uidvalidity, stored_highest_uid)
        }).await;

        match result {
            Ok(Ok(sync_result)) => {
                if !sync_result.emails.is_empty() {
                    had_new_emails = true;
                }
                upsert_sync_result(state, account_id, sync_result, &mb_for_fallback).await;
            }
            Ok(Err(e)) => {
                log::error!("IMAP sync error account={} mailbox={}: {}", account_id, mailbox, e);
                fallback_sync(app, state, account, mailbox).await;
            }
            Err(e) => {
                log::error!("IMAP sync task panicked account={} mailbox={}: {}", account_id, mailbox, e);
            }
        }
    }

    emit_notifications_and_event(app_clone, state, account, had_new_emails).await;
}

async fn fallback_sync(_app: &tauri::AppHandle, state: &DbState, account: &Account, mailbox: &str) {
    use crate::imap_sync::sync_imap_mailbox;
    let acc = account.clone();
    let mb = mailbox.to_string();
    let account_id = account.id;

    let result = tokio::task::spawn_blocking(move || {
        sync_imap_mailbox(&acc, &mb, 50)
    }).await;

    if let Ok(Ok(emails)) = result {
        upsert_emails(state, account_id, emails, mailbox).await;
    }
}

async fn upsert_sync_result(state: &DbState, account_id: i64, result: SyncResult, mailbox: &str) {
    upsert_emails(state, account_id, result.emails, mailbox).await;

    if result.highest_uid > 0 || result.uidvalidity > 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let sync_state = MailboxSyncState {
            account_id,
            mailbox_name: mailbox.to_string(),
            highest_uid: result.highest_uid,
            uidvalidity: result.uidvalidity,
            last_synced_at: now,
        };

        let conn = state.conn.lock().await;
        let _ = set_mailbox_sync_state(&conn, &sync_state);
    }
}

async fn emit_notifications_and_event(app: tauri::AppHandle, state: &DbState, account: &Account, had_new: bool) {
    let account_id = account.id;

    if had_new {
        let unread_emails = {
            let conn = state.conn.lock().await;
            let mut stmt = conn.prepare(
                "SELECT id, subject, sender FROM emails WHERE account_id=?1 AND mailbox='INBOX' AND is_read=0 AND notified=0 AND internal_ts > (strftime('%s','now') - 3600) LIMIT 50"
            ).unwrap();
            let rows = stmt.query_map([account_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            }).unwrap();
            rows.filter_map(Result::ok).collect::<Vec<_>>()
        };

        if !unread_emails.is_empty() {
        use tauri_plugin_notification::NotificationExt;
        let count = unread_emails.len();
        let acc_name = account.display_name.as_deref().unwrap_or(&account.email);

        let title = if count == 1 {
            format!("New Email - {}", acc_name)
        } else {
            format!("{} New Emails - {}", count, acc_name)
        };

        let body = if count == 1 {
            let (_, subj, send) = &unread_emails[0];
            format!("From: {}\n{}", send, subj)
        } else {
            format!("You have {} new messages in your inbox.", count)
        };

        let _ = app.notification().builder()
            .title(title)
            .body(body)
            .show();

            let conn = state.conn.lock().await;
            for (id, _, _) in unread_emails {
                let _ = conn.execute(
                    "UPDATE emails SET notified=1 WHERE id=?1 AND account_id=?2",
                    rusqlite::params![id, account_id],
                );
            }
        }
    }

    use tauri::Emitter;
    let _ = app.emit("emails-synced", ());
}

pub async fn upsert_emails(state: &DbState, account_id: i64, emails: Vec<crate::db::Email>, mailbox: &str) {
    let mut synced_ids = Vec::new();
    let conn = state.conn.lock().await;

    for email in &emails {
        synced_ids.push(email.id.clone());

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

    if !synced_ids.is_empty() {
        let mut oldest_ts = i64::MAX;
        for email in &emails {
            if email.internal_ts < oldest_ts { oldest_ts = email.internal_ts; }
        }

        let mut placeholders = String::new();
        for i in 0..synced_ids.len() {
            if i > 0 { placeholders.push_str(","); }
            placeholders.push_str("?");
        }

        let sql = format!(
            "UPDATE emails SET mailbox='OTHER' 
             WHERE account_id=?1 AND mailbox=?2 AND internal_ts >= ?3 AND id NOT IN ({})",
            placeholders
        );

        let mut params: Vec<rusqlite::types::Value> = vec![
            rusqlite::types::Value::Integer(account_id),
            rusqlite::types::Value::Text(mailbox.to_string()),
            rusqlite::types::Value::Integer(oldest_ts),
        ];
        for id in synced_ids {
            params.push(rusqlite::types::Value::Text(id));
        }

        if let Err(e) = conn.execute(&sql, rusqlite::params_from_iter(params)) {
            log::error!("IMAP upsert OTHER marking failed: {}", e);
        }
    }
}
