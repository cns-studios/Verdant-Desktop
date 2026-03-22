use std::sync::Arc;
use tauri::State;

use crate::auth;
use crate::background_sync::{start_account_sync, stop_account_sync};
use crate::crypto::encrypt_password;
use crate::db::{
    delete_account, get_all_accounts, get_account_by_id, insert_imap_account,
    set_active_account, upsert_gmail_account, AccountPublic,
};
use crate::imap_sync::test_imap_connection;
use crate::state::{DbState, get_active_id};



#[tauri::command]
pub async fn list_accounts(state: State<'_, Arc<DbState>>) -> Result<Vec<AccountPublic>, String> {
    let conn = state.conn.lock().await;
    let accounts = get_all_accounts(&conn).map_err(|e| e.to_string())?;
    Ok(accounts.into_iter().map(AccountPublic::from).collect())
}

#[tauri::command]
pub async fn switch_account(
    state: State<'_, Arc<DbState>>,
    account_id: i64,
) -> Result<(), String> {
    {
        let conn = state.conn.lock().await;
        set_active_account(&conn, account_id).map_err(|e| e.to_string())?;
    }
    {
        let mut active = state.active_account_id.lock().await;
        *active = account_id;
    }
    Ok(())
}

#[tauri::command]
pub async fn remove_account(
    state: State<'_, Arc<DbState>>,
    account_id: i64,
) -> Result<(), String> {
    stop_account_sync(&state, account_id).await;

    let active_id = get_active_id(&state).await;

    {
        let conn = state.conn.lock().await;
        delete_account(&conn, account_id).map_err(|e| e.to_string())?;
    }

    
    if active_id == account_id {
        let conn = state.conn.lock().await;
        let accounts = get_all_accounts(&conn).map_err(|e| e.to_string())?;
        if let Some(first) = accounts.first() {
            set_active_account(&conn, first.id).map_err(|e| e.to_string())?;
            let mut aid = state.active_account_id.lock().await;
            *aid = first.id;
        } else {
            let mut aid = state.active_account_id.lock().await;
            *aid = 0;
        }
    }

    Ok(())
}




#[tauri::command]
pub async fn add_gmail_account(
    state: State<'_, Arc<DbState>>,
) -> Result<AccountPublic, String> {
    let token = auth::login_interactive().await?;

    
    let client = reqwest::Client::new();
    let profile = client
        .get("https://gmail.googleapis.com/gmail/v1/users/me/profile")
        .bearer_auth(&token.access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;

    let email = profile
        .get("emailAddress")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown@gmail.com")
        .to_string();

    let account_id = {
        let conn = state.conn.lock().await;
        let id = upsert_gmail_account(&conn, &email, &token).map_err(|e| e.to_string())?;
        
        let current_active = *state.active_account_id.lock().await;
        if current_active == 0 {
            set_active_account(&conn, id).map_err(|e| e.to_string())?;
        }
        id
    };

    {
        let current_active = *state.active_account_id.lock().await;
        if current_active == 0 {
            let mut aid = state.active_account_id.lock().await;
            *aid = account_id;
        }
    }

    
    {
        let mut tokens = state.tokens.lock().await;
        tokens.insert(account_id, token);
    }

    
    let account = {
        let conn = state.conn.lock().await;
        get_account_by_id(&conn, account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Account not found after insert".to_string())?
    };

    start_account_sync((*state).clone().into(), account).await;

    let conn = state.conn.lock().await;
    let acc = get_account_by_id(&conn, account_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Account not found".to_string())?;
    Ok(AccountPublic::from(acc))
}


#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImapAccountPayload {
    pub email: String,
    pub display_name: Option<String>,
    pub imap_host: String,
    pub imap_port: i64,
    pub smtp_host: String,
    pub smtp_port: i64,
    pub username: String,
    pub password: String,
}


#[tauri::command]
pub async fn test_imap_credentials(payload: ImapAccountPayload) -> Result<String, String> {
    let host = payload.imap_host.clone();
    let port = payload.imap_port as u16;
    let username = payload.username.clone();
    let password = payload.password.clone();

    tokio::task::spawn_blocking(move || {
        test_imap_connection(&host, port, &username, &password)
    })
    .await
    .map_err(|e| e.to_string())?
}


#[tauri::command]
pub async fn add_imap_account(
    state: State<'_, Arc<DbState>>,
    payload: ImapAccountPayload,
) -> Result<AccountPublic, String> {
    let encrypted_password = encrypt_password(&payload.password)
        .map_err(|e| format!("Failed to encrypt password: {}", e))?;

    let account_id = {
        let conn = state.conn.lock().await;
        let id = insert_imap_account(
            &conn,
            &payload.email,
            payload.display_name.as_deref(),
            &payload.imap_host,
            payload.imap_port,
            &payload.smtp_host,
            payload.smtp_port,
            &payload.username,
            &encrypted_password,
        )
        .map_err(|e| e.to_string())?;

        let current_active = *state.active_account_id.lock().await;
        if current_active == 0 {
            set_active_account(&conn, id).map_err(|e| e.to_string())?;
        }
        id
    };

    {
        let current_active = *state.active_account_id.lock().await;
        if current_active == 0 {
            let mut aid = state.active_account_id.lock().await;
            *aid = account_id;
        }
    }

    let account = {
        let conn = state.conn.lock().await;
        get_account_by_id(&conn, account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Account not found after insert".to_string())?
    };

    
    start_account_sync((*state).clone().into(), account.clone()).await;

    Ok(AccountPublic::from(account))
}



#[derive(serde::Deserialize)]
pub struct GmxAccountPayload {
    pub email: String,
    pub display_name: Option<String>,
    pub password: String,
}

#[tauri::command]
pub async fn add_gmx_account(
    state: State<'_, Arc<DbState>>,
    payload: GmxAccountPayload,
) -> Result<AccountPublic, String> {
    let imap_payload = ImapAccountPayload {
        email: payload.email.clone(),
        display_name: payload.display_name,
        imap_host: "imap.gmx.com".to_string(),
        imap_port: 993,
        smtp_host: "mail.gmx.com".to_string(),
        smtp_port: 587,
        username: payload.email,
        password: payload.password,
    };
    
    add_imap_account(state, imap_payload).await
}



#[derive(serde::Serialize)]
pub struct ActiveAccountInfo {
    pub id: i64,
    pub email: String,
    pub provider: String,
    pub display_name: Option<String>,
}

#[tauri::command]
pub async fn get_active_account_info(
    state: State<'_, Arc<DbState>>,
) -> Result<Option<ActiveAccountInfo>, String> {
    let active_id = get_active_id(&state).await;
    if active_id == 0 {
        return Ok(None);
    }
    let conn = state.conn.lock().await;
    let account = get_account_by_id(&conn, active_id).map_err(|e| e.to_string())?;
    Ok(account.map(|a| ActiveAccountInfo {
        id: a.id,
        email: a.email,
        provider: a.provider,
        display_name: a.display_name,
    }))
}
