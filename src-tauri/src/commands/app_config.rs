use tokio::sync::Mutex;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct AppConfig {
    pub run_in_background: bool,
}

pub struct AppConfigState(pub Mutex<AppConfig>);

#[tauri::command]
pub async fn update_app_config(
    config: AppConfig,
    state: tauri::State<'_, AppConfigState>,
) -> Result<(), String> {
    let mut s = state.0.lock().await;
    *s = config;
    Ok(())
}
