use mailparse::parse_mail;

use super::{imap_folder_for_mailbox, list_folders, retry_connect, ImapCredentials};
use crate::db::Account;

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
