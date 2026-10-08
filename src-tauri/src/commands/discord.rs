use std::sync::Mutex;

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};

static CLIENT: Mutex<Option<DiscordIpcClient>> = Mutex::new(None);

fn read_non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
    })
}

fn configured_discord_client_id() -> Option<String> {
    read_non_empty(std::env::var("DISCORD_CLIENT_ID").ok()).or_else(|| {
        read_non_empty(option_env!("DISCORD_CLIENT_ID").map(|v| v.to_string()))
    })
}

fn publish(
    client: &mut DiscordIpcClient,
    details: &str,
    state: Option<&str>,
    started_at: i64,
) -> Result<(), String> {
    let mut payload = activity::Activity::new()
        .details(details)
        .timestamps(activity::Timestamps::new().start(started_at));
    if let Some(state) = state.filter(|value| !value.is_empty()) {
        payload = payload.state(state);
    }
    client.set_activity(payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn discord_set_activity(
    details: String,
    state: Option<String>,
    started_at: i64,
) -> Result<(), String> {
    let id = configured_discord_client_id().ok_or_else(|| {
        log::warn!("Discord presence skipped: DISCORD_CLIENT_ID is not configured");
        "Missing DISCORD_CLIENT_ID".to_string()
    })?;
    let mut guard = CLIENT.lock().map_err(|e| e.to_string())?;

    if let Some(client) = guard.as_mut() {
        match publish(client, &details, state.as_deref(), started_at) {
            Ok(()) => return Ok(()),
            Err(e) => log::info!("Discord connection dropped, reconnecting: {e}"),
        }
        let _ = client.close();
        *guard = None;
    }

    let mut client = DiscordIpcClient::new(&id);
    client.connect().map_err(|e| {
        log::warn!("Discord IPC connect failed: {e}");
        e.to_string()
    })?;
    publish(&mut client, &details, state.as_deref(), started_at).map_err(|e| {
        log::warn!("Discord presence update failed: {e}");
        e
    })?;
    log::info!("Discord presence connected");
    *guard = Some(client);
    Ok(())
}

#[tauri::command]
pub fn discord_clear_activity() -> Result<(), String> {
    let mut guard = CLIENT.lock().map_err(|e| e.to_string())?;
    if let Some(mut client) = guard.take() {
        let _ = client.clear_activity();
        let _ = client.close();
    }
    Ok(())
}
