use rusqlite::{params, Connection, Result};

use super::MailboxSyncState;

pub fn get_mailbox_sync_state(conn: &Connection, account_id: i64, mailbox_name: &str) -> Result<Option<MailboxSyncState>> {
    let mut stmt = conn.prepare(
        "SELECT account_id, mailbox_name, highest_uid, uidvalidity, last_synced_at
         FROM mailbox_sync_state WHERE account_id = ?1 AND mailbox_name = ?2"
    )?;
    let mut rows = stmt.query(params![account_id, mailbox_name])?;
    if let Some(row) = rows.next()? {
        Ok(Some(MailboxSyncState {
            account_id: row.get(0)?,
            mailbox_name: row.get(1)?,
            highest_uid: row.get::<_, i64>(2)? as u32,
            uidvalidity: row.get::<_, i64>(3)? as u32,
            last_synced_at: row.get(4)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn set_mailbox_sync_state(conn: &Connection, state: &MailboxSyncState) -> Result<()> {
    conn.execute(
        "INSERT INTO mailbox_sync_state (account_id, mailbox_name, highest_uid, uidvalidity, last_synced_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(account_id, mailbox_name) DO UPDATE SET
            highest_uid = excluded.highest_uid,
            uidvalidity = excluded.uidvalidity,
            last_synced_at = excluded.last_synced_at",
        params![state.account_id, state.mailbox_name, state.highest_uid as i64, state.uidvalidity as i64, state.last_synced_at],
    )?;
    Ok(())
}

pub fn get_gmail_history_id(conn: &Connection, account_id: i64) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT history_id FROM gmail_sync_state WHERE account_id = ?1"
    )?;
    let mut rows = stmt.query(params![account_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

pub fn set_gmail_history_id(conn: &Connection, account_id: i64, history_id: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO gmail_sync_state (account_id, history_id, last_synced_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(account_id) DO UPDATE SET
            history_id = excluded.history_id,
            last_synced_at = excluded.last_synced_at",
        params![account_id, history_id, crate::state::now_epoch()],
    )?;
    Ok(())
}

pub fn gmail_message_cached(conn: &Connection, account_id: i64, message_id: &str) -> Result<bool> {
    let composite_id = format!("{}:{}", account_id, message_id);
    let cached: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM emails WHERE id = ?1 AND account_id = ?2 AND body_html != '')",
        params![composite_id, account_id],
        |r| r.get(0),
    ).unwrap_or(false);
    Ok(cached)
}
