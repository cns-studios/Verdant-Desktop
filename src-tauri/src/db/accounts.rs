use rusqlite::{params, Connection, Result};

use super::{Account, StoredToken};

pub fn get_all_accounts(conn: &Connection) -> Result<Vec<Account>> {
    let mut stmt = conn.prepare(
        "SELECT id, email, provider, display_name, is_active,
                access_token, refresh_token, expires_at_epoch,
                imap_host, imap_port, smtp_host, smtp_port, username, encrypted_password
         FROM accounts ORDER BY id ASC"
    )?;
    let accounts = stmt.query_map([], map_account_row)?
        .filter_map(Result::ok)
        .collect();
    Ok(accounts)
}

pub fn get_active_account(conn: &Connection) -> Result<Option<Account>> {
    let mut stmt = conn.prepare(
        "SELECT id, email, provider, display_name, is_active,
                access_token, refresh_token, expires_at_epoch,
                imap_host, imap_port, smtp_host, smtp_port, username, encrypted_password
         FROM accounts WHERE is_active = 1 LIMIT 1"
    )?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(map_account_row(row)?));
    }
    Ok(None)
}

pub fn get_account_by_id(conn: &Connection, id: i64) -> Result<Option<Account>> {
    let mut stmt = conn.prepare(
        "SELECT id, email, provider, display_name, is_active,
                access_token, refresh_token, expires_at_epoch,
                imap_host, imap_port, smtp_host, smtp_port, username, encrypted_password
         FROM accounts WHERE id = ?1"
    )?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(map_account_row(row)?));
    }
    Ok(None)
}

pub(crate) fn map_account_row(row: &rusqlite::Row<'_>) -> Result<Account> {
    Ok(Account {
        id: row.get(0)?,
        email: row.get(1)?,
        provider: row.get(2)?,
        display_name: row.get(3)?,
        is_active: row.get::<_, i64>(4)? != 0,
        access_token: row.get(5)?,
        refresh_token: row.get(6)?,
        expires_at_epoch: row.get(7)?,
        imap_host: row.get(8)?,
        imap_port: row.get(9)?,
        smtp_host: row.get(10)?,
        smtp_port: row.get(11)?,
        username: row.get(12)?,
        encrypted_password: row.get(13)?,
    })
}

pub fn set_active_account(conn: &Connection, account_id: i64) -> Result<()> {
    conn.execute("UPDATE accounts SET is_active = 0", [])?;
    conn.execute("UPDATE accounts SET is_active = 1 WHERE id = ?1", params![account_id])?;
    Ok(())
}

pub fn upsert_gmail_account(conn: &Connection, email: &str, token: &StoredToken) -> Result<i64> {
    conn.execute(
        "INSERT INTO accounts (email, provider, is_active, access_token, refresh_token, expires_at_epoch)
         VALUES (?1, 'gmail', 0, ?2, ?3, ?4)
         ON CONFLICT(email) DO UPDATE SET
            access_token = excluded.access_token,
            refresh_token = COALESCE(excluded.refresh_token, accounts.refresh_token),
            expires_at_epoch = excluded.expires_at_epoch",
        params![email, token.access_token, token.refresh_token, token.expires_at_epoch],
    )?;
    let id = conn.query_row(
        "SELECT id FROM accounts WHERE email = ?1",
        params![email],
        |r| r.get::<_, i64>(0),
    )?;
    Ok(id)
}

pub fn insert_imap_account(
    conn: &Connection,
    email: &str,
    display_name: Option<&str>,
    imap_host: &str,
    imap_port: i64,
    smtp_host: &str,
    smtp_port: i64,
    username: &str,
    encrypted_password: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO accounts (email, provider, display_name, is_active, imap_host, imap_port, smtp_host, smtp_port, username, encrypted_password)
         VALUES (?1, 'imap', ?2, 0, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![email, display_name, imap_host, imap_port, smtp_host, smtp_port, username, encrypted_password],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_gmail_token(conn: &Connection, account_id: i64, token: &StoredToken) -> Result<()> {
    conn.execute(
        "UPDATE accounts SET access_token = ?1, refresh_token = COALESCE(?2, refresh_token), expires_at_epoch = ?3 WHERE id = ?4",
        params![token.access_token, token.refresh_token, token.expires_at_epoch, account_id],
    )?;
    Ok(())
}

pub fn delete_account(conn: &Connection, account_id: i64) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for table in ["emails", "mailbox_sync_state", "gmail_sync_state", "inbox_categories", "inbox_smart_state"] {
        tx.execute(&format!("DELETE FROM {table} WHERE account_id = ?1"), params![account_id])?;
    }
    tx.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
    tx.commit()
}
