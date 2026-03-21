use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use pulldown_cmark::{html, Options, Parser};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailAttachment {
    pub filename: String,
    #[serde(default)]
    pub content_type: String,
    pub data_base64: String,
}

pub fn sanitize_header_value(input: &str) -> String {
    input.replace(['\r', '\n'], " ").trim().to_string()
}

pub fn fold_base64_for_mime(encoded: &str) -> String {
    let mut out = String::with_capacity(encoded.len() + (encoded.len() / 76) + 8);
    for chunk in encoded.as_bytes().chunks(76) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        out.push_str("\r\n");
    }
    out
}

pub fn markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);
    html_out
}

pub fn build_raw_mime_message(
    to: String,
    cc: String,
    subject: String,
    body: String,
    mode: String,
    body_html: Option<String>,
    attachments: Vec<EmailAttachment>,
) -> Result<String, String> {
    let to = sanitize_header_value(&to);
    let cc = sanitize_header_value(&cc);
    let subject = sanitize_header_value(&subject);
    let is_markdown = mode.eq_ignore_ascii_case("markdown");
    let is_html = mode.eq_ignore_ascii_case("html");
    let html_body = if is_html {
        let provided = body_html.unwrap_or_default();
        if provided.trim().is_empty() {
            markdown_to_html(&body)
        } else {
            provided
        }
    } else {
        markdown_to_html(&body)
    };

    let mut raw_message = String::new();
    if !to.is_empty() {
        raw_message.push_str(&format!("To: {}\r\n", to));
    }
    if !cc.is_empty() {
        raw_message.push_str(&format!("Cc: {}\r\n", cc));
    }
    raw_message.push_str(&format!("Subject: {}\r\n", subject));
    raw_message.push_str("MIME-Version: 1.0\r\n");

    if attachments.is_empty() && !is_markdown && !is_html {
        raw_message.push_str("Content-Type: text/plain; charset=UTF-8\r\n\r\n");
        raw_message.push_str(&body);
    } else {
        let mixed_boundary = "verdant-mixed-001";
        let alt_boundary = "verdant-alt-001";

        if attachments.is_empty() {
            raw_message.push_str(&format!(
                "Content-Type: multipart/alternative; boundary=\"{}\"\r\n\r\n",
                alt_boundary
            ));
            raw_message.push_str(&format!("--{}\r\n", alt_boundary));
            raw_message.push_str("Content-Type: text/plain; charset=UTF-8\r\n\r\n");
            raw_message.push_str(&body);
            raw_message.push_str("\r\n");
            raw_message.push_str(&format!("--{}\r\n", alt_boundary));
            raw_message.push_str("Content-Type: text/html; charset=UTF-8\r\n\r\n");
            raw_message.push_str(&html_body);
            raw_message.push_str("\r\n");
            raw_message.push_str(&format!("--{}--\r\n", alt_boundary));
        } else {
            raw_message.push_str(&format!(
                "Content-Type: multipart/mixed; boundary=\"{}\"\r\n\r\n",
                mixed_boundary
            ));

            raw_message.push_str(&format!("--{}\r\n", mixed_boundary));
            if is_markdown || is_html {
                raw_message.push_str(&format!(
                    "Content-Type: multipart/alternative; boundary=\"{}\"\r\n\r\n",
                    alt_boundary
                ));
                raw_message.push_str(&format!("--{}\r\n", alt_boundary));
                raw_message.push_str("Content-Type: text/plain; charset=UTF-8\r\n\r\n");
                raw_message.push_str(&body);
                raw_message.push_str("\r\n");
                raw_message.push_str(&format!("--{}\r\n", alt_boundary));
                raw_message.push_str("Content-Type: text/html; charset=UTF-8\r\n\r\n");
                raw_message.push_str(&html_body);
                raw_message.push_str("\r\n");
                raw_message.push_str(&format!("--{}--\r\n", alt_boundary));
            } else {
                raw_message.push_str("Content-Type: text/plain; charset=UTF-8\r\n\r\n");
                raw_message.push_str(&body);
                raw_message.push_str("\r\n");
            }

            for attachment in attachments {
                let raw_bytes = STANDARD
                    .decode(attachment.data_base64.as_bytes())
                    .map_err(|_| format!("Invalid attachment encoding for {}", attachment.filename))?;
                let attachment_encoded = STANDARD.encode(raw_bytes);
                let content_type = if attachment.content_type.trim().is_empty() {
                    "application/octet-stream".to_string()
                } else {
                    attachment.content_type
                };
                let safe_filename = sanitize_header_value(&attachment.filename);

                raw_message.push_str(&format!("--{}\r\n", mixed_boundary));
                raw_message.push_str(&format!(
                    "Content-Type: {}; name=\"{}\"\r\n",
                    content_type, safe_filename
                ));
                raw_message.push_str("Content-Transfer-Encoding: base64\r\n");
                raw_message.push_str(&format!(
                    "Content-Disposition: attachment; filename=\"{}\"\r\n\r\n",
                    safe_filename
                ));
                raw_message.push_str(&fold_base64_for_mime(&attachment_encoded));
            }

            raw_message.push_str(&format!("--{}--\r\n", mixed_boundary));
        }
    }

    Ok(URL_SAFE_NO_PAD.encode(raw_message.as_bytes()))
}
