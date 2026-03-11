use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub email: String,
    pub imap_host: String,
    pub imap_port: i64,
    pub imap_tls: bool,
    pub smtp_host: String,
    pub smtp_port: i64,
    pub smtp_tls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Mailbox {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub full_name: String,
    pub flags: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Message {
    pub id: String,
    pub account_id: String,
    pub mailbox_id: String,
    pub uid: Option<i64>,
    pub message_id: Option<String>,
    pub subject: String,
    pub sender_name: String,
    pub sender_email: String,
    pub recipients: String,
    pub date_str: String,
    pub date_ts: i64,
    pub flags: String,
    pub preview: String,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub in_reply_to: Option<String>,
    pub references_hdr: Option<String>,
    pub has_attachments: bool,
    pub headers_fetched: bool,
    pub body_fetched: bool,
}
