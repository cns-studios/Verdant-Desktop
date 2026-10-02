use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::sync::baseline_sync_mailbox;
use super::{fetch_and_store_messages, gmail_api_get, GmailMessageDraft, LabelDelta};
use crate::db::gmail_message_cached;
use crate::state::{ensure_token_for, DbState};

pub(crate) const MAX_HISTORY_PAGES: u32 = 50;

pub(crate) fn labels_of(msg: &Value) -> Vec<String> {
    msg.get("labelIds")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

pub(crate) fn message_id_of(msg: &Value) -> Option<String> {
    msg.get("id").and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn history_message_id(msg: &Value) -> Option<String> {
    message_id_of(msg).or_else(|| msg.get("message").and_then(message_id_of))
}

pub(crate) fn history_message_labels(msg: &Value) -> Vec<String> {
    let source = msg.get("message").unwrap_or(msg);
    labels_of(source)
}

pub(crate) async fn history_sync_mailbox(state: &DbState, account_id: i64, mailbox: &str) -> Result<(), String> {
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
                            if let Some(mid) = history_message_id(m) {
                                added_ids.insert(mid.clone());
                                let delta = label_changes.entry(mid).or_default();
                                for l in history_message_labels(m) {
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
