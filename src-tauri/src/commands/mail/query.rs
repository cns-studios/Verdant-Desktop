use std::sync::Arc;

use rusqlite::{params, Connection, Row};
use tauri::State;

use crate::db::{map_email_row, Email, EMAIL_COLUMNS, EMAIL_LIST_COLUMNS};
use crate::state::{get_active_id, DbState};

#[derive(serde::Serialize)]
pub struct MailboxCounts {
    pub inbox_total: i64,
    pub inbox_unread: i64,
    pub starred_total: i64,
    pub sent_total: i64,
    pub drafts_total: i64,
    pub archive_total: i64,
    pub trash_total: i64,
}

#[derive(serde::Serialize)]
pub struct ThreadSummary {
    pub thread_id: String,
    pub subject: String,
    pub participants: String,
    pub snippet: String,
    pub latest_ts: i64,
    pub latest_date: String,
    pub message_count: i64,
    pub unread_count: i64,
    pub is_read: bool,
    pub starred: bool,
    pub has_attachments: bool,
    pub labels: String,
}

fn query_columns(conn: &Connection, columns: &str, filter: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<Email>, String> {
    let mut stmt = conn
        .prepare_cached(&format!("SELECT {columns} FROM emails WHERE {filter}"))
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map(args, map_email_row).map_err(|e| e.to_string())?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn query_emails(conn: &Connection, filter: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<Email>, String> {
    query_columns(conn, EMAIL_COLUMNS, filter, args)
}

fn related_by_subject(conn: &Connection, account_id: i64, seed: &Email) -> Result<Vec<Email>, String> {
    let target = normalize_subject(&seed.subject);
    let mut stmt = conn
        .prepare_cached("SELECT id, subject FROM emails WHERE account_id=?1 AND id<>?2 ORDER BY internal_ts ASC, rowid ASC")
        .map_err(|e| e.to_string())?;
    let ids: Vec<String> = stmt
        .query_map(params![account_id, seed.id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|(_, subject)| normalize_subject(subject) == target)
        .map(|(id, _)| id)
        .collect();

    let mut related = Vec::with_capacity(ids.len());
    for id in ids {
        related.extend(query_emails(conn, "id=?1 AND account_id=?2", params![id, account_id])?);
    }
    Ok(related)
}

pub fn thread_summaries(conn: &Connection, account_id: i64, category_slug: Option<&str>) -> Result<Vec<ThreadSummary>, String> {
    let (join, filter) = match category_slug {
        Some(_) => ("JOIN inbox_categories c ON c.id=e.category_id", "AND c.slug=?2"),
        None => ("", "AND ?2 IS NULL"),
    };
    let sql = format!(
        "SELECT e.thread_id, latest.subject, latest.snippet, latest.date, latest.labels,
                COUNT(e.id), SUM(CASE WHEN e.is_read=0 THEN 1 ELSE 0 END),
                MAX(e.internal_ts), MAX(e.starred), MAX(e.has_attachments),
                GROUP_CONCAT(DISTINCT e.sender)
         FROM emails e
         {join}
         INNER JOIN emails latest ON latest.id = (
             SELECT id FROM emails
             WHERE thread_id = e.thread_id AND mailbox = 'INBOX' AND account_id = ?1
             ORDER BY internal_ts DESC LIMIT 1
         )
         WHERE e.mailbox='INBOX' AND e.account_id=?1 {filter}
         GROUP BY e.thread_id
         ORDER BY MAX(e.internal_ts) DESC
         LIMIT 500"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![account_id, category_slug], map_thread_row)
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn map_thread_row(row: &Row<'_>) -> rusqlite::Result<ThreadSummary> {
    let unread_count: i64 = row.get(6)?;
    Ok(ThreadSummary {
        thread_id: row.get(0)?,
        subject: row.get(1)?,
        snippet: row.get(2)?,
        latest_date: row.get(3)?,
        labels: row.get(4)?,
        message_count: row.get(5)?,
        unread_count,
        latest_ts: row.get(7)?,
        starred: row.get::<_, i64>(8)? != 0,
        has_attachments: row.get::<_, i64>(9)? != 0,
        is_read: unread_count == 0,
        participants: row.get(10).unwrap_or_default(),
    })
}

fn normalize_subject(subject: &str) -> String {
    let mut normalized = subject.trim().to_lowercase();
    loop {
        let next = normalized
            .strip_prefix("re:")
            .or_else(|| normalized.strip_prefix("fw:"))
            .or_else(|| normalized.strip_prefix("fwd:"));
        match next {
            Some(value) => normalized = value.trim().to_string(),
            None => break,
        }
    }
    normalized
}

#[tauri::command]
pub async fn get_emails(state: State<'_, Arc<DbState>>, mailbox: Option<String>) -> Result<Vec<Email>, String> {
    let account_id = get_active_id(&state).await;
    let box_name = mailbox.unwrap_or_else(|| "INBOX".to_string());
    let conn = state.conn.lock().await;

    if box_name == "STARRED" {
        query_columns(
            &conn,
            EMAIL_LIST_COLUMNS,
            "starred=1 AND account_id=?1 ORDER BY internal_ts DESC, rowid DESC LIMIT 500",
            params![account_id],
        )
    } else {
        query_columns(
            &conn,
            EMAIL_LIST_COLUMNS,
            "mailbox=?1 AND account_id=?2 ORDER BY internal_ts DESC, rowid DESC LIMIT 500",
            params![box_name, account_id],
        )
    }
}

#[tauri::command]
pub async fn get_mailbox_counts(state: State<'_, Arc<DbState>>) -> Result<MailboxCounts, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;

    let mut counts = MailboxCounts {
        inbox_total: 0,
        inbox_unread: 0,
        starred_total: 0,
        sent_total: 0,
        drafts_total: 0,
        archive_total: 0,
        trash_total: 0,
    };

    let mut stmt = conn
        .prepare_cached(
            "SELECT mailbox, COUNT(*), SUM(CASE WHEN is_read=0 THEN 1 ELSE 0 END)
             FROM emails WHERE account_id=?1 GROUP BY mailbox",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![account_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
        })
        .map_err(|e| e.to_string())?;
    for (mailbox, total, unread) in rows.filter_map(Result::ok) {
        match mailbox.as_str() {
            "INBOX" => {
                counts.inbox_total = total;
                counts.inbox_unread = unread;
            }
            "SENT" => counts.sent_total = total,
            "DRAFT" => counts.drafts_total = total,
            "ARCHIVE" => counts.archive_total = total,
            "TRASH" => counts.trash_total = total,
            _ => {}
        }
    }
    counts.starred_total = conn
        .query_row("SELECT COUNT(*) FROM emails WHERE account_id=?1 AND starred=1", params![account_id], |r| r.get(0))
        .unwrap_or(0);

    Ok(counts)
}

#[tauri::command]
pub async fn get_inbox_threads(state: State<'_, Arc<DbState>>) -> Result<Vec<ThreadSummary>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    thread_summaries(&conn, account_id, None)
}

#[tauri::command]
pub async fn get_thread_messages(state: State<'_, Arc<DbState>>, thread_id: String) -> Result<Vec<Email>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;

    let mut emails = query_emails(
        &conn,
        "thread_id=?1 AND account_id=?2 ORDER BY internal_ts ASC, rowid ASC",
        params![thread_id, account_id],
    )?;

    if emails.len() <= 1 {
        if let Some(seed) = emails.first() {
            let related = related_by_subject(&conn, account_id, seed)?;
            emails.extend(related);
        }
    }

    Ok(emails)
}

#[tauri::command]
pub async fn get_email(state: State<'_, Arc<DbState>>, email_id: String) -> Result<Option<Email>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    Ok(query_emails(&conn, "id=?1 AND account_id=?2", params![email_id, account_id])?.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        conn.execute("INSERT INTO accounts (id, email) VALUES (1, 'a@example.com')", []).unwrap();
        conn.execute(
            "INSERT INTO inbox_categories (id, account_id, slug, name) VALUES (7, 1, 'account-1-work', 'Work')",
            [],
        )
        .unwrap();
        for (id, thread, ts, read, category) in [("a", "t1", 1, 1, Some(7)), ("b", "t1", 3, 0, Some(7)), ("c", "t2", 2, 1, None)] {
            conn.execute(
                "INSERT INTO emails (id, account_id, thread_id, subject, sender, mailbox, body_html, date, is_read, internal_ts, category_id)
                 VALUES (?1, 1, ?2, ?1, 'x@y.z', 'INBOX', '', '', ?3, ?4, ?5)",
                params![id, thread, read, ts, category],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn threads_are_grouped_newest_first() {
        let conn = setup();
        let threads = thread_summaries(&conn, 1, None).unwrap();
        let ids: Vec<_> = threads.iter().map(|t| t.thread_id.as_str()).collect();
        assert_eq!(ids, ["t1", "t2"]);
        assert_eq!(threads[0].subject, "b");
        assert_eq!(threads[0].message_count, 2);
        assert_eq!(threads[0].unread_count, 1);
        assert!(!threads[0].is_read);
    }

    #[test]
    fn category_filter_only_returns_its_threads() {
        let conn = setup();
        let threads = thread_summaries(&conn, 1, Some("account-1-work")).unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].thread_id, "t1");
        assert!(thread_summaries(&conn, 1, Some("missing")).unwrap().is_empty());
    }

    #[test]
    fn list_columns_leave_out_the_body() {
        let conn = setup();
        conn.execute("UPDATE emails SET body_html='<p>hi</p>'", []).unwrap();
        let listed = query_columns(&conn, EMAIL_LIST_COLUMNS, "account_id=?1", params![1]).unwrap();
        assert!(listed.iter().all(|email| email.body_html.is_empty()));
        let full = query_emails(&conn, "id='a' AND account_id=?1", params![1]).unwrap();
        assert_eq!(full[0].body_html, "<p>hi</p>");
    }

    #[test]
    fn single_message_threads_pick_up_replies_by_subject() {
        let conn = setup();
        conn.execute(
            "INSERT INTO emails (id, account_id, thread_id, subject, sender, mailbox, body_html, date, is_read, internal_ts)
             VALUES ('d', 1, 't9', 'Re: c', 'x@y.z', 'SENT', '', '', 1, 9)",
            [],
        )
        .unwrap();
        let seed = query_emails(&conn, "id='c' AND account_id=?1", params![1]).unwrap().remove(0);
        let related = related_by_subject(&conn, 1, &seed).unwrap();
        assert_eq!(related.iter().map(|email| email.id.as_str()).collect::<Vec<_>>(), ["d"]);
    }

    #[test]
    fn subjects_are_normalised_for_thread_fallback() {
        assert_eq!(normalize_subject("Re: FWD: re:  Hello "), "hello");
    }
}
