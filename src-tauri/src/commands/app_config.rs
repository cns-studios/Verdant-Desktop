use std::fs;
use std::path::PathBuf;

use tauri::Manager;
use tokio::sync::Mutex;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct AppConfig {
    pub run_in_background: bool,
    pub update_channel: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            run_in_background: true,
            update_channel: "stable".to_string(),
        }
    }
}

#[derive(serde::Deserialize)]
pub struct AppConfigPatch {
    pub run_in_background: Option<bool>,
    pub update_channel: Option<String>,
}

fn normalize_update_channel(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "nightly" | "beta" => "nightly".to_string(),
        _ => "stable".to_string(),
    }
}

fn app_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    fs::create_dir_all(&data_dir).map_err(|e| format!("Failed to create app data dir: {e}"))?;
    Ok(data_dir.join("app-config.json"))
}

fn persist_app_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = app_config_path(app)?;
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize app config: {e}"))?;
    fs::write(path, json).map_err(|e| format!("Failed to save app config: {e}"))
}

pub fn load_app_config(app: &tauri::AppHandle) -> AppConfig {
    let path = match app_config_path(app) {
        Ok(path) => path,
        Err(_) => return AppConfig::default(),
    };

    let loaded = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<AppConfig>(&raw).ok());

    let mut config = loaded.unwrap_or_default();
    config.update_channel = normalize_update_channel(&config.update_channel);
    config
}

pub struct AppConfigState(pub Mutex<AppConfig>);

#[tauri::command]
pub async fn update_app_config(
    config: AppConfigPatch,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppConfigState>,
) -> Result<(), String> {
    let mut s = state.0.lock().await;
    if let Some(run_in_background) = config.run_in_background {
        s.run_in_background = run_in_background;
    }
    if let Some(update_channel) = config.update_channel {
        s.update_channel = normalize_update_channel(&update_channel);
    }

    persist_app_config(&app, &s)?;
    Ok(())
}

#[tauri::command]
pub async fn get_app_config(state: tauri::State<'_, AppConfigState>) -> Result<AppConfig, String> {
    let s = state.0.lock().await;
    Ok(s.clone())
}
