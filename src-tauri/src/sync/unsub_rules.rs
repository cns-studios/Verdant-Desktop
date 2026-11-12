//! Local rule for senders the user has unsubscribed from: whatever they still
//! send afterwards is archived automatically instead of landing in the inbox,
//! because many senders ignore or only slowly honor unsubscribe requests.

use rusqlite::{params, Connection};

use crate::commands::mail::moves::archive_email_for;
use crate::commands::subscriptions::{sender_address, sender_name};
use crate::state::DbState;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Remembers the sender of `email_id` as unsubscribed and flags the mail we
/// already have from them (without moving it).
pub fn block_sender_of(conn: &Connection, account_id: i64, email_id: &str) -> rusqlite::Result<()> {
    let sender: String = conn.query_row(
        "SELECT sender FROM emails WHERE id=?1 AND account_id=?2",
        params![email_id, account_id],
        |r| r.get(0),
    )?;
    let key = sender_address(&sender);
    if key.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT OR IGNORE INTO unsubscribed_senders (account_id, sender_key, created_at) VALUES (?1,?2,?3)",
        params![account_id, key, now()],
    )?;

    let ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id, sender FROM emails WHERE account_id=?1 AND unsubscribed=0")?;
        let rows = stmt.query_map(params![account_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.filter_map(Result::ok)
            .filter(|(_, s)| sender_address(s) == key)
            .map(|(id, _)| id)
            .collect()
    };
    for id in ids {
        conn.execute(
            "UPDATE emails SET unsubscribed=1 WHERE id=?1 AND account_id=?2",
            params![id, account_id],
        )?;
    }
    Ok(())
}

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct BlockedSender {
    pub sender_key: String,
    pub name: String,
    pub created_at: i64,
}

pub fn list_blocked(conn: &Connection, account_id: i64) -> rusqlite::Result<Vec<BlockedSender>> {
    let mut stmt = conn.prepare(
        "SELECT sender_key, created_at FROM unsubscribed_senders WHERE account_id=?1 ORDER BY created_at DESC, sender_key",
    )?;
    let keys: Vec<(String, i64)> = stmt
        .query_map(params![account_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(Result::ok)
        .collect();
    let mut names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut stmt = conn.prepare("SELECT sender FROM emails WHERE account_id=?1 ORDER BY internal_ts DESC")?;
    for sender in stmt.query_map(params![account_id], |r| r.get::<_, String>(0))?.filter_map(Result::ok) {
        names.entry(sender_address(&sender)).or_insert(sender);
    }
    Ok(keys
        .into_iter()
        .map(|(key, created_at)| {
            let name = names.get(&key).map(|s| sender_name(s, &key)).unwrap_or_else(|| key.clone());
            BlockedSender { sender_key: key, name, created_at }
        })
        .collect())
}

/// Stops auto-archiving a sender. Their mail is un-flagged too, so the
/// startup backfill (which rebuilds the list from flagged mail) can't re-add them.
pub fn unblock(conn: &Connection, account_id: i64, sender_key: &str) -> rusqlite::Result<()> {
    let key = sender_key.trim().to_lowercase();
    conn.execute(
        "DELETE FROM unsubscribed_senders WHERE account_id=?1 AND sender_key=?2",
        params![account_id, key],
    )?;
    let ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id, sender FROM emails WHERE account_id=?1 AND unsubscribed=1")?;
        let rows = stmt.query_map(params![account_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.filter_map(Result::ok)
            .filter(|(_, s)| sender_address(s) == key)
            .map(|(id, _)| id)
            .collect()
    };
    for id in ids {
        conn.execute("UPDATE emails SET unsubscribed=0 WHERE id=?1 AND account_id=?2", params![id, account_id])?;
    }
    Ok(())
}

/// Inbox mail not yet handled (`unsubscribed=0`) from a blocked sender.
/// Mail the user later moves back to the inbox keeps `unsubscribed=1`, so it
/// is not archived a second time.
fn pending_ids(conn: &Connection, account_id: i64) -> rusqlite::Result<Vec<String>> {
    let blocked: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("SELECT sender_key FROM unsubscribed_senders WHERE account_id=?1")?;
        let rows = stmt.query_map(params![account_id], |r| r.get::<_, String>(0))?;
        rows.filter_map(Result::ok).collect()
    };
    if blocked.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT id, sender FROM emails WHERE account_id=?1 AND mailbox='INBOX' AND unsubscribed=0",
    )?;
    let rows = stmt.query_map(params![account_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows
        .filter_map(Result::ok)
        .filter(|(_, s)| blocked.contains(&sender_address(s)))
        .map(|(id, _)| id)
        .collect())
}

pub async fn suppress_unsubscribed(state: &DbState, account_id: i64) {
    let ids = {
        let conn = state.conn.lock().await;
        match pending_ids(&conn, account_id) {
            Ok(ids) => ids,
            Err(e) => {
                log::error!("Unsubscribed-sender lookup failed account={}: {}", account_id, e);
                return;
            }
        }
    };
    for id in ids {
        match archive_email_for(state, account_id, &id).await {
            Ok(()) => {
                let conn = state.conn.lock().await;
                let _ = conn.execute(
                    "UPDATE emails SET unsubscribed=1, is_read=1 WHERE id=?1 AND account_id=?2",
                    params![id, account_id],
                );
            }
            // Leave unsubscribed=0 so the next sync retries (e.g. offline / rate limit).
            Err(e) => log::warn!("Auto-archive of unsubscribed mail {} failed: {}", id, e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE emails (id TEXT, account_id INTEGER, sender TEXT, mailbox TEXT, internal_ts INTEGER DEFAULT 0, unsubscribed INTEGER DEFAULT 0);
             CREATE TABLE unsubscribed_senders (account_id INTEGER NOT NULL, sender_key TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (account_id, sender_key));",
        ).unwrap();
        conn
    }

    fn add(conn: &Connection, id: &str, sender: &str, mailbox: &str) {
        conn.execute("INSERT INTO emails (id, account_id, sender, mailbox) VALUES (?1,1,?2,?3)", params![id, sender, mailbox]).unwrap();
    }

    #[test]
    fn new_mail_from_blocked_sender_is_pending_but_old_mail_is_not() {
        let conn = setup();
        add(&conn, "old", "News <n@x.com>", "INBOX");
        add(&conn, "other", "Friend <f@y.com>", "INBOX");
        block_sender_of(&conn, 1, "old").unwrap();
        assert!(pending_ids(&conn, 1).unwrap().is_empty());

        add(&conn, "new", "\"News\" <N@X.com>", "INBOX");
        add(&conn, "new-archived", "n@x.com", "ARCHIVE");
        assert_eq!(pending_ids(&conn, 1).unwrap(), vec!["new".to_string()]);
    }

    #[test]
    fn unblocking_stops_the_rule_and_unflags_mail() {
        let conn = setup();
        add(&conn, "old", "News <n@x.com>", "INBOX");
        block_sender_of(&conn, 1, "old").unwrap();
        let listed = list_blocked(&conn, 1).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].sender_key, "n@x.com");
        assert_eq!(listed[0].name, "News");

        unblock(&conn, 1, "N@x.com").unwrap();
        assert!(list_blocked(&conn, 1).unwrap().is_empty());
        add(&conn, "new", "News <n@x.com>", "INBOX");
        assert!(pending_ids(&conn, 1).unwrap().is_empty());
        let flagged: i64 = conn.query_row("SELECT COUNT(*) FROM emails WHERE unsubscribed=1", [], |r| r.get(0)).unwrap();
        assert_eq!(flagged, 0);
    }
}
