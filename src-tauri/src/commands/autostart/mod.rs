#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
#[path = "unsupported.rs"]
mod platform;

#[cfg(target_os = "linux")]
use tauri::Manager;

#[tauri::command]
pub async fn autostart_enable(app: tauri::AppHandle) -> Result<(), String> {
    platform::enable(&app)
}

#[tauri::command]
pub async fn autostart_disable(app: tauri::AppHandle) -> Result<(), String> {
    platform::disable(&app)
}

#[tauri::command]
pub async fn autostart_is_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    platform::is_enabled(&app)
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn target_exe(app: &tauri::AppHandle) -> Result<String, String> {
    #[cfg(target_os = "linux")]
    if let Some(appimage) = app
        .env()
        .appimage
        .and_then(|p| p.to_str().map(|s| s.to_string()))
    {
        return Ok(appimage);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;

    std::env::current_exe()
        .map(|p| p.display().to_string())
        .map_err(|e| format!("Failed to resolve binary path: {e}"))
}
