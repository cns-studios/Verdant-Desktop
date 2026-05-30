use std::sync::Arc;
use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde_json::Value;
use tauri::State;

use crate::state::{ensure_token_for, get_active_id, DbState};

#[derive(serde::Serialize)]
pub struct AttachmentDownload {
    pub filename: String,
    pub content_type: String,
    pub data_base64: String,
}

#[tauri::command]
pub async fn download_attachment(
    state: State<'_, Arc<DbState>>,
    email_id: String,
    attachment_id: String,
    filename: String,
    content_type: String,
) -> Result<AttachmentDownload, String> {
    let parts: Vec<&str> = email_id.splitn(2, ':').collect();
    let (account_id, message_id) = if parts.len() == 2 {
        let aid = parts[0].parse::<i64>().map_err(|e| format!("Invalid account ID in email_id: {}", e))?;
        (aid, parts[1])
    } else {
        (get_active_id(&state).await, email_id.as_str())
    };

    let account = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Account {} not found", account_id))?
    };

    if attachment_id.trim().starts_with("imap-") || account.provider == "imap" {
        let attachment_id_clean = attachment_id.trim().to_string();
        let account_clone = account.clone();

        let result = tokio::task::spawn_blocking(move || {
            crate::imap_sync::fetch_attachment(&account_clone, &attachment_id_clean)
        }).await.map_err(|e| e.to_string())??;

        return Ok(AttachmentDownload {
            filename: if result.filename.is_empty() { filename } else { result.filename },
            content_type: if result.mime_type.is_empty() { content_type } else { result.mime_type },
            data_base64: result.data_base64,
        });
    }

    let token = ensure_token_for(&state, account_id).await?.access_token;

    let client = reqwest::Client::new();
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/attachments/{}",
        message_id.trim(),
        attachment_id.trim()
    );

    let res = client.get(url).bearer_auth(&token).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Gmail attachment download failed: {} {}", status, body));
    }

    let json = res.json::<Value>().await.map_err(|e| e.to_string())?;
    let encoded = json.get("data").and_then(Value::as_str)
        .ok_or_else(|| "Attachment data missing from Gmail response".to_string())?;

    let bytes = URL_SAFE_NO_PAD.decode(encoded.as_bytes())
        .or_else(|_| URL_SAFE.decode(encoded.as_bytes()))
        .or_else(|_| STANDARD.decode(encoded.as_bytes()))
        .map_err(|e| format!("Failed to decode Gmail attachment data: {}", e))?;

    Ok(AttachmentDownload {
        filename,
        content_type: if content_type.trim().is_empty() { "application/octet-stream".to_string() } else { content_type },
        data_base64: STANDARD.encode(bytes),
    })
}
