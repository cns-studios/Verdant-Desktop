use super::{imap_folder_for_mailbox, list_folders, parse_imap_messages, retry_connect, Folder, ImapCredentials, TlsSession, WindowEntry};
use super::parse::{has_flag, imap_email_id};
use crate::db::{Account, Email};

pub(crate) const RECONCILE_WINDOW: u32 = 200;

pub(crate) const INITIAL_WINDOW: u32 = 50;

pub struct ReconcileWindow {
    pub min_uid: u32,
    pub entries: Vec<WindowEntry>,
}

pub struct SyncResult {
    pub emails: Vec<Email>,
    pub highest_uid: u32,
    pub uidvalidity: u32,
    pub uids_reset: bool,
    pub window: Option<ReconcileWindow>,
}

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

pub(crate) fn fetch_reconcile_window(
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

pub(crate) fn sync_mailbox_in_session(
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
