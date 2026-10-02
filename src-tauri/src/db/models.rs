use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Account {
    pub id: i64,
    pub email: String,
    pub provider: String,
    pub display_name: Option<String>,
    pub is_active: bool,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at_epoch: Option<i64>,
    pub imap_host: Option<String>,
    pub imap_port: Option<i64>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<i64>,
    pub username: Option<String>,
    pub encrypted_password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccountPublic {
    pub id: i64,
    pub email: String,
    pub provider: String,
    pub display_name: Option<String>,
    pub is_active: bool,
}

impl From<Account> for AccountPublic {
    fn from(a: Account) -> Self {
        AccountPublic {
            id: a.id,
            email: a.email,
            provider: a.provider,
            display_name: a.display_name,
            is_active: a.is_active,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Email {
    pub id: String,
    pub account_id: i64,
    pub draft_id: Option<String>,
    pub thread_id: String,
    pub subject: String,
    pub sender: String,
    pub to_recipients: String,
    pub cc_recipients: String,
    pub bcc_recipients: String,
    pub snippet: String,
    pub body_html: String,
    pub attachments_json: String,
    pub has_attachments: bool,
    pub date: String,
    pub is_read: bool,
    pub starred: bool,
    pub mailbox: String,
    pub labels: String,
    pub internal_ts: i64,
    pub notified: bool,
    pub list_unsubscribe: String,
    pub unsubscribed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_epoch: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxSyncState {
    pub account_id: i64,
    pub mailbox_name: String,
    pub highest_uid: u32,
    pub uidvalidity: u32,
    pub last_synced_at: i64,
}
