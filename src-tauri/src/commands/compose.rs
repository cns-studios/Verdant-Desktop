use serde_json::{json, Value};
use tauri::State;

use crate::commands::mail::sync_mailbox_internal;
use crate::mime::{build_raw_mime_message, EmailAttachment};
use crate::state::{ensure_token, DbState};

#[derive(serde::Serialize)]
pub struct DraftSaveResult {
    pub draft_id: String,
}

#[tauri::command]
pub async fn send_email(
    state: State<'_, DbState>,
    to: String,
    cc: String,
    subject: String,
    body: String,
    mode: String,
    body_html: Option<String>,
    attachments: Vec<EmailAttachment>,
) -> Result<(), String> {
    let token = ensure_token(&state).await?.access_token;
    let encoded = build_raw_mime_message(to, cc, subject, body, mode, body_html, attachments)?;

    let client = reqwest::Client::new();
    let res = client
        .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
        .bearer_auth(&token)
        .json(&json!({ "raw": encoded }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status().is_success() {
        Ok(())
    } else {
        Err(format!("Error: {}", res.status()))
    }
}

#[tauri::command]
pub async fn save_draft(
    state: State<'_, DbState>,
    to: String,
    cc: String,
    subject: String,
    body: String,
    mode: String,
    body_html: Option<String>,
    attachments: Vec<EmailAttachment>,
    draft_id: Option<String>,
) -> Result<DraftSaveResult, String> {
    let token = ensure_token(&state).await?.access_token;
    let encoded = build_raw_mime_message(to, cc, subject, body, mode, body_html, attachments)?;

    let client = reqwest::Client::new();
    let payload = json!({
        "message": { "raw": encoded }
    });

    let res = if let Some(existing_id) = draft_id.clone().filter(|d| !d.trim().is_empty()) {
        client
            .put(format!("https://gmail.googleapis.com/gmail/v1/users/me/drafts/{}", existing_id))
            .bearer_auth(&token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?
    } else {
        client
            .post("https://gmail.googleapis.com/gmail/v1/users/me/drafts")
            .bearer_auth(&token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?
    };

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Draft save failed: {} {}", status, body));
    }

    let data = res.json::<Value>().await.map_err(|e| e.to_string())?;
    let saved_draft_id = data
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Draft save returned no draft id".to_string())?;

    sync_mailbox_internal(&state, "DRAFT").await?;

    Ok(DraftSaveResult {
        draft_id: saved_draft_id,
    })
}

#[tauri::command]
pub async fn send_existing_draft(
    state: State<'_, DbState>,
    draft_id: String,
) -> Result<(), String> {
    let token = ensure_token(&state).await?.access_token;
    let client = reqwest::Client::new();
    let draft_id_clean = draft_id.trim().to_string();

    let res = client
        .post("https://gmail.googleapis.com/gmail/v1/users/me/drafts/send")
        .bearer_auth(&token)
        .json(&json!({ "id": draft_id_clean }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Draft send failed: {} {}", status, body));
    }

    let sent_msg = res.json::<Value>().await.ok();
    let sent_message_id = sent_msg
        .as_ref()
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);

    {
        let conn = state.conn.lock().await;
        if let Some(message_id) = sent_message_id {
            let _ = conn.execute(
                "DELETE FROM emails WHERE mailbox = 'DRAFT' AND (draft_id = ?1 OR id = ?2)",
                (&draft_id_clean, &message_id),
            );
        } else {
            let _ = conn.execute(
                "DELETE FROM emails WHERE mailbox = 'DRAFT' AND draft_id = ?1",
                [&draft_id_clean],
            );
        }
    }

    let _ = sync_mailbox_internal(&state, "DRAFT").await;
    let _ = sync_mailbox_internal(&state, "SENT").await;

    Ok(())
}
