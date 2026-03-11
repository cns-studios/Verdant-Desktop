use anyhow::{anyhow, Result};
use async_imap::Session;
use async_native_tls::TlsStream;
use chrono::DateTime;
use mail_parser::{MessageParser, MimeHeaders};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;

use crate::{credentials, db::Db};

pub type TlsSession = Session<TlsStream<TcpStream>>;
pub type PlainSession = Session<TcpStream>;

pub enum ImapSession {
    Tls(TlsSession),
    Plain(PlainSession),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxInfo {
    pub name: String,
    pub full_name: String,
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeader {
    pub id: String,
    pub uid: u32,
    pub message_id: String,
    pub subject: String,
    pub sender_name: String,
    pub sender_email: String,
    pub date_str: String,
    pub date_ts: i64,
    pub flags: Vec<String>,
    pub preview: String,
    pub has_attachments: bool,
    pub in_reply_to: String,
    pub references_hdr: String,
}

pub async fn connect_tls(host: &str, port: u16) -> Result<async_imap::Client<TlsStream<TcpStream>>> {
    let tcp = TcpStream::connect((host, port)).await?;
    let tls = async_native_tls::TlsConnector::new()
        .connect(host, tcp)
        .await?;
    Ok(async_imap::Client::new(tls))
}

pub async fn connect_plain(host: &str, port: u16) -> Result<async_imap::Client<TcpStream>> {
    let tcp = TcpStream::connect((host, port)).await?;
    Ok(async_imap::Client::new(tcp))
}

pub async fn login(
    account_id: &str,
    host: &str,
    port: u16,
    use_tls: bool,
    username: &str,
) -> Result<ImapSession> {
    let password = credentials::load_password(account_id)?;
    if use_tls {
        let client = connect_tls(host, port).await?;
        let session = client
            .login(username, &password)
            .await
            .map_err(|(e, _)| anyhow!("IMAP login failed: {e}"))?;
        Ok(ImapSession::Tls(session))
    } else {
        let client = connect_plain(host, port).await?;
        let session = client
            .login(username, &password)
            .await
            .map_err(|(e, _)| anyhow!("IMAP login failed: {e}"))?;
        Ok(ImapSession::Plain(session))
    }
}

pub async fn list_mailboxes(session: &mut ImapSession) -> Result<Vec<MailboxInfo>> {
    let names = match session {
        ImapSession::Tls(s) => s.list(Some(""), Some("*")).await?,
        ImapSession::Plain(s) => s.list(Some(""), Some("*")).await?,
    };

    let mut result = Vec::new();
    for name in names.iter() {
        let full_name = name.name().to_string();
        let display = full_name
            .rsplit('/')
            .next()
            .unwrap_or(&full_name)
            .to_string();
        let flags: Vec<String> = name
            .attributes()
            .iter()
            .map(|a| format!("{:?}", a))
            .collect();
        result.push(MailboxInfo {
            name: display,
            full_name,
            flags,
        });
    }
    Ok(result)
}

macro_rules! fetch_uids {
    ($session:expr, $mailbox:expr) => {{
        $session.select($mailbox).await?;
        let uids = $session.uid_search("ALL").await?;
        uids
    }};
}

pub async fn fetch_headers(
    session: &mut ImapSession,
    account_id: &str,
    mailbox_id: &str,
    mailbox_full_name: &str,
    db: &Db,
) -> Result<Vec<MessageHeader>> {
    match session {
        ImapSession::Tls(s) => { s.select(mailbox_full_name).await?; }
        ImapSession::Plain(s) => { s.select(mailbox_full_name).await?; }
    }

    let fetch_set = "1:*";
    let header_section = "(UID FLAGS BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE MESSAGE-ID IN-REPLY-TO REFERENCES CONTENT-TYPE)] BODY.PEEK[TEXT]<0.300>)";

    let messages_stream = match session {
        ImapSession::Tls(s) => s.fetch(fetch_set, header_section).await?,
        ImapSession::Plain(s) => s.fetch(fetch_set, header_section).await?,
    };

    let messages: Vec<_> = {
        use futures::TryStreamExt;
        messages_stream.try_collect().await?
    };

    let parser = MessageParser::default();
    let mut headers = Vec::new();

    for msg in &messages {
        let uid = match msg.uid {
            Some(u) => u,
            None => continue,
        };

        let flags: Vec<String> = msg.flags().iter().map(|f| format!("{f:?}")).collect();

        let exists: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM messages WHERE account_id = ? AND mailbox_id = ? AND uid = ?",
        )
        .bind(account_id)
        .bind(mailbox_id)
        .bind(uid as i64)
        .fetch_optional(db)
        .await?;

        let raw_headers = msg
            .header()
            .map(|b| b.to_vec())
            .unwrap_or_default();

        let raw_body_preview = msg.text().map(|b| b.to_vec()).unwrap_or_default();

        let parsed = parser.parse(&raw_headers);
        let (subject, sender_name, sender_email, date_str, date_ts, message_id, preview, in_reply_to, references_hdr, has_attachments) =
            if let Some(p) = parsed {
                let subject = p
                    .subject()
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let (sname, semail) = p
                    .from()
                    .and_then(|a| a.first())
                    .map(|addr| {
                        (
                            addr.name().unwrap_or("").to_string(),
                            addr.address().unwrap_or("").to_string(),
                        )
                    })
                    .unwrap_or_default();

                let (date_str, date_ts) = p
                    .date()
                    .map(|d| {
                        let ts = d.to_timestamp();
                        (d.to_rfc3339(), ts)
                    })
                    .unwrap_or_else(|| ("".to_string(), 0));

                let mid = p
                    .message_id()
                    .unwrap_or("")
                    .to_string();

                let preview = String::from_utf8_lossy(&raw_body_preview)
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(200)
                    .collect::<String>();

                let irt = p.in_reply_to().map(|h| h.to_string()).unwrap_or_default();
                let refs = p.references().map(|h| h.to_string()).unwrap_or_default();
                let has_att = false;

                (subject, sname, semail, date_str, date_ts, mid, preview, irt, refs, has_att)
            } else {
                (
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    0i64,
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    false,
                )
            };

        let id = if let Some((existing_id,)) = exists {
            sqlx::query(
                "UPDATE messages SET flags = ? WHERE id = ?",
            )
            .bind(serde_json::to_string(&flags).unwrap_or_default())
            .bind(&existing_id)
            .execute(db)
            .await?;
            existing_id
        } else {
            let new_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                r#"INSERT INTO messages
                   (id, account_id, mailbox_id, uid, message_id, subject, sender_name,
                    sender_email, date_str, date_ts, flags, preview, in_reply_to,
                    references_hdr, has_attachments, headers_fetched)
                   VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,1)"#,
            )
            .bind(&new_id)
            .bind(account_id)
            .bind(mailbox_id)
            .bind(uid as i64)
            .bind(&message_id)
            .bind(&subject)
            .bind(&sender_name)
            .bind(&sender_email)
            .bind(&date_str)
            .bind(date_ts)
            .bind(serde_json::to_string(&flags).unwrap_or_default())
            .bind(&preview)
            .bind(&in_reply_to)
            .bind(&references_hdr)
            .bind(has_attachments)
            .execute(db)
            .await?;
            new_id
        };

        headers.push(MessageHeader {
            id,
            uid,
            message_id,
            subject,
            sender_name,
            sender_email,
            date_str,
            date_ts,
            flags,
            preview,
            has_attachments,
            in_reply_to,
            references_hdr,
        });
    }

    headers.sort_by(|a, b| b.date_ts.cmp(&a.date_ts));
    Ok(headers)
}

pub async fn fetch_body(
    session: &mut ImapSession,
    mailbox_full_name: &str,
    uid: u32,
    message_id: &str,
    db: &Db,
) -> Result<(Option<String>, Option<String>)> {
    match session {
        ImapSession::Tls(s) => { s.select(mailbox_full_name).await?; }
        ImapSession::Plain(s) => { s.select(mailbox_full_name).await?; }
    }

    let uid_str = uid.to_string();
    let messages_stream = match session {
        ImapSession::Tls(s) => s.uid_fetch(&uid_str, "BODY[]").await?,
        ImapSession::Plain(s) => s.uid_fetch(&uid_str, "BODY[]").await?,
    };

    let messages: Vec<_> = {
        use futures::TryStreamExt;
        messages_stream.try_collect().await?
    };

    let raw = messages
        .first()
        .and_then(|m| m.body())
        .unwrap_or(&[]);

    let parser = MessageParser::default();
    let parsed = parser.parse(raw).ok_or_else(|| anyhow!("failed to parse message"))?;

    let text_body = parsed
        .body_text(0)
        .map(|s| s.to_string());

    let html_body = parsed
        .body_html(0)
        .map(|s| {
            let s = s.to_string();
            regex_lite_strip_scripts(&s)
        });

    sqlx::query(
        "UPDATE messages SET body_text = ?, body_html = ?, body_fetched = 1 WHERE message_id = ?",
    )
    .bind(&text_body)
    .bind(&html_body)
    .bind(message_id)
    .execute(db)
    .await?;

    Ok((text_body, html_body))
}

fn regex_lite_strip_scripts(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let lower = html.to_lowercase();
    let mut pos = 0;
    while pos < html.len() {
        if let Some(start) = lower[pos..].find("<script") {
            result.push_str(&html[pos..pos + start]);
            let search_from = pos + start;
            if let Some(end_rel) = lower[search_from..].find("</script>") {
                pos = search_from + end_rel + "</script>".len();
            } else {
                break;
            }
        } else {
            result.push_str(&html[pos..]);
            break;
        }
    }
    result
}
