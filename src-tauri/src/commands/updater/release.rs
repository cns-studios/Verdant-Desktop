use std::env;

use semver::Version;
use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateChannel { Stable, Nightly }

pub(crate) fn updater_repo_owner() -> String {
    env::var("UPDATER_REPO_OWNER").unwrap_or_else(|_| "cns-studios".to_string())
}

pub(crate) fn updater_repo_name() -> String {
    env::var("UPDATER_REPO_NAME").unwrap_or_else(|_| "Verdant-Desktop".to_string())
}

pub(crate) fn parse_update_channel(raw: Option<String>) -> UpdateChannel {
    match raw.unwrap_or_else(|| "stable".to_string()).trim().to_ascii_lowercase().as_str() {
        "nightly" | "beta" => UpdateChannel::Nightly,
        _ => UpdateChannel::Stable,
    }
}

pub(crate) fn normalize_version(raw: &str) -> String {
    let s = raw.trim();
    if let Some(rest) = s.strip_prefix("nightly-v") {
        if let Some(idx) = rest.rfind('-') { return rest[..idx].to_string(); }
        return rest.to_string();
    }
    s.trim_start_matches('v').to_string()
}

pub(crate) fn version_is_newer(current: &str, latest: &str) -> bool {
    match (Version::parse(current), Version::parse(latest)) {
        (Ok(c), Ok(l)) => l > c,
        _ => false,
    }
}

pub(crate) fn preferred_asset_score(name: &str) -> i32 {
    let lower = name.to_ascii_lowercase();
    if cfg!(target_os = "windows") {
        if lower.ends_with(".msi") { return 100; }
        if lower.ends_with(".exe") { return 90; }
        if lower.contains("nsis") { return 80; }
    }
    if cfg!(target_os = "linux") {
        let has_pacman = std::process::Command::new("which").arg("pacman").output().map(|o| o.status.success()).unwrap_or(false);
        let has_dpkg = std::process::Command::new("which").arg("dpkg").output().map(|o| o.status.success()).unwrap_or(false);
        let has_rpm = std::process::Command::new("which").arg("rpm").output().map(|o| o.status.success()).unwrap_or(false);
        if has_pacman && lower.ends_with(".pacman") { return 100; }
        if has_dpkg && lower.ends_with(".deb") { return 100; }
        if has_rpm && lower.ends_with(".rpm") { return 100; }
        if lower.ends_with(".appimage") { return 50; }
    }
    if cfg!(target_os = "macos") {
        if lower.ends_with(".dmg") { return 100; }
        if lower.ends_with(".app.tar.gz") { return 90; }
    }
    1
}

pub(crate) async fn fetch_latest_release() -> Result<Value, String> {
    let url = format!("https://api.github.com/repos/{}/{}/releases/latest", updater_repo_owner(), updater_repo_name());
    let client = reqwest::Client::new();
    let mut request = client.get(url)
        .header(reqwest::header::USER_AGENT, "verdant-desktop-updater")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json");

    if let Ok(token) = env::var("GH_TOKEN") {
        request = request.header("Authorization", format!("token {}", token));
    }

    let response = request.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("GitHub release lookup failed: {}", response.status()));
    }
    response.json::<Value>().await.map_err(|e| e.to_string())
}

pub(crate) async fn fetch_latest_nightly_release() -> Result<Value, String> {
    let url = format!("https://api.github.com/repos/{}/{}/releases?per_page=30", updater_repo_owner(), updater_repo_name());
    let client = reqwest::Client::new();
    let mut request = client.get(url)
        .header(reqwest::header::USER_AGENT, "verdant-desktop-updater")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json");

    if let Ok(token) = env::var("GH_TOKEN") {
        request = request.header("Authorization", format!("token {}", token));
    }

    let response = request.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("GitHub nightly lookup failed: {}", response.status()));
    }
    let releases = response.json::<Value>().await.map_err(|e| e.to_string())?;
    let items = releases.as_array().ok_or("Unexpected response")?;
    for release in items {
        let is_prerelease = release.get("prerelease").and_then(Value::as_bool).unwrap_or(false);
        let is_draft = release.get("draft").and_then(Value::as_bool).unwrap_or(false);
        let has_assets = release.get("assets").and_then(Value::as_array).map(|a| !a.is_empty()).unwrap_or(false);
        if is_prerelease && !is_draft && has_assets { return Ok(release.clone()); }
    }
    Err("No nightly prerelease with assets found".to_string())
}

pub(crate) async fn fetch_release_for_channel(channel: UpdateChannel) -> Result<Value, String> {
    match channel {
        UpdateChannel::Stable => fetch_latest_release().await,
        UpdateChannel::Nightly => fetch_latest_nightly_release().await,
    }
}

pub(crate) fn select_best_asset(release: &Value) -> Result<(String, String), String> {
    let assets = release.get("assets").and_then(Value::as_array)
        .ok_or_else(|| "Release has no assets".to_string())?;
    let mut chosen_name = String::new();
    let mut chosen_url = String::new();
    let mut best_score = -1;
    for asset in assets {
        let Some(name) = asset.get("name").and_then(Value::as_str) else { continue; };
        let Some(url) = asset.get("browser_download_url").and_then(Value::as_str) else { continue; };
        let score = preferred_asset_score(name);
        if score > best_score { best_score = score; chosen_name = name.to_string(); chosen_url = url.to_string(); }
    }
    if chosen_url.is_empty() { return Err("No downloadable release asset found".to_string()); }
    Ok((chosen_name, chosen_url))
}
