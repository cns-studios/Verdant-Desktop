use std::sync::Arc;
use serde_json::{json, Value};
use tauri::State;
use futures::StreamExt;

use crate::db::{clear_account_emails, get_account_by_id, Account, Email};
use crate::gmail::{
    collect_attachments, extract_body, header_value, mailbox_from_labels, mailbox_label,
    strip_confusable_chars, AttachmentMeta,
};
use crate::state::{ensure_token, ensure_token_for, DbState, get_active_id};

#[derive(serde::Serialize)]
pub struct MailboxCounts {
    pub inbox_total: i64,
    pub inbox_unread: i64,
    pub starred_total: i64,
    pub sent_total: i64,
    pub drafts_total: i64,
    pub archive_total: i64,
    pub trash_total: i64,
}



use std::collections::{HashMap, HashSet};
use std::time::Duration;
use crate::db::gmail_message_cached;
use crate::state::{clear_rate_limited, mark_rate_limited, rate_limited_remain};

const GMAIL_FETCH_CONCURRENCY: usize = 6;
const MAX_LIST_PAGES: u32 = 50;
const MAX_HISTORY_PAGES: u32 = 50;

fn rate_limit_message(account_id: i64, remain: Duration) -> String {
    format!(
        "Gmail rate limit reached (429). Backing off for {}s — new mail will appear automatically.",
        remain.as_secs()
    )
}

async fn gmail_api_get(
    state: &DbState,
    account_id: i64,
    client: &reqwest::Client,
    token: &str,
    url: String,
) -> Result<Value, String> {
    let res = client.get(&url).bearer_auth(token).send().await.map_err(|e| e.to_string())?;

    if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = res
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60)
            .min(600);
        let backoff = Duration::from_secs(retry_after);
        mark_rate_limited(state, account_id, backoff);
        return Err(rate_limit_message(account_id, backoff));
    }

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        let truncated = if body.len() > 400 { body[..400].to_string() } else { body };
        return Err(format!("Gmail API {} failed: {}", status.as_u16(), truncated));
    }

    clear_rate_limited(state, account_id);
    res.json::<Value>().await.map_err(|e| e.to_string())
}

struct GmailMessageDraft {
    message_id: String,
    draft_id: Option<String>,
}

fn parse_message_refs(json: &Value, mailbox: &str) -> Vec<GmailMessageDraft> {
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
struct LabelDelta {
    added: Vec<String>,
    removed: Vec<String>,
}

fn labels_of(msg: &Value) -> Vec<String> {
    msg.get("labelIds")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn message_id_of(msg: &Value) -> Option<String> {
    msg.get("id").and_then(Value::as_str).map(str::to_string)
}

async fn fetch_and_store_messages(
    state: &DbState,
    account_id: i64,
    client: &reqwest::Client,
    token: &str,
    full_refs: Vec<GmailMessageDraft>,
    meta_changes: Vec<(String, LabelDelta)>,
) -> Result<(), String> {
    let full_len = full_refs.len();
    let stream = futures::stream::iter(full_refs.into_iter().map(|m| {
        FetchJob::Full(m)
    }))
    .chain(futures::stream::iter(meta_changes.into_iter().map(|(id, delta)| {
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
                        "INSERT INTO emails (id, account_id, draft_id, thread_id, subject, sender, to_recipients, cc_recipients,
                                             snippet, body_html, attachments_json, has_attachments, date, is_read, mailbox, labels, internal_ts,
                                             list_unsubscribe)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
                         ON CONFLICT(id, account_id) DO UPDATE SET
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
                            is_read = excluded.is_read,
                            mailbox = excluded.mailbox,
                            labels = excluded.labels,
                            internal_ts = excluded.internal_ts,
                            list_unsubscribe = excluded.list_unsubscribe",
                        rusqlite::params![
                            composite_id, account_id, resolved_draft_id, thread_id,
                            subject, sender, to_recipients, cc_recipients,
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
    Ok(())
}

enum FetchJob {
    Full(GmailMessageDraft),
    Label { message_id: String, delta: LabelDelta },
}

fn mailbox_from_labels_array(labels: &[String]) -> String {
    mailbox_from_labels(&labels.join(","))
}

async fn sync_drafts_mailbox(state: &DbState, account_id: i64) -> Result<(), String> {
    let client = reqwest::Client::new();
    let token = ensure_token_for(state, account_id).await?.access_token;

    let json = gmail_api_get(
        state,
        account_id,
        &client,
        &token,
        "https://gmail.googleapis.com/gmail/v1/users/me/drafts?maxResults=100".to_string(),
    ).await?;

    let draft_items = json.get("drafts").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut refs: Vec<GmailMessageDraft> = Vec::new();
    for d in &draft_items {
        let did = d.get("id").and_then(Value::as_str);
        let mid = d.get("message").and_then(|m| m.get("id")).and_then(Value::as_str);
        if let (Some(did), Some(mid)) = (did, mid) {
            let mid = mid.to_string();
            let cached = {
                let conn = state.conn.lock().await;
                gmail_message_cached(&conn, account_id, &mid).unwrap_or(false)
            };
            if !cached {
                refs.push(GmailMessageDraft { message_id: mid, draft_id: Some(did.to_string()) });
            }
        }
    }

    fetch_and_store_messages(state, account_id, &client, &token, refs, Vec::new()).await
}

async fn baseline_sync_mailbox(
    state: &DbState,
    account_id: i64,
    mailbox: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let token = ensure_token_for(state, account_id).await?.access_token;
    let label = mailbox_label(mailbox).ok_or_else(|| "Unknown mailbox".to_string())?;

    let mut all_ids: Vec<GmailMessageDraft> = Vec::new();
    let mut page_token: Option<String> = None;
    let mut pages: u32 = 0;

    loop {
        pages += 1;
        if pages > MAX_LIST_PAGES {
            break;
        }

        let mut list_url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages?labelIds={}&maxResults=100",
            label
        );
        if let Some(ref pt) = page_token {
            list_url.push_str("&pageToken=");
            list_url.push_str(pt);
        }

        let json = gmail_api_get(state, account_id, &client, &token, list_url).await?;
        for m in parse_message_refs(&json, mailbox) {
            all_ids.push(m);
        }

        if let Some(hid) = json.get("historyId").and_then(Value::as_str) {
            let conn = state.conn.lock().await;
            let _ = crate::db::set_gmail_history_id(&conn, account_id, hid);
        }

        page_token = json.get("nextPageToken").and_then(Value::as_str).map(str::to_string);
        if page_token.is_none() {
            break;
        }
    }

    fetch_and_store_messages(state, account_id, &client, &token, all_ids, Vec::new()).await
}

async fn history_sync_mailbox(state: &DbState, account_id: i64, mailbox: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let token = ensure_token_for(state, account_id).await?.access_token;

    let start_history_id = {
        let conn = state.conn.lock().await;
        crate::db::get_gmail_history_id(&conn, account_id).map_err(|e| e.to_string())?
    };

    let Some(start) = start_history_id else {
        return baseline_sync_mailbox(state, account_id, mailbox).await;
    };

    let mut page_token: Option<String> = None;
    let mut new_history_id: Option<String> = None;
    let mut full_fetch: Vec<GmailMessageDraft> = Vec::new();
    let mut label_changes: HashMap<String, LabelDelta> = HashMap::new();
    let mut added_ids: HashSet<String> = HashSet::new();

    for _page in 0..MAX_HISTORY_PAGES {
        let mut url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/history?startHistoryId={}&maxResults=500",
            start
        );
        if let Some(ref pt) = page_token {
            url.push_str("&pageToken=");
            url.push_str(pt);
        }

        let json = gmail_api_get(state, account_id, &client, &token, url).await?;

        if let Some(hid) = json.get("historyId").and_then(Value::as_str) {
            new_history_id = Some(hid.to_string());
        }

        if let Some(records) = json.get("history").and_then(Value::as_array) {
            for record in records {
                for field in ["messagesAdded", "messages"] {
                    if let Some(items) = record.get(field).and_then(Value::as_array) {
                        for m in items {
                            if let Some(mid) = message_id_of(m) {
                                added_ids.insert(mid.clone());
                                let delta = label_changes.entry(mid).or_default();
                                for l in labels_of(m) {
                                    if !delta.added.contains(&l) {
                                        delta.added.push(l);
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(items) = record.get("labelsAdded").and_then(Value::as_array) {
                    for la in items {
                        if let Some(mid) = la.get("message").and_then(message_id_of) {
                            let delta = label_changes.entry(mid).or_default();
                            for l in labels_of(la) {
                                if !delta.added.contains(&l) {
                                    delta.added.push(l);
                                }
                            }
                        }
                    }
                }
                if let Some(items) = record.get("labelsRemoved").and_then(Value::as_array) {
                    for lr in items {
                        if let Some(mid) = lr.get("message").and_then(message_id_of) {
                            let delta = label_changes.entry(mid).or_default();
                            for l in labels_of(lr) {
                                if !delta.removed.contains(&l) {
                                    delta.removed.push(l);
                                }
                            }
                        }
                    }
                }
                if let Some(items) = record.get("messagesDeleted").and_then(Value::as_array) {
                    for m in items {
                        if let Some(mid) = message_id_of(m) {
                            let composite = format!("{}:{}", account_id, mid);
                            let conn = state.conn.lock().await;
                            let _ = conn.execute("DELETE FROM emails WHERE id = ?1 AND account_id = ?2", rusqlite::params![composite, account_id]);
                        }
                    }
                }
            }
        }

        page_token = json.get("nextPageToken").and_then(Value::as_str).map(str::to_string);
        if page_token.is_none() {
            break;
        }
    }

    if let Some(hid) = new_history_id {
        let conn = state.conn.lock().await;
        crate::db::set_gmail_history_id(&conn, account_id, &hid).map_err(|e| e.to_string())?;
    }

    let mut meta_changes: Vec<(String, LabelDelta)> = Vec::new();
    for (mid, delta) in label_changes {
        if added_ids.contains(&mid) {
            full_fetch.push(GmailMessageDraft { message_id: mid, draft_id: None });
        } else {
            let cached = {
                let conn = state.conn.lock().await;
                gmail_message_cached(&conn, account_id, &mid).unwrap_or(false)
            };
            if cached {
                meta_changes.push((mid, delta));
            } else {
                full_fetch.push(GmailMessageDraft { message_id: mid, draft_id: None });
            }
        }
    }

    fetch_and_store_messages(state, account_id, &client, &token, full_fetch, meta_changes).await
}

pub async fn sync_gmail_single_mailbox(state: &DbState, account_id: i64, mailbox: &str) -> Result<(), String> {
    if let Some(remain) = rate_limited_remain(state, account_id) {
        return Err(rate_limit_message(account_id, remain));
    }

    if mailbox == "DRAFT" {
        return sync_drafts_mailbox(state, account_id).await;
    }

    match history_sync_mailbox(state, account_id, mailbox).await {
        Ok(()) => Ok(()),
        Err(e) => {
            if e.to_lowercase().contains("history") || e.to_lowercase().contains("440") {
                log::error!("Gmail history unavailable, resetting to baseline: {}", e);
                let conn = state.conn.lock().await;
                let _ = conn.execute("DELETE FROM gmail_sync_state WHERE account_id = ?1", rusqlite::params![account_id]);
                return baseline_sync_mailbox(state, account_id, mailbox).await;
            }
            Err(e)
        }
    }
}



pub async fn sync_mailbox_page_internal_for(
    state: &DbState,
    account_id: i64,
    mailbox: &str,
    page_token: Option<String>,
) -> Result<Option<String>, String> {
    let Some(label) = mailbox_label(mailbox) else {
        return Ok(None);
    };

    let client = reqwest::Client::new();
    let token = ensure_token_for(state, account_id).await?.access_token;

    let mut list_url = if mailbox == "DRAFT" {
        "https://gmail.googleapis.com/gmail/v1/users/me/drafts?maxResults=50".to_string()
    } else {
        format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages?labelIds={}&maxResults=50",
            label
        )
    };
    if let Some(pt) = page_token {
        if !pt.trim().is_empty() {
            list_url.push_str("&pageToken=");
            list_url.push_str(pt.trim());
        }
    }

    let json = gmail_api_get(state, account_id, &client, &token, list_url).await?;
    let next_page_token = json.get("nextPageToken").and_then(Value::as_str).map(str::to_string);

    let message_refs: Vec<GmailMessageDraft> = parse_message_refs(&json, mailbox);

    fetch_and_store_messages(state, account_id, &client, &token, message_refs, Vec::new()).await?;

    Ok(next_page_token)
}

pub async fn sync_imap_mailbox_internal_for(state: &DbState, account: &Account, mailbox: &str) -> Result<(), String> {
    let account_id = account.id;
    let acc = account.clone();
    let mb = mailbox.to_string();
    let mb_for_fallback = mb.clone();

    let stored_state = {
        let conn = state.conn.lock().await;
        crate::db::get_mailbox_sync_state(&conn, account_id, &mb)
            .ok().flatten()
    };
    let (stored_uidvalidity, stored_highest_uid) = stored_state
        .map(|s| (Some(s.uidvalidity), Some(s.highest_uid)))
        .unwrap_or((None, None));

    let result = tokio::task::spawn_blocking(move || {
        crate::imap_sync::sync_imap_mailbox_incremental(&acc, &mb, stored_uidvalidity, stored_highest_uid)
    }).await.map_err(|e| format!("IMAP task error: {}", e))?;

    match result {
        Ok(sync_result) => {
            crate::background_sync::upsert_emails(state, account_id, sync_result.emails, &mb_for_fallback).await;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let sync_state = crate::db::MailboxSyncState {
                account_id,
                mailbox_name: mb_for_fallback.clone(),
                highest_uid: sync_result.highest_uid,
                uidvalidity: sync_result.uidvalidity,
                last_synced_at: now,
            };
            let conn = state.conn.lock().await;
            let _ = crate::db::set_mailbox_sync_state(&conn, &sync_state);
            Ok(())
        }
        Err(e) => {
            log::error!("IMAP sync error account={} mailbox={}: {}", account_id, mailbox, e);
            let acc2 = account.clone();
            let mb2 = mailbox.to_string();
            let mb2_fb = mb2.clone();
            let fallback = tokio::task::spawn_blocking(move || {
                crate::imap_sync::sync_imap_mailbox(&acc2, &mb2, 50)
            }).await.map_err(|e| format!("IMAP fallback task error: {}", e))?;
            if let Ok(emails) = fallback {
                crate::background_sync::upsert_emails(state, account_id, emails, &mb2_fb).await;
            }
            Ok(())
        }
    }
}

pub async fn sync_mailbox_internal_for(state: &DbState, account_id: i64, mailbox: &str) -> Result<(), String> {
    let account = {
        let conn = state.conn.lock().await;
        get_account_by_id(&conn, account_id)
            .ok()
            .flatten()
    };
    if let Some(ref acc) = account {
        if acc.provider == "imap" {
            return sync_imap_mailbox_internal_for(state, acc, mailbox).await;
        }
    }

    sync_gmail_single_mailbox(state, account_id, mailbox).await
}



#[tauri::command]
pub async fn sync_emails(state: State<'_, Arc<DbState>>) -> Result<(), String> {
    let id = get_active_id(&state).await;
    sync_mailbox_internal_for(&state, id, "INBOX").await
}

#[tauri::command]
pub async fn sync_mailbox(state: State<'_, Arc<DbState>>, mailbox: String) -> Result<(), String> {
    let id = get_active_id(&state).await;
    sync_mailbox_internal_for(&state, id, mailbox.as_str()).await
}

#[tauri::command]
pub async fn sync_mailbox_page(
    state: State<'_, Arc<DbState>>,
    mailbox: String,
    page_token: Option<String>,
) -> Result<Option<String>, String> {
    let id = get_active_id(&state).await;
    sync_mailbox_page_internal_for(&state, id, mailbox.as_str(), page_token).await
}

fn map_email_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Email> {
    Ok(Email {
        id: row.get(0)?,
        account_id: row.get(1)?,
        draft_id: row.get(2)?,
        thread_id: row.get(3)?,
        subject: row.get(4)?,
        sender: row.get(5)?,
        to_recipients: row.get(6)?,
        cc_recipients: row.get(7)?,
        snippet: row.get(8)?,
        body_html: row.get(9)?,
        attachments_json: row.get(10)?,
        has_attachments: row.get::<_, i32>(11)? != 0,
        date: row.get(12)?,
        is_read: row.get::<_, i32>(13)? != 0,
        starred: row.get::<_, i32>(14)? != 0,
        mailbox: row.get(15)?,
        labels: row.get(16)?,
        internal_ts: row.get(17)?,
        notified: row.get::<_, i32>(18)? != 0,
        list_unsubscribe: row.get(19)?,
        unsubscribed: row.get::<_, i32>(20)? != 0,
    })
}

#[tauri::command]
pub async fn get_emails(
    state: State<'_, Arc<DbState>>,
    mailbox: Option<String>,
) -> Result<Vec<Email>, String> {
    let account_id = get_active_id(&state).await;
    let box_name = mailbox.unwrap_or_else(|| "INBOX".to_string());
    let conn = state.conn.lock().await;

    let emails = if box_name == "STARRED" {
        let mut stmt = conn.prepare(
            "SELECT id,account_id,draft_id,thread_id,subject,sender,to_recipients,cc_recipients,
                    snippet,body_html,attachments_json,has_attachments,date,is_read,starred,mailbox,labels,internal_ts,notified,
                    list_unsubscribe,unsubscribed
     FROM emails WHERE starred=1 AND account_id=?1 ORDER BY internal_ts DESC, rowid DESC LIMIT 500"
).map_err(|e| e.to_string())?;
        let x = stmt.query_map([account_id], map_email_row).map_err(|e| e.to_string())?
            .filter_map(Result::ok).collect(); x
    } else {
        let mut stmt = conn.prepare(
            "SELECT id,account_id,draft_id,thread_id,subject,sender,to_recipients,cc_recipients,
                    snippet,body_html,attachments_json,has_attachments,date,is_read,starred,mailbox,labels,internal_ts,notified,
                    list_unsubscribe,unsubscribed
     FROM emails WHERE mailbox=?1 AND account_id=?2 ORDER BY internal_ts DESC, rowid DESC LIMIT 500"
).map_err(|e| e.to_string())?;
let x = stmt.query_map(rusqlite::params![box_name, account_id], map_email_row)
            .map_err(|e| e.to_string())?.filter_map(Result::ok).collect(); x
    };

    Ok(emails)
}

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
                crate::imap_sync::imap_search_emails(&acc_clone, &q, 100)
            }).await;
            return match result {
                Ok(Ok(emails)) => Ok(emails),
                Ok(Err(_e)) => {
                    let conn = state.conn.lock().await;
                    let pattern = format!("%{}%", query);
                    let mut stmt = conn.prepare(
                        "SELECT id,account_id,draft_id,thread_id,subject,sender,to_recipients,cc_recipients,
                                snippet,body_html,attachments_json,has_attachments,date,is_read,starred,mailbox,labels,internal_ts,notified,
                                list_unsubscribe,unsubscribed
                         FROM emails WHERE account_id=?1 AND (subject LIKE ?2 OR sender LIKE ?2 OR snippet LIKE ?2)
                         ORDER BY internal_ts DESC LIMIT 100"
                    ).map_err(|e| e.to_string())?;
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
            subject, sender, to_recipients, cc_recipients, snippet, body_html, attachments_json,
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

#[tauri::command]
pub async fn set_email_read_status(
    state: State<'_, Arc<DbState>>,
    email_id: String,
    is_read: bool,
) -> Result<(), String> {
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
                let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify", gmail_id);
                let body = if is_read {
                    json!({"removeLabelIds": ["UNREAD"]})
                } else {
                    json!({"addLabelIds": ["UNREAD"]})
                };
                let _ = client.post(url).bearer_auth(&token_info.access_token).json(&body).send().await;
            }
        } else if acc.provider == "imap" {
            let msg_id = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
            let mailbox = {
                let conn = state.conn.lock().await;
                conn.query_row("SELECT mailbox FROM emails WHERE id=?1 AND account_id=?2",
                    rusqlite::params![email_id, account_id], |r| r.get::<_, String>(0)).unwrap_or_else(|_| "INBOX".to_string())
            };
            let acc_clone = acc.clone();
            let _ = tokio::task::spawn_blocking(move || {
                crate::imap_sync::imap_set_flag(&acc_clone, &msg_id, "\\Seen", is_read, &mailbox)
            }).await;
        }
    }

    let conn = state.conn.lock().await;
    conn.execute(
        "UPDATE emails SET is_read=?1 WHERE id=?2 AND account_id=?3",
        rusqlite::params![is_read as i32, email_id, account_id],
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
            let _ = tokio::task::spawn_blocking(move || {
                crate::imap_sync::imap_set_flag(&acc_clone, &msg_id, "\\Flagged", will_be_starred, &mb)
            }).await;
        }
    }

    let conn = state.conn.lock().await;
    conn.execute(
        "UPDATE emails SET starred=CASE WHEN starred=1 THEN 0 ELSE 1 END WHERE id=?1 AND account_id=?2",
        rusqlite::params![email_id, account_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

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
                let _ = tokio::task::spawn_blocking(move || {
                    crate::imap_sync::imap_move_to_folder(&acc_clone, &msg_id, &mailbox, "ARCHIVE")
                }).await;
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
            let _ = tokio::task::spawn_blocking(move || {
                crate::imap_sync::imap_move_to_folder(&acc_clone, &msg_id, &mailbox, "TRASH")
            }).await;
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
pub async fn get_mailbox_counts(state: State<'_, Arc<DbState>>) -> Result<MailboxCounts, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;

    let count = |sql: &str| -> i64 {
        conn.query_row(sql, rusqlite::params![account_id], |r| r.get(0)).unwrap_or(0)
    };

    Ok(MailboxCounts {
        inbox_total: count("SELECT COUNT(*) FROM emails WHERE mailbox='INBOX' AND account_id=?1"),
        inbox_unread: count("SELECT COUNT(*) FROM emails WHERE mailbox='INBOX' AND is_read=0 AND account_id=?1"),
        starred_total: count("SELECT COUNT(*) FROM emails WHERE starred=1 AND account_id=?1"),
        sent_total: count("SELECT COUNT(*) FROM emails WHERE mailbox='SENT' AND account_id=?1"),
        drafts_total: count("SELECT COUNT(*) FROM emails WHERE mailbox='DRAFT' AND account_id=?1"),
        archive_total: count("SELECT COUNT(*) FROM emails WHERE mailbox='ARCHIVE' AND account_id=?1"),
        trash_total: count("SELECT COUNT(*) FROM emails WHERE mailbox='TRASH' AND account_id=?1"),
    })
}

#[tauri::command]
pub async fn clear_local_data(state: State<'_, Arc<DbState>>) -> Result<(), String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    clear_account_emails(&conn, account_id).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct ThreadSummary {
    pub thread_id: String,
    pub subject: String,
    pub participants: String,
    pub snippet: String,
    pub latest_ts: i64,
    pub latest_date: String,
    pub message_count: i64,
    pub unread_count: i64,
    pub is_read: bool,
    pub starred: bool,
    pub has_attachments: bool,
    pub labels: String,
}

#[tauri::command]
pub async fn get_inbox_threads(state: State<'_, Arc<DbState>>) -> Result<Vec<ThreadSummary>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;

    let sql = "
        SELECT
            e.thread_id,
            latest.subject,
            latest.snippet,
            latest.date,
            latest.labels,
            COUNT(e.id) AS message_count,
            SUM(CASE WHEN e.is_read=0 THEN 1 ELSE 0 END) AS unread_count,
            MAX(e.internal_ts) AS latest_ts,
            MAX(e.starred) AS any_starred,
            MAX(e.has_attachments) AS any_attachments,
            GROUP_CONCAT(DISTINCT e.sender) AS all_senders
        FROM emails e
        INNER JOIN emails latest ON latest.id = (
            SELECT id FROM emails
            WHERE thread_id = e.thread_id AND mailbox = 'INBOX' AND account_id = ?1
            ORDER BY internal_ts DESC LIMIT 1
        )
        WHERE e.mailbox='INBOX' AND e.account_id=?1
        GROUP BY e.thread_id
        ORDER BY latest_ts DESC
        LIMIT 500
    ";

    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let threads = stmt.query_map(rusqlite::params![account_id], |row| {
        let unread_count: i64 = row.get(6)?;
        Ok(ThreadSummary {
            thread_id: row.get(0)?,
            subject: row.get(1)?,
            snippet: row.get(2)?,
            latest_date: row.get(3)?,
            labels: row.get(4)?,
            message_count: row.get(5)?,
            unread_count,
            latest_ts: row.get(7)?,
            starred: row.get::<_, i64>(8)? != 0,
            has_attachments: row.get::<_, i64>(9)? != 0,
            is_read: unread_count == 0,
            participants: row.get(10).unwrap_or_default(),
        })
    }).map_err(|e| e.to_string())?
    .filter_map(Result::ok).collect();

    Ok(threads)
}

#[tauri::command]
pub async fn get_thread_messages(
    state: State<'_, Arc<DbState>>,
    thread_id: String,
) -> Result<Vec<Email>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;

    let mut stmt = conn.prepare(
        "SELECT id,account_id,draft_id,thread_id,subject,sender,to_recipients,cc_recipients,
                snippet,body_html,attachments_json,has_attachments,date,is_read,starred,mailbox,labels,internal_ts,notified,
                list_unsubscribe,unsubscribed
         FROM emails WHERE thread_id=?1 AND account_id=?2 ORDER BY internal_ts ASC, rowid ASC"
    ).map_err(|e| e.to_string())?;

    let emails = stmt.query_map(rusqlite::params![thread_id, account_id], map_email_row)
        .map_err(|e| e.to_string())?.filter_map(Result::ok).collect();
    Ok(emails)
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
                crate::imap_sync::imap_set_flag(&acc_clone, &msg_id, "\\Deleted", true, "TRASH")
            }).await
            .map_err(|e| format!("IMAP task error: {}", e))?
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
                crate::imap_sync::imap_move_to_folder(&acc_clone, &msg_id, "TRASH", "INBOX")
            }).await
            .map_err(|e| format!("IMAP task error: {}", e))?
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
pub async fn sync_imap_mailbox_page(
    state: State<'_, Arc<DbState>>,
    mailbox: String,
    offset: u32,
) -> Result<bool, String> {
    let account_id = get_active_id(&state).await;

    let account = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id)
            .ok().flatten()
            .ok_or_else(|| "Account not found".to_string())?
    };

    if account.provider != "imap" {
        return Err("Not an IMAP account".to_string());
    }

    let acc_clone = account.clone();
    let mb = mailbox.clone();
    let result = tokio::task::spawn_blocking(move || {
        crate::imap_sync::sync_imap_mailbox_page(&acc_clone, &mb, offset, 50)
    }).await;

    match result {
        Ok(Ok(emails)) => {
            let has_more = !emails.is_empty();
            let conn = state.conn.lock().await;
            for email in emails {
                if let Err(e) = conn.execute(
                    "INSERT INTO emails (id, account_id, draft_id, thread_id, subject, sender, to_recipients, cc_recipients,
                                         snippet, body_html, attachments_json, has_attachments, date, is_read, starred,
                                         mailbox, labels, internal_ts, list_unsubscribe)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
                    ON CONFLICT(id, account_id) DO UPDATE SET
                        snippet = excluded.snippet,
                        body_html = excluded.body_html,
                        is_read = excluded.is_read,
                        mailbox = excluded.mailbox,
                        labels = excluded.labels,
                        internal_ts = excluded.internal_ts,
                        list_unsubscribe = excluded.list_unsubscribe",
                    rusqlite::params![
                        email.id, email.account_id, email.draft_id, email.thread_id,
                        email.subject, email.sender, email.to_recipients, email.cc_recipients,
                        email.snippet, email.body_html, email.attachments_json,
                        email.has_attachments as i32, email.date, email.is_read as i32,
                        email.starred as i32, email.mailbox, email.labels, email.internal_ts,
                        email.list_unsubscribe
                    ],
                ) {
                    log::error!("sync_imap_mailbox_page upsert failed: {}", e);
                }
            }
            Ok(has_more)
        }
        Ok(Err(e)) => Err(e),
        Err(e) => Err(format!("Task error: {}", e)),
    }
}
