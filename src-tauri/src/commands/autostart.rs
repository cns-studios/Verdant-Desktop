use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[tauri::command]
pub async fn autostart_enable(app: tauri::AppHandle) -> Result<(), String> {
    enable_impl(&app)
}

#[tauri::command]
pub async fn autostart_disable(app: tauri::AppHandle) -> Result<(), String> {
    disable_impl(&app)
}

#[tauri::command]
pub async fn autostart_is_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    is_enabled_impl(&app)
}


fn target_exe(app: &tauri::AppHandle) -> Result<String, String> {
    #[cfg(target_os = "linux")]
    if let Some(appimage) = app
        .env()
        .appimage
        .and_then(|p| p.to_str().map(|s| s.to_string()))
    {
        return Ok(appimage);
    }

    std::env::current_exe()
        .map(|p| p.display().to_string())
        .map_err(|e| format!("Failed to resolve binary path: {e}"))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn app_key() -> &'static str {
    "Verdant-Desktop"
}


#[cfg(target_os = "linux")]
fn enable_impl(app: &tauri::AppHandle) -> Result<(), String> {
    let exe = target_exe(app)?;
    let dir = systemd_user_dir()?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create systemd user dir: {e}"))?;

    let path = service_path()?;
    let content = format!(
        "[Unit]\n\
        Description=Verdant Desktop\n\
        After=graphical-session.target\n\
        PartOf=graphical-session.target\n\
        \n\
        [Service]\n\
        ExecStart={} --autostart\n\
        Restart=on-failure\n\
        RestartSec=5\n\
        \n\
        [Install]\n\
        WantedBy=default.target",
        exe
    );

    fs::write(&path, &content).map_err(|e| format!("Failed to write service file: {e}"))?;

    cleanup_old_desktop_file();

    run_systemctl(&["--user", "daemon-reload"])?;
    run_systemctl(&["--user", "enable", "verdant-desktop.service"])?;
    run_systemctl(&["--user", "start", "--no-block", "verdant-desktop.service"])?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn disable_impl(_app: &tauri::AppHandle) -> Result<(), String> {
    let _ = run_systemctl(&["--user", "disable", "--now", "verdant-desktop.service"]);

    let path = service_path()?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to remove service file: {e}"))?;
    }

    run_systemctl(&["--user", "daemon-reload"])?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn is_enabled_impl(_app: &tauri::AppHandle) -> Result<bool, String> {
    match run_systemctl(&["--user", "is-enabled", "verdant-desktop.service"]) {
        Ok(out) => Ok(out.trim() == "enabled"),
        Err(_) => Ok(service_path().map_or(false, |p| p.exists())),
    }
}

#[cfg(target_os = "linux")]
fn systemd_user_dir() -> Result<PathBuf, String> {
    if let Ok(config) = std::env::var("XDG_CONFIG_HOME") {
        Ok(PathBuf::from(config).join("systemd").join("user"))
    } else if let Ok(home) = std::env::var("HOME") {
        Ok(PathBuf::from(home).join(".config").join("systemd").join("user"))
    } else {
        Err("Cannot determine home directory; $HOME is not set".to_string())
    }
}

#[cfg(target_os = "linux")]
fn service_path() -> Result<PathBuf, String> {
    Ok(systemd_user_dir()?.join("verdant-desktop.service"))
}

#[cfg(target_os = "linux")]
fn cleanup_old_desktop_file() {
    let old_dir = if let Ok(config) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(config).join("autostart")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("autostart")
    } else {
        return;
    };
    let old_path = old_dir.join("Verdant-Desktop.desktop");
    if old_path.exists() {
        let _ = fs::remove_file(&old_path);
    }
}

#[cfg(target_os = "linux")]
fn run_systemctl(args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run systemctl: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("systemctl {} failed: {}", args.join(" "), stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}



#[cfg(target_os = "macos")]
fn enable_impl(app: &tauri::AppHandle) -> Result<(), String> {
    let dir = launch_agents_dir()?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create LaunchAgents dir: {e}"))?;

    let exe = target_exe(app)?;
    let label = app_key();
    let path = dir.join(format!("{}.plist", label));

    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>--autostart</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#
    );

    fs::write(&path, &content).map_err(|e| format!("Failed to write plist: {e}"))?;

    let out = std::process::Command::new("launchctl")
        .args(["load", path.to_str().unwrap()])
        .output()
        .map_err(|e| format!("Failed to run launchctl: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("launchctl load failed: {stderr}"));
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn disable_impl(_app: &tauri::AppHandle) -> Result<(), String> {
    let dir = launch_agents_dir()?;
    let path = dir.join(format!("{}.plist", app_key()));

    if path.exists() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", path.to_str().unwrap()])
            .output();
        fs::remove_file(&path).map_err(|e| format!("Failed to remove plist: {e}"))?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn is_enabled_impl(_app: &tauri::AppHandle) -> Result<bool, String> {
    Ok(launch_agents_dir()?.join(format!("{}.plist", app_key())).exists())
}

#[cfg(target_os = "macos")]
fn launch_agents_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "$HOME not set".to_string())?;
    Ok(PathBuf::from(home).join("Library").join("LaunchAgents"))
}


#[cfg(target_os = "windows")]
fn enable_impl(app: &tauri::AppHandle) -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let exe = target_exe(app)?;
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE,
        )
        .map_err(|e| format!("Failed to open registry key: {e}"))?;

    key.set_value(app_key(), &format!(r#""{}" --autostart"#, exe))
        .map_err(|e| format!("Failed to set registry value: {e}"))?;

    Ok(())
}

#[cfg(target_os = "windows")]
fn disable_impl(_app: &tauri::AppHandle) -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE,
        )
        .map_err(|e| format!("Failed to open registry key: {e}"))?;

    let _ = key.delete_value(app_key());
    Ok(())
}

#[cfg(target_os = "windows")]
fn is_enabled_impl(_app: &tauri::AppHandle) -> Result<bool, String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            KEY_READ,
        )
        .map_err(|e| format!("Failed to open registry key: {e}"))?;

    Ok(key.get_value::<String, _>(app_key()).is_ok())
}


#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn enable_impl(_app: &tauri::AppHandle) -> Result<(), String> {
    Err("Autostart is not supported on this platform".to_string())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn disable_impl(_app: &tauri::AppHandle) -> Result<(), String> {
    Err("Autostart is not supported on this platform".to_string())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn is_enabled_impl(_app: &tauri::AppHandle) -> Result<bool, String> {
    Ok(false)
}
