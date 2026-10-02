mod dispatch;
mod gmail_loop;
mod imap_loop;
mod notify;
mod store;

use std::sync::Arc;

use tokio::sync::oneshot;

use crate::db::{get_all_accounts, Account};
use crate::state::DbState;
use gmail_loop::run_gmail_sync_loop;
use imap_loop::run_imap_sync_loop;

pub use dispatch::*;
pub use store::*;

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
