use super::{apply_sync_result, upsert_emails};
use crate::db::{get_account_by_id, Account};
use crate::gmail::sync_gmail_single_mailbox;
use crate::state::DbState;

pub async fn sync_imap_mailbox_internal_for(state: &DbState, account: &Account, mailbox: &str) -> Result<(), String> {
    let account_id = account.id;
    let acc = account.clone();
    let mb = mailbox.to_string();
    let mb_for_fallback = mb.clone();

    let stored_state = {
        let conn = state.conn.lock().await;
        crate::db::get_mailbox_sync_state(&conn, account_id, &mb)
            .ok().flatten()
    };
    let (stored_uidvalidity, stored_highest_uid) = stored_state
        .map(|s| (Some(s.uidvalidity), Some(s.highest_uid)))
        .unwrap_or((None, None));

    let result = tokio::task::spawn_blocking(move || {
        crate::imap_client::sync_imap_mailbox_incremental(&acc, &mb, stored_uidvalidity, stored_highest_uid)
    }).await.map_err(|e| format!("IMAP task error: {}", e))?;

    match result {
        Ok(sync_result) => {
            apply_sync_result(state, account_id, sync_result, &mb_for_fallback).await;
            Ok(())
        }
        Err(e) => {
            log::error!("IMAP sync error account={} mailbox={}: {}", account_id, mailbox, e);
            let acc2 = account.clone();
            let mb2 = mailbox.to_string();
            let mb2_fb = mb2.clone();
            let fallback = tokio::task::spawn_blocking(move || {
                crate::imap_client::sync_imap_mailbox(&acc2, &mb2, 50)
            }).await.map_err(|e| format!("IMAP fallback task error: {}", e))?;
            if let Ok(emails) = fallback {
                upsert_emails(state, account_id, emails, &mb2_fb).await;
            }
            Ok(())
        }
    }
}

pub async fn sync_mailbox_internal_for(state: &DbState, account_id: i64, mailbox: &str) -> Result<(), String> {
    let account = {
        let conn = state.conn.lock().await;
        get_account_by_id(&conn, account_id)
            .ok()
            .flatten()
    };
    if let Some(ref acc) = account {
        if acc.provider == "imap" {
            return sync_imap_mailbox_internal_for(state, acc, mailbox).await;
        }
    }

    sync_gmail_single_mailbox(state, account_id, mailbox).await
}
