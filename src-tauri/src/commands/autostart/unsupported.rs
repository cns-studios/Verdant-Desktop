pub fn enable(_app: &tauri::AppHandle) -> Result<(), String> {
    Err("Autostart is not supported on this platform".to_string())
}

pub fn disable(_app: &tauri::AppHandle) -> Result<(), String> {
    Err("Autostart is not supported on this platform".to_string())
}

pub fn is_enabled(_app: &tauri::AppHandle) -> Result<bool, String> {
    Ok(false)
}
