mod changelog;
mod install;
mod release;

use std::env;
#[cfg(unix)]
use std::ffi::CStr;
use std::path::PathBuf;

use serde_json::Value;

use install::{downloads_dir, install_update_sync};
use release::{fetch_release_for_channel, normalize_version, parse_update_channel, select_best_asset, version_is_newer, UpdateChannel};

pub use changelog::*;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_name: String,
    pub published_at: String,
    pub notes: String,
    pub update_available: bool,
    pub download_asset_name: String,
    pub download_url: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDownloadResult {
    pub file_path: String,
    pub file_name: String,
    pub version: String,
}

pub(crate) fn updater_data_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        if let Ok(raw_uid) = env::var("SUDO_UID") {
            if let Ok(uid) = raw_uid.parse::<libc::uid_t>() {
                let passwd = unsafe { libc::getpwuid(uid) };
                if !passwd.is_null() {
                    let home_dir = unsafe { (*passwd).pw_dir };
                    if !home_dir.is_null() {
                        if let Ok(home) = unsafe { CStr::from_ptr(home_dir) }.to_str() {
                            let path = PathBuf::from(home);
                            if cfg!(target_os = "macos") {
                                return Some(path.join("Library").join("Application Support"));
                            } else {
                                return Some(env::var_os("XDG_DATA_HOME")
                                    .map(PathBuf::from)
                                    .filter(|data_dir| data_dir.starts_with(&path))
                                    .unwrap_or_else(|| path.join(".local").join("share")));
                            }
                        }
                    }
                }
            }
        }
    }

    dirs::data_dir()
}

pub(crate) fn load_saved_update_channel() -> String {
    let config_path = updater_data_dir()
        .map(|d| d.join("com.cns-studios.verdant").join("app-config.json"));

    match config_path {
        Some(path) => {
            std::fs::read_to_string(path).ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .and_then(|v| v.get("update_channel").and_then(|c| c.as_str().map(|s| s.to_string())))
                .unwrap_or_else(|| "stable".to_string())
        }
        None => "stable".to_string(),
    }
}

#[tauri::command]
pub async fn check_for_updates(channel: Option<String>) -> Result<UpdateInfo, String> {
    let channel = parse_update_channel(channel);
    let current_version = normalize_version(env!("CARGO_PKG_VERSION"));
    let release = fetch_release_for_channel(channel).await?;
    let latest_tag = release.get("tag_name").and_then(Value::as_str).unwrap_or_default().to_string();
    let latest_version = normalize_version(&latest_tag);
    let update_available = match channel {
        UpdateChannel::Nightly => true,
        UpdateChannel::Stable => version_is_newer(&current_version, &latest_version),
    };
    let (download_asset_name, download_url) = select_best_asset(&release)?;
    Ok(UpdateInfo {
        current_version,
        latest_version,
        release_name: release.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        published_at: release.get("published_at").and_then(Value::as_str).unwrap_or("").to_string(),
        notes: release.get("body").and_then(Value::as_str).unwrap_or("").to_string(),
        update_available,
        download_asset_name,
        download_url,
    })
}

#[tauri::command]
pub async fn download_latest_update(channel: Option<String>) -> Result<UpdateDownloadResult, String> {
    let info = check_for_updates(channel).await?;
    if !info.update_available { return Err("No update available".to_string()); }
    let client = reqwest::Client::new();
    let response = client.get(&info.download_url)
        .header(reqwest::header::USER_AGENT, "verdant-desktop-updater")
        .send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Update download failed: {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    let folder = downloads_dir()?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let file_path = folder.join(&info.download_asset_name);
    std::fs::write(&file_path, &bytes).map_err(|e| e.to_string())?;
    Ok(UpdateDownloadResult {
        file_path: file_path.to_string_lossy().to_string(),
        file_name: info.download_asset_name,
        version: info.latest_version,
    })
}

#[tauri::command]
pub async fn install_and_relaunch(file_path: String) -> Result<(), String> {
    let name = std::path::Path::new(&file_path)
        .file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
    install_update_sync(&file_path, &name)
}

pub async fn handle_cli_update() {
    println!("Checking for updates...");
    let channel_str = load_saved_update_channel();
    match check_for_updates(Some(channel_str.clone())).await {
        Ok(info) => {
            if !info.update_available {
                println!("Verdant is already up to date (v{}).", info.current_version);
                return;
            }
            println!("New version available: v{} -> v{}", info.current_version, info.latest_version);
            println!("Downloading {}...", info.download_asset_name);
            match download_latest_update(Some(channel_str)).await {
                Ok(result) => {
                    println!("Download complete. Installing...");
                    if let Err(e) = install_and_relaunch(result.file_path).await {
                        eprintln!("Error during installation: {}", e);
                    } else {
                        println!("Installation successful.");
                    }
                }
                Err(e) => eprintln!("Download error: {}", e),
            }
        }
        Err(e) => eprintln!("Error checking for updates: {}", e),
    }
}
