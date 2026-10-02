use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot;

use super::notify::notify_new_mail;
use super::sync_mailbox_internal_for;
use crate::db::Account;
use crate::gmail::sync_mailbox_page_internal_for;
use crate::state::DbState;

pub(crate) const GMAIL_SYNC_INTERVAL_SECS: u64 = 120;

pub(crate) const GMAIL_STAGGER_CYCLE: u32 = 4;

pub(crate) async fn run_gmail_sync_loop(
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

pub(crate) async fn sync_gmail_account(app: &tauri::AppHandle, state: &DbState, account: &Account, cycle: u32) {
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
            if let Err(e) = sync_mailbox_page_internal_for(
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
