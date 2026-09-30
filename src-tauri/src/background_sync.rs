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

    emit_notifications_and_event_gmail(app.clone(), state, account).await;
    use tauri::Emitter;
    let _ = app.emit("emails-synced", ());
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

async fn emit_notifications_and_event_gmail(app: tauri::AppHandle, state: &DbState, account: &Account) {
    let account_id = account.id;

    let unread_emails = {
        let conn = state.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, subject, sender FROM emails WHERE account_id=?1 AND mailbox='INBOX' AND is_read=0 AND notified=0 AND internal_ts > (strftime('%s','now') - 3600) LIMIT 50"
        ).unwrap();
        let rows = stmt.query_map([account_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        }).unwrap();
        rows.filter_map(Result::ok).collect::<Vec<_>>()
    };

    if !unread_emails.is_empty() {
        use tauri_plugin_notification::NotificationExt;
        let count = unread_emails.len();
        let acc_name = account.display_name.as_deref().unwrap_or(&account.email);

        let title = if count == 1 {
            format!("New Email - {}", acc_name)
        } else {
            format!("{} New Emails - {}", count, acc_name)
        };

        let body = if count == 1 {
            let (_, subj, send) = &unread_emails[0];
            format!("From: {}\n{}", send, subj)
        } else {
            format!("You have {} new messages in your inbox.", count)
        };

        let _ = app.notification().builder()
            .title(title)
            .body(body)
            .show();

        let conn = state.conn.lock().await;
        for (id, _, _) in unread_emails {
            let _ = conn.execute(
                "UPDATE emails SET notified=1 WHERE id=?1 AND account_id=?2",
                rusqlite::params![id, account_id],
            );
        }
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

    emit_notifications_and_event(app.clone(), state, account, had_new_emails).await;
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

async fn emit_notifications_and_event(app: tauri::AppHandle, state: &DbState, account: &Account, had_new: bool) {
    let account_id = account.id;

    if had_new {
        let unread_emails = {
            let conn = state.conn.lock().await;
            let mut stmt = conn.prepare(
                "SELECT id, subject, sender FROM emails WHERE account_id=?1 AND mailbox='INBOX' AND is_read=0 AND notified=0 AND internal_ts > (strftime('%s','now') - 3600) LIMIT 50"
            ).unwrap();
            let rows = stmt.query_map([account_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            }).unwrap();
            rows.filter_map(Result::ok).collect::<Vec<_>>()
        };

        if !unread_emails.is_empty() {
        use tauri_plugin_notification::NotificationExt;
        let count = unread_emails.len();
        let acc_name = account.display_name.as_deref().unwrap_or(&account.email);

        let title = if count == 1 {
            format!("New Email - {}", acc_name)
        } else {
            format!("{} New Emails - {}", count, acc_name)
        };

        let body = if count == 1 {
            let (_, subj, send) = &unread_emails[0];
            format!("From: {}\n{}", send, subj)
        } else {
            format!("You have {} new messages in your inbox.", count)
        };

        let _ = app.notification().builder()
            .title(title)
            .body(body)
            .show();

            let conn = state.conn.lock().await;
            for (id, _, _) in unread_emails {
                let _ = conn.execute(
                    "UPDATE emails SET notified=1 WHERE id=?1 AND account_id=?2",
                    rusqlite::params![id, account_id],
                );
            }
        }
    }

    use tauri::Emitter;
    let _ = app.emit("emails-synced", ());
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
