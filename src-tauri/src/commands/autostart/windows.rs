use winreg::enums::*;
use winreg::RegKey;

use super::target_exe;

const APP_KEY: &str = "Verdant-Desktop";

pub fn enable(app: &tauri::AppHandle) -> Result<(), String> {
    let exe = target_exe(app)?;
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE,
        )
        .map_err(|e| format!("Failed to open registry key: {e}"))?;

    key.set_value(APP_KEY, &format!(r#""{}" --autostart"#, exe))
        .map_err(|e| format!("Failed to set registry value: {e}"))?;

    Ok(())
}

pub fn disable(_app: &tauri::AppHandle) -> Result<(), String> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE,
        )
        .map_err(|e| format!("Failed to open registry key: {e}"))?;

    let _ = key.delete_value(APP_KEY);
    Ok(())
}

pub fn is_enabled(_app: &tauri::AppHandle) -> Result<bool, String> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            KEY_READ,
        )
        .map_err(|e| format!("Failed to open registry key: {e}"))?;

    Ok(key.get_value::<String, _>(APP_KEY).is_ok())
}
