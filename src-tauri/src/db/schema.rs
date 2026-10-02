use rusqlite::{params, Connection, Result};

pub fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS accounts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            email TEXT NOT NULL UNIQUE,
            provider TEXT NOT NULL DEFAULT 'gmail',
            display_name TEXT,
            is_active INTEGER NOT NULL DEFAULT 0,
            access_token TEXT,
            refresh_token TEXT,
            expires_at_epoch INTEGER,
            imap_host TEXT,
            imap_port INTEGER,
            smtp_host TEXT,
            smtp_port INTEGER,
            username TEXT,
            encrypted_password TEXT
        );
    ")?;

    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS emails (
            id TEXT NOT NULL,
            account_id INTEGER NOT NULL DEFAULT 1,
            draft_id TEXT,
            thread_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            sender TEXT NOT NULL,
            to_recipients TEXT NOT NULL DEFAULT '',
            cc_recipients TEXT NOT NULL DEFAULT '',
            bcc_recipients TEXT NOT NULL DEFAULT '',
            snippet TEXT NOT NULL DEFAULT '',
            body_html TEXT NOT NULL,
            attachments_json TEXT NOT NULL DEFAULT '[]',
            has_attachments INTEGER NOT NULL DEFAULT 0,
            date TEXT NOT NULL,
            is_read INTEGER NOT NULL,
            starred INTEGER NOT NULL DEFAULT 0,
            mailbox TEXT NOT NULL DEFAULT 'INBOX',
            labels TEXT NOT NULL DEFAULT '',
            internal_ts INTEGER NOT NULL DEFAULT 0,
            notified INTEGER NOT NULL DEFAULT 0,
            list_unsubscribe TEXT NOT NULL DEFAULT '',
            unsubscribed INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (id, account_id)
        );
    ")?;
    let legacy_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='oauth_tokens'",
        [],
        |r| r.get::<_, i64>(0),
    ).unwrap_or(0) > 0;

    if legacy_exists {
        let migrated = conn.query_row(
            "SELECT access_token, refresh_token, expires_at_epoch FROM oauth_tokens WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        );

        if let Ok((access_token, refresh_token, expires)) = migrated {
            conn.execute(
                "INSERT OR IGNORE INTO accounts (email, provider, is_active, access_token, refresh_token, expires_at_epoch)
                 VALUES ('migrated@gmail.com', 'gmail', 1, ?1, ?2, ?3)",
                params![access_token, refresh_token, expires],
            )?;
        }

        let _ = conn.execute("ALTER TABLE emails ADD COLUMN account_id INTEGER NOT NULL DEFAULT 1", []);
    }

    let _ = conn.execute(
        "DELETE FROM accounts WHERE email = 'migrated@gmail.com' AND NOT EXISTS (
            SELECT 1 FROM emails WHERE account_id = accounts.id
        )",
        [],
    );

    let _ = conn.execute("ALTER TABLE emails ADD COLUMN account_id INTEGER NOT NULL DEFAULT 1", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN snippet TEXT NOT NULL DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN to_recipients TEXT NOT NULL DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN cc_recipients TEXT NOT NULL DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN bcc_recipients TEXT NOT NULL DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN starred INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN mailbox TEXT NOT NULL DEFAULT 'INBOX'", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN labels TEXT NOT NULL DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN internal_ts INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN draft_id TEXT", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN attachments_json TEXT NOT NULL DEFAULT '[]'", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN has_attachments INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN notified INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN list_unsubscribe TEXT NOT NULL DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN unsubscribed INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN category_id INTEGER", []);
    let _ = conn.execute("ALTER TABLE accounts ADD COLUMN notify_ready INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN imap_uid INTEGER", []);
    let _ = conn.execute("ALTER TABLE emails ADD COLUMN imap_uid_mailbox TEXT", []);

    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS inbox_categories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id INTEGER,
            slug TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            icon TEXT NOT NULL DEFAULT 'tag',
            color TEXT NOT NULL DEFAULT '#6c7065',
            sort_order INTEGER NOT NULL DEFAULT 0,
            is_fixed INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS inbox_smart_state (
            account_id INTEGER PRIMARY KEY,
            initialized INTEGER NOT NULL DEFAULT 0,
            enabled INTEGER NOT NULL DEFAULT 1
        );
    ")?;
    let _ = conn.execute("ALTER TABLE inbox_categories ADD COLUMN color TEXT NOT NULL DEFAULT '#6c7065'", []);
    let _ = conn.execute("ALTER TABLE inbox_categories ADD COLUMN account_id INTEGER", []);
    let _ = conn.execute("ALTER TABLE inbox_smart_state ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1", []);

    let _ = conn.execute(
        "UPDATE inbox_smart_state SET initialized = 0 WHERE account_id NOT IN (
            SELECT DISTINCT account_id FROM inbox_categories WHERE account_id IS NOT NULL AND slug LIKE 'account-%'
         )",
        [],
    );
    let _ = conn.execute("DELETE FROM inbox_categories WHERE account_id IS NULL", []);
    let _ = conn.execute(
        "UPDATE emails SET category_id=NULL WHERE category_id IS NOT NULL
         AND category_id NOT IN (SELECT id FROM inbox_categories)",
        [],
    );
    conn.execute(
        "INSERT OR IGNORE INTO inbox_smart_state (account_id, initialized)
         SELECT id, 0 FROM accounts",
        [],
    )?;

    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS mailbox_sync_state (
            account_id INTEGER NOT NULL,
            mailbox_name TEXT NOT NULL,
            highest_uid INTEGER NOT NULL DEFAULT 0,
            uidvalidity INTEGER NOT NULL DEFAULT 0,
            last_synced_at INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (account_id, mailbox_name)
        );
    ")?;

    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS gmail_sync_state (
            account_id INTEGER PRIMARY KEY,
            history_id TEXT NOT NULL,
            last_synced_at INTEGER NOT NULL DEFAULT 0
        );
    ")?;

    let _ = conn.execute(
        "DELETE FROM emails WHERE subject = 'Welcome to Verdant' AND sender = 'foo@example.com'",
        [],
    );

    conn.execute_batch("
        CREATE INDEX IF NOT EXISTS idx_emails_mailbox_ts
            ON emails(account_id, mailbox, internal_ts);
        CREATE INDEX IF NOT EXISTS idx_emails_thread_ts
            ON emails(thread_id, mailbox, account_id, internal_ts);
        CREATE INDEX IF NOT EXISTS idx_emails_thread_id
            ON emails(thread_id, account_id);
    ")?;

    Ok(())
}
