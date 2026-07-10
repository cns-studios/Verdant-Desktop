use crate::crypto::decrypt_password;
use crate::db::{Account, Email};
use mailparse::{parse_mail, MailHeaderMap};
use native_tls::TlsConnector;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub struct ImapCredentials {
    pub imap_host: String,
    pub imap_port: u16,
    pub username: String,
    pub password: String,
}

impl ImapCredentials {
    pub fn from_account(account: &Account) -> Result<Self, String> {
        let imap_host = account.imap_host.clone()
            .ok_or_else(|| "Missing IMAP host".to_string())?;
        let imap_port = account.imap_port
            .ok_or_else(|| "Missing IMAP port".to_string())? as u16;
        let username = account.username.clone()
            .ok_or_else(|| "Missing IMAP username".to_string())?;
        let encrypted_password = account.encrypted_password.clone()
            .ok_or_else(|| "Missing encrypted password".to_string())?;
        let password = decrypt_password(&encrypted_password)?;
        Ok(ImapCredentials { imap_host, imap_port, username, password })
    }
}

type TlsSession = imap::Session<native_tls::TlsStream<std::net::TcpStream>>;

const CONNECT_TIMEOUT_SECS: u64 = 15;

pub fn connect(creds: &ImapCredentials) -> Result<TlsSession, String> {
    connect_with_timeout(creds, CONNECT_TIMEOUT_SECS)
}

pub fn connect_with_timeout(creds: &ImapCredentials, timeout_secs: u64) -> Result<TlsSession, String> {
    let tls = TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS build error: {}", e))?;

    let addr = format!("{}:{}", creds.imap_host, creds.imap_port)
        .to_socket_addrs()
        .map_err(|e| format!("DNS resolution error: {}", e))?
        .next()
        .ok_or_else(|| "DNS resolution returned no addresses".to_string())?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(timeout_secs))
        .map_err(|e| format!("IMAP connect error: {}", e))?;

    let _ = tcp.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(timeout_secs)));

    let tls_stream = tls.connect(&creds.imap_host, tcp)
        .map_err(|e| format!("IMAP TLS error: {}", e))?;

    let client = imap::Client::new(tls_stream);
    let session = client
        .login(&creds.username, &creds.password)
        .map_err(|(e, _)| format!("IMAP login error: {}", e))?;

    Ok(session)
}

pub fn retry_connect(creds: &ImapCredentials) -> Result<TlsSession, String> {
    let mut last_err = String::new();
    for attempt in 0..3 {
        match connect_with_timeout(creds, CONNECT_TIMEOUT_SECS) {
            Ok(session) => return Ok(session),
            Err(e) => {
                last_err = e;
                if attempt < 2 {
                    std::thread::sleep(Duration::from_millis(500 * (attempt as u64 + 1)));
                }
            }
        }
    }
    Err(format!("IMAP connect failed after 3 retries: {}", last_err))
}

fn decode_imap_utf7(input: &str) -> String {
    input
        .replace("&APw-", "ü")
        .replace("&APY-", "ö")
        .replace("&AOQ-", "ä")
        .replace("&AOU-", "Ö")
        .replace("&AMD-", "Ä")
        .replace("&AUQ-", "Ü")
        .replace("&AQ8-", "ß")
}

pub fn imap_folder_for_mailbox(mailbox: &str, folders: &[String]) -> Option<String> {
    let target = mailbox.to_uppercase();

    for folder in folders {
        let decoded = decode_imap_utf7(folder);
        if decoded.to_uppercase() == target {
            return Some(folder.clone());
        }
    }

    let candidates: &[&str] = match target.as_str() {
        "SENT" => &[
            "SENT", "SENT ITEMS", "SENT MESSAGES",
            "GESENDET", "GESENDETE ELEMENTE",
            "[GMAIL]/SENT MAIL", "INBOX.SENT",
        ],
        "DRAFT" => &[
            "DRAFTS", "DRAFT", "ENTW\u{00DC}RFE",
            "[GMAIL]/DRAFTS", "INBOX.DRAFTS",
        ],
        "ARCHIVE" => &[
            "ARCHIVE", "ALL MAIL", "ARCHIV",
            "[GMAIL]/ALL MAIL", "INBOX.ARCHIVE",
        ],
        "TRASH" => &[
            "TRASH", "DELETED", "DELETED MESSAGES", "DELETED ITEMS",
            "PAPIERKORB", "[GMAIL]/TRASH", "INBOX.TRASH",
        ],
        _ => return None,
    };

    for candidate in candidates {
        for folder in folders {
            let decoded = decode_imap_utf7(folder);
            if decoded.to_uppercase() == *candidate {
                return Some(folder.clone());
            }
        }
    }

    for folder in folders {
        let decoded = decode_imap_utf7(folder);
        if decoded.to_uppercase().contains(&target) {
            return Some(folder.clone());
        }
    }

    None
}

fn parse_body(parsed: &mailparse::ParsedMail) -> String {
    let embedded_images = collect_embedded_images(parsed);
    let html = extract_html(parsed);
    replace_cid_with_data_uris(&html, &embedded_images)
}

fn extract_html(parsed: &mailparse::ParsedMail) -> String {
    if parsed.subparts.is_empty() {
        let ct = parsed.ctype.mimetype.to_lowercase();
        if ct == "text/html" {
            return parsed.get_body().unwrap_or_default();
        }
        if ct == "text/plain" {
            return format!("<pre>{}</pre>", html_escape(&parsed.get_body().unwrap_or_default()));
        }
        return String::new();
    }
    let mut html_result = None;
    let mut plain_result = None;
    for part in &parsed.subparts {
        let ct = part.ctype.mimetype.to_lowercase();
        if ct == "text/html" && html_result.is_none() {
            html_result = part.get_body().ok();
        } else if ct == "text/plain" && plain_result.is_none() {
            if let Ok(body) = part.get_body() {
                plain_result = Some(format!("<pre>{}</pre>", html_escape(&body)));
            }
        } else if ct.starts_with("multipart/") {
            let nested = extract_html(part);
            if !nested.is_empty() && html_result.is_none() {
                html_result = Some(nested);
            }
        }
    }
    html_result.or(plain_result).unwrap_or_default()
}

fn collect_embedded_images(parsed: &mailparse::ParsedMail) -> std::collections::HashMap<String, String> {
    let mut images = std::collections::HashMap::new();
    collect_images_recursive(parsed, &mut images);
    images
}

fn collect_images_recursive(parsed: &mailparse::ParsedMail, images: &mut std::collections::HashMap<String, String>) {
    for part in &parsed.subparts {
        let ct = part.ctype.mimetype.to_lowercase();
        
        if let Some(content_id) = part.headers.get_first_value("Content-ID") {
            let cid = content_id.trim().trim_matches('<').trim_matches('>').to_string();
            
            if ct.starts_with("image/") {
                if let Ok(body) = part.get_body_raw() {
                    use base64::Engine as _;
                    let base64_data = base64::engine::general_purpose::STANDARD.encode(&body);
                    images.insert(cid, format!("data:{};base64,{}", ct, base64_data));
                }
            }
        }
        
        if !part.subparts.is_empty() {
            collect_images_recursive(part, images);
        }
    }
}

fn replace_cid_with_data_uris(html: &str, images: &std::collections::HashMap<String, String>) -> String {
    let mut result = html.to_string();
    
    for (cid, data_uri) in images.iter() {
        let cid_ref = format!("cid:{}", cid);
        result = result.replace(&cid_ref, data_uri);
    }
    
    result
}

fn html_escape(input: &str) -> String {
    input.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn collect_imap_attachments(parsed: &mailparse::ParsedMail, uid: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let mut idx = 0;
    collect_imap_attachments_recursive(parsed, uid, &mut out, &mut idx);
    out
}

fn collect_imap_attachments_recursive(
    part: &mailparse::ParsedMail,
    uid: &str,
    out: &mut Vec<serde_json::Value>,
    idx: &mut usize,
) {
    for sub in &part.subparts {
        let ct = sub.ctype.mimetype.to_lowercase();
        let disp = sub.get_content_disposition();
        let filename = disp.params.get("filename")
            .or_else(|| sub.ctype.params.get("name"))
            .cloned().unwrap_or_default();

        let is_attachment = !filename.is_empty() &&
            (ct != "text/plain" && ct != "text/html"
             || disp.disposition == mailparse::DispositionType::Attachment);

        if is_attachment {
            let size = sub.get_body_raw().map(|b| b.len()).unwrap_or(0);
            out.push(serde_json::json!({
                "filename": filename,
                "mime_type": ct,
                "attachment_id": format!("imap-{}-{}", uid, idx),
                "size": size,
            }));
            *idx += 1;
        }

        if !sub.subparts.is_empty() {
            collect_imap_attachments_recursive(sub, uid, out, idx);
        }
    }
}

fn rfc2822_to_epoch(date_str: &str) -> i64 {
    use chrono::DateTime;
    if let Ok(dt) = DateTime::parse_from_rfc2822(date_str) {
        return dt.timestamp();
    }
    let patterns = [
        "%d %b %Y %H:%M:%S %z",
        "%a, %d %b %Y %H:%M:%S %z",
        "%d %b %Y %H:%M:%S %Z",
        "%a, %d %b %Y %H:%M:%S %Z",
    ];
    let clean = date_str.trim();
    for pattern in &patterns {
        if let Ok(dt) = DateTime::parse_from_str(clean, pattern) {
            return dt.timestamp();
        }
    }
    if let Ok(ts) = mailparse::dateparse(date_str) {
        return ts;
    }
    0
}

pub fn sync_imap_mailbox(
    account: &Account,
    mailbox_label: &str,
    max_messages: u32,
) -> Result<Vec<Email>, String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = retry_connect(&creds)?;

    let folders: Vec<String> = session
        .list(None, Some("*"))
        .map_err(|e| format!("IMAP LIST error: {}", e))?
        .iter().map(|n| n.name().to_string()).collect();

    let folder = match imap_folder_for_mailbox(mailbox_label, &folders) {
        Some(f) => f,
        None => { let _ = session.logout(); return Ok(vec![]); }
    };

    let mailbox_info = session.select(&folder)
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let total = mailbox_info.exists as u32;
    if total == 0 { let _ = session.logout(); return Ok(vec![]); }

    let start = if total > max_messages { total - max_messages + 1 } else { 1 };
    let messages = session
        .fetch(&format!("{}:{}", start, total), "(BODY.PEEK[] FLAGS UID)")
        .map_err(|e| format!("IMAP FETCH error: {}", e))?;

    let emails = parse_imap_messages(&messages, account, mailbox_label);
    let _ = session.logout();
    Ok(emails)
}

fn strip_noise(input: &str) -> String {
    input.chars().filter(|c| !matches!(*c,
        '\u{00AD}' | '\u{034F}' | '\u{061C}' | '\u{180E}'
        | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2069}' | '\u{FEFF}'
    )).collect()
}

pub struct SyncResult {
    pub emails: Vec<Email>,
    pub highest_uid: u32,
    pub uidvalidity: u32,
}

fn parse_imap_messages(
    messages: &imap::types::ZeroCopy<Vec<imap::types::Fetch>>,
    account: &Account,
    mailbox_label: &str,
) -> Vec<Email> {
    let mut emails = Vec::new();
    let mut seen_uids = std::collections::HashSet::new();
    for msg in messages.iter() {
        let uid = msg.uid.map(|u| u.to_string())
            .unwrap_or_else(|| msg.message.to_string());
        let body_bytes = msg.body().unwrap_or(b"");
        let min_size = if mailbox_label == "INBOX" { 500 } else { 50 };
        if body_bytes.len() < min_size { continue; }
        if !seen_uids.insert(uid.clone()) { continue; }

        let parsed = match parse_mail(body_bytes) { Ok(p) => p, Err(_) => continue };
        let headers = parsed.get_headers();
        let subject = headers.get_first_value("Subject").unwrap_or_else(|| "(No Subject)".to_string());
        let sender = headers.get_first_value("From").unwrap_or_else(|| "Unknown Sender".to_string());
        let to_recipients = headers.get_first_value("To").unwrap_or_default();
        let cc_recipients = headers.get_first_value("Cc").unwrap_or_default();
        let date = headers.get_first_value("Date").unwrap_or_default();
        let list_unsubscribe = headers.get_first_value("List-Unsubscribe").unwrap_or_default();
        let message_id = headers.get_first_value("Message-ID")
            .unwrap_or_else(|| format!("imap-{}-{}-{}", account.id, mailbox_label, uid));
        let thread_id = headers.get_first_value("In-Reply-To")
            .unwrap_or_else(|| message_id.clone());

        let is_read = msg.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen));
        let body_html = parse_body(&parsed);
        let snippet: String = parsed.get_body().unwrap_or_default()
            .chars().take(180).collect::<String>().replace('\n', " ");
        let attachments = collect_imap_attachments(&parsed, &uid);
        let has_attachments = !attachments.is_empty();
        let attachments_json = serde_json::to_string(&attachments).unwrap_or_else(|_| "[]".to_string());
        let internal_ts = rfc2822_to_epoch(&date);
        let id = format!("{}:{}", account.id, message_id.trim_matches(|c: char| c == '<' || c == '>'));

        emails.push(Email {
            id,
            account_id: account.id,
            draft_id: None,
            thread_id: thread_id.trim_matches(|c: char| c == '<' || c == '>').to_string(),
            subject: strip_noise(&subject),
            sender: strip_noise(&sender),
            to_recipients: strip_noise(&to_recipients),
            cc_recipients: strip_noise(&cc_recipients),
            snippet: strip_noise(&snippet),
            body_html: if body_html.is_empty() { format!("<pre>{}</pre>", html_escape(&snippet)) } else { body_html },
            attachments_json,
            has_attachments,
            date,
            is_read,
            starred: false,
            mailbox: mailbox_label.to_string(),
            labels: mailbox_label.to_string(),
            internal_ts,
            notified: false,
            list_unsubscribe,
            unsubscribed: false,
        });
    }
    emails.sort_by(|a, b| b.internal_ts.cmp(&a.internal_ts));
    emails
}

pub fn sync_imap_mailbox_incremental(
    account: &Account,
    mailbox_label: &str,
    stored_uidvalidity: Option<u32>,
    stored_highest_uid: Option<u32>,
) -> Result<SyncResult, String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = retry_connect(&creds)?;

    let folders: Vec<String> = session
        .list(None, Some("*"))
        .map_err(|e| format!("IMAP LIST error: {}", e))?
        .iter().map(|n| n.name().to_string()).collect();

    let folder = match imap_folder_for_mailbox(mailbox_label, &folders) {
        Some(f) => f,
        None => { let _ = session.logout(); return Ok(SyncResult { emails: vec![], highest_uid: 0, uidvalidity: 0 }); }
    };

    let mailbox_info = session.select(&folder)
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let uidvalidity = mailbox_info.uid_validity.unwrap_or(0);
    let total = mailbox_info.exists as u32;

    if total == 0 {
        let _ = session.logout();
        return Ok(SyncResult { emails: vec![], highest_uid: 0, uidvalidity });
    }

    let uidnext = mailbox_info.uid_next.unwrap_or(total + 1);

    let (messages, new_highest_uid) = if let (Some(s_validity), Some(s_uid)) = (stored_uidvalidity, stored_highest_uid) {
        if uidvalidity == s_validity && uidnext > s_uid + 1 {
            let fetch_str = format!("{}:*", s_uid + 1);
            let fetched = session.uid_fetch(&fetch_str, "(BODY.PEEK[] FLAGS UID)")
                .map_err(|e| format!("IMAP UID FETCH error: {}", e))?;
            let max_uid = fetched.iter().filter_map(|m| m.uid).max().unwrap_or(s_uid);
            (fetched, max_uid)
        } else {
            let start = if total > 50 { total - 50 + 1 } else { 1 };
            let fetched = session.fetch(&format!("{}:*", start), "(BODY.PEEK[] FLAGS UID)")
                .map_err(|e| format!("IMAP FETCH error: {}", e))?;
            let max_uid = fetched.iter().filter_map(|m| m.uid).max().unwrap_or(uidnext.saturating_sub(1));
            (fetched, max_uid)
        }
    } else {
        let start = if total > 50 { total - 50 + 1 } else { 1 };
        let fetched = session.fetch(&format!("{}:*", start), "(BODY.PEEK[] FLAGS UID)")
            .map_err(|e| format!("IMAP FETCH error: {}", e))?;
        let max_uid = fetched.iter().filter_map(|m| m.uid).max().unwrap_or(uidnext.saturating_sub(1));
        (fetched, max_uid)
    };

    let mut emails = parse_imap_messages(&messages, account, mailbox_label);
    let _ = session.logout();
    Ok(SyncResult { emails, highest_uid: new_highest_uid.max(stored_highest_uid.unwrap_or(0)), uidvalidity })
}

pub fn test_imap_connection(
    imap_host: &str,
    imap_port: u16,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let tls = TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS error: {}", e))?;

    let client = imap::connect((imap_host, imap_port), imap_host, &tls)
        .map_err(|e| format!("Connection failed: {}", e))?;

    let mut session = client
        .login(username, password)
        .map_err(|(e, _)| format!("Login failed: {}", e))?;

    let _ = session.logout();
    Ok(username.to_string())
}

pub fn append_to_sent(
    account: &crate::db::Account,
    to: &str,
    cc: &str,
    subject: &str,
    body_plain: &str,
    body_html: Option<&str>,
) -> Result<(), String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = connect(&creds)?;

    let folders: Vec<String> = session
        .list(None, Some("*"))
        .map_err(|e| format!("IMAP LIST error: {}", e))?
        .iter().map(|n| n.name().to_string()).collect();

    let sent_folder = imap_folder_for_mailbox("SENT", &folders)
        .ok_or_else(|| "Could not find Sent folder".to_string())?;

    
    let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S +0000").to_string();
    let body = if let Some(html) = body_html {
        format!(
            "To: {}\r\nCc: {}\r\nSubject: {}\r\nDate: {}\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=\"verdant-alt\"\r\n\r\n--verdant-alt\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{}\r\n--verdant-alt\r\nContent-Type: text/html; charset=UTF-8\r\n\r\n{}\r\n--verdant-alt--\r\n",
            to, cc, subject, date, body_plain, html
        )
    } else {
        format!(
            "To: {}\r\nCc: {}\r\nSubject: {}\r\nDate: {}\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{}\r\n",
            to, cc, subject, date, body_plain
        )
    };

    let flags = imap::types::Flag::Seen;
    session
        .append_with_flags(&sent_folder, body.as_bytes(), &[flags])
        .map_err(|e| format!("IMAP APPEND error: {}", e))?;

    let _ = session.logout();
    Ok(())
}

pub fn imap_search_emails(
    account: &Account,
    query: &str,
    max_results: u32,
) -> Result<Vec<Email>, String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = connect(&creds)?;

    session.select("INBOX")
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let q = query.trim().replace('"', "");
    let search_criteria = format!(
        "OR OR SUBJECT \"{}\" FROM \"{}\" BODY \"{}\"",
        q, q, q
    );

    let uids = session.search(&search_criteria)
        .map_err(|e| format!("IMAP SEARCH error: {}", e))?;

    if uids.is_empty() {
        let _ = session.logout();
        return Ok(vec![]);
    }

    let mut uid_list: Vec<u32> = uids.into_iter().collect();
    uid_list.sort_unstable_by(|a, b| b.cmp(a));
    uid_list.truncate(max_results as usize);

    let uid_set = uid_list.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
    let messages = session.fetch(&uid_set, "(BODY.PEEK[] FLAGS UID)")
        .map_err(|e| format!("IMAP FETCH error: {}", e))?;

    let mut emails = Vec::new();
    for msg in messages.iter() {
        let uid = msg.uid.map(|u| u.to_string())
            .unwrap_or_else(|| msg.message.to_string());

        let body_bytes = msg.body().unwrap_or(b"");
        if body_bytes.len() < 50 { continue; }

        let parsed = match parse_mail(body_bytes) { Ok(p) => p, Err(_) => continue };
        let headers = parsed.get_headers();
        let subject = headers.get_first_value("Subject").unwrap_or_else(|| "(No Subject)".to_string());
        let sender = headers.get_first_value("From").unwrap_or_else(|| "Unknown Sender".to_string());
        let to_recipients = headers.get_first_value("To").unwrap_or_default();
        let cc_recipients = headers.get_first_value("Cc").unwrap_or_default();
        let date = headers.get_first_value("Date").unwrap_or_default();
        let list_unsubscribe = headers.get_first_value("List-Unsubscribe").unwrap_or_default();
        let message_id = headers.get_first_value("Message-ID")
            .unwrap_or_else(|| format!("imap-{}-search-{}", account.id, uid));
        let thread_id = headers.get_first_value("In-Reply-To")
            .unwrap_or_else(|| message_id.clone());

        let is_read = msg.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen));
        let body_html = parse_body(&parsed);
        let snippet: String = parsed.get_body().unwrap_or_default()
            .chars().take(180).collect::<String>().replace('\n', " ");
        let attachments = collect_imap_attachments(&parsed, &uid);
        let has_attachments = !attachments.is_empty();
        let attachments_json = serde_json::to_string(&attachments).unwrap_or_else(|_| "[]".to_string());
        let internal_ts = rfc2822_to_epoch(&date);
        let id = format!("{}:{}", account.id, message_id.trim_matches(|c: char| c == '<' || c == '>'));

        emails.push(Email {
            id,
            account_id: account.id,
            draft_id: None,
            thread_id: thread_id.trim_matches(|c: char| c == '<' || c == '>').to_string(),
            subject: strip_noise(&subject),
            sender: strip_noise(&sender),
            to_recipients: strip_noise(&to_recipients),
            cc_recipients: strip_noise(&cc_recipients),
            snippet: strip_noise(&snippet),
            body_html: if body_html.is_empty() { format!("<pre>{}</pre>", html_escape(&snippet)) } else { body_html },
            attachments_json,
            has_attachments,
            date,
            is_read,
            starred: false,
            mailbox: "INBOX".to_string(),
            labels: "INBOX".to_string(),
            internal_ts,
            notified: false,
            list_unsubscribe,
            unsubscribed: false,
        });
    }

    let _ = session.logout();
    emails.sort_by(|a, b| b.internal_ts.cmp(&a.internal_ts));
    Ok(emails)
}

pub fn imap_set_flag(
    account: &Account,
    message_id_header: &str,
    flag: &str,
    add: bool,
    mailbox: &str,
) -> Result<(), String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = connect(&creds)?;

    let folders: Vec<String> = session.list(None, Some("*"))
        .map_err(|e| format!("IMAP LIST error: {}", e))?
        .iter().map(|n| n.name().to_string()).collect();

    let folder = imap_folder_for_mailbox(mailbox, &folders)
        .unwrap_or_else(|| "INBOX".to_string());
    session.select(&folder)
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let search_result = session.search(format!("HEADER Message-ID \"{}\"", message_id_header))
        .unwrap_or_default();

    if !search_result.is_empty() {
        let seq = search_result.iter().next().unwrap();
        let seq_set = seq.to_string();
        if add {
            session.store(&seq_set, format!("+FLAGS ({})", flag))
                .map_err(|e| format!("IMAP STORE error: {}", e))?;
            if flag == "\\Deleted" {
                let _ = session.expunge();
            }
        } else {
            session.store(&seq_set, format!("-FLAGS ({})", flag))
                .map_err(|e| format!("IMAP STORE error: {}", e))?;
        }
    }

    let _ = session.logout();
    Ok(())
}

pub fn imap_move_to_folder(
    account: &Account,
    message_id_header: &str,
    source_mailbox: &str,
    target_mailbox: &str,
) -> Result<(), String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = connect(&creds)?;

    let folders: Vec<String> = session.list(None, Some("*"))
        .map_err(|e| format!("IMAP LIST error: {}", e))?
        .iter().map(|n| n.name().to_string()).collect();

    let src_folder = imap_folder_for_mailbox(source_mailbox, &folders)
        .unwrap_or_else(|| "INBOX".to_string());
    let dst_folder = imap_folder_for_mailbox(target_mailbox, &folders)
        .ok_or_else(|| format!("Could not find {} folder", target_mailbox))?;

    session.select(&src_folder)
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let search_result = session.search(format!("HEADER Message-ID \"{}\"", message_id_header))
        .unwrap_or_default();

    if !search_result.is_empty() {
        let seq = search_result.iter().next().unwrap();
        let seq_set = seq.to_string();
        session.copy(&seq_set, &dst_folder)
            .map_err(|e| format!("IMAP COPY error: {}", e))?;
        session.store(&seq_set, "+FLAGS (\\Deleted)")
            .map_err(|e| format!("IMAP STORE error: {}", e))?;
        session.expunge()
            .map_err(|e| format!("IMAP EXPUNGE error: {}", e))?;
    }

    let _ = session.logout();
    Ok(())
}

pub struct FetchedAttachment {
    pub filename: String,
    pub mime_type: String,
    pub data_base64: String,
}

pub fn fetch_attachment(
    account: &Account,
    attachment_id: &str,
) -> Result<FetchedAttachment, String> {
    let parts: Vec<&str> = attachment_id.split('-').collect();
    if parts.len() < 3 || parts[0] != "imap" {
        return Err(format!("Invalid IMAP attachment ID: {}", attachment_id));
    }
    let uid = parts[1];
    let part_idx = parts[2].parse::<usize>()
        .map_err(|e| format!("Invalid part index in attachment ID: {}", e))?;

    let creds = ImapCredentials::from_account(account)?;
    let mut session = connect(&creds)?;

    let folders: Vec<String> = session
        .list(None, Some("*"))
        .map_err(|e| format!("IMAP LIST error: {}", e))?
        .iter().map(|n| n.name().to_string()).collect();

    let target_mailboxes = ["INBOX", "SENT", "DRAFTS", "ARCHIVE", "TRASH"];
    let mut found_body = None;

    for mb in target_mailboxes {
        if let Some(folder) = imap_folder_for_mailbox(mb, &folders) {
            if session.select(&folder).is_ok() {
                let fetch_res = session.uid_fetch(uid, "(BODY.PEEK[])");
                if let Ok(messages) = fetch_res {
                    if let Some(msg) = messages.iter().next() {
                        found_body = Some(msg.body().unwrap_or_default().to_vec());
                        break;
                    }
                }
            }
        }
    }

    let body_bytes = found_body.ok_or_else(|| {
        format!("Could not find message with UID {} in any common mailbox", uid)
    })?;

    let parsed = parse_mail(&body_bytes)
        .map_err(|e| format!("Failed to parse mail: {}", e))?;

    let mut current_idx = 0;

    fn find_part<'a>(
        part: &'a mailparse::ParsedMail<'a>,
        target: usize,
        current: &mut usize,
    ) -> Option<&'a mailparse::ParsedMail<'a>> {
        for sub in &part.subparts {
            let ct = sub.ctype.mimetype.to_lowercase();
            let disp = sub.get_content_disposition();
            let filename = disp.params.get("filename")
                .or_else(|| sub.ctype.params.get("name"));

            let is_attachment = filename.is_some() &&
                (ct != "text/plain" && ct != "text/html"
                 || disp.disposition == mailparse::DispositionType::Attachment);

            if is_attachment {
                if *current == target {
                    return Some(sub);
                }
                *current += 1;
            } else if !sub.subparts.is_empty() {
                if let Some(found) = find_part(sub, target, current) {
                    return Some(found);
                }
            }
        }
        None
    }

    let attachment = find_part(&parsed, part_idx, &mut current_idx)
        .ok_or_else(|| format!("Could not find attachment part {} in message {}", part_idx, uid))?;

    let filename = attachment.get_content_disposition().params.get("filename")
        .or_else(|| attachment.ctype.params.get("name"))
        .cloned().unwrap_or_else(|| "attachment".to_string());

    let mime_type = attachment.ctype.mimetype.clone();
    let data = attachment.get_body_raw()
        .map_err(|e| format!("Failed to get attachment body: {}", e))?;

    use base64::Engine as _;
    let data_base64 = base64::engine::general_purpose::STANDARD.encode(data);

    let _ = session.logout();

    Ok(FetchedAttachment { filename, mime_type, data_base64 })
}

pub fn sync_imap_mailbox_page(
    account: &Account,
    mailbox_label: &str,
    offset: u32,
    count: u32,
) -> Result<Vec<Email>, String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = retry_connect(&creds)?;

    let folders: Vec<String> = session
        .list(None, Some("*"))
        .map_err(|e| format!("IMAP LIST error: {}", e))?
        .iter().map(|n| n.name().to_string()).collect();

    let folder = match imap_folder_for_mailbox(mailbox_label, &folders) {
        Some(f) => f,
        None => { let _ = session.logout(); return Ok(vec![]); }
    };

    let mailbox_info = session.select(&folder)
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let total = mailbox_info.exists as u32;
    if total == 0 || offset >= total {
        let _ = session.logout();
        return Ok(vec![]);
    }

    let end = if total > offset { total - offset } else { 0 };
    if end == 0 { let _ = session.logout(); return Ok(vec![]); }
    let start = if end > count { end - count + 1 } else { 1 };

    let messages = session
        .fetch(&format!("{}:{}", start, end), "(BODY.PEEK[] FLAGS UID)")
        .map_err(|e| format!("IMAP FETCH error: {}", e))?;

    let emails = parse_imap_messages(&messages, account, mailbox_label);
    let _ = session.logout();
    Ok(emails)
}

pub fn fetch_list_unsubscribe_header(account: &Account, uid_str: &str) -> Result<String, String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = connect(&creds)?;

    let uid: u32 = uid_str.parse().map_err(|_| format!("Invalid UID: {}", uid_str))?;

    let messages = session.fetch(
        format!("{}:{}", uid, uid).as_str(),
        "(BODY.PEEK[HEADER.FIELDS (List-Unsubscribe)])",
    ).map_err(|e| format!("IMAP fetch error: {}", e))?;

    for msg in messages.iter() {
        if let Some(body) = msg.body() {
            let parsed = mailparse::parse_mail(body).map_err(|e| format!("Parse error: {}", e))?;
            if let Some(val) = parsed.get_headers().get_first_value("List-Unsubscribe") {
                let _ = session.logout();
                return Ok(val);
            }
        }
    }

    let _ = session.logout();
    Err("No List-Unsubscribe header found".to_string())
}
