use url::Url;

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {

    let parsed = Url::parse(&url).map_err(|err| {
        format!("Invalid URL: {err}")
    })?;

    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!("Blocked URL scheme: {other}"));
        }
    }

    open::that(url.clone()).map_err(|err| {
        format!("Failed to open external URL: {err}")
    })?;
    Ok(())
}

#[tauri::command]
pub async fn fetch_remote_image(url: String) -> Result<Option<String>, String> {
    let parsed = Url::parse(&url).map_err(|err| format!("Invalid URL: {err}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Blocked URL scheme".into());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|err| format!("HTTP client error: {err}"))?;

    let resp = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Verdant Desktop) AppleWebKit/537.36",
        )
        .send()
        .await
        .map_err(|err| format!("Request failed: {err}"))?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
        .filter(|m| m.starts_with("image/"))
        .unwrap_or_else(|| "image/png".to_string());

    let bytes = resp.bytes().await.map_err(|err| format!("Read failed: {err}"))?;
    if bytes.len() > 512 * 1024 {
        return Ok(None);
    }

    use base64::Engine;
    Ok(Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    )))
}
