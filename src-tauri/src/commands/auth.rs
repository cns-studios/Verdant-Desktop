use tauri::State;

use crate::auth;
use crate::db::get_token;
use crate::state::DbState;

#[derive(serde::Serialize)]
pub struct AuthStatus {
    pub has_client_id: bool,
    pub connected: bool,
}

#[derive(serde::Serialize)]
pub struct UserProfile {
    pub name: String,
    pub email: String,
    pub initials: String,
}

#[tauri::command]
pub async fn connect_gmail(state: State<'_, DbState>) -> Result<(), String> {
    let fresh = auth::login_interactive().await?;
    let _ = crate::state::persist_token(&state, fresh).await?;
    Ok(())
}

#[tauri::command]
pub async fn auth_status(state: State<'_, DbState>) -> Result<AuthStatus, String> {
    let has_client_id = auth::has_google_client_id_configured();

    let connected = {
        if state.token.lock().await.is_some() {
            true
        } else {
            let conn = state.conn.lock().await;
            get_token(&conn).map_err(|e| e.to_string())?.is_some()
        }
    };

    Ok(AuthStatus {
        has_client_id,
        connected,
    })
}

#[tauri::command]
pub async fn logout(state: State<'_, DbState>) -> Result<(), String> {
    {
        let mut cache = state.token.lock().await;
        *cache = None;
    }
    let conn = state.conn.lock().await;
    crate::db::clear_tokens(&conn).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_user_profile(state: State<'_, DbState>) -> Result<UserProfile, String> {
    let token = crate::state::ensure_token(&state).await?.access_token;
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
