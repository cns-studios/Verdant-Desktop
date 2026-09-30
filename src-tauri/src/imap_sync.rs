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

const CONNECT_TIMEOUT_SECS: u64 = 8;
/// Read/write timeout once logged in. FETCHing full bodies or a server-side
/// SEARCH on a large mailbox routinely takes longer than the connect timeout,
/// and hitting it mid-response surfaced as random "flaky" sync failures.
const SESSION_IO_TIMEOUT_SECS: u64 = 60;
/// How many of the newest messages per mailbox get their flags and presence
/// re-checked on every sync (cheap: UID + FLAGS + ENVELOPE only).
const RECONCILE_WINDOW: u32 = 200;
/// How many recent messages to download when there is no usable sync state.
const INITIAL_WINDOW: u32 = 50;

pub fn connect_with_timeout(creds: &ImapCredentials, timeout_secs: u64) -> Result<TlsSession, String> {
    let tls = TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS build error: {}", e))?;

    let addrs: Vec<std::net::SocketAddr> = format!("{}:{}", creds.imap_host, creds.imap_port)
        .to_socket_addrs()
        .map_err(|e| format!("DNS resolution error: {}", e))?
        .collect();
    if addrs.is_empty() {
        return Err("DNS resolution returned no addresses".to_string());
    }
    // Try every resolved address: hosts commonly publish an AAAA record that
    // is unreachable on the user's network, and only trying the first one
    // made connecting fail whenever the resolver happened to list IPv6 first.
    let mut last_err = None;
    let tcp = addrs
        .iter()
        .find_map(|addr| match TcpStream::connect_timeout(addr, Duration::from_secs(timeout_secs)) {
            Ok(stream) => Some(stream),
            Err(e) => {
                last_err = Some(e);
                None
            }
        })
        .ok_or_else(|| format!("IMAP connect error: {}", last_err.map(|e| e.to_string()).unwrap_or_default()))?;

    let _ = tcp.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(timeout_secs)));
    // Socket timeouts are per-socket, so a cloned handle lets us relax them
    // after login even though the stream itself moves into the TLS wrapper.
    let socket = tcp.try_clone().ok();

    let tls_stream = tls.connect(&creds.imap_host, tcp)
        .map_err(|e| format!("IMAP TLS error: {}", e))?;

    let client = imap::Client::new(tls_stream);
    let session = client
        .login(&creds.username, &creds.password)
        .map_err(|(e, _)| format!("IMAP login error: {}", e))?;

    if let Some(socket) = socket {
        let _ = socket.set_read_timeout(Some(Duration::from_secs(SESSION_IO_TIMEOUT_SECS)));
        let _ = socket.set_write_timeout(Some(Duration::from_secs(SESSION_IO_TIMEOUT_SECS)));
    }

    Ok(session)
}

pub fn retry_connect(creds: &ImapCredentials) -> Result<TlsSession, String> {
    let mut last_err = String::new();
    for attempt in 0..2 {
        match connect_with_timeout(creds, CONNECT_TIMEOUT_SECS) {
            Ok(session) => return Ok(session),
            // Retrying a rejected password only brings the account closer to
            // the provider's lockout threshold.
            Err(e) if e.starts_with("IMAP login error") => return Err(e),
            Err(e) => {
                last_err = e;
                if attempt < 1 {
                    std::thread::sleep(Duration::from_millis(500 * (attempt as u64 + 1)));
                }
            }
        }
    }
    Err(format!("IMAP connect failed after 2 retries: {}", last_err))
}

/// A mailbox as returned by LIST, with its RFC 6154 special-use role.
pub struct Folder {
    pub name: String,
    special: Option<&'static str>,
    selectable: bool,
}

pub fn list_folders(session: &mut TlsSession) -> Result<Vec<Folder>, String> {
    use imap::types::NameAttribute;
    let names = session
        .list(None, Some("*"))
        .map_err(|e| format!("IMAP LIST error: {}", e))?;
    Ok(names
        .iter()
        .map(|n| {
            let mut special = None;
            let mut selectable = true;
            for attr in n.attributes() {
                match attr {
                    NameAttribute::NoSelect => selectable = false,
                    NameAttribute::Custom(flag) => {
                        special = match flag.to_ascii_lowercase().as_str() {
                            "\\sent" => Some("SENT"),
                            "\\drafts" => Some("DRAFT"),
                            "\\trash" => Some("TRASH"),
                            "\\archive" => Some("ARCHIVE"),
                            "\\all" => Some("ALL"),
                            "\\junk" => Some("JUNK"),
                            "\\noselect" | "\\nonexistent" => {
                                selectable = false;
                                special
                            }
                            _ => special,
                        };
                    }
                    _ => {}
                }
            }
            Folder { name: n.name().to_string(), special, selectable }
        })
        .collect())
}

/// Decodes an RFC 3501 "modified UTF-7" mailbox name (e.g. `Entw&APw-rfe`).
fn decode_imap_utf7(input: &str) -> String {
    use base64::Engine as _;
    let mut out = String::new();
    let mut rest = input;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        let Some(end) = after.find('-') else {
            out.push_str(&rest[pos..]);
            return out;
        };
        let chunk = &after[..end];
        if chunk.is_empty() {
            out.push('&');
        } else {
            let decoded = base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(chunk.replace(',', "/"))
                .ok()
                .filter(|bytes| bytes.len() % 2 == 0);
            match decoded {
                Some(bytes) => {
                    let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                    out.push_str(&String::from_utf16_lossy(&units));
                }
                None => {
                    out.push('&');
                    out.push_str(chunk);
                    out.push('-');
                }
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

pub fn imap_folder_for_mailbox(mailbox: &str, folders: &[Folder]) -> Option<String> {
    let target = mailbox.to_uppercase();
    let selectable = || folders.iter().filter(|f| f.selectable);

    for folder in selectable() {
        if decode_imap_utf7(&folder.name).to_uppercase() == target {
            return Some(folder.name.clone());
        }
    }

    // Servers that advertise special-use flags tell us exactly which folder
    // is which, independent of the UI language the mailbox was created in.
    let roles: &[&str] = match target.as_str() {
        "SENT" => &["SENT"],
        "DRAFT" => &["DRAFT"],
        "TRASH" => &["TRASH"],
        "ARCHIVE" => &["ARCHIVE", "ALL"],
        _ => &[],
    };
    for role in roles {
        if let Some(folder) = selectable().find(|f| f.special == Some(*role)) {
            return Some(folder.name.clone());
        }
    }

    let candidates: &[&str] = match target.as_str() {
        "SENT" => &[
            "SENT", "SENT ITEMS", "SENT MESSAGES",
            "GESENDET", "GESENDETE ELEMENTE", "GESENDETE OBJEKTE",
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
            "PAPIERKORB", "GEL\u{00D6}SCHT", "GEL\u{00D6}SCHTE ELEMENTE",
            "[GMAIL]/TRASH", "INBOX.TRASH",
        ],
        _ => return None,
    };

    for candidate in candidates {
        for folder in selectable() {
            if decode_imap_utf7(&folder.name).to_uppercase() == *candidate {
                return Some(folder.name.clone());
            }
        }
    }

    for folder in selectable() {
        if decode_imap_utf7(&folder.name).to_uppercase().contains(&target) {
            return Some(folder.name.clone());
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

fn extract_snippet(parsed: &mailparse::ParsedMail) -> String {
    if parsed.subparts.is_empty() {
        let ct = parsed.ctype.mimetype.to_lowercase();
        if ct == "text/plain" || ct == "text/html" {
            return parsed.get_body().unwrap_or_default();
        }
        return String::new();
    }

    let mut html_fallback = String::new();
    for part in &parsed.subparts {
        let snippet = extract_snippet(part);
        if snippet.is_empty() {
            continue;
        }
        if part.ctype.mimetype.eq_ignore_ascii_case("text/plain") {
            return snippet;
        }
        if html_fallback.is_empty() {
            html_fallback = snippet;
        }
    }
    html_fallback
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

fn canonical_thread_id(headers: &mailparse::headers::Headers<'_>, message_id: &str) -> String {
    headers
        .get_first_value("References")
        .and_then(|references| references.split_whitespace().next().map(str::to_string))
        .or_else(|| {
            headers
                .get_first_value("In-Reply-To")
                .and_then(|reply_to| reply_to.split_whitespace().next().map(str::to_string))
        })
        .unwrap_or_else(|| message_id.to_string())
        .trim_matches(|c: char| c == '<' || c == '>')
        .to_string()
}

fn collect_imap_attachments(parsed: &mailparse::ParsedMail, uid: &str, mailbox_label: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let mut idx = 0;
    collect_imap_attachments_recursive(parsed, uid, mailbox_label, &mut out, &mut idx);
    out
}

fn collect_imap_attachments_recursive(
    part: &mailparse::ParsedMail,
    uid: &str,
    mailbox_label: &str,
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
                "attachment_id": format!("imap-{}-{}-{}", uid, idx, mailbox_label),
                "size": size,
            }));
            *idx += 1;
        }

        if !sub.subparts.is_empty() {
            collect_imap_attachments_recursive(sub, uid, mailbox_label, out, idx);
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

/// Downloads the newest `max_messages` of a mailbox in full. Used as the
/// fallback when incremental sync fails; it reconciles nothing.
pub fn sync_imap_mailbox(
    account: &Account,
    mailbox_label: &str,
    max_messages: u32,
) -> Result<Vec<Email>, String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = retry_connect(&creds)?;
    let folders = list_folders(&mut session)?;

    let folder = match imap_folder_for_mailbox(mailbox_label, &folders) {
        Some(f) => f,
        None => { let _ = session.logout(); return Ok(vec![]); }
    };

    let mailbox_info = session.select(&folder)
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let total = mailbox_info.exists;
    if total == 0 { let _ = session.logout(); return Ok(vec![]); }

    let start = if total > max_messages { total - max_messages + 1 } else { 1 };
    let messages = session
        .fetch(&format!("{}:{}", start, total), "(BODY.PEEK[] FLAGS UID)")
        .map_err(|e| format!("IMAP FETCH error: {}", e))?;

    let (emails, _) = parse_imap_messages(&messages, account, mailbox_label);
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

/// Server-side state of one message inside the reconcile window.
pub struct WindowEntry {
    pub uid: u32,
    pub id: String,
    pub is_read: bool,
    pub starred: bool,
}

/// Every message the server holds in a mailbox with a UID >= `min_uid`.
/// Anything stored locally for that mailbox in the same UID range that is
/// not listed here has been deleted or moved away on the server.
pub struct ReconcileWindow {
    pub min_uid: u32,
    pub entries: Vec<WindowEntry>,
}

pub struct SyncResult {
    pub emails: Vec<Email>,
    pub highest_uid: u32,
    pub uidvalidity: u32,
    /// True when previously stored UIDs for this mailbox are meaningless
    /// (first sync or UIDVALIDITY changed) and must be forgotten.
    pub uids_reset: bool,
    pub window: Option<ReconcileWindow>,
}

/// Local id for an IMAP message. Must stay stable across versions: it is the
/// primary key messages are stored under.
fn imap_email_id(account_id: i64, message_id: Option<&str>, mailbox_label: &str, uid: &str) -> String {
    let message_id = message_id
        .map(str::to_string)
        .unwrap_or_else(|| format!("imap-{}-{}-{}", account_id, mailbox_label, uid));
    format!("{}:{}", account_id, message_id.trim_matches(|c: char| c == '<' || c == '>'))
}

fn has_flag(msg: &imap::types::Fetch, wanted: &imap::types::Flag) -> bool {
    msg.flags().iter().any(|f| f == wanted)
}

fn parse_imap_messages(
    messages: &imap::types::ZeroCopy<Vec<imap::types::Fetch>>,
    account: &Account,
    mailbox_label: &str,
) -> (Vec<Email>, Vec<WindowEntry>) {
    let mut emails = Vec::new();
    let mut entries = Vec::new();
    let mut seen_uids = std::collections::HashSet::new();
    for msg in messages.iter() {
        let uid = msg.uid.map(|u| u.to_string())
            .unwrap_or_else(|| msg.message.to_string());
        let body_bytes = msg.body().unwrap_or(b"");
        if body_bytes.is_empty() { continue; }
        if !seen_uids.insert(uid.clone()) { continue; }

        let parsed = match parse_mail(body_bytes) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Failed to parse IMAP message (uid={}): {}", uid, e);
                continue;
            }
        };
        let headers = parsed.get_headers();
        let subject = headers.get_first_value("Subject").unwrap_or_else(|| "(No Subject)".to_string());
        let sender = headers.get_first_value("From").unwrap_or_else(|| "Unknown Sender".to_string());
        let to_recipients = headers.get_first_value("To").unwrap_or_default();
        let cc_recipients = headers.get_first_value("Cc").unwrap_or_default();
        let bcc_recipients = headers.get_first_value("Bcc").unwrap_or_default();
        let date = headers.get_first_value("Date").unwrap_or_default();
        let list_unsubscribe = headers.get_first_value("List-Unsubscribe").unwrap_or_default();
        let header_message_id = headers.get_first_value("Message-ID");
        let id = imap_email_id(account.id, header_message_id.as_deref(), mailbox_label, &uid);
        let message_id = id.splitn(2, ':').nth(1).unwrap_or_default().to_string();
        let thread_id = canonical_thread_id(&headers, &message_id);

        let is_read = has_flag(msg, &imap::types::Flag::Seen);
        let starred = has_flag(msg, &imap::types::Flag::Flagged);
        let body_html = parse_body(&parsed);
        let snippet: String = extract_snippet(&parsed)
            .chars().take(180).collect::<String>().replace('\n', " ");
        let attachments = collect_imap_attachments(&parsed, &uid, mailbox_label);
        let has_attachments = !attachments.is_empty();
        let attachments_json = serde_json::to_string(&attachments).unwrap_or_else(|_| "[]".to_string());
        let internal_ts = rfc2822_to_epoch(&date);

        if let Some(real_uid) = msg.uid {
            entries.push(WindowEntry { uid: real_uid, id: id.clone(), is_read, starred });
        }
        emails.push(Email {
            id,
            account_id: account.id,
            draft_id: None,
            thread_id,
            subject: strip_noise(&subject),
            sender: strip_noise(&sender),
            to_recipients: strip_noise(&to_recipients),
            cc_recipients: strip_noise(&cc_recipients),
            bcc_recipients: strip_noise(&bcc_recipients),
            snippet: strip_noise(&snippet),
            body_html: if body_html.is_empty() { format!("<pre>{}</pre>", html_escape(&snippet)) } else { body_html },
            attachments_json,
            has_attachments,
            date,
            is_read,
            starred,
            mailbox: mailbox_label.to_string(),
            labels: mailbox_label.to_string(),
            internal_ts,
            notified: false,
            list_unsubscribe,
            unsubscribed: false,
        });
    }
    emails.sort_by(|a, b| b.internal_ts.cmp(&a.internal_ts));
    (emails, entries)
}

/// Lightweight pass over the newest `RECONCILE_WINDOW` messages: UID, flags
/// and Message-ID only. This is what lets read/star changes made on another
/// device, and messages deleted or moved elsewhere, show up in Verdant.
fn fetch_reconcile_window(
    session: &mut TlsSession,
    account: &Account,
    mailbox_label: &str,
    total: u32,
) -> Result<ReconcileWindow, String> {
    let start = if total > RECONCILE_WINDOW { total - RECONCILE_WINDOW + 1 } else { 1 };
    let fetched = session
        .fetch(&format!("{}:{}", start, total), "(UID FLAGS ENVELOPE)")
        .map_err(|e| format!("IMAP FETCH (flags) error: {}", e))?;
    let mut entries = Vec::new();
    for msg in fetched.iter() {
        let Some(uid) = msg.uid else { continue };
        let message_id = msg
            .envelope()
            .and_then(|env| env.message_id)
            .map(|raw| String::from_utf8_lossy(raw).trim().to_string())
            .filter(|mid| !mid.is_empty());
        entries.push(WindowEntry {
            uid,
            id: imap_email_id(account.id, message_id.as_deref(), mailbox_label, &uid.to_string()),
            is_read: has_flag(msg, &imap::types::Flag::Seen),
            starred: has_flag(msg, &imap::types::Flag::Flagged),
        });
    }
    // The window only proves absence from its lowest UID upwards.
    let min_uid = entries.iter().map(|e| e.uid).min().unwrap_or(0);
    Ok(ReconcileWindow { min_uid, entries })
}

pub fn sync_imap_mailbox_incremental(
    account: &Account,
    mailbox_label: &str,
    stored_uidvalidity: Option<u32>,
    stored_highest_uid: Option<u32>,
) -> Result<SyncResult, String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = retry_connect(&creds)?;
    let folders = list_folders(&mut session)?;
    let result = sync_mailbox_in_session(
        &mut session, &folders, account, mailbox_label, stored_uidvalidity, stored_highest_uid,
    );
    let _ = session.logout();
    result
}

/// Syncs several mailboxes over a single login. Providers like GMX, web.de
/// and Outlook throttle or temporarily block accounts that log in too often;
/// one session per mailbox per cycle was a steady source of failed syncs.
pub fn sync_imap_mailboxes(
    account: &Account,
    mailboxes: &[(String, Option<u32>, Option<u32>)],
) -> Result<Vec<(String, Result<SyncResult, String>)>, String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = retry_connect(&creds)?;
    let folders = list_folders(&mut session)?;
    let results = mailboxes
        .iter()
        .map(|(label, uidvalidity, highest_uid)| {
            let result = sync_mailbox_in_session(&mut session, &folders, account, label, *uidvalidity, *highest_uid);
            (label.clone(), result)
        })
        .collect();
    let _ = session.logout();
    Ok(results)
}

fn sync_mailbox_in_session(
    session: &mut TlsSession,
    folders: &[Folder],
    account: &Account,
    mailbox_label: &str,
    stored_uidvalidity: Option<u32>,
    stored_highest_uid: Option<u32>,
) -> Result<SyncResult, String> {
    let empty = |uidvalidity| SyncResult {
        emails: vec![],
        highest_uid: 0,
        uidvalidity,
        uids_reset: false,
        window: None,
    };

    let folder = match imap_folder_for_mailbox(mailbox_label, folders) {
        Some(f) => f,
        None => return Ok(empty(0)),
    };

    let mailbox_info = session.select(&folder)
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let uidvalidity = mailbox_info.uid_validity.unwrap_or(0);
    let total = mailbox_info.exists;

    let state_valid = stored_uidvalidity == Some(uidvalidity) && stored_highest_uid.is_some();
    if let Some(s_validity) = stored_uidvalidity.filter(|v| *v != uidvalidity) {
        log::info!(
            "IMAP UIDVALIDITY changed for account {} mailbox {} (stored: {}, current: {}) - resyncing recent messages",
            account.id, mailbox_label, s_validity, uidvalidity
        );
    }

    if total == 0 {
        // Empty on the server: everything we still show locally is stale.
        return Ok(SyncResult {
            emails: vec![],
            highest_uid: stored_highest_uid.filter(|_| state_valid).unwrap_or(0),
            uidvalidity,
            uids_reset: !state_valid,
            window: Some(ReconcileWindow { min_uid: 0, entries: vec![] }),
        });
    }

    if state_valid {
        let s_uid = stored_highest_uid.unwrap_or(0);
        let uidnext = mailbox_info.uid_next.unwrap_or(0);
        let mut emails = Vec::new();
        let mut highest_uid = s_uid;
        // UIDNEXT can be missing; in that case just ask. A `N:*` range where
        // N is past the last UID still returns the last message, so results
        // are filtered to genuinely new UIDs.
        if uidnext == 0 || uidnext > s_uid + 1 {
            let fetched = session
                .uid_fetch(&format!("{}:*", s_uid + 1), "(BODY.PEEK[] FLAGS UID)")
                .map_err(|e| format!("IMAP UID FETCH error: {}", e))?;
            highest_uid = fetched.iter().filter_map(|m| m.uid).fold(s_uid, u32::max);
            let (parsed, entries) = parse_imap_messages(&fetched, account, mailbox_label);
            let new_ids: std::collections::HashSet<String> = entries
                .into_iter()
                .filter(|e| e.uid > s_uid)
                .map(|e| e.id)
                .collect();
            emails = parsed.into_iter().filter(|e| new_ids.contains(&e.id)).collect();
        }
        let window = fetch_reconcile_window(session, account, mailbox_label, total)?;
        return Ok(SyncResult { emails, highest_uid, uidvalidity, uids_reset: false, window: Some(window) });
    }

    // No usable state (first sync or UIDVALIDITY changed): download only the
    // most recent messages. Older ones are loaded on demand by paging;
    // pulling every body of a large mailbox here used to time out.
    log::info!("No usable IMAP sync state for account {} mailbox {} - fetching recent messages",
               account.id, mailbox_label);
    let start = if total > INITIAL_WINDOW { total - INITIAL_WINDOW + 1 } else { 1 };
    let fetched = session.fetch(&format!("{}:{}", start, total), "(BODY.PEEK[] FLAGS UID)")
        .map_err(|e| format!("IMAP FETCH error: {}", e))?;
    let highest_uid = fetched.iter().filter_map(|m| m.uid).max()
        .unwrap_or(mailbox_info.uid_next.unwrap_or(1).saturating_sub(1));
    let (emails, _) = parse_imap_messages(&fetched, account, mailbox_label);
    let window = fetch_reconcile_window(session, account, mailbox_label, total)?;
    Ok(SyncResult { emails, highest_uid, uidvalidity, uids_reset: true, window: Some(window) })
}

pub fn test_imap_connection(
    imap_host: &str,
    imap_port: u16,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let creds = ImapCredentials {
        imap_host: imap_host.to_string(),
        imap_port,
        username: username.to_string(),
        password: password.to_string(),
    };

    let mut session = connect_with_timeout(&creds, CONNECT_TIMEOUT_SECS)?;
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
    let mut session = retry_connect(&creds)?;
    let folders = list_folders(&mut session)?;

    let sent_folder = imap_folder_for_mailbox("SENT", &folders)
        .ok_or_else(|| "Could not find Sent folder".to_string())?;

    
    let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S +0000").to_string();
    // Without From the saved copy showed up as "Unknown Sender" in Sent, and
    // raw UTF-8 in Subject is invalid in a header, so encode it (RFC 2047).
    let from = match account.display_name.as_deref().filter(|n| !n.trim().is_empty()) {
        Some(name) => format!("{} <{}>", encode_header_word(name), account.email),
        None => account.email.clone(),
    };
    let subject = encode_header_word(subject);
    let body = if let Some(html) = body_html {
        format!(
            "From: {}\r\nTo: {}\r\nCc: {}\r\nSubject: {}\r\nDate: {}\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=\"verdant-alt\"\r\n\r\n--verdant-alt\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{}\r\n--verdant-alt\r\nContent-Type: text/html; charset=UTF-8\r\n\r\n{}\r\n--verdant-alt--\r\n",
            from, to, cc, subject, date, body_plain, html
        )
    } else {
        format!(
            "From: {}\r\nTo: {}\r\nCc: {}\r\nSubject: {}\r\nDate: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{}\r\n",
            from, to, cc, subject, date, body_plain
        )
    };

    let flags = imap::types::Flag::Seen;
    session
        .append_with_flags(&sent_folder, body.as_bytes(), &[flags])
        .map_err(|e| format!("IMAP APPEND error: {}", e))?;

    let _ = session.logout();
    Ok(())
}

fn encode_header_word(value: &str) -> String {
    if value.is_ascii() {
        return value.to_string();
    }
    use base64::Engine as _;
    format!("=?UTF-8?B?{}?=", base64::engine::general_purpose::STANDARD.encode(value.as_bytes()))
}

pub fn imap_search_emails(
    account: &Account,
    query: &str,
    max_results: u32,
) -> Result<Vec<Email>, String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = retry_connect(&creds)?;

    session.select("INBOX")
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let q = quote_imap_string(query.trim());
    let uids = session.uid_search(format!("OR OR SUBJECT {q} FROM {q} BODY {q}"))
        .map_err(|e| format!("IMAP SEARCH error: {}", e))?;

    if uids.is_empty() {
        let _ = session.logout();
        return Ok(vec![]);
    }

    let mut uid_list: Vec<u32> = uids.into_iter().collect();
    uid_list.sort_unstable_by(|a, b| b.cmp(a));
    uid_list.truncate(max_results as usize);

    let uid_set = uid_list.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
    let messages = session.uid_fetch(&uid_set, "(BODY.PEEK[] FLAGS UID)")
        .map_err(|e| format!("IMAP FETCH error: {}", e))?;

    // Same parser as sync, so search hits get the same ids and thread ids as
    // the stored copies instead of creating near-duplicates.
    let (emails, _) = parse_imap_messages(&messages, account, "INBOX");
    let _ = session.logout();
    Ok(emails)
}

fn quote_imap_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Finds the UIDs of a message in the currently selected folder. Messages
/// that had no Message-ID header are stored as `imap-{account}-{MAILBOX}-{uid}`
/// and can only be addressed by that UID, in that mailbox.
fn resolve_uids(session: &mut TlsSession, message_id_header: &str, mailbox: &str) -> Result<Vec<u32>, String> {
    let parts: Vec<&str> = message_id_header.splitn(4, '-').collect();
    if parts.len() == 4 && parts[0] == "imap" {
        if parts[2].eq_ignore_ascii_case(mailbox) {
            if let Ok(uid) = parts[3].parse::<u32>() {
                return Ok(vec![uid]);
            }
        }
        return Ok(vec![]);
    }
    let found = session
        .uid_search(format!("HEADER Message-ID {}", quote_imap_string(message_id_header)))
        .map_err(|e| format!("IMAP SEARCH error: {}", e))?;
    let mut uids: Vec<u32> = found.into_iter().collect();
    uids.sort_unstable();
    Ok(uids)
}

/// Error prefix for operations on a message the server no longer has.
pub const MESSAGE_NOT_FOUND: &str = "Message not found on server";

fn uid_set(uids: &[u32]) -> String {
    uids.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",")
}

fn has_capability(session: &mut TlsSession, name: &str) -> bool {
    session.capabilities().map(|caps| caps.has_str(name)).unwrap_or(false)
}

/// Removes exactly these UIDs. A bare EXPUNGE would also permanently delete
/// every other message another client had marked `\Deleted` in the folder.
fn expunge_uids(session: &mut TlsSession, uids: &str) -> Result<(), String> {
    if has_capability(session, "UIDPLUS") {
        session.uid_expunge(uids).map(|_| ()).map_err(|e| format!("IMAP EXPUNGE error: {}", e))
    } else {
        session.expunge().map(|_| ()).map_err(|e| format!("IMAP EXPUNGE error: {}", e))
    }
}

pub fn imap_set_flag(
    account: &Account,
    message_id_header: &str,
    flag: &str,
    add: bool,
    mailbox: &str,
) -> Result<(), String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = retry_connect(&creds)?;
    let folders = list_folders(&mut session)?;

    let folder = imap_folder_for_mailbox(mailbox, &folders)
        .unwrap_or_else(|| "INBOX".to_string());
    session.select(&folder)
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let uids = resolve_uids(&mut session, message_id_header, mailbox)?;
    if uids.is_empty() {
        let _ = session.logout();
        return Err(format!("{}: {} in {}", MESSAGE_NOT_FOUND, message_id_header, folder));
    }
    let set = uid_set(&uids);
    let op = if add { "+FLAGS.SILENT" } else { "-FLAGS.SILENT" };
    session.uid_store(&set, format!("{} ({})", op, flag))
        .map_err(|e| format!("IMAP STORE error: {}", e))?;
    if add && flag == "\\Deleted" {
        expunge_uids(&mut session, &set)?;
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
    let mut session = retry_connect(&creds)?;
    let folders = list_folders(&mut session)?;

    let src_folder = imap_folder_for_mailbox(source_mailbox, &folders)
        .unwrap_or_else(|| "INBOX".to_string());
    let dst_folder = imap_folder_for_mailbox(target_mailbox, &folders)
        .ok_or_else(|| format!("Could not find {} folder", target_mailbox))?;

    session.select(&src_folder)
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let uids = resolve_uids(&mut session, message_id_header, source_mailbox)?;
    if uids.is_empty() {
        let _ = session.logout();
        return Err(format!("{}: {} in {}", MESSAGE_NOT_FOUND, message_id_header, src_folder));
    }
    let set = uid_set(&uids);
    if has_capability(&mut session, "MOVE") {
        session.uid_mv(&set, &dst_folder)
            .map_err(|e| format!("IMAP MOVE error: {}", e))?;
    } else {
        session.uid_copy(&set, &dst_folder)
            .map_err(|e| format!("IMAP COPY error: {}", e))?;
        session.uid_store(&set, "+FLAGS.SILENT (\\Deleted)")
            .map_err(|e| format!("IMAP STORE error: {}", e))?;
        expunge_uids(&mut session, &set)?;
    }

    let _ = session.logout();
    Ok(())
}

/// Fetches the raw RFC 822 source of a stored message.
pub fn fetch_raw_message(account: &Account, message_id_header: &str, mailbox: &str) -> Result<Vec<u8>, String> {
    let creds = ImapCredentials::from_account(account)?;
    let mut session = retry_connect(&creds)?;
    let folders = list_folders(&mut session)?;
    let folder = imap_folder_for_mailbox(mailbox, &folders).unwrap_or_else(|| "INBOX".to_string());
    session.select(&folder).map_err(|e| format!("IMAP SELECT error: {}", e))?;
    let uids = resolve_uids(&mut session, message_id_header, mailbox)?;
    let Some(uid) = uids.first() else {
        let _ = session.logout();
        return Err(format!("{}: {} in {}", MESSAGE_NOT_FOUND, message_id_header, folder));
    };
    let fetched = session.uid_fetch(uid.to_string(), "(BODY.PEEK[])")
        .map_err(|e| format!("IMAP FETCH error: {}", e))?;
    let body = fetched.iter().find_map(|m| m.body().map(<[u8]>::to_vec));
    let _ = session.logout();
    body.ok_or_else(|| "No body found".to_string())
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
    let mut session = retry_connect(&creds)?;
    let folders = list_folders(&mut session)?;

    // UIDs are only unique per folder. Newer ids record the folder the
    // message was synced from; older ids fall back to probing common ones.
    let target_mailboxes: Vec<&str> = match parts.get(3) {
        Some(label) => vec![*label],
        None => vec!["INBOX", "SENT", "DRAFT", "ARCHIVE", "TRASH"],
    };
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
    let folders = list_folders(&mut session)?;

    let folder = match imap_folder_for_mailbox(mailbox_label, &folders) {
        Some(f) => f,
        None => { let _ = session.logout(); return Ok(vec![]); }
    };

    let mailbox_info = session.select(&folder)
        .map_err(|e| format!("IMAP SELECT error: {}", e))?;

    let total = mailbox_info.exists;
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

    let (emails, _) = parse_imap_messages(&messages, account, mailbox_label);
    let _ = session.logout();
    Ok(emails)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, special: Option<&'static str>) -> Folder {
        Folder { name: name.to_string(), special, selectable: true }
    }

    #[test]
    fn decodes_modified_utf7_folder_names() {
        assert_eq!(decode_imap_utf7("Entw&APw-rfe"), "Entwürfe");
        assert_eq!(decode_imap_utf7("Gel&APY-schte Elemente"), "Gelöschte Elemente");
        assert_eq!(decode_imap_utf7("&AMk-l&AOk-ments envoy&AOk-s"), "Éléments envoyés");
        assert_eq!(decode_imap_utf7("Tom &- Jerry"), "Tom & Jerry");
        assert_eq!(decode_imap_utf7("INBOX"), "INBOX");
    }

    #[test]
    fn special_use_flags_win_over_localised_names() {
        let folders = vec![
            folder("INBOX", None),
            folder("&AMk-l&AOk-ments envoy&AOk-s", Some("SENT")),
            folder("Corbeille", Some("TRASH")),
        ];
        assert_eq!(imap_folder_for_mailbox("SENT", &folders).as_deref(), Some("&AMk-l&AOk-ments envoy&AOk-s"));
        assert_eq!(imap_folder_for_mailbox("TRASH", &folders).as_deref(), Some("Corbeille"));
        assert_eq!(imap_folder_for_mailbox("inbox", &folders).as_deref(), Some("INBOX"));
    }

    #[test]
    fn falls_back_to_known_names_and_skips_unselectable() {
        let mut parent = folder("[Gmail]", None);
        parent.selectable = false;
        let folders = vec![parent, folder("[Gmail]/All Mail", None), folder("Entw&APw-rfe", None)];
        assert_eq!(imap_folder_for_mailbox("ARCHIVE", &folders).as_deref(), Some("[Gmail]/All Mail"));
        assert_eq!(imap_folder_for_mailbox("DRAFT", &folders).as_deref(), Some("Entw&APw-rfe"));
        assert_eq!(imap_folder_for_mailbox("SENT", &folders), None);
    }

    #[test]
    fn email_ids_are_stable() {
        assert_eq!(imap_email_id(3, Some("<abc@host>"), "INBOX", "7"), "3:abc@host");
        assert_eq!(imap_email_id(3, None, "SENT", "7"), "3:imap-3-SENT-7");
    }

    #[test]
    fn search_strings_are_quoted() {
        assert_eq!(quote_imap_string(r#"say "hi" \ bye"#), r#""say \"hi\" \\ bye""#);
    }
}
