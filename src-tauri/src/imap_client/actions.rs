use super::{has_capability, imap_folder_for_mailbox, list_folders, parse_imap_messages, retry_connect, ImapCredentials, TlsSession};
use crate::db::{Account, Email};

pub const MESSAGE_NOT_FOUND: &str = "Message not found on server";

pub(crate) fn quote_imap_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub(crate) fn resolve_uids(session: &mut TlsSession, message_id_header: &str, mailbox: &str) -> Result<Vec<u32>, String> {
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

pub(crate) fn uid_set(uids: &[u32]) -> String {
    uids.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",")
}

pub(crate) fn expunge_uids(session: &mut TlsSession, uids: &str) -> Result<(), String> {
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

    let (emails, _) = parse_imap_messages(&messages, account, "INBOX");
    let _ = session.logout();
    Ok(emails)
}

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
