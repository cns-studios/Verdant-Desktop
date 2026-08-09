use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::auth;
use crate::db::{get_account_by_id, update_gmail_token, StoredToken};

pub struct DbState {
    pub conn: Mutex<Connection>,
    
    pub tokens: Mutex<HashMap<i64, StoredToken>>,
    
    pub active_account_id: Mutex<i64>,
    
    pub sync_handles: Mutex<HashMap<i64, tokio::sync::oneshot::Sender<()>>>,
    
    pub rate_limited_until: Mutex<HashMap<i64, Instant>>,
}

pub type SharedState = Arc<DbState>;

pub fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub async fn get_active_id(state: &DbState) -> i64 {
    *state.active_account_id.lock().await
}

pub fn mark_rate_limited(state: &DbState, account_id: i64, retry_after: Duration) {
    let until = Instant::now() + retry_after;
    if let Ok(mut map) = state.rate_limited_until.try_lock() {
        map.insert(account_id, until);
    }
}

pub fn rate_limited_remain(state: &DbState, account_id: i64) -> Option<Duration> {
    let Ok(map) = state.rate_limited_until.try_lock() else {
        return None;
    };
    match map.get(&account_id) {
        Some(until) if *until > Instant::now() => Some(*until - Instant::now()),
        _ => None,
    }
}

pub fn clear_rate_limited(state: &DbState, account_id: i64) {
    if let Ok(mut map) = state.rate_limited_until.try_lock() {
        map.remove(&account_id);
    }
}



pub async fn ensure_token_for(state: &DbState, account_id: i64) -> Result<StoredToken, String> {
    
    {
        let tokens = state.tokens.lock().await;
        if let Some(cached) = tokens.get(&account_id) {
            let valid = cached
                .expires_at_epoch
                .map(|exp| exp > now_epoch() + 60)
                .unwrap_or(true);
            if valid {
                return Ok(cached.clone());
            }
        }
    }

    
    let account = {
        let conn = state.conn.lock().await;
        get_account_by_id(&conn, account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Account {} not found", account_id))?
    };

    if account.provider != "gmail" {
        return Err("ensure_token_for called on non-Gmail account".to_string());
    }

    let db_token = account.access_token.map(|at| StoredToken {
        access_token: at,
        refresh_token: account.refresh_token.clone(),
        expires_at_epoch: account.expires_at_epoch,
    });

    if let Some(token) = db_token {
        let valid = token
            .expires_at_epoch
            .map(|exp| exp > now_epoch() + 60)
            .unwrap_or(true);

        if valid {
            let mut tokens = state.tokens.lock().await;
            tokens.insert(account_id, token.clone());
            return Ok(token);
        }

        
        if let Some(refresh) = &account.refresh_token {
            let refreshed = auth::refresh_access_token(refresh).await?;
            {
                let conn = state.conn.lock().await;
                update_gmail_token(&conn, account_id, &refreshed).map_err(|e| e.to_string())?;
            }
            {
                let mut tokens = state.tokens.lock().await;
                tokens.insert(account_id, refreshed.clone());
            }
            return Ok(refreshed);
        }
    }

    
    let fresh = auth::login_interactive().await?;
    {
        let conn = state.conn.lock().await;
        update_gmail_token(&conn, account_id, &fresh).map_err(|e| e.to_string())?;
    }
    {
        let mut tokens = state.tokens.lock().await;
        tokens.insert(account_id, fresh.clone());
    }
    Ok(fresh)
}


pub async fn ensure_token(state: &DbState) -> Result<StoredToken, String> {
    let id = get_active_id(state).await;
    if id == 0 {
        return Err("No active account".to_string());
    }
    ensure_token_for(state, id).await
}

pub async fn persist_token_for(
    state: &DbState,
    account_id: i64,
    token: StoredToken,
) -> Result<StoredToken, String> {
    {
        let conn = state.conn.lock().await;
        update_gmail_token(&conn, account_id, &token).map_err(|e| e.to_string())?;
    }
    {
        let mut tokens = state.tokens.lock().await;
        tokens.insert(account_id, token.clone());
    }
    Ok(token)
}


pub async fn persist_token(state: &DbState, token: StoredToken) -> Result<StoredToken, String> {
    let id = get_active_id(state).await;
    persist_token_for(state, id, token).await
}
