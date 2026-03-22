use std::sync::Arc;
use tauri::State;

use crate::db::{get_account_by_id, get_all_accounts, set_active_account};
use crate::state::{DbState, ensure_token, get_active_id};

#[derive(serde::Serialize)]
pub struct AuthStatus {
    pub has_client_id: bool,
    pub connected: bool,
    pub active_account_id: i64,
}

#[derive(serde::Serialize)]
pub struct UserProfile {
    pub name: String,
    pub email: String,
    pub initials: String,
}


#[tauri::command]
pub async fn connect_gmail(state: State<'_, Arc<DbState>>) -> Result<(), String> {
    use crate::commands::accounts::add_gmail_account;
    add_gmail_account(state).await.map(|_| ())
}

#[tauri::command]
pub async fn auth_status(state: State<'_, Arc<DbState>>) -> Result<AuthStatus, String> {
    let has_client_id = crate::auth::has_google_client_id_configured();
    let active_id = get_active_id(&state).await;

    let connected = if active_id == 0 {
        
        let conn = state.conn.lock().await;
        !get_all_accounts(&conn).unwrap_or_default().is_empty()
    } else {
        true
    };

    
    if active_id == 0 && connected {
        let conn = state.conn.lock().await;
        if let Some(first) = get_all_accounts(&conn).unwrap_or_default().into_iter().next() {
            let _ = set_active_account(&conn, first.id);
            drop(conn);
            let mut aid = state.active_account_id.lock().await;
            *aid = first.id;
        }
    }

    let active = get_active_id(&state).await;
    Ok(AuthStatus {
        has_client_id,
        connected,
        active_account_id: active,
    })
}

#[tauri::command]
pub async fn logout(state: State<'_, Arc<DbState>>) -> Result<(), String> {
    let active_id = get_active_id(&state).await;

    
    {
        let mut tokens = state.tokens.lock().await;
        tokens.remove(&active_id);
    }

    
    {
        let conn = state.conn.lock().await;
        crate::db::delete_account(&conn, active_id).map_err(|e| e.to_string())?;

        
        let remaining = get_all_accounts(&conn).unwrap_or_default();
        if let Some(next) = remaining.first() {
            set_active_account(&conn, next.id).map_err(|e| e.to_string())?;
            drop(conn);
            let mut aid = state.active_account_id.lock().await;
            *aid = next.id;
        } else {
            drop(conn);
            let mut aid = state.active_account_id.lock().await;
            *aid = 0;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn get_user_profile(state: State<'_, Arc<DbState>>) -> Result<UserProfile, String> {
    let active_id = get_active_id(&state).await;

    if active_id == 0 {
        return Err("No active account".to_string());
    }

    let account = {
        let conn = state.conn.lock().await;
        get_account_by_id(&conn, active_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Active account not found".to_string())?
    };

    
    if account.provider == "imap" {
        let name = account.display_name.clone()
            .unwrap_or_else(|| account.email.split('@').next().unwrap_or("User").replace('.', " "));
        let initials = name
            .split_whitespace()
            .take(2)
            .filter_map(|p| p.chars().next())
            .collect::<String>()
            .to_uppercase();
        return Ok(UserProfile {
            name,
            email: account.email,
            initials: if initials.is_empty() { "U".to_string() } else { initials },
        });
    }

    
    let token = ensure_token(&state).await?.access_token;
    let client = reqwest::Client::new();

    let res = client
        .get("https://gmail.googleapis.com/gmail/v1/users/me/profile")
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Profile request failed: {}", res.status()));
    }

    let body = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    let email = body
        .get("emailAddress")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown@example.com")
        .to_string();

    let name = email.split('@').next().unwrap_or("User").replace('.', " ");
    let initials = name
        .split_whitespace()
        .take(2)
        .filter_map(|p| p.chars().next())
        .collect::<String>()
        .to_uppercase();

    Ok(UserProfile {
        name,
        email,
        initials: if initials.is_empty() { "U".to_string() } else { initials },
    })
}
