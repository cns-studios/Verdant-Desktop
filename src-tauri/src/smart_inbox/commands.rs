use std::sync::atomic::Ordering;
use std::sync::Arc;

use rusqlite::{params, Connection};
use tauri::State;

use super::assign::categorize_all;
use super::plan::plan_categories;
use super::{CategorizeProgress, CategorizeResult, InboxCategory, PlannedCategory, CATEGORIZE_ABORT, CATEGORIZE_DONE, CATEGORIZE_TOTAL};
use crate::commands::mail::{thread_summaries, ThreadSummary};
use crate::db::{map_email_row, Email, EMAIL_COLUMNS};
use crate::state::{get_active_id, DbState};

pub(crate) fn category_id(conn: &Connection, account_id: i64, slug: &str) -> Option<i64> {
    conn.query_row(
        "SELECT id FROM inbox_categories WHERE account_id=?1 AND slug=?2",
        params![account_id, slug],
        |r| r.get(0),
    )
    .ok()
}

#[tauri::command]
pub async fn preview_inbox_categories(state: State<'_, Arc<DbState>>) -> Result<Vec<PlannedCategory>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    plan_categories(&conn, account_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_inbox_categories(
    state: State<'_, Arc<DbState>>,
) -> Result<Vec<InboxCategory>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    let initialized: i64 = conn.query_row(
        "SELECT COALESCE(initialized, 0) FROM inbox_smart_state WHERE account_id=?1 AND COALESCE(enabled,1)=1",
        [account_id],
        |r| r.get(0),
    ).unwrap_or(0);
    if initialized == 0 {
        return Ok(Vec::new());
    }

    let mut stmt = conn
        .prepare(
            "SELECT c.id,c.slug,c.name,c.icon,c.color,c.sort_order,
                COUNT(e.id),COALESCE(SUM(CASE WHEN e.is_read=0 THEN 1 ELSE 0 END),0)
         FROM inbox_categories c
         LEFT JOIN emails e ON e.category_id=c.id AND e.account_id=?1 AND e.mailbox='INBOX'
         WHERE c.account_id=?1
         GROUP BY c.id ORDER BY c.sort_order,c.id",
        )
        .map_err(|e| e.to_string())?;
    let result = stmt
        .query_map([account_id], |r| {
            Ok(InboxCategory {
                id: r.get(0)?,
                slug: r.get(1)?,
                name: r.get(2)?,
                icon: r.get(3)?,
                color: r.get(4)?,
                sort_order: r.get(5)?,
                message_count: r.get(6)?,
                unread_count: r.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string());
    result
}

#[tauri::command]
pub async fn get_smart_inbox_enabled(
    state: State<'_, Arc<DbState>>,
) -> Result<bool, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    Ok(conn.query_row(
        "SELECT COALESCE(enabled, 1) FROM inbox_smart_state WHERE account_id=?1",
        [account_id],
        |r| r.get::<_, i64>(0),
    ).unwrap_or(1) != 0)
}

#[tauri::command]
pub async fn categorize_inbox(state: State<'_, Arc<DbState>>) -> Result<CategorizeResult, String> {
    let account_id = get_active_id(&state).await;
    CATEGORIZE_ABORT.store(false, Ordering::SeqCst);
    CATEGORIZE_DONE.store(0, Ordering::SeqCst);
    let conn = state.conn.lock().await;
    let result = categorize_all(&conn, account_id);
    CATEGORIZE_TOTAL.store(0, Ordering::SeqCst);
    CATEGORIZE_DONE.store(0, Ordering::SeqCst);
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_inbox_category(
    state: State<'_, Arc<DbState>>,
    slug: String,
    name: String,
) -> Result<(), String> {
    let clean = name.trim();
    if clean.is_empty() || clean.len() > 40 {
        return Err("Category name must be 1-40 characters".into());
    }
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    conn.execute(
        "UPDATE inbox_categories SET name=?1 WHERE account_id=?2 AND slug=?3",
        params![clean, account_id, slug],
    )
    .map_err(|e| e.to_string())
    .and_then(|n| {
        if n == 0 {
            Err("Unknown category".into())
        } else {
            Ok(())
        }
    })
}

#[tauri::command]
pub async fn abort_categorize_inbox() -> Result<(), String> {
    CATEGORIZE_ABORT.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub async fn get_categorize_progress() -> CategorizeProgress {
    CategorizeProgress { processed: CATEGORIZE_DONE.load(Ordering::SeqCst), total: CATEGORIZE_TOTAL.load(Ordering::SeqCst), aborted: CATEGORIZE_ABORT.load(Ordering::SeqCst) }
}

#[tauri::command]
pub async fn move_emails_to_category(state: State<'_, Arc<DbState>>, email_ids: Vec<String>, slug: String) -> Result<(), String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    let cid = category_id(&conn, account_id, &slug).ok_or_else(|| "Unknown category".to_string())?;
    for id in email_ids {
        conn.execute(
            "UPDATE emails SET category_id=?1 WHERE account_id=?3 AND mailbox='INBOX' AND thread_id IN (
                SELECT thread_id FROM emails WHERE id=?2 AND account_id=?3
             )",
            params![cid, id, account_id],
        ).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_smart_inbox_enabled(state: State<'_, Arc<DbState>>, enabled: bool) -> Result<(), String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    if enabled {
        conn.execute(
            "INSERT INTO inbox_smart_state (account_id, initialized, enabled) VALUES (?1,0,1)
             ON CONFLICT(account_id) DO UPDATE SET
                initialized = CASE WHEN enabled=1 THEN initialized ELSE 0 END,
                enabled = 1",
            [account_id],
        ).map_err(|e| e.to_string())?;
    } else {
        conn.execute("UPDATE emails SET category_id=NULL WHERE account_id=?1", [account_id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM inbox_categories WHERE account_id=?1", [account_id])
            .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO inbox_smart_state (account_id, initialized, enabled) VALUES (?1,0,0)
             ON CONFLICT(account_id) DO UPDATE SET initialized=0, enabled=0",
            [account_id],
        ).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_category_emails(state: State<'_, Arc<DbState>>, slug: String) -> Result<Vec<Email>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    let columns = EMAIL_COLUMNS
        .split(',')
        .map(|column| format!("e.{}", column.trim()))
        .collect::<Vec<_>>()
        .join(",");
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {columns} FROM emails e JOIN inbox_categories c ON c.id=e.category_id
             WHERE e.account_id=?1 AND e.mailbox='INBOX' AND c.slug=?2 ORDER BY e.internal_ts DESC LIMIT 500"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![account_id, slug], map_email_row)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_category_threads(state: State<'_, Arc<DbState>>, slug: String) -> Result<Vec<ThreadSummary>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    thread_summaries(&conn, account_id, Some(&slug))
}
