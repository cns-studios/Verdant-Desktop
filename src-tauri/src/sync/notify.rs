use crate::db::Account;
use crate::state::DbState;

pub(crate) async fn notify_new_mail(app: &tauri::AppHandle, state: &DbState, account: &Account) {
    use tauri::Manager;
    let account_id = account.id;
    let (show, important_only, language) = match app.try_state::<crate::commands::app_config::AppConfigState>() {
        Some(config) => {
            let config = config.0.lock().await;
            (config.show_notifications, config.notify_important_only, config.language.clone())
        }
        None => (true, false, "en".to_string()),
    };

    let fresh: Vec<(String, String)> = {
        let conn = state.conn.lock().await;
        match collect_new_mail(&conn, account_id, important_only) {
            Ok(rows) => rows,
            Err(e) => {
                log::error!("Collecting new mail for notifications failed account={}: {}", account_id, e);
                Vec::new()
            }
        }
    };

    if show && !fresh.is_empty() {
        use tauri_plugin_notification::NotificationExt;
        let acc_name = account.display_name.as_deref().unwrap_or(&account.email);
        let (title, body) = notification_text(&language, acc_name, &fresh);
        let _ = app.notification().builder().title(title).body(body).show();
    }

    use tauri::Emitter;
    let _ = app.emit("emails-synced", ());
}

pub(crate) fn notification_text(language: &str, account: &str, fresh: &[(String, String)]) -> (String, String) {
    let german = language == "de";
    if let [(subject, sender)] = fresh {
        let subject = match (subject.trim().is_empty(), german) {
            (false, _) => subject.as_str(),
            (true, true) => "(Kein Betreff)",
            (true, false) => "(No subject)",
        };
        return if german {
            (format!("Neue E-Mail – {account}"), format!("Von: {sender}\n{subject}"))
        } else {
            (format!("New email – {account}"), format!("From: {sender}\n{subject}"))
        };
    }
    let n = fresh.len();
    if german {
        (format!("{n} neue E-Mails – {account}"), format!("Du hast {n} neue Nachrichten im Posteingang."))
    } else {
        (format!("{n} new emails – {account}"), format!("You have {n} new messages in your inbox."))
    }
}

pub(crate) fn collect_new_mail(conn: &rusqlite::Connection, account_id: i64, important_only: bool) -> rusqlite::Result<Vec<(String, String)>> {
    const TS_SECONDS: &str = "(CASE WHEN internal_ts > 100000000000 THEN internal_ts / 1000 ELSE internal_ts END)";
    let tx = conn.unchecked_transaction()?;

    let ready: i64 = tx
        .query_row("SELECT COALESCE(notify_ready, 0) FROM accounts WHERE id=?1", [account_id], |r| r.get(0))
        .unwrap_or(1);
    if ready == 0 {
        let synced: i64 = tx.query_row(
            "SELECT (SELECT COUNT(*) FROM mailbox_sync_state WHERE account_id=?1)
                  + (SELECT COUNT(*) FROM gmail_sync_state WHERE account_id=?1)",
            [account_id],
            |r| r.get(0),
        )?;
        if synced == 0 {
            return Ok(Vec::new());
        }
        tx.execute("UPDATE emails SET notified=1 WHERE account_id=?1 AND notified=0", [account_id])?;
        tx.execute("UPDATE accounts SET notify_ready=1 WHERE id=?1", [account_id])?;
        tx.commit()?;
        return Ok(Vec::new());
    }

    tx.execute(
        &format!(
            "UPDATE emails SET notified=1 WHERE account_id=?1 AND notified=0
             AND (mailbox<>'INBOX' OR is_read=1 OR {TS_SECONDS} < strftime('%s','now') - 3600)"
        ),
        [account_id],
    )?;

    let bulk = "(list_unsubscribe<>'' OR labels LIKE '%CATEGORY_PROMOTIONS%' OR labels LIKE '%CATEGORY_SOCIAL%'
                 OR labels LIKE '%CATEGORY_UPDATES%' OR labels LIKE '%CATEGORY_FORUMS%')";
    let filter = if important_only { format!("AND NOT {bulk}") } else { String::new() };
    let fresh = {
        let mut stmt = tx.prepare(&format!(
            "SELECT subject, sender FROM emails WHERE account_id=?1 AND notified=0 {filter}
             ORDER BY internal_ts DESC"
        ))?;
        let rows = stmt.query_map([account_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    tx.execute("UPDATE emails SET notified=1 WHERE account_id=?1 AND notified=0", [account_id])?;
    tx.commit()?;
    Ok(fresh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        conn.execute("INSERT INTO accounts (id, email) VALUES (1, 'a@example.com')", []).unwrap();
        conn
    }

    #[test]
    fn notification_text_follows_the_app_language() {
        let one = vec![("".to_string(), "Mia".to_string())];
        let two = vec![("a".to_string(), "x".to_string()), ("b".to_string(), "y".to_string())];
        assert_eq!(notification_text("de", "Work", &one), ("Neue E-Mail – Work".to_string(), "Von: Mia\n(Kein Betreff)".to_string()));
        assert_eq!(notification_text("en", "Work", &one).1, "From: Mia\n(No subject)");
        assert_eq!(notification_text("de", "Work", &two).0, "2 neue E-Mails – Work");
        assert_eq!(notification_text("fr", "Work", &two).0, "2 new emails – Work");
    }

    fn insert(conn: &Connection, id: &str, ts: i64, unsub: &str) {
        conn.execute(
            "INSERT INTO emails (id, account_id, thread_id, subject, sender, mailbox, body_html, date, is_read, internal_ts, list_unsubscribe)
             VALUES (?1, 1, ?1, 'Subject', 'x@y.z', 'INBOX', '', '', 0, ?2, ?3)",
            params![id, ts, unsub],
        ).unwrap();
    }

    fn now() -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
    }

    #[test]
    fn initial_download_is_never_announced() {
        let conn = setup();
        for i in 0..120 {
            insert(&conn, &format!("m{i}"), (now() - 86_400 * i) * 1000, "");
        }
        assert!(collect_new_mail(&conn, 1, false).unwrap().is_empty());
        conn.execute("INSERT INTO gmail_sync_state (account_id, history_id) VALUES (1, 'h')", []).unwrap();
        assert!(collect_new_mail(&conn, 1, false).unwrap().is_empty());
        assert!(collect_new_mail(&conn, 1, false).unwrap().is_empty());

        insert(&conn, "fresh", now() * 1000, "");
        insert(&conn, "old-page", (now() - 86_400 * 30) * 1000, "");
        let fresh = collect_new_mail(&conn, 1, false).unwrap();
        assert_eq!(fresh.len(), 1);
        assert!(collect_new_mail(&conn, 1, false).unwrap().is_empty());
    }

    #[test]
    fn important_only_skips_bulk_mail() {
        let conn = setup();
        conn.execute("INSERT INTO mailbox_sync_state (account_id, mailbox_name) VALUES (1, 'INBOX')", []).unwrap();
        assert!(collect_new_mail(&conn, 1, true).unwrap().is_empty());
        insert(&conn, "person", now(), "");
        insert(&conn, "promo", now(), "<https://unsubscribe>");
        assert_eq!(collect_new_mail(&conn, 1, true).unwrap().len(), 1);
        assert!(collect_new_mail(&conn, 1, false).unwrap().is_empty());
    }
}
