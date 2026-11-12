use std::collections::HashMap;
use std::sync::Arc;

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::State;

use crate::commands::unsubscribe::unsubscribe_email;
use crate::state::{get_active_id, DbState};

const RECENT_PER_SENDER: usize = 5;
const UNSUBSCRIBE_ATTEMPTS: usize = 3;

#[derive(Debug, Serialize, PartialEq)]
pub struct SubscriptionMessage {
    pub id: String,
    pub subject: String,
    pub date: String,
    pub is_read: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Subscription {
    pub sender_key: String,
    pub name: String,
    pub address: String,
    pub sender: String,
    pub count: usize,
    pub unread_count: usize,
    pub latest_ts: i64,
    pub latest_date: String,
    pub unsubscribed: bool,
    pub messages: Vec<SubscriptionMessage>,
}

struct Row {
    id: String,
    sender: String,
    subject: String,
    date: String,
    internal_ts: i64,
    is_read: bool,
    unsubscribed: bool,
}

pub(crate) fn sender_address(sender: &str) -> String {
    let address = match (sender.rfind('<'), sender.rfind('>')) {
        (Some(open), Some(close)) if open < close => &sender[open + 1..close],
        _ => sender,
    };
    address.trim().trim_matches('"').to_lowercase()
}

pub(crate) fn sender_name(sender: &str, address: &str) -> String {
    let name = match sender.find('<') {
        Some(open) => &sender[..open],
        None => "",
    };
    let name = name.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    if name.is_empty() { address.to_string() } else { name.to_string() }
}

fn load_rows(conn: &Connection, account_id: i64) -> rusqlite::Result<Vec<Row>> {
    let mut stmt = conn.prepare(
        "SELECT id, sender, subject, date, internal_ts, is_read, unsubscribed FROM emails
         WHERE account_id=?1 AND mailbox IN ('INBOX','ARCHIVE') AND TRIM(list_unsubscribe)<>''
         ORDER BY internal_ts DESC",
    )?;
    let rows = stmt.query_map(params![account_id], |r| {
        Ok(Row {
            id: r.get(0)?,
            sender: r.get(1)?,
            subject: r.get(2)?,
            date: r.get(3)?,
            internal_ts: r.get(4)?,
            is_read: r.get::<_, i32>(5)? != 0,
            unsubscribed: r.get::<_, i32>(6)? != 0,
        })
    })?;
    rows.collect()
}

/// Rows must be ordered newest first; subscriptions come out ordered by latest message.
fn group(rows: Vec<Row>) -> Vec<Subscription> {
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<Subscription> = Vec::new();

    for row in rows {
        let key = sender_address(&row.sender);
        if key.is_empty() {
            continue;
        }
        let slot = *index.entry(key.clone()).or_insert_with(|| {
            out.push(Subscription {
                name: sender_name(&row.sender, &key),
                address: key.clone(),
                sender: row.sender.clone(),
                sender_key: key.clone(),
                count: 0,
                unread_count: 0,
                latest_ts: row.internal_ts,
                latest_date: row.date.clone(),
                unsubscribed: false,
                messages: Vec::new(),
            });
            out.len() - 1
        });
        let sub = &mut out[slot];
        sub.count += 1;
        sub.unsubscribed |= row.unsubscribed;
        if !row.is_read {
            sub.unread_count += 1;
        }
        if sub.messages.len() < RECENT_PER_SENDER {
            sub.messages.push(SubscriptionMessage {
                id: row.id,
                subject: row.subject,
                date: row.date,
                is_read: row.is_read,
            });
        }
    }
    out
}

#[tauri::command]
pub async fn get_subscriptions(state: State<'_, Arc<DbState>>) -> Result<Vec<Subscription>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    let rows = load_rows(&conn, account_id).map_err(|e| e.to_string())?;
    Ok(group(rows))
}

#[tauri::command]
pub async fn get_sender_emails(
    state: State<'_, Arc<DbState>>,
    sender_key: String,
) -> Result<Vec<SubscriptionMessage>, String> {
    let account_id = get_active_id(&state).await;
    let key = sender_key.trim().to_lowercase();
    let conn = state.conn.lock().await;
    let rows = load_rows(&conn, account_id).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter(|row| sender_address(&row.sender) == key)
        .map(|row| SubscriptionMessage {
            id: row.id,
            subject: row.subject,
            date: row.date,
            is_read: row.is_read,
        })
        .collect())
}

#[tauri::command]
pub async fn unsubscribe_sender(
    state: State<'_, Arc<DbState>>,
    sender_key: String,
) -> Result<(), String> {
    let account_id = get_active_id(&state).await;
    let key = sender_key.trim().to_lowercase();

    let ids: Vec<String> = {
        let conn = state.conn.lock().await;
        load_rows(&conn, account_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|row| sender_address(&row.sender) == key)
            .map(|row| row.id)
            .collect()
    };
    if ids.is_empty() {
        return Err(format!("No subscription found for {}", key));
    }

    let mut last_error = String::new();
    for id in ids.iter().take(UNSUBSCRIBE_ATTEMPTS) {
        match unsubscribe_email(&state, account_id, id).await {
            Ok(()) => {
                let conn = state.conn.lock().await;
                for id in &ids {
                    let _ = conn.execute(
                        "UPDATE emails SET unsubscribed=1 WHERE id=?1 AND account_id=?2",
                        params![id, account_id],
                    );
                }
                return Ok(());
            }
            Err(e) => last_error = e,
        }
    }
    Err(last_error)
}

#[tauri::command]
pub async fn get_blocked_senders(state: State<'_, Arc<DbState>>) -> Result<Vec<crate::sync::BlockedSender>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    crate::sync::list_blocked(&conn, account_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unblock_sender(state: State<'_, Arc<DbState>>, sender_key: String) -> Result<(), String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    crate::sync::unblock(&conn, account_id, &sender_key).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, sender: &str, ts: i64, read: bool, unsub: bool) -> Row {
        Row {
            id: id.into(),
            sender: sender.into(),
            subject: format!("subject {id}"),
            date: format!("d{ts}"),
            internal_ts: ts,
            is_read: read,
            unsubscribed: unsub,
        }
    }

    #[test]
    fn parses_sender_forms() {
        assert_eq!(sender_address("Acme News <News@Acme.com>"), "news@acme.com");
        assert_eq!(sender_address("\"Acme\" <a@b.c>"), "a@b.c");
        assert_eq!(sender_address(" plain@x.org "), "plain@x.org");
        assert_eq!(sender_name("\"Acme News\" <a@b.c>", "a@b.c"), "Acme News");
        assert_eq!(sender_name("<a@b.c>", "a@b.c"), "a@b.c");
        assert_eq!(sender_name("a@b.c", "a@b.c"), "a@b.c");
    }

    #[test]
    fn groups_by_address_newest_first() {
        let subs = group(vec![
            row("3", "Acme <news@acme.com>", 30, false, false),
            row("2", "Other <o@o.com>", 20, true, false),
            row("1", "ACME Team <NEWS@acme.com>", 10, false, true),
        ]);
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].address, "news@acme.com");
        assert_eq!(subs[0].name, "Acme");
        assert_eq!(subs[0].count, 2);
        assert_eq!(subs[0].unread_count, 2);
        assert_eq!(subs[0].latest_ts, 30);
        assert!(subs[0].unsubscribed);
        assert!(!subs[1].unsubscribed);
    }

    #[test]
    fn caps_recent_messages() {
        let rows = (0..9).map(|i| row(&i.to_string(), "a@b.c", 100 - i, true, false)).collect();
        let subs = group(rows);
        assert_eq!(subs[0].count, 9);
        assert_eq!(subs[0].messages.len(), RECENT_PER_SENDER);
    }
}
