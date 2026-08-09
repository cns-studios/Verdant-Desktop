use oauth2::{
    AuthType,
    basic::BasicClient,
    reqwest::async_http_client,
    AuthUrl,
    ClientId,
    ClientSecret,
    CsrfToken,
    PkceCodeChallenge,
    PkceCodeVerifier,
    RedirectUrl,
    RefreshToken,
    Scope,
    TokenResponse,
    TokenUrl,
};
use tiny_http::{Header, Response, Server};
use url::Url;
use std::collections::HashMap;
use std::time::{SystemTime, Duration, UNIX_EPOCH};

use crate::db::StoredToken;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REDIRECT_PORT: u16 = 8765;
const AUTH_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const DRAIN_GRACE: Duration = Duration::from_secs(2);

fn read_non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
    })
}

fn configured_google_client_id() -> Option<String> {
    read_non_empty(std::env::var("GOOGLE_CLIENT_ID").ok()).or_else(|| {
        read_non_empty(option_env!("GOOGLE_CLIENT_ID").map(|v| v.to_string()))
    })
}

fn configured_google_client_secret() -> Option<String> {
    read_non_empty(std::env::var("GOOGLE_CLIENT_SECRET").ok()).or_else(|| {
        read_non_empty(option_env!("GOOGLE_CLIENT_SECRET").map(|v| v.to_string()))
    })
}

pub fn has_google_client_id_configured() -> bool {
    configured_google_client_id().is_some()
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn google_client() -> Result<BasicClient, String> {
    let client_id = configured_google_client_id()
        .ok_or_else(|| "Missing GOOGLE_CLIENT_ID".to_string())?;

    let client_secret = configured_google_client_secret().map(ClientSecret::new);
    let has_client_secret = client_secret.is_some();

    let auth_url = AuthUrl::new(AUTH_URL.to_string()).map_err(|e| e.to_string())?;
    let token_url = TokenUrl::new(TOKEN_URL.to_string()).map_err(|e| e.to_string())?;
    let redirect = RedirectUrl::new(format!("http://127.0.0.1:{}/callback", REDIRECT_PORT))
        .map_err(|e| e.to_string())?;

    let client = BasicClient::new(
        ClientId::new(client_id),
        client_secret,
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(redirect);

    let client = if has_client_secret { client } else { client.set_auth_type(AuthType::RequestBody) };
    Ok(client)
}

fn build_response(body: String, status: u16) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut response = Response::from_string(body).with_status_code(status);
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]) {
        response = response.with_header(header);
    }
    if let Ok(header) = Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]) {
        response = response.with_header(header);
    }
    if let Ok(header) = Header::from_bytes(&b"Connection"[..], &b"close"[..]) {
        response = response.with_header(header);
    }
    response
}

fn error_page(title: &str, message: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Verdant - {title}</title><style>\
         body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#f5f3ef;color:#1e2119;font-family:'DM Sans',system-ui,sans-serif;padding:24px}}\
         .card{{width:min(480px,100%);background:#fafaf8;border:1px solid #b4afa2;border-radius:16px;padding:28px;box-shadow:0 8px 32px rgba(30,33,25,.14)}}\
         h1{{font-size:22px;margin:0 0 10px;color:#8a3b3b}}\
         p{{font-size:14px;line-height:1.6;color:#4a4d45;margin:0 0 14px}}\
         </style></head><body><div class=\"card\"><h1>{title}</h1><p>{message}</p><p>You can close this tab and return to the Verdant app.</p></div></body></html>"
    )
}

fn respond_ok(request: tiny_http::Request, html: &str) {
    let _ = request.respond(build_response(html.to_string(), 200));
}

fn respond_no_content(request: tiny_http::Request) {
    let mut response = Response::empty(204);
    if let Ok(header) = Header::from_bytes(&b"Connection"[..], &b"close"[..]) {
        response = response.with_header(header);
    }
    let _ = request.respond(response);
}

fn respond_error(request: tiny_http::Request, title: &str, message: &str) {
    let _ = request.respond(build_response(error_page(title, message), 400));
}

fn wait_for_auth_code(server: &Server, expected_state: String) -> Result<String, String> {
    let deadline = SystemTime::now() + AUTH_TIMEOUT;
    let success_page = include_str!("../assets/oauth-success.html").to_string();

    loop {
        let remaining = deadline
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO);

        if remaining.is_zero() {
            return Err("Authentication timed out. No callback received; please try again.".to_string());
        }

        let request = server
            .recv_timeout(remaining)
            .map_err(|e| format!("OAuth callback server error: {}", e))?
            .ok_or_else(|| "Authentication timed out. No callback received; please try again.".to_string())?;

        let url = Url::parse(&format!("http://127.0.0.1:{}{}", REDIRECT_PORT, request.url()))
            .map_err(|e| format!("Invalid callback URL: {}", e))?;

        if url.path() != "/callback" {
            respond_no_content(request);
            continue;
        }

        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();

        if let Some(error) = query.get("error") {
            respond_error(request, "Sign-in failed", &format!("Google returned an error: {}.", error));
            return Err(format!("Google sign-in failed: {}", error));
        }

        let state = match query.get("state").cloned() {
            Some(state) => state,
            None => {
                respond_error(request, "Sign-in failed", "The callback was missing the state parameter.");
                return Err("OAuth state missing in callback".to_string());
            }
        };

        if state != expected_state {
            respond_error(request, "Sign-in failed", "The state parameter did not match. Please try again.");
            return Err("OAuth state mismatch".to_string());
        }

        let code = match query.get("code").cloned() {
            Some(code) => code,
            None => {
                respond_error(request, "Sign-in failed", "The callback was missing the authorization code.");
                return Err("Authorization code missing in callback".to_string());
            }
        };

        respond_ok(request, &success_page);

        let drain_deadline = SystemTime::now() + DRAIN_GRACE;
        loop {
            let drain_remaining = drain_deadline
                .duration_since(SystemTime::now())
                .unwrap_or(Duration::ZERO);
            if drain_remaining.is_zero() {
                break;
            }
            match server.recv_timeout(drain_remaining) {
                Ok(Some(extra)) => respond_no_content(extra),
                _ => break,
            }
        }

        return Ok(code);
    }
}

pub async fn login_interactive() -> Result<StoredToken, String> {
    let client = google_client()?;

    let server = Server::http(("127.0.0.1", REDIRECT_PORT)).map_err(|e| {
        format!(
            "Could not open the OAuth callback port {} ({}). Another instance of Verdant or another app may be using it. Please close it and try again.",
            REDIRECT_PORT, e
        )
    })?;

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("https://mail.google.com/".to_string()))
        .add_extra_param("access_type", "offline")
        .add_extra_param("prompt", "consent")
        .set_pkce_challenge(pkce_challenge)
        .url();

    open::that(auth_url.as_str()).map_err(|e| e.to_string())?;

    let state = csrf_token.secret().to_string();
    let code = tokio::task::spawn_blocking(move || wait_for_auth_code(&server, state))
        .await
        .map_err(|e| e.to_string())??;

    let token_result = client
        .exchange_code(oauth2::AuthorizationCode::new(code))
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier.secret().to_string()))
        .request_async(async_http_client)
        .await
        .map_err(|e| e.to_string())?;

    let expires = token_result.expires_in()
        .map(|d| now_epoch() + d.as_secs() as i64);

    Ok(StoredToken {
        access_token: token_result.access_token().secret().to_string(),
        refresh_token: token_result.refresh_token().map(|t| t.secret().to_string()),
        expires_at_epoch: expires,
    })
}

pub async fn refresh_access_token(refresh_token: &str) -> Result<StoredToken, String> {
    let client = google_client()?;
    let token_result = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
        .request_async(async_http_client)
        .await
        .map_err(|e| e.to_string())?;

    let expires = token_result.expires_in()
        .map(|d| now_epoch() + d.as_secs() as i64);

    Ok(StoredToken {
        access_token: token_result.access_token().secret().to_string(),
        refresh_token: token_result.refresh_token()
            .map(|t| t.secret().to_string())
            .or(Some(refresh_token.to_string())),
        expires_at_epoch: expires,
    })
}
