use std::sync::Arc;
use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde_json::Value;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::state::{ensure_token_for, get_active_id, DbState};

#[derive(serde::Serialize)]
pub struct AttachmentSaved {
    pub path: String,
    pub filename: String,
}

#[tauri::command]
pub async fn download_attachment(
    app: tauri::AppHandle,
    state: State<'_, Arc<DbState>>,
    email_id: String,
    attachment_id: String,
    filename: String,
    content_type: String,
) -> Result<AttachmentSaved, String> {
    let parts: Vec<&str> = email_id.splitn(2, ':').collect();
    let (account_id, message_id) = if parts.len() == 2 {
        let aid = parts[0].parse::<i64>()
            .map_err(|e| format!("Invalid account ID in email_id: {}", e))?;
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

    let (bytes, resolved_filename, _resolved_mime) = if attachment_id.trim().starts_with("imap-")
        || account.provider == "imap"
    {
        let attachment_id_clean = attachment_id.trim().to_string();
        let account_clone = account.clone();
        let result = tokio::task::spawn_blocking(move || {
            crate::imap_sync::fetch_attachment(&account_clone, &attachment_id_clean)
        }).await.map_err(|e| e.to_string())??;

        let fname = if result.filename.is_empty() {
            filename.clone()
        } else {
            result.filename
        };

        let bytes = STANDARD
            .decode(result.data_base64.as_bytes())
            .map_err(|e| format!("Failed to decode IMAP attachment data: {}", e))?;

        (bytes, fname, result.mime_type)
    } else {
        let token = ensure_token_for(&state, account_id).await?.access_token;

        let client = reqwest::Client::new();
        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/attachments/{}",
            message_id.trim(),
            attachment_id.trim()
        );

        let res = client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(format!("Gmail attachment download failed: {} {}", status, body));
        }

        let json = res.json::<Value>().await.map_err(|e| e.to_string())?;
        let encoded = json
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| "Attachment data missing from Gmail response".to_string())?;

        let bytes = URL_SAFE_NO_PAD
            .decode(encoded.as_bytes())
            .or_else(|_| URL_SAFE.decode(encoded.as_bytes()))
            .or_else(|_| STANDARD.decode(encoded.as_bytes()))
            .map_err(|e| format!("Failed to decode Gmail attachment data: {}", e))?;

        let fname = filename.clone();
        (bytes, fname, content_type.clone())
    };

    let effective_filename = if resolved_filename.is_empty() {
        "attachment".to_string()
    } else {
        resolved_filename
    };

    let file_path = app
        .dialog()
        .file()
        .set_file_name(&effective_filename)
        .blocking_save_file()
        .ok_or_else(|| "Save cancelled".to_string())?;

    let save_path = file_path
        .into_path()
        .map_err(|e| format!("Invalid file path: {}", e))?;

    std::fs::write(&save_path, &bytes)
        .map_err(|e| format!("Failed to save file: {}", e))?;

    Ok(AttachmentSaved {
        path: save_path.to_string_lossy().to_string(),
        filename: effective_filename,
    })
}
