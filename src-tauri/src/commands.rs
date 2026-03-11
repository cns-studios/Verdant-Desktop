use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::credentials;
use crate::db::Db;
use crate::imap;
use crate::models::{Account, Mailbox, Message};
use crate::smtp::{self, OutgoingMessage};
use crate::sync;

pub struct AppDb(pub std::sync::Arc<Db>);

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Debug, Deserialize)]
pub struct AddAccountInput {
    pub name: String,
    pub email: String,
    pub password: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_tls: bool,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_tls: bool,
}

#[tauri::command]
pub async fn add_account(
    input: AddAccountInput,
    state: State<'_, AppDb>,
) -> Result<Account, String> {
    let db = &*state.0;
    let id = Uuid::new_v4().to_string();

    credentials::store_password(&id, &input.password).map_err(err)?;

    sqlx::query(
        r#"INSERT INTO accounts (id, name, email, imap_host, imap_port, imap_tls, smtp_host, smtp_port, smtp_tls)
           VALUES (?,?,?,?,?,?,?,?,?)"#,
    )
    .bind(&id)
    .bind(&input.name)
    .bind(&input.email)
    .bind(&input.imap_host)
    .bind(input.imap_port as i64)
    .bind(input.imap_tls)
    .bind(&input.smtp_host)
    .bind(input.smtp_port as i64)
    .bind(input.smtp_tls)
    .execute(db)
    .await
    .map_err(err)?;

    let account: Account = sqlx::query_as(
        "SELECT id, name, email, imap_host, imap_port, imap_tls, smtp_host, smtp_port, smtp_tls FROM accounts WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(db)
    .await
    .map_err(err)?;

    if let Ok(mut session) = imap::login(
        &account.id,
        &account.imap_host,
        account.imap_port as u16,
        account.imap_tls,
        &account.email,
    )
    .await
    {
        if let Ok(mailboxes) = imap::list_mailboxes(&mut session).await {
            for mb in mailboxes {
                let mb_id = Uuid::new_v4().to_string();
                let _ = sqlx::query(
                    "INSERT OR IGNORE INTO mailboxes (id, account_id, name, full_name, flags) VALUES (?,?,?,?,?)",
                )
                .bind(&mb_id)
                .bind(&account.id)
                .bind(&mb.name)
                .bind(&mb.full_name)
                .bind(mb.flags.join(","))
                .execute(db)
                .await;
            }
        }
        match session {
            imap::ImapSession::Tls(mut s) => { let _ = s.logout().await; }
            imap::ImapSession::Plain(mut s) => { let _ = s.logout().await; }
        }
    }

    Ok(account)
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppDb>) -> Result<Vec<Account>, String> {
    let db = &*state.0;
    sqlx::query_as(
        "SELECT id, name, email, imap_host, imap_port, imap_tls, smtp_host, smtp_port, smtp_tls FROM accounts",
    )
    .fetch_all(db)
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn delete_account(id: String, state: State<'_, AppDb>) -> Result<(), String> {
    let db = &*state.0;
    credentials::delete_password(&id).map_err(err)?;
    sqlx::query("DELETE FROM accounts WHERE id = ?")
        .bind(&id)
        .execute(db)
        .await
        .map_err(err)?;
    Ok(())
}

#[tauri::command]
pub async fn list_mailboxes(
    account_id: String,
    state: State<'_, AppDb>,
) -> Result<Vec<Mailbox>, String> {
    let db = &*state.0;
    sqlx::query_as(
        "SELECT id, account_id, name, full_name, flags FROM mailboxes WHERE account_id = ? ORDER BY full_name",
    )
    .bind(&account_id)
    .fetch_all(db)
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn list_messages(
    mailbox_id: String,
    limit: Option<i64>,
    offset: Option<i64>,
    state: State<'_, AppDb>,
) -> Result<Vec<Message>, String> {
    let db = &*state.0;
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);

    sqlx::query_as(
        r#"SELECT id, account_id, mailbox_id, uid, message_id, subject, sender_name,
                  sender_email, recipients, date_str, date_ts, flags, preview,
                  body_text, body_html, in_reply_to, references_hdr,
                  has_attachments, headers_fetched, body_fetched
           FROM messages
           WHERE mailbox_id = ?
           ORDER BY date_ts DESC
           LIMIT ? OFFSET ?"#,
    )
    .bind(&mailbox_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn get_message(id: String, state: State<'_, AppDb>) -> Result<Message, String> {
    let db = &*state.0;
    sqlx::query_as(
        r#"SELECT id, account_id, mailbox_id, uid, message_id, subject, sender_name,
                  sender_email, recipients, date_str, date_ts, flags, preview,
                  body_text, body_html, in_reply_to, references_hdr,
                  has_attachments, headers_fetched, body_fetched
           FROM messages WHERE id = ?"#,
    )
    .bind(&id)
    .fetch_one(db)
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn fetch_message_body(
    message_id: String,
    state: State<'_, AppDb>,
) -> Result<Message, String> {
    let db = &*state.0;

    let msg: Message = sqlx::query_as(
        r#"SELECT id, account_id, mailbox_id, uid, message_id, subject, sender_name,
                  sender_email, recipients, date_str, date_ts, flags, preview,
                  body_text, body_html, in_reply_to, references_hdr,
                  has_attachments, headers_fetched, body_fetched
           FROM messages WHERE id = ?"#,
    )
    .bind(&message_id)
    .fetch_one(db)
    .await
    .map_err(err)?;

    if msg.body_fetched {
        return Ok(msg);
    }

    let account: Account = sqlx::query_as(
        "SELECT id, name, email, imap_host, imap_port, imap_tls, smtp_host, smtp_port, smtp_tls FROM accounts WHERE id = ?",
    )
    .bind(&msg.account_id)
    .fetch_one(db)
    .await
    .map_err(err)?;

    let mailbox: Mailbox = sqlx::query_as(
        "SELECT id, account_id, name, full_name, flags FROM mailboxes WHERE id = ?",
    )
    .bind(&msg.mailbox_id)
    .fetch_one(db)
    .await
    .map_err(err)?;

    let uid = msg.uid.ok_or("Message has no UID")? as u32;
    let mid = msg.message_id.as_deref().unwrap_or("");

    let mut session = imap::login(
        &account.id,
        &account.imap_host,
        account.imap_port as u16,
        account.imap_tls,
        &account.email,
    )
    .await
    .map_err(err)?;

    imap::fetch_body(&mut session, &mailbox.full_name, uid, mid, db)
        .await
        .map_err(err)?;

    match session {
        imap::ImapSession::Tls(mut s) => { let _ = s.logout().await; }
        imap::ImapSession::Plain(mut s) => { let _ = s.logout().await; }
    }

    sqlx::query_as(
        r#"SELECT id, account_id, mailbox_id, uid, message_id, subject, sender_name,
                  sender_email, recipients, date_str, date_ts, flags, preview,
                  body_text, body_html, in_reply_to, references_hdr,
                  has_attachments, headers_fetched, body_fetched
           FROM messages WHERE id = ?"#,
    )
    .bind(&message_id)
    .fetch_one(db)
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn sync_account(
    account_id: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppDb>,
) -> Result<(), String> {
    let db = &*state.0;
    if let Some(id) = account_id {
        let account: Account = sqlx::query_as(
            "SELECT id, name, email, imap_host, imap_port, imap_tls, smtp_host, smtp_port, smtp_tls FROM accounts WHERE id = ?",
        )
        .bind(&id)
        .fetch_one(db)
        .await
        .map_err(err)?;

        sync::sync_all(&app, db).await.map_err(err)
    } else {
        sync::sync_all(&app, db).await.map_err(err)
    }
}


#[tauri::command]
pub async fn search_messages(
    query: String,
    account_id: Option<String>,
    state: State<'_, AppDb>,
) -> Result<Vec<Message>, String> {
    let db = &*state.0;

    let raw = format!("{query}*");

    if let Some(aid) = account_id {
        sqlx::query_as(
            r#"SELECT m.id, m.account_id, m.mailbox_id, m.uid, m.message_id, m.subject,
                      m.sender_name, m.sender_email, m.recipients, m.date_str, m.date_ts,
                      m.flags, m.preview, m.body_text, m.body_html, m.in_reply_to,
                      m.references_hdr, m.has_attachments, m.headers_fetched, m.body_fetched
               FROM messages_fts f
               JOIN messages m ON m.rowid = f.rowid
               WHERE messages_fts MATCH ? AND m.account_id = ?
               ORDER BY rank LIMIT 100"#,
        )
        .bind(&raw)
        .bind(&aid)
        .fetch_all(db)
        .await
        .map_err(err)
    } else {
        sqlx::query_as(
            r#"SELECT m.id, m.account_id, m.mailbox_id, m.uid, m.message_id, m.subject,
                      m.sender_name, m.sender_email, m.recipients, m.date_str, m.date_ts,
                      m.flags, m.preview, m.body_text, m.body_html, m.in_reply_to,
                      m.references_hdr, m.has_attachments, m.headers_fetched, m.body_fetched
               FROM messages_fts f
               JOIN messages m ON m.rowid = f.rowid
               WHERE messages_fts MATCH ?
               ORDER BY rank LIMIT 100"#,
        )
        .bind(&raw)
        .fetch_all(db)
        .await
        .map_err(err)
    }
}


#[tauri::command]
pub async fn send_email(
    account_id: String,
    msg: OutgoingMessage,
    state: State<'_, AppDb>,
) -> Result<(), String> {
    let db = &*state.0;

    let account: Account = sqlx::query_as(
        "SELECT id, name, email, imap_host, imap_port, imap_tls, smtp_host, smtp_port, smtp_tls FROM accounts WHERE id = ?",
    )
    .bind(&account_id)
    .fetch_one(db)
    .await
    .map_err(err)?;

    smtp::send(
        &account.id,
        &account.smtp_host,
        account.smtp_port as u16,
        account.smtp_tls,
        &account.email,
        &msg,
    )
    .await
    .map_err(err)
}


#[tauri::command]
pub async fn mark_read(id: String, state: State<'_, AppDb>) -> Result<(), String> {
    let db = &*state.0;
    sqlx::query(
        r#"UPDATE messages SET flags = json_remove(flags, '$[0]') WHERE id = ? AND json_type(flags, '$[0]') = 'text'"#,
    )
    .bind(&id)
    .execute(db)
    .await
    .ok();
    Ok(())
}

#[tauri::command]
pub async fn save_draft(
    account_id: String,
    mailbox_id: String,
    msg: OutgoingMessage,
    state: State<'_, AppDb>,
) -> Result<String, String> {
    let db = &*state.0;
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO messages (id, account_id, mailbox_id, subject, sender_name, sender_email,
           recipients, preview, body_text, flags, headers_fetched, body_fetched)
           VALUES (?,?,?,?,?,?,?,?,?,json_array('\\Draft'),1,1)"#,
    )
    .bind(&id)
    .bind(&account_id)
    .bind(&mailbox_id)
    .bind(&msg.subject)
    .bind(&msg.from_name)
    .bind(&msg.from_email)
    .bind(serde_json::to_string(&msg.to).unwrap_or_default())
    .bind(msg.body.chars().take(200).collect::<String>())
    .bind(&msg.body)
    .execute(db)
    .await
    .map_err(err)?;
    Ok(id)
}
