use futures_util::StreamExt;
use serde_json::{json, Value};

use super::{collect_attachments, extract_body, gmail_api_get, header_value, mailbox_from_labels, strip_confusable_chars, AttachmentMeta};
use crate::db::gmail_message_cached;
use crate::state::DbState;

pub(crate) const GMAIL_FETCH_CONCURRENCY: usize = 6;

pub(crate) struct GmailMessageDraft {
    pub(crate) message_id: String,
    pub(crate) draft_id: Option<String>,
}

pub(crate) fn parse_message_refs(json: &Value, mailbox: &str) -> Vec<GmailMessageDraft> {
    if mailbox == "DRAFT" {
        json.get("drafts").and_then(Value::as_array).map(|drafts| {
            drafts.iter().filter_map(|draft| {
                let draft_id = draft.get("id").and_then(Value::as_str)?.to_string();
                let message_id = draft.get("message").and_then(|m| m.get("id")).and_then(Value::as_str)?.to_string();
                Some(GmailMessageDraft { message_id, draft_id: Some(draft_id) })
            }).collect::<Vec<_>>()
        }).unwrap_or_default()
    } else {
        json.get("messages").and_then(Value::as_array).map(|messages| {
            messages.iter().filter_map(|msg| {
                msg.get("id").and_then(Value::as_str).map(|id| GmailMessageDraft { message_id: id.to_string(), draft_id: None })
            }).collect::<Vec<_>>()
        }).unwrap_or_default()
    }
}

#[derive(Default, Clone)]
pub(crate) struct LabelDelta {
    pub(crate) added: Vec<String>,
    pub(crate) removed: Vec<String>,
}

pub(crate) enum FetchJob {
    Full(GmailMessageDraft),
    Label { message_id: String, delta: LabelDelta },
}

pub(crate) fn mailbox_from_labels_array(labels: &[String]) -> String {
    mailbox_from_labels(&labels.join(","))
}

pub(crate) async fn fetch_and_store_messages(
    state: &DbState,
    account_id: i64,
    client: &reqwest::Client,
    token: &str,
    full_refs: Vec<GmailMessageDraft>,
    meta_changes: Vec<(String, LabelDelta)>,
) -> Result<(), String> {
    let full_len = full_refs.len();
    let stream = futures_util::stream::iter(full_refs.into_iter().map(|m| {
        FetchJob::Full(m)
    }))
    .chain(futures_util::stream::iter(meta_changes.into_iter().map(|(id, delta)| {
        FetchJob::Label { message_id: id, delta }
    })))
    .map(move |job| {
        let client = client.clone();
        let token = token.to_string();
        let state = state;
        async move {
            match job {
                FetchJob::Label { message_id, delta } => {
                    let composite_id = format!("{}:{}", account_id, message_id);
                    let conn = state.conn.lock().await;
                    let existing: Option<String> = conn.query_row(
                        "SELECT labels FROM emails WHERE id = ?1 AND account_id = ?2",
                        rusqlite::params![composite_id, account_id],
                        |r| r.get(0),
                    ).ok();
                    let mut final_labels: Vec<String> = match &existing {
                        Some(raw) => raw.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect(),
                        None => Vec::new(),
                    };
                    for l in &delta.added {
                        if !final_labels.contains(l) {
                            final_labels.push(l.clone());
                        }
                    }
                    final_labels.retain(|l| !delta.removed.contains(l));
                    let mailbox = mailbox_from_labels_array(&final_labels);
                    let labels_str = final_labels.join(",");
                    let is_read = !labels_str.split(',').any(|l| l == "UNREAD");
                    let _ = conn.execute(
                        "UPDATE emails SET mailbox = ?1, labels = ?2, is_read = ?3 WHERE id = ?4 AND account_id = ?5",
                        rusqlite::params![mailbox, labels_str, is_read as i32, composite_id, account_id],
                    );
                    Ok(())
                }
                FetchJob::Full(r) => {
                    if r.message_id.is_empty() {
                        return Ok(());
                    }
                    let composite_id = format!("{}:{}", account_id, r.message_id);

                    let cached = {
                        let conn = state.conn.lock().await;
                        gmail_message_cached(&conn, account_id, &r.message_id).unwrap_or(false)
                    };
                    if cached {
                        return Ok(());
                    }

                    let detail_url = if r.draft_id.is_some() {
                        format!("https://gmail.googleapis.com/gmail/v1/users/me/drafts/{}?format=full", r.draft_id.as_deref().unwrap_or_default())
                    } else {
                        format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full", r.message_id)
                    };

                    let detail = gmail_api_get(state, account_id, &client, &token, detail_url).await?;
                    let detail_json = if r.draft_id.is_some() {
                        detail.get("message").cloned().unwrap_or_else(|| json!({}))
                    } else {
                        detail
                    };

                    let resolved_draft_id = if r.draft_id.is_some() {
                        r.draft_id.clone().or_else(|| detail_json.get("id").and_then(Value::as_str).map(str::to_string))
                    } else {
                        None
                    };

                    let thread_id = detail_json.get("threadId").and_then(Value::as_str).unwrap_or_default().to_string();
                    let snippet = strip_confusable_chars(detail_json.get("snippet").and_then(Value::as_str).unwrap_or_default());

                    let headers = detail_json.get("payload").and_then(|p| p.get("headers")).and_then(Value::as_array).cloned().unwrap_or_default();
                    let subject = strip_confusable_chars(&header_value(&headers, "Subject").unwrap_or_else(|| "(No Subject)".to_string()));
                    let sender = strip_confusable_chars(&header_value(&headers, "From").unwrap_or_else(|| "Unknown Sender".to_string()));
                    let to_recipients = strip_confusable_chars(&header_value(&headers, "To").unwrap_or_default());
                    let cc_recipients = strip_confusable_chars(&header_value(&headers, "Cc").unwrap_or_default());
                    let bcc_recipients = strip_confusable_chars(&header_value(&headers, "Bcc").unwrap_or_default());
                    let date = header_value(&headers, "Date").unwrap_or_else(|| "Unknown Date".to_string());
                    let list_unsubscribe = header_value(&headers, "List-Unsubscribe").unwrap_or_default();
                    let internal_ts = detail_json.get("internalDate").and_then(Value::as_str).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);

                    let body_html = detail_json.get("payload").and_then(extract_body)
                        .unwrap_or_else(|| format!("<pre>{}</pre>", snippet));

                    let mut attachments: Vec<AttachmentMeta> = Vec::new();
                    if let Some(payload) = detail_json.get("payload") {
                        collect_attachments(payload, &mut attachments);
                    }
                    let attachments_json = if attachments.is_empty() {
                        "[]".to_string()
                    } else {
                        serde_json::to_string(&attachments).unwrap_or_else(|_| "[]".to_string())
                    };
                    let has_attachments = !attachments_json.trim().is_empty() && attachments_json.trim() != "[]";

                    let labels_all = detail_json.get("labelIds").and_then(Value::as_array).map(|a| {
                        a.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>()
                    }).unwrap_or_default();
                    let labels_str = labels_all.join(",");
                    let is_read = !labels_str.split(',').any(|l| l == "UNREAD");
                    let mailbox = mailbox_from_labels_array(&labels_all);

                    let conn = state.conn.lock().await;
                    conn.execute(
                        "INSERT INTO emails (id, account_id, draft_id, thread_id, subject, sender, to_recipients, cc_recipients, bcc_recipients,
                                             snippet, body_html, attachments_json, has_attachments, date, is_read, mailbox, labels, internal_ts,
                                             list_unsubscribe)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
                         ON CONFLICT(id, account_id) DO UPDATE SET
                            draft_id = excluded.draft_id,
                            thread_id = excluded.thread_id,
                            subject = excluded.subject,
                            sender = excluded.sender,
                            to_recipients = excluded.to_recipients,
                            cc_recipients = excluded.cc_recipients,
                            bcc_recipients = excluded.bcc_recipients,
                            snippet = excluded.snippet,
                            body_html = excluded.body_html,
                            attachments_json = excluded.attachments_json,
                            has_attachments = excluded.has_attachments,
                            date = excluded.date,
                            is_read = excluded.is_read,
                            mailbox = excluded.mailbox,
                            labels = excluded.labels,
                            internal_ts = excluded.internal_ts,
                            list_unsubscribe = excluded.list_unsubscribe",
                        rusqlite::params![
                            composite_id, account_id, resolved_draft_id, thread_id,
                            subject, sender, to_recipients, cc_recipients, bcc_recipients,
                            snippet, body_html, attachments_json, has_attachments as i32,
                            date, is_read as i32, mailbox, labels_str, internal_ts,
                            list_unsubscribe
                        ],
                    ).map_err(|e| e.to_string())?;

                    Ok::<(), String>(())
                }
            }
        }
    });

    let results: Vec<Result<(), String>> = stream.buffer_unordered(GMAIL_FETCH_CONCURRENCY).collect().await;
    for res in results {
        if let Err(e) = res {
            log::error!("Gmail fetch error account={}: {}", account_id, e);
            if e.contains("429") {
                return Err(e);
            }
        }
    }
    let _ = full_len;
    {
        let conn = state.conn.lock().await;
        let _ = crate::smart_inbox::assign_unassigned(&conn, account_id);
    }
    Ok(())
}
