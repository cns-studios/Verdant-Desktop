use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::imap_client::html_escape;

#[derive(Serialize, Deserialize, Clone)]
pub struct AttachmentMeta {
    pub filename: String,
    pub mime_type: String,
    pub attachment_id: String,
    pub size: i64,
}

pub fn header_value(headers: &[Value], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|h| {
            h.get("name")
                .and_then(Value::as_str)
                .map(|n| n.eq_ignore_ascii_case(name))
                .unwrap_or(false)
        })
        .and_then(|h| h.get("value").and_then(Value::as_str).map(str::to_string))
}

pub fn strip_confusable_chars(input: &str) -> String {
    input
        .chars()
        .filter(|c| {
            !matches!(
                *c,
                '\u{00AD}'
                    | '\u{034F}'
                    | '\u{061C}'
                    | '\u{180E}'
                    | '\u{200B}'
                    | '\u{200C}'
                    | '\u{200D}'
                    | '\u{200E}'
                    | '\u{200F}'
                    | '\u{202A}'
                    | '\u{202B}'
                    | '\u{202C}'
                    | '\u{202D}'
                    | '\u{202E}'
                    | '\u{2060}'
                    | '\u{2061}'
                    | '\u{2062}'
                    | '\u{2063}'
                    | '\u{2064}'
                    | '\u{2066}'
                    | '\u{2067}'
                    | '\u{2068}'
                    | '\u{2069}'
                    | '\u{FEFF}'
            )
        })
        .collect()
}

fn content_type_charset(payload: &Value) -> Option<String> {
    let headers = payload.get("headers").and_then(Value::as_array)?;
    let content_type = header_value(headers, "Content-Type")?;
    content_type.split(';').skip(1).find_map(|param| {
        let (key, value) = param.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches('"').to_string())
    })
}

// The Gmail API undoes the transfer encoding but leaves the bytes in the part's
// declared charset, so non-UTF-8 mails must be decoded explicitly.
pub(crate) fn decode_part(payload: &Value) -> Option<String> {
    let data = payload
        .get("body")
        .and_then(|b| b.get("data"))
        .and_then(Value::as_str)?;

    use base64::engine::general_purpose::{STANDARD, URL_SAFE};

    let bytes = URL_SAFE_NO_PAD
        .decode(data.as_bytes())
        .or_else(|_| URL_SAFE.decode(data.as_bytes()))
        .or_else(|_| STANDARD.decode(data.as_bytes()))
        .ok()?;

    let encoding = content_type_charset(payload)
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (decoded, _, _) = encoding.decode(&bytes);
    if decoded.is_empty() {
        return None;
    }
    Some(decoded.into_owned())
}

fn part_mime(payload: &Value) -> String {
    payload
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_attachment(payload: &Value) -> bool {
    let has_filename = payload
        .get("filename")
        .and_then(Value::as_str)
        .is_some_and(|name| !name.trim().is_empty());
    let disposition_attachment = payload
        .get("headers")
        .and_then(Value::as_array)
        .and_then(|headers| header_value(headers, "Content-Disposition"))
        .is_some_and(|value| value.trim_start().to_ascii_lowercase().starts_with("attachment"));
    has_filename || disposition_attachment
}

fn is_plain_body(body: &str) -> bool {
    body.trim_start().starts_with("<pre>")
}

pub fn extract_body(payload: &Value) -> Option<String> {
    let mime = part_mime(payload);

    if mime == "text/html" {
        return decode_part(payload);
    }
    if mime == "text/plain" {
        return decode_part(payload).map(|text| format!("<pre>{}</pre>", html_escape(&text)));
    }

    let parts: Vec<&Value> = payload
        .get("parts")
        .and_then(Value::as_array)?
        .iter()
        .filter(|part| !is_attachment(part))
        .collect();

    match mime.as_str() {
        // Alternatives are ordered from plainest to richest: prefer the last HTML one.
        "multipart/alternative" => {
            let mut plain_fallback = None;
            for part in parts.iter().rev() {
                if let Some(body) = extract_body(part) {
                    if !is_plain_body(&body) {
                        return Some(body);
                    }
                    plain_fallback.get_or_insert(body);
                }
            }
            plain_fallback
        }
        // The first part is the document; the rest are resources it references.
        "multipart/related" => parts.iter().find_map(|part| extract_body(part)),
        // multipart/mixed & co.: every inline body part is shown, in order
        // (e.g. Apple Mail splits text around inline attachments).
        _ => {
            let bodies: Vec<String> = parts.iter().filter_map(|part| extract_body(part)).collect();
            (!bodies.is_empty()).then(|| bodies.join("\n"))
        }
    }
}

pub fn collect_attachments(payload: &Value, out: &mut Vec<AttachmentMeta>) {
    let filename = payload
        .get("filename")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let attachment_id = payload
        .get("body")
        .and_then(|b| b.get("attachmentId"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();

    if !filename.is_empty() && !attachment_id.is_empty() {
        let mime_type = payload
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or("application/octet-stream")
            .to_string();
        let size = payload
            .get("body")
            .and_then(|b| b.get("size"))
            .and_then(Value::as_i64)
            .unwrap_or(0);

        out.push(AttachmentMeta {
            filename,
            mime_type,
            attachment_id,
            size,
        });
    }

    if let Some(parts) = payload.get("parts").and_then(Value::as_array) {
        for part in parts {
            collect_attachments(part, out);
        }
    }
}

pub fn mailbox_label(mailbox: &str) -> Option<&'static str> {
    match mailbox {
        "INBOX" => Some("INBOX"),
        "SENT" => Some("SENT"),
        "DRAFT" => Some("DRAFT"),
        "TRASH" => Some("TRASH"),
        "SPAM" => Some("SPAM"),
        _ => None,
    }
}

pub fn mailbox_from_labels(labels: &str) -> String {
    let parts: Vec<&str> = labels.split(',').collect();
    if parts.contains(&"SENT") {
        "SENT".to_string()
    } else if parts.contains(&"DRAFT") {
        "DRAFT".to_string()
    } else if parts.contains(&"TRASH") {
        "TRASH".to_string()
    } else if parts.contains(&"SPAM") {
        "SPAM".to_string()
    } else if parts.contains(&"INBOX") {
        "INBOX".to_string()
    } else {
        "ARCHIVE".to_string()
    }
}
