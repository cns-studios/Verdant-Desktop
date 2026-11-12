use std::sync::Arc;
use tauri::State;
use serde_json::Value;
use mailparse::{MailHeaderMap, ParsedMail};
use crate::state::{DbState, get_active_id, ensure_token_for};

#[tauri::command]
pub async fn unsubscribe_from_list(
    state: State<'_, Arc<DbState>>,
    email_id: String,
) -> Result<(), String> {
    let account_id = get_active_id(&state).await;
    unsubscribe_email(&state, account_id, &email_id).await
}

pub(crate) async fn unsubscribe_email(
    state: &DbState,
    account_id: i64,
    email_id: &str,
) -> Result<(), String> {
    let email_id = email_id.to_string();

    let ((list_unsubscribe, provider), raw_email_id) = {
        let conn = state.conn.lock().await;
        let list = conn.query_row(
            "SELECT e.list_unsubscribe, a.provider FROM emails e
             JOIN accounts a ON a.id = e.account_id
             WHERE e.id=?1 AND e.account_id=?2",
            rusqlite::params![email_id, account_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        ).map_err(|e| {
            let msg = format!("Email not found ({}): {}", email_id, e);
            log::error!("{}", msg);
            msg
        })?;
        let gid = email_id.splitn(2, ':').nth(1).unwrap_or(&email_id).to_string();
        (list, gid)
    };

    if !list_unsubscribe.is_empty() {
        if let Some(url) = parse_unsubscribe_url(&list_unsubscribe) {
            send_unsubscribe_request(&url).await?;
            mark_unsubscribed(state, &email_id, account_id).await;
            return Ok(());
        }
    }

    log::info!("Fetching live email data for {}", email_id);
    let account = {
        let conn = state.conn.lock().await;
        crate::db::get_account_by_id(&conn, account_id).ok().flatten()
            .ok_or_else(|| "Account not found".to_string())?
    };

    let raw_bytes: Vec<u8> = if provider == "gmail" {
        let token = ensure_token_for(state, account_id).await.map_err(|e| {
            log::error!("Failed to get Gmail token: {}", e);
            e.to_string()
        })?;

        let client = reqwest::Client::new();
        let detail = client
            .get(format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=raw", raw_email_id))
            .bearer_auth(&token.access_token)
            .send().await.map_err(|e| {
                let msg = format!("Gmail API fetch failed for {}: {}", email_id, e);
                log::error!("{}", msg);
                msg
            })?;

        if !detail.status().is_success() {
            let status = detail.status();
            let body = detail.text().await.unwrap_or_default();
            let msg = format!("Gmail API returned {} for {}: {}", status, email_id, body);
            log::error!("{}", msg);
            return Err(msg);
        }

        let detail_json: serde_json::Value = detail.json().await.map_err(|e| {
            let msg = format!("Failed to parse Gmail response: {}", e);
            log::error!("{}", msg);
            msg
        })?;

        let raw_b64 = detail_json.get("raw").and_then(Value::as_str)
            .ok_or_else(|| {
                let msg = format!("No 'raw' field in Gmail response for {}", email_id);
                log::error!("{}", msg);
                msg
            })?;

        use base64::engine::general_purpose::{URL_SAFE_NO_PAD, URL_SAFE, STANDARD};
        use base64::Engine as _;
        let data = raw_b64.as_bytes();
        URL_SAFE_NO_PAD.decode(data)
            .or_else(|_| URL_SAFE.decode(data))
            .or_else(|_| STANDARD.decode(data))
            .map_err(|e| {
                let msg = format!("Failed to decode raw message for {}: {}", email_id, e);
                log::error!("{}", msg);
                msg
            })?
    } else if provider == "imap" {
        let acc = account.clone();
        let message_id = raw_email_id.clone();
        let mailbox = {
            let conn = state.conn.lock().await;
            conn.query_row(
                "SELECT mailbox FROM emails WHERE id=?1 AND account_id=?2",
                rusqlite::params![email_id, account_id],
                |r| r.get::<_, String>(0),
            ).unwrap_or_else(|_| "INBOX".to_string())
        };
        tokio::task::spawn_blocking(move || {
            crate::imap_client::fetch_raw_message(&acc, &message_id, &mailbox)
        }).await.map_err(|e| format!("Task error: {}", e))?
        .map_err(|e| {
            log::error!("{}", e);
            e
        })?
    } else {
        let msg = format!("Unknown provider: {}", provider);
        log::error!("{}", msg);
        return Err(msg);
    };

    let parsed = mailparse::parse_mail(&raw_bytes).map_err(|e| {
        let msg = format!("Failed to parse MIME for {}: {}", email_id, e);
        log::error!("{}", msg);
        msg
    })?;

    let (url, persist_value) = {
        let lu = parsed.get_headers().get_first_value("List-Unsubscribe").unwrap_or_default();
        if !lu.is_empty() {
            if let Some(url) = parse_unsubscribe_url(&lu) {
                (url, lu)
            } else {
                find_body_url(&parsed, &email_id)?
            }
        } else {
            find_body_url(&parsed, &email_id)?
        }
    };

    send_unsubscribe_request(&url).await?;

    persist_header(state, persist_value, &email_id, account_id).await;
    mark_unsubscribed(state, &email_id, account_id).await;
    Ok(())
}

fn find_body_url(parsed: &ParsedMail, email_id: &str) -> Result<(String, String), String> {
    let html_body = extract_html_body(parsed);
    if html_body.is_empty() {
        log::error!("No HTML body extracted; MIME type: {}", parsed.ctype.mimetype);
        for (i, part) in parsed.subparts.iter().enumerate() {
            log::error!("  subpart[{}]: {}", i, part.ctype.mimetype);
        }
        let msg = format!("No HTML body found in email {}", email_id);
        log::error!("{}", msg);
        return Err(msg);
    }

    log::info!("HTML body extracted (first 300 chars): {}", &html_body[..html_body.len().min(300)]);

    let body_url = find_unsubscribe_url_in_body(&html_body).ok_or_else(|| {
        log::error!("Body preview (first 2000): {}", &html_body[..html_body.len().min(2000)]);
        let msg = format!("No unsubscribe link found in email body for {}", email_id);
        log::error!("{}", msg);
        msg
    })?;

    Ok((body_url.clone(), body_url))
}

async fn persist_header(state: &DbState, val: String, email_id: &str, account_id: i64) {
    let conn = state.conn.lock().await;
    let _ = conn.execute(
        "UPDATE emails SET list_unsubscribe=?1 WHERE id=?2 AND account_id=?3",
        rusqlite::params![val, email_id, account_id],
    );
}

fn short(body: &str) -> String {
    body.chars().take(200).collect()
}

/// RFC 8058 one-click: a POST with `List-Unsubscribe=One-Click` actually
/// unsubscribes, whereas a GET on the same URL usually just returns a landing
/// page with 200 and changes nothing. We don't store `List-Unsubscribe-Post`,
/// so try the POST first and fall back to GET (for plain link-style senders)
/// when the POST is rejected.
async fn send_unsubscribe_request(url: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Verdant-Desktop/2.3.1")
        .build()
        .map_err(|e| {
            let msg = format!("Failed to build HTTP client: {}", e);
            log::error!("{}", msg);
            msg
        })?;

    log::info!("Sending one-click unsubscribe POST to: {}", url);
    let post_status = match client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body("List-Unsubscribe=One-Click")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => return Ok(()),
        Ok(resp) => resp.status().to_string(),
        Err(e) => format!("error: {}", e),
    };
    log::warn!("Unsubscribe POST gave {} for {}; retrying as GET", post_status, url);

    let resp = client.get(url).send().await.map_err(|e| {
        let msg = format!("Unsubscribe request failed for {} (POST gave {}): {}", url, post_status, e);
        log::error!("{}", msg);
        msg
    })?;
    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let msg = format!(
        "Unsubscribe rejected (POST {}, GET {}) for {}: {}",
        post_status, status, url, short(&body)
    );
    log::error!("{}", msg);
    Err(msg)
}

async fn mark_unsubscribed(state: &DbState, email_id: &str, account_id: i64) {
    let conn = state.conn.lock().await;
    let _ = conn.execute(
        "UPDATE emails SET unsubscribed=1 WHERE id=?1 AND account_id=?2",
        rusqlite::params![email_id, account_id],
    );
    if let Err(e) = crate::sync::block_sender_of(&conn, account_id, email_id) {
        log::error!("Recording unsubscribed sender failed for {}: {}", email_id, e);
    }
    log::info!("Successfully unsubscribed email {}", email_id);
}

fn parse_unsubscribe_url(header: &str) -> Option<String> {
    for part in header.split(',') {
        let part = part.trim();
        if let Some(url) = part.strip_prefix('<') {
            if let Some(url) = url.strip_suffix('>') {
                let url = url.trim();
                if url.starts_with("http://") || url.starts_with("https://") {
                    return Some(url.to_string());
                }
            }
        }
    }
    None
}

fn extract_html_body(parsed: &ParsedMail) -> String {
    let ct = parsed.ctype.mimetype.to_lowercase();
    if parsed.subparts.is_empty() {
        if ct == "text/html" {
            return parsed.get_body().unwrap_or_default();
        }
        return String::new();
    }
    let mut html_result = None;
    for part in &parsed.subparts {
        let pct = part.ctype.mimetype.to_lowercase();
        if pct == "text/html" && html_result.is_none() {
            html_result = part.get_body().ok();
        } else if pct.starts_with("multipart/") {
            let nested = extract_html_body(part);
            if !nested.is_empty() && html_result.is_none() {
                html_result = Some(nested);
            }
        }
    }
    html_result.unwrap_or_default()
}

fn find_unsubscribe_url_in_body(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let mut candidates: Vec<(i32, String)> = Vec::new();

    let mut pos = 0;
    while let Some(start) = lower[pos..].find("<a ") {
        let abs_start = pos + start;
        let remaining = &lower[abs_start..];
        let end = remaining.find("</a>")
            .map(|e| abs_start + e + 4)
            .unwrap_or(abs_start + remaining.find('>').map(|e| e + 1).unwrap_or(0));

        if end <= abs_start {
            pos = abs_start + 3;
            continue;
        }

        let snippet_lower = &lower[abs_start..end];
        let snippet = &html[abs_start..end];

        let href = snippet_lower.split("href=\"").nth(1)
            .and_then(|s| s.split('"').next())
            .filter(|u| u.starts_with("http://") || u.starts_with("https://"));

        if let Some(_) = href {
            let score = if snippet_lower.contains("unsubscribe") { 3 }
                else if snippet_lower.contains("opt-out") || snippet_lower.contains("optout") { 2 }
                else if snippet_lower.contains("email") || snippet_lower.contains("mailing") { 1 }
                else { 0 };

            if score >= 2 {
                let actual_url = {
                    let idx = snippet_lower.find("href=\"").map(|i| i + 6)?;
                    let remaining_href = &snippet[idx..];
                    let url_end = remaining_href.find('"')?;
                    remaining_href[..url_end].to_string()
                };
                candidates.push((score, actual_url));
            }
        }

        pos = if end > abs_start { end } else { abs_start + 3 };
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.into_iter().next().map(|(_, url)| url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Serves `n` requests; POST with one-click body -> `post_status`, GET -> 200.
    fn server(n: usize, post_status: &'static str) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/unsub", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..n {
                let (mut sock, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let len = sock.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..len]).to_string();
                seen.push(req.lines().next().unwrap_or("").to_string());
                let status = if req.starts_with("POST") && req.contains("List-Unsubscribe=One-Click") { post_status } else { "200 OK" };
                let _ = write!(sock, "HTTP/1.1 {}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok", status);
            }
            seen
        });
        (url, handle)
    }

    #[tokio::test]
    async fn posts_one_click_first() {
        let (url, srv) = server(1, "200 OK");
        send_unsubscribe_request(&url).await.unwrap();
        let seen = srv.join().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].starts_with("POST"));
    }

    #[tokio::test]
    async fn falls_back_to_get() {
        let (url, srv) = server(2, "405 Method Not Allowed");
        send_unsubscribe_request(&url).await.unwrap();
        let seen = srv.join().unwrap();
        assert!(seen[0].starts_with("POST"));
        assert!(seen[1].starts_with("GET"));
    }
}
