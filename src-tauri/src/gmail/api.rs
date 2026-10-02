use std::time::Duration;

use serde_json::Value;

use crate::state::{clear_rate_limited, mark_rate_limited, DbState};

pub(crate) fn rate_limit_message(remain: Duration) -> String {
    format!(
        "Gmail rate limit reached (429). Backing off for {}s — new mail will appear automatically.",
        remain.as_secs()
    )
}

pub(crate) async fn gmail_api_get(
    state: &DbState,
    account_id: i64,
    client: &reqwest::Client,
    token: &str,
    url: String,
) -> Result<Value, String> {
    let res = client.get(&url).bearer_auth(token).send().await.map_err(|e| e.to_string())?;

    if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = res
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60)
            .min(600);
        let backoff = Duration::from_secs(retry_after);
        mark_rate_limited(state, account_id, backoff);
        return Err(rate_limit_message(backoff));
    }

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        let truncated = if body.len() > 400 { body[..400].to_string() } else { body };
        return Err(format!("Gmail API {} failed: {}", status.as_u16(), truncated));
    }

    clear_rate_limited(state, account_id);
    res.json::<Value>().await.map_err(|e| e.to_string())
}
