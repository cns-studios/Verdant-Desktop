use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use native_tls::TlsConnector;

use crate::crypto::decrypt_password;
use crate::db::Account;

pub struct ImapCredentials {
    pub imap_host: String,
    pub imap_port: u16,
    pub username: String,
    pub password: String,
}

impl ImapCredentials {
    pub fn from_account(account: &Account) -> Result<Self, String> {
        let imap_host = account.imap_host.clone()
            .ok_or_else(|| "Missing IMAP host".to_string())?;
        let imap_port = account.imap_port
            .ok_or_else(|| "Missing IMAP port".to_string())? as u16;
        let username = account.username.clone()
            .ok_or_else(|| "Missing IMAP username".to_string())?;
        let encrypted_password = account.encrypted_password.clone()
            .ok_or_else(|| "Missing encrypted password".to_string())?;
        let password = decrypt_password(&encrypted_password)?;
        Ok(ImapCredentials { imap_host, imap_port, username, password })
    }
}

pub(crate) type TlsSession = imap::Session<native_tls::TlsStream<std::net::TcpStream>>;

pub(crate) const CONNECT_TIMEOUT_SECS: u64 = 8;

pub(crate) const SESSION_IO_TIMEOUT_SECS: u64 = 60;

pub fn connect_with_timeout(creds: &ImapCredentials, timeout_secs: u64) -> Result<TlsSession, String> {
    let tls = TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS build error: {}", e))?;

    let addrs: Vec<std::net::SocketAddr> = format!("{}:{}", creds.imap_host, creds.imap_port)
        .to_socket_addrs()
        .map_err(|e| format!("DNS resolution error: {}", e))?
        .collect();
    if addrs.is_empty() {
        return Err("DNS resolution returned no addresses".to_string());
    }
    let mut last_err = None;
    let tcp = addrs
        .iter()
        .find_map(|addr| match TcpStream::connect_timeout(addr, Duration::from_secs(timeout_secs)) {
            Ok(stream) => Some(stream),
            Err(e) => {
                last_err = Some(e);
                None
            }
        })
        .ok_or_else(|| format!("IMAP connect error: {}", last_err.map(|e| e.to_string()).unwrap_or_default()))?;

    let _ = tcp.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(timeout_secs)));
    let socket = tcp.try_clone().ok();

    let tls_stream = tls.connect(&creds.imap_host, tcp)
        .map_err(|e| format!("IMAP TLS error: {}", e))?;

    let client = imap::Client::new(tls_stream);
    let session = client
        .login(&creds.username, &creds.password)
        .map_err(|(e, _)| format!("IMAP login error: {}", e))?;

    if let Some(socket) = socket {
        let _ = socket.set_read_timeout(Some(Duration::from_secs(SESSION_IO_TIMEOUT_SECS)));
        let _ = socket.set_write_timeout(Some(Duration::from_secs(SESSION_IO_TIMEOUT_SECS)));
    }

    Ok(session)
}

pub fn retry_connect(creds: &ImapCredentials) -> Result<TlsSession, String> {
    let mut last_err = String::new();
    for attempt in 0..2 {
        match connect_with_timeout(creds, CONNECT_TIMEOUT_SECS) {
            Ok(session) => return Ok(session),
            Err(e) if e.starts_with("IMAP login error") => return Err(e),
            Err(e) => {
                last_err = e;
                if attempt < 1 {
                    std::thread::sleep(Duration::from_millis(500 * (attempt as u64 + 1)));
                }
            }
        }
    }
    Err(format!("IMAP connect failed after 2 retries: {}", last_err))
}

pub fn test_imap_connection(
    imap_host: &str,
    imap_port: u16,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let creds = ImapCredentials {
        imap_host: imap_host.to_string(),
        imap_port,
        username: username.to_string(),
        password: password.to_string(),
    };

    let mut session = connect_with_timeout(&creds, CONNECT_TIMEOUT_SECS)?;
    let _ = session.logout();
    Ok(username.to_string())
}

pub(crate) fn has_capability(session: &mut TlsSession, name: &str) -> bool {
    session.capabilities().map(|caps| caps.has_str(name)).unwrap_or(false)
}
