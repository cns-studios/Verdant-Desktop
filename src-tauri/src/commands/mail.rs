use serde_json::{json, Value};
use tauri::State;

use crate::db::{clear_emails, Email};
use crate::gmail::{
    collect_attachments, extract_body, header_value, mailbox_from_labels, mailbox_label,
    strip_confusable_chars, AttachmentMeta,
};
use crate::state::{ensure_token, DbState};

#[derive(serde::Serialize)]
pub struct MailboxCounts {
    pub inbox_total: i64,
    pub inbox_unread: i64,
    pub starred_total: i64,
    pub sent_total: i64,
    pub drafts_total: i64,
    pub archive_total: i64,
}

pub async fn sync_mailbox_page_internal(
    state: &DbState,
    mailbox: &str,
    page_token: Option<String>,
) -> Result<Option<String>, String> {
    let Some(label) = mailbox_label(mailbox) else {
        return Ok(None);
    };

    let client = reqwest::Client::new();
    let token = ensure_token(state).await?.access_token;

    let mut list_url = if mailbox == "DRAFT" {
        "https://gmail.googleapis.com/gmail/v1/users/me/drafts?maxResults=50".to_string()
    } else {
        format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages?labelIds={}&maxResults=50",
            label
        )
    };
    if let Some(token) = page_token {
        if !token.trim().is_empty() {
            list_url.push_str("&pageToken=");
            list_url.push_str(token.trim());
        }
    }
    let res = client
        .get(list_url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Gmail list API failed: {} {}", status, body));
    }

    let json = res.json::<Value>().await.map_err(|e| e.to_string())?;
    let next_page_token = json
        .get("nextPageToken")
        .and_then(Value::as_str)
        .map(str::to_string);

    let message_refs: Vec<(String, Option<String>)> = if mailbox == "DRAFT" {
        json.get("drafts")
            .and_then(Value::as_array)
            .map(|drafts| {
                drafts
                    .iter()
                    .filter_map(|draft| {
                        let draft_id = draft.get("id").and_then(Value::as_str)?.to_string();
                        let message_id = draft
                            .get("message")
                            .and_then(|m| m.get("id"))
                            .and_then(Value::as_str)?
                            .to_string();
                        Some((message_id, Some(draft_id)))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        json.get("messages")
            .and_then(Value::as_array)
            .map(|messages| {
                messages
                    .iter()
                    .filter_map(|msg| {
                        msg.get("id")
                            .and_then(Value::as_str)
                            .map(|id| (id.to_string(), None))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    for (id, draft_id) in message_refs {
        if id.is_empty() {
            continue;
        }

        let exists = {
            let conn = state.conn.lock().await;
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM emails WHERE id = ?1", [id.as_str()], |r| r.get(0))
                .unwrap_or(0);
            count > 0
        };

        let detail_url = if mailbox == "DRAFT" {
            format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/drafts/{}?format=full",
                draft_id.clone().unwrap_or_default()
            )
        } else {
            format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full",
                id
            )
        };
        let detail = client
            .get(detail_url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !detail.status().is_success() {
            continue;
        }

        let raw_detail = detail.json::<Value>().await.map_err(|e| e.to_string())?;
        let detail_json = if mailbox == "DRAFT" {
            raw_detail
                .get("message")
                .cloned()
                .unwrap_or_else(|| json!({}))
        } else {
            raw_detail.clone()
        };

        let resolved_draft_id = if mailbox == "DRAFT" {
            draft_id.clone().or_else(|| {
                raw_detail
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
        } else {
            None
        };

        let thread_id = detail_json
            .get("threadId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let snippet = strip_confusable_chars(
            detail_json
                .get("snippet")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );

        let headers = detail_json
            .get("payload")
            .and_then(|p| p.get("headers"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let subject = strip_confusable_chars(
            &header_value(&headers, "Subject").unwrap_or_else(|| "(No Subject)".to_string()),
        );
        let sender = strip_confusable_chars(
            &header_value(&headers, "From").unwrap_or_else(|| "Unknown Sender".to_string()),
        );
        let to_recipients = strip_confusable_chars(&header_value(&headers, "To").unwrap_or_default());
        let cc_recipients = strip_confusable_chars(&header_value(&headers, "Cc").unwrap_or_default());
        let date = header_value(&headers, "Date").unwrap_or_else(|| "Unknown Date".to_string());
        let internal_ts = detail_json
            .get("internalDate")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        let (existing_body, existing_attachments) = if exists {
            let conn = state.conn.lock().await;
            let body = conn
                .query_row("SELECT body_html FROM emails WHERE id = ?1", [id.as_str()], |r| r.get::<_, String>(0))
                .ok();
            let attachments = conn
                .query_row(
                    "SELECT attachments_json FROM emails WHERE id = ?1",
                    [id.as_str()],
                    |r| r.get::<_, String>(0),
                )
                .ok();
            (body, attachments)
        } else {
            (None, None)
        };

        let body_html = detail_json
            .get("payload")
            .and_then(extract_body)
            .or(existing_body)
            .unwrap_or_else(|| format!("<pre>{}</pre>", snippet));

        let mut attachments: Vec<AttachmentMeta> = Vec::new();
        if let Some(payload) = detail_json.get("payload") {
            collect_attachments(payload, &mut attachments);
        }

        let attachments_json = if attachments.is_empty() {
            existing_attachments.unwrap_or_else(|| "[]".to_string())
        } else {
            serde_json::to_string(&attachments).unwrap_or_else(|_| "[]".to_string())
        };
        let has_attachments = !attachments_json.trim().is_empty() && attachments_json.trim() != "[]";

        let labels = detail_json
            .get("labelIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();

        let is_read = !labels.split(',').any(|l| l == "UNREAD");

        let conn = state.conn.lock().await;
        conn.execute(
                "INSERT INTO emails (id, draft_id, thread_id, subject, sender, to_recipients, cc_recipients, snippet, body_html, attachments_json, has_attachments, date, is_read, mailbox, labels, internal_ts)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(id)
             DO UPDATE SET
                     draft_id = excluded.draft_id,
                thread_id = excluded.thread_id,
                subject = excluded.subject,
                sender = excluded.sender,
                to_recipients = excluded.to_recipients,
                cc_recipients = excluded.cc_recipients,
                snippet = excluded.snippet,
                body_html = excluded.body_html,
                attachments_json = excluded.attachments_json,
                has_attachments = excluded.has_attachments,
                date = excluded.date,
                mailbox = excluded.mailbox,
                labels = excluded.labels,
                internal_ts = excluded.internal_ts",
            (
                id,
                resolved_draft_id,
                &thread_id,
                &subject,
                &sender,
                &to_recipients,
                &cc_recipients,
                &snippet,
                &body_html,
                &attachments_json,
                has_attachments as i32,
                &date,
                is_read as i32,
                mailbox,
                &labels,
                internal_ts,
            ),
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(next_page_token)
}

pub async fn sync_mailbox_internal(state: &DbState, mailbox: &str) -> Result<(), String> {
    let _ = sync_mailbox_page_internal(state, mailbox, None).await?;
    Ok(())
}

#[tauri::command]
pub async fn sync_emails(state: State<'_, DbState>) -> Result<(), String> {
    sync_mailbox_internal(&state, "INBOX").await
}

#[tauri::command]
pub async fn sync_mailbox(state: State<'_, DbState>, mailbox: String) -> Result<(), String> {
    sync_mailbox_internal(&state, mailbox.as_str()).await
}

#[tauri::command]
pub async fn sync_mailbox_page(
    state: State<'_, DbState>,
    mailbox: String,
    page_token: Option<String>,
) -> Result<Option<String>, String> {
    sync_mailbox_page_internal(&state, mailbox.as_str(), page_token).await
}

#[tauri::command]
pub async fn get_emails(
    state: State<'_, DbState>,
    mailbox: Option<String>,
) -> Result<Vec<Email>, String> {
    let box_name = mailbox.unwrap_or_else(|| "INBOX".to_string());
    let conn = state.conn.lock().await;

    let sql = if box_name == "STARRED" {
        "SELECT id, draft_id, thread_id, subject, sender, to_recipients, cc_recipients, snippet, body_html, attachments_json, has_attachments, date, is_read, starred, mailbox, labels, internal_ts
         FROM emails WHERE starred = 1 ORDER BY internal_ts DESC, rowid DESC LIMIT 500"
    } else {
        "SELECT id, draft_id, thread_id, subject, sender, to_recipients, cc_recipients, snippet, body_html, attachments_json, has_attachments, date, is_read, starred, mailbox, labels, internal_ts
         FROM emails WHERE mailbox = ?1 ORDER BY internal_ts DESC, rowid DESC LIMIT 500"
    };

    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;

    let mapper = |row: &rusqlite::Row<'_>| {
        Ok(Email {
            id: row.get(0)?,
            draft_id: row.get(1)?,
            thread_id: row.get(2)?,
            subject: row.get(3)?,
            sender: row.get(4)?,
            to_recipients: row.get(5)?,
            cc_recipients: row.get(6)?,
            snippet: row.get(7)?,
            body_html: row.get(8)?,
            attachments_json: row.get(9)?,
            has_attachments: row.get::<_, i32>(10)? != 0,
            date: row.get(11)?,
            is_read: row.get::<_, i32>(12)? != 0,
            starred: row.get::<_, i32>(13)? != 0,
            mailbox: row.get(14)?,
            labels: row.get(15)?,
            internal_ts: row.get(16)?,
        })
    };

    let emails = if box_name == "STARRED" {
        stmt.query_map([], mapper)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect()
    } else {
        stmt.query_map([box_name.as_str()], mapper)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect()
    };

    Ok(emails)
}

#[tauri::command]
pub async fn deep_search_emails(
    state: State<'_, DbState>,
    query: String,
) -> Result<Vec<Email>, String> {
    let token = ensure_token(&state).await?.access_token;
    let client = reqwest::Client::new();
    let q = format!("in:anywhere {}", query.trim());

    let list = client
        .get("https://gmail.googleapis.com/gmail/v1/users/me/messages")
        .query(&[("maxResults", "100"), ("q", q.as_str())])
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !list.status().is_success() {
        let status = list.status();
        let body = list.text().await.unwrap_or_default();
        return Err(format!("Deep search failed: {} {}", status, body));
    }

    let json = list.json::<Value>().await.map_err(|e| e.to_string())?;
    let refs = json
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut results = Vec::new();
    for msg in refs {
        let Some(id) = msg.get("id").and_then(Value::as_str) else {
            continue;
        };

        let detail = client
            .get(format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full",
                id
            ))
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !detail.status().is_success() {
            continue;
        }

        let detail_json = detail.json::<Value>().await.map_err(|e| e.to_string())?;
        let headers = detail_json
            .get("payload")
            .and_then(|p| p.get("headers"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let snippet = strip_confusable_chars(
            detail_json
                .get("snippet")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        let subject = strip_confusable_chars(
            &header_value(&headers, "Subject").unwrap_or_else(|| "(No Subject)".to_string()),
        );
        let sender = strip_confusable_chars(
            &header_value(&headers, "From").unwrap_or_else(|| "Unknown Sender".to_string()),
        );
        let to_recipients = strip_confusable_chars(&header_value(&headers, "To").unwrap_or_default());
        let cc_recipients = strip_confusable_chars(&header_value(&headers, "Cc").unwrap_or_default());
        let date = header_value(&headers, "Date").unwrap_or_else(|| "Unknown Date".to_string());
        let labels = detail_json
            .get("labelIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        let internal_ts = detail_json
            .get("internalDate")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let body_html = detail_json
            .get("payload")
            .and_then(extract_body)
            .unwrap_or_else(|| format!("<pre>{}</pre>", snippet));
        let mut attachments: Vec<AttachmentMeta> = Vec::new();
        if let Some(payload) = detail_json.get("payload") {
            collect_attachments(payload, &mut attachments);
        }
        let attachments_json = serde_json::to_string(&attachments).unwrap_or_else(|_| "[]".to_string());

        results.push(Email {
            id: id.to_string(),
            draft_id: None,
            thread_id: detail_json
                .get("threadId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            subject,
            sender,
            to_recipients,
            cc_recipients,
            snippet,
            body_html,
            attachments_json,
            has_attachments: !attachments.is_empty(),
            date,
            is_read: !labels.split(',').any(|l| l == "UNREAD"),
            starred: labels.split(',').any(|l| l == "STARRED"),
            mailbox: mailbox_from_labels(&labels),
            labels,
            internal_ts,
        });
    }

    Ok(results)
}

#[tauri::command]
pub async fn set_email_read_status(
    state: State<'_, DbState>,
    email_id: String,
    is_read: bool,
) -> Result<(), String> {
    let conn = state.conn.lock().await;
    conn.execute(
        "UPDATE emails SET is_read = ?1 WHERE id = ?2",
        (is_read as i32, email_id),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn toggle_starred(state: State<'_, DbState>, email_id: String) -> Result<(), String> {
    let conn = state.conn.lock().await;
    conn.execute(
        "UPDATE emails SET starred = CASE WHEN starred = 1 THEN 0 ELSE 1 END WHERE id = ?1",
        [email_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn archive_email(state: State<'_, DbState>, email_id: String) -> Result<(), String> {
    let token = ensure_token(&state).await?.access_token;
    let client = reqwest::Client::new();
    let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify", email_id);

    let res = client
        .post(url)
        .bearer_auth(&token)
        .json(&json!({"removeLabelIds": ["INBOX"]}))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Archive failed: {}", res.status()));
    }

    let conn = state.conn.lock().await;
    conn.execute("UPDATE emails SET mailbox = 'ARCHIVE' WHERE id = ?1", [email_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn trash_email(state: State<'_, DbState>, email_id: String) -> Result<(), String> {
    let token = ensure_token(&state).await?.access_token;
    let client = reqwest::Client::new();
    let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/trash", email_id);

    let res = client
        .post(url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Trash failed: {}", res.status()));
    }

    let conn = state.conn.lock().await;
    conn.execute("DELETE FROM emails WHERE id = ?1", [email_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_mailbox_counts(state: State<'_, DbState>) -> Result<MailboxCounts, String> {
    let conn = state.conn.lock().await;

    let inbox_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM emails WHERE mailbox = 'INBOX'", [], |r| r.get(0))
        .unwrap_or(0);
    let inbox_unread: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM emails WHERE mailbox = 'INBOX' AND is_read = 0",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let starred_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM emails WHERE starred = 1", [], |r| r.get(0))
        .unwrap_or(0);
    let sent_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM emails WHERE mailbox = 'SENT'", [], |r| r.get(0))
        .unwrap_or(0);
    let drafts_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM emails WHERE mailbox = 'DRAFT'", [], |r| r.get(0))
        .unwrap_or(0);
    let archive_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM emails WHERE mailbox = 'ARCHIVE'", [], |r| r.get(0))
        .unwrap_or(0);

    Ok(MailboxCounts {
        inbox_total,
        inbox_unread,
        starred_total,
        sent_total,
        drafts_total,
        archive_total,
    })
}

#[tauri::command]
pub async fn clear_local_data(state: State<'_, DbState>) -> Result<(), String> {
    let conn = state.conn.lock().await;
    clear_emails(&conn).map_err(|e| e.to_string())?;
    Ok(())
}
