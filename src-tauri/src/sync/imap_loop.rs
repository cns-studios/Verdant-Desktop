use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use super::notify::notify_new_mail;
use super::{apply_sync_result, upsert_emails};
use crate::db::Account;
use crate::imap_client::{connect_with_timeout, ImapCredentials};
use crate::state::DbState;

pub(crate) const IMAP_MIN_SYNC_GAP_SECS: u64 = 15;

pub(crate) const IMAP_POLL_INTERVAL_SECS: u64 = 120;

pub(crate) const IMAP_MAILBOXES: &[&str] = &["INBOX", "SENT", "DRAFT", "TRASH"];

pub(crate) const IDLE_TIMEOUT_SECS: u64 = 60 * 5;

pub(crate) async fn run_imap_sync_loop(
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
        tokio::select! {
            _ = wait_for_imap_change(&acc) => {
                sync_imap_account(&app, &state, &acc).await;
            }
            _ = &mut shutdown => break,
        }
    }
}

pub(crate) async fn wait_for_imap_change(account: &Account) {
    let started = Instant::now();
    let idle_ok = try_imap_idle(account, Duration::from_secs(IDLE_TIMEOUT_SECS)).await;
    let min_wait = Duration::from_secs(if idle_ok { IMAP_MIN_SYNC_GAP_SECS } else { IMAP_POLL_INTERVAL_SECS });
    let elapsed = started.elapsed();
    if elapsed < min_wait {
        tokio::time::sleep(min_wait - elapsed).await;
    }
}

pub(crate) async fn try_imap_idle(account: &Account, timeout: Duration) -> bool {
    let creds = match ImapCredentials::from_account(account) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut session = connect_with_timeout(&creds, 8)?;
        let folders = crate::imap_client::list_folders(&mut session)?;
        let inbox = crate::imap_client::imap_folder_for_mailbox("INBOX", &folders)
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

pub(crate) async fn sync_imap_account(app: &tauri::AppHandle, state: &DbState, account: &Account) {
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
        crate::imap_client::sync_imap_mailboxes(&acc, &stored)
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

pub(crate) async fn fallback_sync(state: &DbState, account: &Account, mailbox: &str) {
    use crate::imap_client::sync_imap_mailbox;
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
