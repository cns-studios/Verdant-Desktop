use std::sync::Arc;

use serde_json::json;
use tauri::State;

use crate::state::{ensure_token, get_active_id, DbState};

pub(crate) fn log_imap_outcome(outcome: Result<Result<(), String>, tokio::task::JoinError>) {
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log::warn!("IMAP server update failed: {}", e),
        Err(e) => log::error!("IMAP server update task failed: {}", e),
    }
}

#[tauri::command]
pub async fn set_email_read_status(
    state: State<'_, Arc<DbState>>,
    email_id: String,
    is_read: bool,
) -> Result<(), String> {
    let account_id = get_active_id(&state).await;
    push_read_status(&state, account_id, &email_id, is_read).await;

    let conn = state.conn.lock().await;
    conn.execute(
        "UPDATE emails SET is_read=?1 WHERE id=?2 AND account_id=?3",
        rusqlite::params![is_read as i32, email_id, account_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) async fn push_read_status(state: &State<'_, Arc<DbState>>, account_id: i64, email_id: &str, is_read: bool) {
    let account = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id).ok().flatten()
    };
    let Some(acc) = account else { return };

    if acc.provider == "gmail" {
        let gmail_id = email_id.splitn(2, ':').nth(1).unwrap_or(email_id).to_string();
        if let Ok(token_info) = ensure_token(state).await {
            let client = reqwest::Client::new();
            let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify", gmail_id);
            let body = if is_read {
                json!({"removeLabelIds": ["UNREAD"]})
            } else {
                json!({"addLabelIds": ["UNREAD"]})
            };
            let _ = client.post(url).bearer_auth(&token_info.access_token).json(&body).send().await;
        }
    } else if acc.provider == "imap" {
        let msg_id = email_id.splitn(2, ':').nth(1).unwrap_or(email_id).to_string();
        let mailbox = {
            let conn = state.conn.lock().await;
            conn.query_row("SELECT mailbox FROM emails WHERE id=?1 AND account_id=?2",
                rusqlite::params![email_id, account_id], |r| r.get::<_, String>(0)).unwrap_or_else(|_| "INBOX".to_string())
        };
        let outcome = tokio::task::spawn_blocking(move || {
            crate::imap_client::imap_set_flag(&acc, &msg_id, "\\Seen", is_read, &mailbox)
        }).await;
        log_imap_outcome(outcome);
    }
}

#[tauri::command]
pub async fn mark_emails_read(state: State<'_, Arc<DbState>>, email_ids: Vec<String>) -> Result<(), String> {
    let account_id = get_active_id(&state).await;
    let unread: Vec<String> = {
        let conn = state.conn.lock().await;
        let mut unread = Vec::new();
        for id in &email_ids {
            let changed = conn.execute(
                "UPDATE emails SET is_read=1 WHERE id=?1 AND account_id=?2 AND is_read=0",
                rusqlite::params![id, account_id],
            ).map_err(|e| e.to_string())?;
            if changed > 0 {
                unread.push(id.clone());
            }
        }
        unread
    };
    for id in &unread {
        push_read_status(&state, account_id, id, true).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn mark_thread_read(state: State<'_, Arc<DbState>>, thread_id: String) -> Result<(), String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    conn.execute(
        "UPDATE emails SET is_read=1 WHERE thread_id=?1 AND account_id=?2",
        rusqlite::params![thread_id, account_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn toggle_starred(state: State<'_, Arc<DbState>>, email_id: String) -> Result<(), String> {
    let account_id = get_active_id(&state).await;

    let (is_currently_starred, mailbox) = {
        let conn = state.conn.lock().await;
        let starred: bool = conn.query_row("SELECT starred FROM emails WHERE id=?1 AND account_id=?2",
            rusqlite::params![email_id, account_id], |r| r.get::<_, i32>(0)).unwrap_or(0) != 0;
        let mb: String = conn.query_row("SELECT mailbox FROM emails WHERE id=?1 AND account_id=?2",
            rusqlite::params![email_id, account_id], |r| r.get::<_, String>(0)).unwrap_or_else(|_| "INBOX".to_string());
        (starred, mb)
    };
    let will_be_starred = !is_currently_starred;

    let account = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id).ok().flatten()
    };

    if let Some(ref acc) = account {
        if acc.provider == "gmail" {
            let gmail_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
            if let Ok(token_info) = ensure_token(&state).await {
                let client = reqwest::Client::new();
                let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify", gmail_id);
                let body = if will_be_starred {
                    json!({"addLabelIds": ["STARRED"]})
                } else {
                    json!({"removeLabelIds": ["STARRED"]})
                };
                let _ = client.post(url).bearer_auth(&token_info.access_token).json(&body).send().await;
            }
        } else if acc.provider == "imap" {
            let msg_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
            let acc_clone = acc.clone();
            let mb = mailbox.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                crate::imap_client::imap_set_flag(&acc_clone, &msg_id, "\\Flagged", will_be_starred, &mb)
            }).await;
            log_imap_outcome(outcome);
        }
    }

    let conn = state.conn.lock().await;
    conn.execute(
        "UPDATE emails SET starred=CASE WHEN starred=1 THEN 0 ELSE 1 END WHERE id=?1 AND account_id=?2",
        rusqlite::params![email_id, account_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}
