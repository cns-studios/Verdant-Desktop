use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

const SERVICE: &str = "verdant-mail";

pub fn store_password(account_id: &str, password: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE, account_id)
        .map_err(|e| anyhow!("keyring error: {e}"))?;
    entry
        .set_password(password)
        .map_err(|e| anyhow!("keyring set_password: {e}"))?;
    Ok(())
}

pub fn load_password(account_id: &str) -> Result<String> {
    let entry = keyring::Entry::new(SERVICE, account_id)
        .map_err(|e| anyhow!("keyring error: {e}"))?;
    let pw = entry
        .get_password()
        .map_err(|e| anyhow!("keyring get_password: {e}"))?;
    Ok(pw)
}

pub fn delete_password(account_id: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE, account_id)
        .map_err(|e| anyhow!("keyring error: {e}"))?;
    entry
        .delete_credential()
        .map_err(|e| anyhow!("keyring delete: {e}"))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub id: String,
    pub name: String,
    pub email: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_tls: bool,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_tls: bool,
}
