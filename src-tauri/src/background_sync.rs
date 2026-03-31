use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

use crate::db::{get_all_accounts, Account};
use crate::state::DbState;

const SYNC_INTERVAL_SECS: u64 = 45;
const IMAP_SYNC_INTERVAL_SECS: u64 = 12;
const IMAP_MAILBOXES: &[&str] = &["INBOX", "SENT", "DRAFT", "TRASH"];


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
        run_sync_loop(app, state_clone, account, rx).await;
    });
}


pub async fn stop_account_sync(state: &DbState, account_id: i64) {
    let mut handles = state.sync_handles.lock().await;
    if let Some(tx) = handles.remove(&account_id) {
        let _ = tx.send(());
    }
}

async fn run_sync_loop(
    app: tauri::AppHandle,
    state: Arc<DbState>,
    account: Account,
    mut shutdown: oneshot::Receiver<()>,
) {
    
    sync_account(&app, &state, &account).await;

    let interval = if account.provider == "imap" { IMAP_SYNC_INTERVAL_SECS } else { SYNC_INTERVAL_SECS };

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(interval)) => {
                
                let fresh_account = {
                    let conn = state.conn.lock().await;
                    crate::db::get_account_by_id(&conn, account.id)
                        .ok()
                        .flatten()
                };
                if let Some(acc) = fresh_account {
                    sync_account(&app, &state, &acc).await;
                } else {
                    
                    break;
                }
            }
            _ = &mut shutdown => {
                break;
            }
        }
    }
}

async fn sync_account(app: &tauri::AppHandle, state: &DbState, account: &Account) {
    match account.provider.as_str() {
        "gmail" => sync_gmail_account(app, state, account).await,
        "imap" => sync_imap_account(app, state, account).await,
        _ => {}
    }
}

async fn sync_gmail_account(app: &tauri::AppHandle, state: &DbState, account: &Account) {
    use crate::commands::mail::sync_mailbox_internal_for;

    let mailboxes = ["INBOX", "SENT", "DRAFT", "TRASH"];
    let mut total_new_unread = 0;

    for mailbox in &mailboxes {
        if let Err(e) = sync_mailbox_internal_for(state, account.id, mailbox).await {
            log::error!("Gmail sync error account={} mailbox={}: {}", account.id, mailbox, e);
        }
    }

    
    let unread_emails = {
        let conn = state.conn.lock().await;
        let mut stmt = conn.prepare("SELECT id, subject, sender FROM emails WHERE account_id=?1 AND mailbox='INBOX' AND is_read=0 AND notified=0 AND internal_ts > (strftime('%s','now') - 3600) LIMIT 50").unwrap();
        let rows = stmt.query_map([account.id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).unwrap();
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
             let _ = conn.execute("UPDATE emails SET notified=1 WHERE id=?1 AND account_id=?2", rusqlite::params![id, account.id]);
        }
    }
    
    use tauri::Emitter;
    let _ = app.emit("emails-synced", ());
}

async fn sync_imap_account(app: &tauri::AppHandle, state: &DbState, account: &Account) {
    use crate::imap_sync::sync_imap_mailbox;

    let account_clone = account.clone();
    let account_id = account.id;
    let app_clone = app.clone();

    for mailbox in IMAP_MAILBOXES {
        let acc = account_clone.clone();
        let mb = mailbox.to_string();

        let result = tokio::task::spawn_blocking(move || {
            sync_imap_mailbox(&acc, &mb, 50)
        }).await;

        match result {
            Ok(Ok(emails)) => {
                upsert_emails(state, account_id, emails, mailbox).await;
            }
            Ok(Err(e)) => {
                log::error!("IMAP sync error account={} mailbox={}: {}", account_id, mailbox, e);
            }
            Err(e) => {
                log::error!("IMAP sync task panicked account={}: {}", account_id, e);
            }
        }
    }

    
    let unread_emails = {
        let conn = state.conn.lock().await;
        let mut stmt = conn.prepare("SELECT id, subject, sender FROM emails WHERE account_id=?1 AND mailbox='INBOX' AND is_read=0 AND notified=0 AND internal_ts > (strftime('%s','now') - 3600) LIMIT 50").unwrap();
        let rows = stmt.query_map([account_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).unwrap();
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
             let _ = conn.execute("UPDATE emails SET notified=1 WHERE id=?1 AND account_id=?2", rusqlite::params![id, account_id]);
        }
    }

    use tauri::Emitter;
    let _ = app.emit("emails-synced", ());
}

async fn upsert_emails(state: &DbState, account_id: i64, emails: Vec<crate::db::Email>, mailbox: &str) {
    let mut synced_ids = Vec::new();
    let conn = state.conn.lock().await;

    for email in &emails {
        synced_ids.push(email.id.clone());

        let _ = conn.execute(
            "INSERT INTO emails (id, account_id, draft_id, thread_id, subject, sender, to_recipients, cc_recipients,
                                 snippet, body_html, attachments_json, has_attachments, date, is_read, starred,
                                 mailbox, labels, internal_ts)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
            ON CONFLICT(id) DO UPDATE SET
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
                internal_ts=excluded.internal_ts",
            rusqlite::params![
                email.id, email.account_id, email.draft_id, email.thread_id,
                email.subject, email.sender, email.to_recipients, email.cc_recipients,
                email.snippet, email.body_html, email.attachments_json,
                email.has_attachments as i32, email.date, email.is_read as i32,
                email.starred as i32, email.mailbox, email.labels, email.internal_ts
            ],
        );
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

        let _ = conn.execute(&sql, rusqlite::params_from_iter(params));
    }
}
