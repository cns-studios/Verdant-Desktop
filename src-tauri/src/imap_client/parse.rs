use mailparse::{parse_mail, MailHeaderMap};

use crate::db::{Account, Email};

pub(crate) fn parse_body(parsed: &mailparse::ParsedMail) -> String {
    let embedded_images = collect_embedded_images(parsed);
    let html = extract_html(parsed);
    replace_cid_with_data_uris(&html, &embedded_images)
}

pub(crate) fn extract_html(parsed: &mailparse::ParsedMail) -> String {
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

pub(crate) fn extract_snippet(parsed: &mailparse::ParsedMail) -> String {
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

pub(crate) fn collect_embedded_images(parsed: &mailparse::ParsedMail) -> std::collections::HashMap<String, String> {
    let mut images = std::collections::HashMap::new();
    collect_images_recursive(parsed, &mut images);
    images
}

pub(crate) fn collect_images_recursive(parsed: &mailparse::ParsedMail, images: &mut std::collections::HashMap<String, String>) {
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

pub(crate) fn replace_cid_with_data_uris(html: &str, images: &std::collections::HashMap<String, String>) -> String {
    let mut result = html.to_string();

    for (cid, data_uri) in images.iter() {
        let cid_ref = format!("cid:{}", cid);
        result = result.replace(&cid_ref, data_uri);
    }

    result
}

pub(crate) fn html_escape(input: &str) -> String {
    input.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub(crate) fn canonical_thread_id(headers: &mailparse::headers::Headers<'_>, message_id: &str) -> String {
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

pub(crate) fn collect_imap_attachments(parsed: &mailparse::ParsedMail, uid: &str, mailbox_label: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let mut idx = 0;
    collect_imap_attachments_recursive(parsed, uid, mailbox_label, &mut out, &mut idx);
    out
}

pub(crate) fn collect_imap_attachments_recursive(
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

pub(crate) fn rfc2822_to_epoch(date_str: &str) -> i64 {
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

pub(crate) fn strip_noise(input: &str) -> String {
    input.chars().filter(|c| !matches!(*c,
        '\u{00AD}' | '\u{034F}' | '\u{061C}' | '\u{180E}'
        | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2069}' | '\u{FEFF}'
    )).collect()
}

pub(crate) fn imap_email_id(account_id: i64, message_id: Option<&str>, mailbox_label: &str, uid: &str) -> String {
    let message_id = message_id
        .map(str::to_string)
        .unwrap_or_else(|| format!("imap-{}-{}-{}", account_id, mailbox_label, uid));
    format!("{}:{}", account_id, message_id.trim_matches(|c: char| c == '<' || c == '>'))
}

pub(crate) fn has_flag(msg: &imap::types::Fetch, wanted: &imap::types::Flag) -> bool {
    msg.flags().iter().any(|f| f == wanted)
}

pub struct WindowEntry {
    pub uid: u32,
    pub id: String,
    pub is_read: bool,
    pub starred: bool,
}

pub(crate) fn parse_imap_messages(
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
