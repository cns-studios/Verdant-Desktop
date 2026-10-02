use std::fs;
use std::path::PathBuf;

use super::target_exe;

pub fn enable(app: &tauri::AppHandle) -> Result<(), String> {
    let exe = target_exe(app)?;
    let dir = systemd_user_dir()?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create systemd user dir: {e}"))?;

    let path = service_path()?;
    let content = format!(
        "[Unit]\n\
        Description=Verdant Desktop\n\
        After=graphical-session.target\n\
        BindsTo=graphical-session.target\n\
        Wants=dbus.socket\n\
        \n\
        [Service]\n\
        Type=exec\n\
        ExecStart={} --autostart\n\
        Restart=on-failure\n\
        RestartSec=5\n\
        \n\
        [Install]\n\
        WantedBy=graphical-session.target",
        exe
    );

    fs::write(&path, &content).map_err(|e| format!("Failed to write service file: {e}"))?;

    cleanup_old_desktop_file();

    run_systemctl(&["--user", "daemon-reload"])?;
    run_systemctl(&["--user", "enable", "verdant-desktop.service"])?;
    run_systemctl(&["--user", "start", "--no-block", "verdant-desktop.service"])?;
    Ok(())
}

pub fn disable(_app: &tauri::AppHandle) -> Result<(), String> {
    let _ = run_systemctl(&["--user", "disable", "--now", "verdant-desktop.service"]);

    let path = service_path()?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to remove service file: {e}"))?;
    }

    run_systemctl(&["--user", "daemon-reload"])?;
    Ok(())
}

pub fn is_enabled(_app: &tauri::AppHandle) -> Result<bool, String> {
    match run_systemctl(&["--user", "is-enabled", "verdant-desktop.service"]) {
        Ok(out) => Ok(out.trim() == "enabled"),
        Err(_) => Ok(service_path().map_or(false, |p| p.exists())),
    }
}

fn systemd_user_dir() -> Result<PathBuf, String> {
    if let Ok(config) = std::env::var("XDG_CONFIG_HOME") {
        Ok(PathBuf::from(config).join("systemd").join("user"))
    } else if let Ok(home) = std::env::var("HOME") {
        Ok(PathBuf::from(home).join(".config").join("systemd").join("user"))
    } else {
        Err("Cannot determine home directory; $HOME is not set".to_string())
    }
}

fn service_path() -> Result<PathBuf, String> {
    Ok(systemd_user_dir()?.join("verdant-desktop.service"))
}

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
