use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

use crate::db::{get_all_accounts, set_mailbox_sync_state, Account, MailboxSyncState};
use crate::imap_sync::{ImapCredentials, connect_with_timeout, SyncResult};
use std::time::Instant;
use crate::state::DbState;

const SYNC_INTERVAL_SECS: u64 = 45;
const GMAIL_SYNC_INTERVAL_SECS: u64 = 120;
/// Never sync an IMAP account more often than this, even if IDLE keeps
/// waking up (some servers push untagged updates constantly).
const IMAP_MIN_SYNC_GAP_SECS: u64 = 15;
/// Poll interval when the server does not support IDLE or IDLE failed.
const IMAP_POLL_INTERVAL_SECS: u64 = 120;
const IMAP_MAILBOXES: &[&str] = &["INBOX", "SENT", "DRAFT", "TRASH"];
const GMAIL_STAGGER_CYCLE: u32 = 4;
const IDLE_TIMEOUT_SECS: u64 = 60 * 5;
const RETRY_BASE_MS: u64 = 1000;
const RETRY_MAX_MS: u64 = 30000;

pub async fn start_all_sync_tasks(app: tauri::AppHandle, state: Arc<DbState>) {
    let accounts = {
        let conn = state.conn.lock().await;
        get_all_accounts(&conn).unwrap_or_default()
    };

    for account in accounts {
        start_account_sync(app.clone(), state.clone(), account).await;
    }
}

pub async fn start_account_sync(app: tauri::AppHandle, state: Arc<DbState>, account: Account) {
    let account_id = account.id;
    stop_account_sync(&state, account_id).await;

    let (tx, rx) = oneshot::channel::<()>();

    {
        let mut handles = state.sync_handles.lock().await;
        handles.insert(account_id, tx);
    }

    let state_clone = state.clone();
    tokio::spawn(async move {
        if account.provider == "imap" {
            run_imap_sync_loop(app, state_clone, account, rx).await;
        } else {
            run_gmail_sync_loop(app, state_clone, account, rx).await;
        }
    });
}

pub async fn stop_account_sync(state: &DbState, account_id: i64) {
    let mut handles = state.sync_handles.lock().await;
    if let Some(tx) = handles.remove(&account_id) {
        let _ = tx.send(());
    }
}

async fn run_gmail_sync_loop(
    app: tauri::AppHandle,
    state: Arc<DbState>,
    account: Account,
    mut shutdown: oneshot::Receiver<()>,
) {
    let account_id = account.id;
    let mut cycle: u32 = 0;

    sync_gmail_account(&app, &state, &account, cycle).await;

    loop {
        match state.get_fresh_account(account_id).await {
            Ok(Some(acc)) => {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(GMAIL_SYNC_INTERVAL_SECS)) => {
                        cycle = cycle.wrapping_add(1);

                        if crate::state::rate_limited_remain(&state, account_id).is_some() {
                            log::warn!("Gmail account={} rate limited — skipping sync cycle", account_id);
                            continue;
                        }

                        sync_gmail_account(&app, &state, &acc, cycle).await;
                    }
                    _ = &mut shutdown => break,
                }
            }
            Ok(None) => {
                log::info!("Gmail account {} not found in DB, stopping sync", account_id);
                break;
            }
            Err(e) => {
                log::error!("Failed to refresh Gmail account {}: {}", account_id, e);
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        }
    }
    let _ = app;
}

async fn sync_gmail_account(app: &tauri::AppHandle, state: &DbState, account: &Account, cycle: u32) {
    use crate::commands::mail::sync_mailbox_internal_for;

    let mailboxes: &[&str] = if cycle % GMAIL_STAGGER_CYCLE == 0 {
        &["INBOX", "SENT", "DRAFT", "TRASH"]
    } else {
        &["INBOX"]
    };

    for mailbox in mailboxes {
        if let Err(e) = sync_mailbox_internal_for(state, account.id, mailbox).await {
            log::error!("Gmail sync error account={} mailbox={}: {}", account.id, mailbox, e);
            continue;
        }
        if *mailbox == "INBOX" {
            // History is incremental, but a stale/truncated cursor can miss
            // an incoming message. Reconcile the newest Gmail page too.
            if let Err(e) = crate::commands::mail::sync_mailbox_page_internal_for(
                state,
                account.id,
                mailbox,
                None,
            ).await {
                log::error!("Gmail Inbox newest-page reconciliation failed account={}: {}", account.id, e);
            }
        }
    }

    notify_new_mail(app, state, account).await;
}

async fn run_imap_sync_loop(
    app: tauri::AppHandle,
    state: Arc<DbState>,
    account: Account,
    mut shutdown: oneshot::Receiver<()>,
) {
    let account_id = account.id;
    sync_imap_account(&app, &state, &account).await;

    loop {
        let acc = match state.get_fresh_account(account_id).await {
            Ok(Some(acc)) => acc,
            Ok(None) => {
                log::info!("Account {} not found in DB, stopping sync", account_id);
                break;
            }
            Err(e) => {
                log::error!("Failed to refresh account {}: {}", account_id, e);
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        // Wait for the server to push a change (IDLE) instead of a blind
        // five-minute sleep, which made new mail feel randomly delayed.
        tokio::select! {
            _ = wait_for_imap_change(&acc) => {
                sync_imap_account(&app, &state, &acc).await;
            }
            _ = &mut shutdown => break,
        }
    }
}

async fn wait_for_imap_change(account: &Account) {
    let started = Instant::now();
    let idle_ok = try_imap_idle(account, Duration::from_secs(IDLE_TIMEOUT_SECS)).await;
    let min_wait = Duration::from_secs(if idle_ok { IMAP_MIN_SYNC_GAP_SECS } else { IMAP_POLL_INTERVAL_SECS });
    let elapsed = started.elapsed();
    if elapsed < min_wait {
        tokio::time::sleep(min_wait - elapsed).await;
    }
}

async fn try_imap_idle(account: &Account, timeout: Duration) -> bool {
    let creds = match ImapCredentials::from_account(account) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut session = connect_with_timeout(&creds, 8)?;
        let folders = crate::imap_sync::list_folders(&mut session)?;
        let inbox = crate::imap_sync::imap_folder_for_mailbox("INBOX", &folders)
            .unwrap_or_else(|| "INBOX".to_string());

        session.select(&inbox)
            .map_err(|e| format!("{}", e))?;

        let handle = session.idle()
            .map_err(|e| format!("IDLE not supported: {}", e))?;

        handle.wait_with_timeout(timeout)
            .map(|_| ())
            .map_err(|e| format!("IDLE wait failed: {}", e))
    }).await;

    match result {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            log::debug!("IMAP IDLE unavailable for account {}: {}", account.id, e);
            false
        }
        Err(_) => false,
    }
}

async fn sync_imap_account(app: &tauri::AppHandle, state: &DbState, account: &Account) {
    let account_id = account.id;
    let mut had_new_emails = false;

    let stored: Vec<(String, Option<u32>, Option<u32>)> = {
        let conn = state.conn.lock().await;
        IMAP_MAILBOXES
            .iter()
            .map(|mb| {
                let s = crate::db::get_mailbox_sync_state(&conn, account_id, mb).ok().flatten();
                (mb.to_string(), s.as_ref().map(|s| s.uidvalidity), s.as_ref().map(|s| s.highest_uid))
            })
            .collect()
    };

    let acc = account.clone();
    let result = tokio::task::spawn_blocking(move || {
        crate::imap_sync::sync_imap_mailboxes(&acc, &stored)
    }).await;

    match result {
        Ok(Ok(per_mailbox)) => {
            for (mailbox, outcome) in per_mailbox {
                match outcome {
                    Ok(sync_result) => {
                        had_new_emails |= !sync_result.emails.is_empty();
                        apply_sync_result(state, account_id, sync_result, &mailbox).await;
                    }
                    Err(e) => {
                        log::error!("IMAP sync error account={} mailbox={}: {}", account_id, mailbox, e);
                        fallback_sync(state, account, &mailbox).await;
                    }
                }
            }
        }
        Ok(Err(e)) => {
            // Connection-level failure: the next cycle retries; per-mailbox
            // fallbacks would only hammer an unreachable server.
            log::warn!("IMAP server unavailable account={}: {}", account_id, e);
        }
        Err(e) => {
            log::error!("IMAP sync task panicked account={}: {}", account_id, e);
        }
    }

    if had_new_emails {
        log::debug!("IMAP sync downloaded new mail for account {}", account_id);
    }
    notify_new_mail(app, state, account).await;
}

async fn fallback_sync(state: &DbState, account: &Account, mailbox: &str) {
    use crate::imap_sync::sync_imap_mailbox;
    let acc = account.clone();
    let mb = mailbox.to_string();
    let account_id = account.id;

    let result = tokio::task::spawn_blocking(move || {
        sync_imap_mailbox(&acc, &mb, 50)
    }).await;

    if let Ok(Ok(emails)) = result {
        upsert_emails(state, account_id, emails, mailbox).await;
    }
}

/// Stores a sync result: new messages, server-side flag changes, removals
/// inside the reconcile window, and the new UID high-water mark.
pub async fn apply_sync_result(state: &DbState, account_id: i64, result: SyncResult, mailbox: &str) {
    let SyncResult { emails, highest_uid, uidvalidity, uids_reset, window } = result;
    upsert_emails(state, account_id, emails, mailbox).await;

    let conn = state.conn.lock().await;
    if let Err(e) = apply_reconcile_window(&conn, account_id, mailbox, uids_reset, window.as_ref()) {
        log::error!("IMAP reconcile failed account={} mailbox={}: {}", account_id, mailbox, e);
    }

    if highest_uid > 0 || uidvalidity > 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let sync_state = MailboxSyncState {
            account_id,
            mailbox_name: mailbox.to_string(),
            highest_uid,
            uidvalidity,
            last_synced_at: now,
        };
        let _ = set_mailbox_sync_state(&conn, &sync_state);
    }

    // Messages restored from 'OTHER' by the window need a category too.
    if mailbox == "INBOX" {
        if let Err(e) = crate::smart_inbox::assign_unassigned(&conn, account_id) {
            log::error!("Smart Inbox assignment after IMAP reconcile failed: {}", e);
        }
    }
}

fn apply_reconcile_window(
    conn: &rusqlite::Connection,
    account_id: i64,
    mailbox: &str,
    uids_reset: bool,
    window: Option<&crate::imap_sync::ReconcileWindow>,
) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    if uids_reset {
        tx.execute(
            "UPDATE emails SET imap_uid=NULL, imap_uid_mailbox=NULL WHERE account_id=?1 AND imap_uid_mailbox=?2",
            rusqlite::params![account_id, mailbox],
        )?;
    }
    if let Some(window) = window {
        {
            // 'OTHER' is where earlier versions parked messages they wrongly
            // believed were gone; anything the server still lists comes back.
            let mut update = tx.prepare(
                "UPDATE emails SET is_read=?1, starred=?2, imap_uid=?3, imap_uid_mailbox=?4, mailbox=?4
                 WHERE id=?5 AND account_id=?6 AND (mailbox=?4 OR mailbox='OTHER')",
            )?;
            for entry in &window.entries {
                update.execute(rusqlite::params![
                    entry.is_read as i32, entry.starred as i32, entry.uid, mailbox, entry.id, account_id
                ])?;
            }
        }

        let present: std::collections::HashSet<u32> = window.entries.iter().map(|e| e.uid).collect();
        let stale: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT id, imap_uid FROM emails
                 WHERE account_id=?1 AND mailbox=?2 AND imap_uid_mailbox=?2 AND imap_uid >= ?3",
            )?;
            let rows = stmt.query_map(rusqlite::params![account_id, mailbox, window.min_uid], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?))
            })?;
            rows.filter_map(Result::ok)
                .filter(|(_, uid)| !present.contains(uid))
                .map(|(id, _)| id)
                .collect()
        };
        for id in stale {
            tx.execute(
                "UPDATE emails SET mailbox='OTHER', imap_uid=NULL, imap_uid_mailbox=NULL WHERE id=?1 AND account_id=?2",
                rusqlite::params![id, account_id],
            )?;
        }
    }
    tx.commit()
}

/// Shows one desktop notification for mail that genuinely just arrived.
///
/// Everything already in the mailbox when an account is first synced is
/// history, not news: it is marked as notified without a notification. That
/// baseline, plus normalising Gmail's millisecond timestamps (IMAP stores
/// seconds), is what stops "50 new emails" from firing repeatedly after
/// onboarding. Older mail loaded later by paging never counts as new either.
async fn notify_new_mail(app: &tauri::AppHandle, state: &DbState, account: &Account) {
    use tauri::Manager;
    let account_id = account.id;
    let (show, important_only, language) = match app.try_state::<crate::commands::app_config::AppConfigState>() {
        Some(config) => {
            let config = config.0.lock().await;
            (config.show_notifications, config.notify_important_only, config.language.clone())
        }
        None => (true, false, "en".to_string()),
    };

    let fresh: Vec<(String, String)> = {
        let conn = state.conn.lock().await;
        match collect_new_mail(&conn, account_id, important_only) {
            Ok(rows) => rows,
            Err(e) => {
                log::error!("Collecting new mail for notifications failed account={}: {}", account_id, e);
                Vec::new()
            }
        }
    };

    if show && !fresh.is_empty() {
        use tauri_plugin_notification::NotificationExt;
        let acc_name = account.display_name.as_deref().unwrap_or(&account.email);
        let (title, body) = notification_text(&language, acc_name, &fresh);
        let _ = app.notification().builder().title(title).body(body).show();
    }

    use tauri::Emitter;
    let _ = app.emit("emails-synced", ());
}

/// Title and body of the new-mail notification in the app's language.
/// `fresh` holds (subject, sender) and must not be empty.
fn notification_text(language: &str, account: &str, fresh: &[(String, String)]) -> (String, String) {
    let german = language == "de";
    if let [(subject, sender)] = fresh {
        let subject = match (subject.trim().is_empty(), german) {
            (false, _) => subject.as_str(),
            (true, true) => "(Kein Betreff)",
            (true, false) => "(No subject)",
        };
        return if german {
            (format!("Neue E-Mail – {account}"), format!("Von: {sender}\n{subject}"))
        } else {
            (format!("New email – {account}"), format!("From: {sender}\n{subject}"))
        };
    }
    let n = fresh.len();
    if german {
        (format!("{n} neue E-Mails – {account}"), format!("Du hast {n} neue Nachrichten im Posteingang."))
    } else {
        (format!("{n} new emails – {account}"), format!("You have {n} new messages in your inbox."))
    }
}

/// Returns (subject, sender) of mail to announce and marks every candidate as
/// handled, so nothing is ever announced twice.
fn collect_new_mail(conn: &rusqlite::Connection, account_id: i64, important_only: bool) -> rusqlite::Result<Vec<(String, String)>> {
    const TS_SECONDS: &str = "(CASE WHEN internal_ts > 100000000000 THEN internal_ts / 1000 ELSE internal_ts END)";
    let tx = conn.unchecked_transaction()?;

    let ready: i64 = tx
        .query_row("SELECT COALESCE(notify_ready, 0) FROM accounts WHERE id=?1", [account_id], |r| r.get(0))
        .unwrap_or(1);
    if ready == 0 {
        // Only a completed sync defines the baseline; after a failed first
        // attempt (e.g. offline) the initial download is still to come.
        let synced: i64 = tx.query_row(
            "SELECT (SELECT COUNT(*) FROM mailbox_sync_state WHERE account_id=?1)
                  + (SELECT COUNT(*) FROM gmail_sync_state WHERE account_id=?1)",
            [account_id],
            |r| r.get(0),
        )?;
        if synced == 0 {
            return Ok(Vec::new());
        }
        tx.execute("UPDATE emails SET notified=1 WHERE account_id=?1 AND notified=0", [account_id])?;
        tx.execute("UPDATE accounts SET notify_ready=1 WHERE id=?1", [account_id])?;
        tx.commit()?;
        return Ok(Vec::new());
    }

    // Anything that is not recent unread inbox mail is settled for good.
    tx.execute(
        &format!(
            "UPDATE emails SET notified=1 WHERE account_id=?1 AND notified=0
             AND (mailbox<>'INBOX' OR is_read=1 OR {TS_SECONDS} < strftime('%s','now') - 3600)"
        ),
        [account_id],
    )?;

    let bulk = "(list_unsubscribe<>'' OR labels LIKE '%CATEGORY_PROMOTIONS%' OR labels LIKE '%CATEGORY_SOCIAL%'
                 OR labels LIKE '%CATEGORY_UPDATES%' OR labels LIKE '%CATEGORY_FORUMS%')";
    let filter = if important_only { format!("AND NOT {bulk}") } else { String::new() };
    let fresh = {
        let mut stmt = tx.prepare(&format!(
            "SELECT subject, sender FROM emails WHERE account_id=?1 AND notified=0 {filter}
             ORDER BY internal_ts DESC"
        ))?;
        let rows = stmt.query_map([account_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    tx.execute("UPDATE emails SET notified=1 WHERE account_id=?1 AND notified=0", [account_id])?;
    tx.commit()?;
    Ok(fresh)
}

/// Inserts or refreshes downloaded messages. It deliberately does not infer
/// deletions: an earlier version hid every stored message dated after the
/// oldest message of an *incremental* batch, so one new mail with an old
/// Date header made most of the inbox vanish. Removals are now derived from
/// server UIDs in `apply_sync_result`.
pub async fn upsert_emails(state: &DbState, account_id: i64, emails: Vec<crate::db::Email>, mailbox: &str) {
    let conn = state.conn.lock().await;

    for email in &emails {
        if let Err(e) = conn.execute(
            "INSERT INTO emails (id, account_id, draft_id, thread_id, subject, sender, to_recipients, cc_recipients,
                                 snippet, body_html, attachments_json, has_attachments, date, is_read, starred,
                                 mailbox, labels, internal_ts, list_unsubscribe)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
             ON CONFLICT(id, account_id) DO UPDATE SET
                thread_id=excluded.thread_id,
                subject=excluded.subject,
                sender=excluded.sender,
                to_recipients=excluded.to_recipients,
                cc_recipients=excluded.cc_recipients,
                snippet=excluded.snippet,
                body_html=excluded.body_html,
                attachments_json=excluded.attachments_json,
                has_attachments=excluded.has_attachments,
                date=excluded.date,
                is_read=excluded.is_read,
                starred=excluded.starred,
                mailbox=excluded.mailbox,
                labels=excluded.labels,
                internal_ts=excluded.internal_ts,
                list_unsubscribe=excluded.list_unsubscribe",
            rusqlite::params![
                email.id, email.account_id, email.draft_id, email.thread_id,
                email.subject, email.sender, email.to_recipients, email.cc_recipients,
                email.snippet, email.body_html, email.attachments_json,
                email.has_attachments as i32, email.date, email.is_read as i32,
                email.starred as i32, email.mailbox, email.labels, email.internal_ts,
                email.list_unsubscribe
            ],
        ) {
            log::error!("IMAP upsert email {} failed: {}", email.id, e);
        }
    }

    if mailbox == "INBOX" {
        if let Err(e) = crate::smart_inbox::assign_unassigned(&conn, account_id) {
            log::error!("Smart Inbox assignment after IMAP sync failed: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        conn.execute("INSERT INTO accounts (id, email) VALUES (1, 'a@example.com')", []).unwrap();
        conn
    }

    #[test]
    fn notification_text_follows_the_app_language() {
        let one = vec![("".to_string(), "Mia".to_string())];
        let two = vec![("a".to_string(), "x".to_string()), ("b".to_string(), "y".to_string())];
        assert_eq!(notification_text("de", "Work", &one), ("Neue E-Mail – Work".to_string(), "Von: Mia\n(Kein Betreff)".to_string()));
        assert_eq!(notification_text("en", "Work", &one).1, "From: Mia\n(No subject)");
        assert_eq!(notification_text("de", "Work", &two).0, "2 neue E-Mails – Work");
        assert_eq!(notification_text("fr", "Work", &two).0, "2 new emails – Work");
    }

    fn insert(conn: &Connection, id: &str, ts: i64, unsub: &str) {
        conn.execute(
            "INSERT INTO emails (id, account_id, thread_id, subject, sender, mailbox, body_html, date, is_read, internal_ts, list_unsubscribe)
             VALUES (?1, 1, ?1, 'Subject', 'x@y.z', 'INBOX', '', '', 0, ?2, ?3)",
            params![id, ts, unsub],
        ).unwrap();
    }

    fn now() -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
    }

    #[test]
    fn initial_download_is_never_announced() {
        let conn = setup();
        // Gmail stores milliseconds; this used to pass the "last hour" check forever.
        for i in 0..120 {
            insert(&conn, &format!("m{i}"), (now() - 86_400 * i) * 1000, "");
        }
        // Nothing synced successfully yet: no baseline, nothing announced.
        assert!(collect_new_mail(&conn, 1, false).unwrap().is_empty());
        conn.execute("INSERT INTO gmail_sync_state (account_id, history_id) VALUES (1, 'h')", []).unwrap();
        assert!(collect_new_mail(&conn, 1, false).unwrap().is_empty());
        assert!(collect_new_mail(&conn, 1, false).unwrap().is_empty());

        insert(&conn, "fresh", now() * 1000, "");
        insert(&conn, "old-page", (now() - 86_400 * 30) * 1000, "");
        let fresh = collect_new_mail(&conn, 1, false).unwrap();
        assert_eq!(fresh.len(), 1);
        assert!(collect_new_mail(&conn, 1, false).unwrap().is_empty());
    }

    #[test]
    fn important_only_skips_bulk_mail() {
        let conn = setup();
        conn.execute("INSERT INTO mailbox_sync_state (account_id, mailbox_name) VALUES (1, 'INBOX')", []).unwrap();
        assert!(collect_new_mail(&conn, 1, true).unwrap().is_empty());
        insert(&conn, "person", now(), "");
        insert(&conn, "promo", now(), "<https://unsubscribe>");
        assert_eq!(collect_new_mail(&conn, 1, true).unwrap().len(), 1);
        assert!(collect_new_mail(&conn, 1, false).unwrap().is_empty());
    }
}
