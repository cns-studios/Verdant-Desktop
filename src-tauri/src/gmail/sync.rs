use serde_json::Value;

use super::history::history_sync_mailbox;
use super::{fetch_and_store_messages, gmail_api_get, mailbox_label, parse_message_refs, rate_limit_message, GmailMessageDraft};
use crate::db::gmail_message_cached;
use crate::state::{ensure_token_for, rate_limited_remain, DbState};

pub(crate) const MAX_LIST_PAGES: u32 = 50;

pub(crate) async fn sync_drafts_mailbox(state: &DbState, account_id: i64) -> Result<(), String> {
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

pub(crate) async fn baseline_sync_mailbox(
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

pub async fn sync_gmail_single_mailbox(state: &DbState, account_id: i64, mailbox: &str) -> Result<(), String> {
    if let Some(remain) = rate_limited_remain(state, account_id) {
        return Err(rate_limit_message(remain));
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
