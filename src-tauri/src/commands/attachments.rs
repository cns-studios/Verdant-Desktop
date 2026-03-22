use std::sync::Arc;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde_json::Value;
use tauri::State;

use crate::state::{ensure_token, DbState};

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
    
    if attachment_id.trim().starts_with("imap-") {
        return Err("IMAP attachment download not yet supported via this path".to_string());
    }

    let token = ensure_token(&state).await?.access_token;

    
    let gmail_email_id = email_id.splitn(2, ':').nth(1).unwrap_or(email_id.trim());

    let client = reqwest::Client::new();
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/attachments/{}",
        gmail_email_id.trim(),
        attachment_id.trim()
    );

    let res = client.get(url).bearer_auth(&token).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Attachment download failed: {} {}", status, body));
    }

    let json = res.json::<Value>().await.map_err(|e| e.to_string())?;
    let encoded = json.get("data").and_then(Value::as_str)
        .ok_or_else(|| "Attachment data missing".to_string())?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded.as_bytes()).map_err(|e| e.to_string())?;

    Ok(AttachmentDownload {
        filename,
        content_type: if content_type.trim().is_empty() { "application/octet-stream".to_string() } else { content_type },
        data_base64: STANDARD.encode(bytes),
    })
}
