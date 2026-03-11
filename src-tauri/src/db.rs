use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::path::Path;

pub type Db = Pool<Sqlite>;

pub async fn open(db_path: &Path) -> Result<Db> {
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

async fn migrate(pool: &Db) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS accounts (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            email       TEXT NOT NULL UNIQUE,
            imap_host   TEXT NOT NULL,
            imap_port   INTEGER NOT NULL DEFAULT 993,
            imap_tls    INTEGER NOT NULL DEFAULT 1,
            smtp_host   TEXT NOT NULL,
            smtp_port   INTEGER NOT NULL DEFAULT 587,
            smtp_tls    INTEGER NOT NULL DEFAULT 1,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS mailboxes (
            id          TEXT PRIMARY KEY,
            account_id  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            name        TEXT NOT NULL,
            full_name   TEXT NOT NULL,
            flags       TEXT NOT NULL DEFAULT '',
            uid_next    INTEGER,
            uid_validity INTEGER,
            UNIQUE(account_id, full_name)
        );

        CREATE TABLE IF NOT EXISTS messages (
            id              TEXT PRIMARY KEY,
            account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            mailbox_id      TEXT NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
            uid             INTEGER,
            message_id      TEXT,
            subject         TEXT NOT NULL DEFAULT '',
            sender_name     TEXT NOT NULL DEFAULT '',
            sender_email    TEXT NOT NULL DEFAULT '',
            recipients      TEXT NOT NULL DEFAULT '[]',
            date_str        TEXT NOT NULL DEFAULT '',
            date_ts         INTEGER NOT NULL DEFAULT 0,
            flags           TEXT NOT NULL DEFAULT '[]',
            preview         TEXT NOT NULL DEFAULT '',
            body_text       TEXT,
            body_html       TEXT,
            in_reply_to     TEXT,
            references_hdr  TEXT,
            has_attachments INTEGER NOT NULL DEFAULT 0,
            headers_fetched INTEGER NOT NULL DEFAULT 0,
            body_fetched    INTEGER NOT NULL DEFAULT 0,
            UNIQUE(account_id, mailbox_id, uid)
        );

        CREATE TABLE IF NOT EXISTS attachments (
            id          TEXT PRIMARY KEY,
            message_id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
            filename    TEXT NOT NULL,
            mime_type   TEXT NOT NULL DEFAULT 'application/octet-stream',
            size        INTEGER NOT NULL DEFAULT 0,
            data        BLOB
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            subject, sender_name, sender_email, preview, body_text,
            content='messages', content_rowid='rowid'
        );

        CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
            INSERT INTO messages_fts(rowid, subject, sender_name, sender_email, preview, body_text)
            VALUES (new.rowid, new.subject, new.sender_name, new.sender_email, new.preview, new.body_text);
        END;

        CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, subject, sender_name, sender_email, preview, body_text)
            VALUES ('delete', old.rowid, old.subject, old.sender_name, old.sender_email, old.preview, old.body_text);
            INSERT INTO messages_fts(rowid, subject, sender_name, sender_email, preview, body_text)
            VALUES (new.rowid, new.subject, new.sender_name, new.sender_email, new.preview, new.body_text);
        END;

        CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, subject, sender_name, sender_email, preview, body_text)
            VALUES ('delete', old.rowid, old.subject, old.sender_name, old.sender_email, old.preview, old.body_text);
        END;
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
