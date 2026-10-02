use std::sync::Arc;

use serde_json::json;
use tauri::State;

use super::flags::log_imap_outcome;
use crate::db::clear_account_emails;
use crate::state::{ensure_token, get_active_id, DbState};

#[tauri::command]
pub async fn archive_email(state: State<'_, Arc<DbState>>, email_id: String) -> Result<(), String> {
    let account_id = get_active_id(&state).await;

    log::info!("[DEBUG] Archiving email {} (account {})", email_id, account_id);

    let account = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id).ok().flatten()
    };
    let is_gmail = account.clone().map(|a| a.provider == "gmail").unwrap_or(false);

    if is_gmail {
        let gmail_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
        let token = ensure_token(&state).await.map_err(|e| {
            e
        })?.access_token;
        let client = reqwest::Client::new();
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify", gmail_id);
        let res = client.post(url).bearer_auth(&token)
            .json(&json!({"removeLabelIds": ["INBOX"]}))
            .send().await.map_err(|e| {
                e.to_string()
            })?;
        if !res.status().is_success() {
            let err = format!("Archive failed: {}", res.status());
            return Err(err);
        }
    } else {
        if let Some(acc) = account {
            if acc.provider == "imap" {
                let msg_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
                let mailbox = {
                    let conn = state.conn.lock().await;
                    conn.query_row("SELECT mailbox FROM emails WHERE id=?1 AND account_id=?2",
                        rusqlite::params![email_id, account_id], |r| r.get::<_, String>(0)).unwrap_or_else(|_| "INBOX".to_string())
                };
                let acc_clone = acc.clone();
                let outcome = tokio::task::spawn_blocking(move || {
                    crate::imap_client::imap_move_to_folder(&acc_clone, &msg_id, &mailbox, "ARCHIVE")
                }).await;
                log_imap_outcome(outcome);
            }
        }
    }

    let conn = state.conn.lock().await;

    if is_gmail {
         let thread_id: Option<String> = conn.query_row(
            "SELECT thread_id FROM emails WHERE id=?1 AND account_id=?2",
            rusqlite::params![email_id, account_id],
            |r| r.get(0)
        ).ok();

        if let Some(tid) = thread_id {
            let _ = conn.execute(
                "UPDATE emails SET mailbox='OTHER', labels=replace(replace(','||labels||',', ',INBOX,', ','), ',,', ',')
                 WHERE thread_id=?1 AND account_id=?2 AND mailbox='INBOX'",
                rusqlite::params![tid, account_id],
            );
        }
    }

    conn.execute(
        "UPDATE emails SET mailbox='ARCHIVE' WHERE id=?1 AND account_id=?2",
        rusqlite::params![email_id, account_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn trash_email(state: State<'_, Arc<DbState>>, email_id: String) -> Result<(), String> {
    let account_id = get_active_id(&state).await;

    let account = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id).ok().flatten()
    };

    if let Some(ref acc) = account {
        if acc.provider == "gmail" {
            let gmail_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
            let token = ensure_token(&state).await.map_err(|e| {
                e
            })?.access_token;
            let client = reqwest::Client::new();
            let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/trash", gmail_id);
            let res = client.post(url)
                .bearer_auth(&token)
                .header(reqwest::header::CONTENT_LENGTH, 0)
                .send().await.map_err(|e| {
                    e.to_string()
                })?;
            if !res.status().is_success() {
                let err = format!("Trash failed: {}", res.status());
                return Err(err);
            }
        } else if acc.provider == "imap" {
            let msg_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
            let mailbox = {
                let conn = state.conn.lock().await;
                conn.query_row("SELECT mailbox FROM emails WHERE id=?1 AND account_id=?2",
                    rusqlite::params![email_id, account_id], |r| r.get::<_, String>(0)).unwrap_or_else(|_| "INBOX".to_string())
            };
            let acc_clone = acc.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                crate::imap_client::imap_move_to_folder(&acc_clone, &msg_id, &mailbox, "TRASH")
            }).await;
            log_imap_outcome(outcome);
        }
    }

    let conn = state.conn.lock().await;

    let is_gmail = account.map(|a| a.provider == "gmail").unwrap_or(false);
    if is_gmail {
        let thread_id: Option<String> = conn.query_row(
            "SELECT thread_id FROM emails WHERE id=?1 AND account_id=?2",
            rusqlite::params![email_id, account_id],
            |r| r.get(0)
        ).ok();

        if let Some(tid) = thread_id {
             let _ = conn.execute(
                "UPDATE emails SET mailbox='OTHER', labels=replace(replace(','||labels||',', ',INBOX,', ','), ',,', ',')
                 WHERE thread_id=?1 AND account_id=?2 AND mailbox='INBOX' AND id != ?3",
                rusqlite::params![tid, account_id, email_id],
            );
        }
    }

    conn.execute(
        "UPDATE emails SET mailbox='TRASH', labels=(
            CASE WHEN instr(','||labels||',', ',INBOX,') > 0 THEN
                trim(replace(','||labels||',', ',INBOX,', ','), ',') || ',TRASH'
            ELSE
                CASE WHEN labels IS NULL OR labels = '' THEN 'TRASH' ELSE labels || ',TRASH' END
            END
        ) WHERE id=?1 AND account_id=?2",
        rusqlite::params![email_id, account_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn permanent_delete_email(state: State<'_, Arc<DbState>>, email_id: String) -> Result<(), String> {
    let account_id = get_active_id(&state).await;

    let account = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id).ok().flatten()
    };

    if let Some(ref acc) = account {
        if acc.provider == "gmail" {
            let gmail_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
            if let Ok(token_info) = ensure_token(&state).await {
                let client = reqwest::Client::new();
                let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}", gmail_id);
                client.delete(url).bearer_auth(&token_info.access_token).send().await
                    .map_err(|e| format!("Gmail delete error: {}", e))?;
            } else {
                return Err("Failed to get Gmail token".to_string());
            }
        } else if acc.provider == "imap" {
            let msg_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
            let acc_clone = acc.clone();
            tokio::task::spawn_blocking(move || {
                crate::imap_client::imap_set_flag(&acc_clone, &msg_id, "\\Deleted", true, "TRASH")
            }).await
            .map_err(|e| format!("IMAP task error: {}", e))?
            .or_else(|e| if e.starts_with(crate::imap_client::MESSAGE_NOT_FOUND) { Ok(()) } else { Err(e) })
            .map_err(|e| format!("IMAP delete error: {}", e))?;
        }
    }

    let conn = state.conn.lock().await;
    conn.execute(
        "DELETE FROM emails WHERE id=?1 AND account_id=?2",
        rusqlite::params![email_id, account_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn restore_from_trash(state: State<'_, Arc<DbState>>, email_id: String) -> Result<(), String> {
    let account_id = get_active_id(&state).await;

    let account = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id).ok().flatten()
    };

    if let Some(ref acc) = account {
        if acc.provider == "gmail" {
            let gmail_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
            let token = ensure_token(&state).await.map_err(|e| {
                e
            })?.access_token;
            let client = reqwest::Client::new();
            let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify", gmail_id);
            let res = client.post(url).bearer_auth(&token)
                .json(&json!({
                    "addLabelIds": ["INBOX"],
                    "removeLabelIds": ["TRASH"]
                }))
                .send().await.map_err(|e| {
                    e.to_string()
                })?;
            if !res.status().is_success() {
                let err = format!("Restore failed: {}", res.status());
                return Err(err);
            }
        } else if acc.provider == "imap" {
            let msg_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
            let acc_clone = acc.clone();
            tokio::task::spawn_blocking(move || {
                crate::imap_client::imap_move_to_folder(&acc_clone, &msg_id, "TRASH", "INBOX")
            }).await
            .map_err(|e| format!("IMAP task error: {}", e))?
            .or_else(|e| if e.starts_with(crate::imap_client::MESSAGE_NOT_FOUND) { Ok(()) } else { Err(e) })
            .map_err(|e| format!("IMAP restore error: {}", e))?;
        }
    }

    let conn = state.conn.lock().await;
    conn.execute(
        "UPDATE emails SET mailbox='INBOX', labels=(
            CASE WHEN instr(','||labels||',', ',TRASH,') > 0 THEN
                trim(replace(','||labels||',', ',TRASH,', ','), ',') || ',INBOX'
            ELSE
                CASE WHEN labels IS NULL OR labels = '' THEN 'INBOX' ELSE labels || ',INBOX' END
            END
        ) WHERE id=?1 AND account_id=?2",
        rusqlite::params![email_id, account_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn move_to_inbox(state: State<'_, Arc<DbState>>, email_id: String) -> Result<(), String> {
    let account_id = get_active_id(&state).await;

    let account = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id).ok().flatten()
    };

    if let Some(ref acc) = account {
        if acc.provider == "gmail" {
            let gmail_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
            let token = ensure_token(&state).await.map_err(|e| {
                e
            })?.access_token;
            let client = reqwest::Client::new();
            let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify", gmail_id);
            let res = client.post(url).bearer_auth(&token)
                .json(&json!({ "addLabelIds": ["INBOX"] }))
                .send().await.map_err(|e| {
                    e.to_string()
                })?;
            if !res.status().is_success() {
                let err = format!("Move to inbox failed: {}", res.status());
                return Err(err);
            }
        } else if acc.provider == "imap" {
            let msg_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
            let acc_clone = acc.clone();
            tokio::task::spawn_blocking(move || {
                crate::imap_client::imap_move_to_folder(&acc_clone, &msg_id, "ARCHIVE", "INBOX")
            }).await
            .map_err(|e| format!("IMAP task error: {}", e))?
            .or_else(|e| if e.starts_with(crate::imap_client::MESSAGE_NOT_FOUND) { Ok(()) } else { Err(e) })
            .map_err(|e| format!("IMAP move error: {}", e))?;
        }
    }

    let conn = state.conn.lock().await;
    conn.execute(
        "UPDATE emails SET mailbox='INBOX', labels=(
            CASE WHEN instr(','||labels||',', ',ARCHIVE,') > 0 THEN
                trim(replace(','||labels||',', ',ARCHIVE,', ','), ',') || ',INBOX'
            ELSE
                CASE WHEN labels IS NULL OR labels = '' THEN 'INBOX' ELSE labels || ',INBOX' END
            END
        ) WHERE id=?1 AND account_id=?2",
        rusqlite::params![email_id, account_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn clear_local_data(state: State<'_, Arc<DbState>>) -> Result<(), String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    clear_account_emails(&conn, account_id).map_err(|e| e.to_string())?;
    Ok(())
}
