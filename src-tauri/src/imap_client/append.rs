use super::{imap_folder_for_mailbox, list_folders, retry_connect, ImapCredentials};

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

pub(crate) fn encode_header_word(value: &str) -> String {
    if value.is_ascii() {
        return value.to_string();
    }
    use base64::Engine as _;
    format!("=?UTF-8?B?{}?=", base64::engine::general_purpose::STANDARD.encode(value.as_bytes()))
}
