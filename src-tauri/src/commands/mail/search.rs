use std::sync::Arc;

use serde_json::Value;
use tauri::State;

use crate::db::{map_email_row, Email, EMAIL_COLUMNS};
use crate::gmail::{collect_attachments, extract_body, header_value, mailbox_from_labels, strip_confusable_chars, AttachmentMeta};
use crate::state::{ensure_token, get_active_id, DbState};

#[tauri::command]
pub async fn deep_search_emails(
    state: State<'_, Arc<DbState>>,
    query: String,
) -> Result<Vec<Email>, String> {
    let account_id = get_active_id(&state).await;

    let account_info = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id)
            .ok().flatten()
    };

    if let Some(ref acc) = account_info {
        if acc.provider == "imap" {
            let acc_clone = acc.clone();
            let q = query.clone();
            let result = tokio::task::spawn_blocking(move || {
                crate::imap_client::imap_search_emails(&acc_clone, &q, 100)
            }).await;
            return match result {
                Ok(Ok(emails)) => Ok(emails),
                Ok(Err(_e)) => {
                    let conn = state.conn.lock().await;
                    let pattern = format!("%{}%", query);
                    let mut stmt = conn.prepare(
                        &format!("SELECT {EMAIL_COLUMNS} FROM emails WHERE account_id=?1 AND (subject LIKE ?2 OR sender LIKE ?2 OR snippet LIKE ?2) ORDER BY internal_ts DESC LIMIT 100")).map_err(|e| e.to_string())?;
                    let emails = stmt.query_map(rusqlite::params![account_id, pattern], map_email_row)
                        .map_err(|e| e.to_string())?.filter_map(Result::ok).collect();
                    Ok(emails)
                },
                Err(e) => Err(format!("Search task error: {}", e)),
            };
        }
    }

    let token = ensure_token(&state).await?.access_token;
    let client = reqwest::Client::new();
    let q = format!("in:anywhere {}", query.trim());

    let list = client
        .get("https://gmail.googleapis.com/gmail/v1/users/me/messages")
        .query(&[("maxResults", "100"), ("q", q.as_str())])
        .bearer_auth(&token)
        .send().await.map_err(|e| e.to_string())?;

    if !list.status().is_success() {
        return Err(format!("Deep search failed: {}", list.status()));
    }

    let json = list.json::<Value>().await.map_err(|e| e.to_string())?;
    let refs = json.get("messages").and_then(Value::as_array).cloned().unwrap_or_default();

    let mut results = Vec::new();
    for msg in refs {
        let Some(id) = msg.get("id").and_then(Value::as_str) else { continue; };

        let detail = client
            .get(format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full", id))
            .bearer_auth(&token).send().await.map_err(|e| e.to_string())?;
        if !detail.status().is_success() { continue; }

        let detail_json = detail.json::<Value>().await.map_err(|e| e.to_string())?;
        let headers = detail_json.get("payload").and_then(|p| p.get("headers")).and_then(Value::as_array).cloned().unwrap_or_default();

        let snippet = strip_confusable_chars(detail_json.get("snippet").and_then(Value::as_str).unwrap_or_default());
        let subject = strip_confusable_chars(&header_value(&headers, "Subject").unwrap_or_else(|| "(No Subject)".to_string()));
        let sender = strip_confusable_chars(&header_value(&headers, "From").unwrap_or_else(|| "Unknown Sender".to_string()));
        let to_recipients = strip_confusable_chars(&header_value(&headers, "To").unwrap_or_default());
        let cc_recipients = strip_confusable_chars(&header_value(&headers, "Cc").unwrap_or_default());
        let bcc_recipients = strip_confusable_chars(&header_value(&headers, "Bcc").unwrap_or_default());
        let date = header_value(&headers, "Date").unwrap_or_default();
        let list_unsubscribe = header_value(&headers, "List-Unsubscribe").unwrap_or_default();
        let labels = detail_json.get("labelIds").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(",")).unwrap_or_default();
        let internal_ts = detail_json.get("internalDate").and_then(Value::as_str).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        let body_html = detail_json.get("payload").and_then(extract_body).unwrap_or_else(|| format!("<pre>{}</pre>", snippet));
        let mut attachments: Vec<AttachmentMeta> = Vec::new();
        if let Some(payload) = detail_json.get("payload") { collect_attachments(payload, &mut attachments); }
        let attachments_json = serde_json::to_string(&attachments).unwrap_or_else(|_| "[]".to_string());

        results.push(Email {
            id: format!("{}:{}", account_id, id),
            account_id,
            draft_id: None,
            thread_id: detail_json.get("threadId").and_then(Value::as_str).unwrap_or_default().to_string(),
            subject, sender, to_recipients, cc_recipients, bcc_recipients, snippet, body_html, attachments_json,
            has_attachments: !attachments.is_empty(),
            date,
            is_read: !labels.split(',').any(|l| l == "UNREAD"),
            starred: labels.split(',').any(|l| l == "STARRED"),
            mailbox: mailbox_from_labels(&labels),
            labels,
            internal_ts,
            notified: false,
            list_unsubscribe,
            unsubscribed: false,
        });
    }

    Ok(results)
}
