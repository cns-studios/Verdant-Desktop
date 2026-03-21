use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::auth;
use crate::db::{get_token, upsert_token, StoredToken};

pub struct DbState {
    pub conn: Mutex<Connection>,
    pub token: Mutex<Option<StoredToken>>,
}

pub fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub async fn persist_token(state: &DbState, token: StoredToken) -> Result<StoredToken, String> {
    {
        let conn = state.conn.lock().await;
        upsert_token(&conn, &token).map_err(|e| e.to_string())?;
    }
    {
        let mut cache = state.token.lock().await;
        *cache = Some(token.clone());
    }
    Ok(token)
}

pub async fn ensure_token(state: &DbState) -> Result<StoredToken, String> {
    if let Some(cached) = state.token.lock().await.clone() {
        let valid = cached
            .expires_at_epoch
            .map(|exp| exp > now_epoch() + 60)
            .unwrap_or(true);
        if valid {
            return Ok(cached);
        }
    }

    let from_db = {
        let conn = state.conn.lock().await;
        get_token(&conn).map_err(|e| e.to_string())?
    };

    if let Some(db_token) = from_db {
        let valid = db_token
            .expires_at_epoch
            .map(|exp| exp > now_epoch() + 60)
            .unwrap_or(true);
        if valid {
            let mut cache = state.token.lock().await;
            *cache = Some(db_token.clone());
            return Ok(db_token);
        }

        if let Some(refresh) = db_token.refresh_token.clone() {
            let refreshed = auth::refresh_access_token(&refresh).await?;
            return persist_token(state, refreshed).await;
        }
    }

    let fresh = auth::login_interactive().await?;
    persist_token(state, fresh).await
}
