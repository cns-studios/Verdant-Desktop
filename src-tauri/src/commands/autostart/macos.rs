use std::fs;
use std::path::PathBuf;

use super::target_exe;

const APP_KEY: &str = "Verdant-Desktop";

pub fn enable(app: &tauri::AppHandle) -> Result<(), String> {
    let dir = launch_agents_dir()?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create LaunchAgents dir: {e}"))?;

    let exe = target_exe(app)?;
    let label = APP_KEY;
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

pub fn disable(_app: &tauri::AppHandle) -> Result<(), String> {
    let dir = launch_agents_dir()?;
    let path = dir.join(format!("{}.plist", APP_KEY));

    if path.exists() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", path.to_str().unwrap()])
            .output();
        fs::remove_file(&path).map_err(|e| format!("Failed to remove plist: {e}"))?;
    }

    Ok(())
}

pub fn is_enabled(_app: &tauri::AppHandle) -> Result<bool, String> {
    Ok(launch_agents_dir()?.join(format!("{}.plist", APP_KEY)).exists())
}

fn launch_agents_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "$HOME not set".to_string())?;
    Ok(PathBuf::from(home).join("Library").join("LaunchAgents"))
}
