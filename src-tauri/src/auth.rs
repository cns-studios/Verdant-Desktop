use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::OsRng;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};
use url::Url;

use crate::db::StoredToken;
use crate::state::now_epoch;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://mail.google.com/";
const REDIRECT_PORT: u16 = 8765;
const AUTH_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const DRAIN_GRACE: Duration = Duration::from_secs(2);
const ACCEPT_POLL: Duration = Duration::from_millis(50);
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REQUEST_BYTES: usize = 16 * 1024;
const TIMEOUT_MESSAGE: &str = "Authentication timed out. No callback received; please try again.";

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

fn redirect_uri() -> String {
    format!("http://127.0.0.1:{}/callback", REDIRECT_PORT)
}

fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn authorize_url(client_id: &str, state: &str, challenge: &str) -> Result<Url, String> {
    let redirect = redirect_uri();
    Url::parse_with_params(
        AUTH_URL,
        &[
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", redirect.as_str()),
            ("scope", SCOPE),
            ("state", state),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ],
    )
    .map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

async fn request_token(mut params: Vec<(&'static str, String)>) -> Result<StoredToken, String> {
    let client_id = configured_google_client_id().ok_or_else(|| "Missing GOOGLE_CLIENT_ID".to_string())?;
    params.push(("client_id", client_id));
    if let Some(secret) = configured_google_client_secret() {
        params.push(("client_secret", secret));
    }

    let res = reqwest::Client::new()
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| {
                v.get("error_description")
                    .or_else(|| v.get("error"))
                    .and_then(|d| d.as_str().map(str::to_string))
            })
            .unwrap_or(body);
        return Err(format!("Google token request failed ({}): {}", status.as_u16(), detail));
    }

    let token = res.json::<TokenResponse>().await.map_err(|e| e.to_string())?;
    Ok(StoredToken {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at_epoch: token.expires_in.map(|secs| now_epoch() + secs),
    })
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

struct CallbackRequest {
    stream: TcpStream,
    target: String,
}

impl CallbackRequest {
    fn respond(mut self, status: &str, body: &str) {
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        let _ = self.stream.write_all(head.as_bytes());
        let _ = self.stream.write_all(body.as_bytes());
        let _ = self.stream.flush();
    }

    fn respond_no_content(self) {
        self.respond("204 No Content", "");
    }

    fn respond_error(self, title: &str, message: &str) {
        self.respond("400 Bad Request", &error_page(title, message));
    }
}

fn read_request_target(stream: &mut TcpStream) -> Option<String> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(REQUEST_READ_TIMEOUT)).ok()?;
    let mut data = Vec::new();
    let mut chunk = [0u8; 1024];
    while !data.windows(4).any(|w| w == b"\r\n\r\n") && data.len() < MAX_REQUEST_BYTES {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
        }
    }
    let text = String::from_utf8_lossy(&data);
    let mut parts = text.lines().next()?.split_whitespace();
    let _method = parts.next()?;
    parts.next().map(str::to_string)
}

fn next_request(listener: &TcpListener, deadline: Instant) -> Option<CallbackRequest> {
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Some(target) = read_request_target(&mut stream) {
                    return Some(CallbackRequest { stream, target });
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(ACCEPT_POLL);
            }
            Err(_) => return None,
        }
    }
}

fn wait_for_auth_code(listener: &TcpListener, expected_state: &str) -> Result<String, String> {
    let deadline = Instant::now() + AUTH_TIMEOUT;
    let success_page = include_str!("../assets/oauth-success.html");

    loop {
        let request = next_request(listener, deadline).ok_or_else(|| TIMEOUT_MESSAGE.to_string())?;

        let url = Url::parse(&format!("http://127.0.0.1:{}{}", REDIRECT_PORT, request.target))
            .map_err(|e| format!("Invalid callback URL: {}", e))?;

        if url.path() != "/callback" {
            request.respond_no_content();
            continue;
        }

        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();

        if let Some(error) = query.get("error") {
            request.respond_error("Sign-in failed", &format!("Google returned an error: {}.", error));
            return Err(format!("Google sign-in failed: {}", error));
        }

        let Some(state) = query.get("state") else {
            request.respond_error("Sign-in failed", "The callback was missing the state parameter.");
            return Err("OAuth state missing in callback".to_string());
        };

        if state != expected_state {
            request.respond_error("Sign-in failed", "The state parameter did not match. Please try again.");
            return Err("OAuth state mismatch".to_string());
        }

        let Some(code) = query.get("code").cloned() else {
            request.respond_error("Sign-in failed", "The callback was missing the authorization code.");
            return Err("Authorization code missing in callback".to_string());
        };

        request.respond("200 OK", success_page);

        let drain_deadline = Instant::now() + DRAIN_GRACE;
        while let Some(extra) = next_request(listener, drain_deadline) {
            extra.respond_no_content();
        }

        return Ok(code);
    }
}

pub async fn login_interactive() -> Result<StoredToken, String> {
    let client_id = configured_google_client_id().ok_or_else(|| "Missing GOOGLE_CLIENT_ID".to_string())?;

    let listener = TcpListener::bind(("127.0.0.1", REDIRECT_PORT)).map_err(|e| {
        format!(
            "Could not open the OAuth callback port {} ({}). Another instance of Verdant or another app may be using it. Please close it and try again.",
            REDIRECT_PORT, e
        )
    })?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;

    let verifier = random_token(32);
    let state = random_token(16);
    let auth_url = authorize_url(&client_id, &state, &pkce_challenge(&verifier))?;

    open::that(auth_url.as_str()).map_err(|e| e.to_string())?;

    let code = tokio::task::spawn_blocking(move || wait_for_auth_code(&listener, &state))
        .await
        .map_err(|e| e.to_string())??;

    request_token(vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code),
        ("redirect_uri", redirect_uri()),
        ("code_verifier", verifier),
    ])
    .await
}

pub async fn refresh_access_token(refresh_token: &str) -> Result<StoredToken, String> {
    let mut token = request_token(vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
    ])
    .await?;
    if token.refresh_token.is_none() {
        token.refresh_token = Some(refresh_token.to_string());
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_rfc7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn authorize_url_carries_pkce_and_offline_access() {
        let url = authorize_url("client", "st", "ch").unwrap();
        let q: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["client_id"], "client");
        assert_eq!(q["state"], "st");
        assert_eq!(q["code_challenge"], "ch");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["access_type"], "offline");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:8765/callback");
    }

    fn get(port: u16, target: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(stream, "GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        let mut out = String::new();
        let _ = stream.read_to_string(&mut out);
        out
    }

    #[test]
    fn callback_listener_returns_the_code_for_a_matching_state() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = std::thread::spawn(move || {
            let ignored = get(port, "/favicon.ico");
            let ok = get(port, "/callback?state=abc&code=the-code");
            (ignored, ok)
        });
        assert_eq!(wait_for_auth_code(&listener, "abc").unwrap(), "the-code");
        let (ignored, ok) = client.join().unwrap();
        assert!(ignored.starts_with("HTTP/1.1 204"));
        assert!(ok.starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn callback_listener_rejects_a_wrong_state() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = std::thread::spawn(move || get(port, "/callback?state=evil&code=x"));
        assert!(wait_for_auth_code(&listener, "abc").is_err());
        assert!(client.join().unwrap().starts_with("HTTP/1.1 400"));
    }
}
