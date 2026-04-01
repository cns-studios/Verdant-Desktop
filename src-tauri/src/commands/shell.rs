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
