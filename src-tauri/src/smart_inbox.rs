//! Local smart-inbox categorisation. Categories are discovered once from
//! the whole inbox by clustering on *sender organisation* (domain) plus
//! newsletter detection via the `List-Unsubscribe` header - a low-noise,
//! deterministic signal that mirrors how inboxes are naturally organised,
//! rather than free-text topic modelling on subject-line words (which
//! proved unreliable in practice: it surfaced sender aliases and stray
//! stopwords as "categories"). No network dependency, no online learning.
use crate::state::{get_active_id, DbState};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use tauri::State;

#[derive(Debug, Serialize, Clone)]
pub struct InboxCategory {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub sort_order: i64,
    pub message_count: i64,
    pub unread_count: i64,
}

#[derive(Debug, Serialize)]
pub struct CategorizeResult {
    pub processed: i64,
    pub assigned: i64,
}

static CATEGORIZE_ABORT: AtomicBool = AtomicBool::new(false);
static CATEGORIZE_DONE: AtomicI64 = AtomicI64::new(0);
static CATEGORIZE_TOTAL: AtomicI64 = AtomicI64::new(0);

#[derive(Debug, Serialize, Clone)]
pub struct CategorizeProgress { pub processed: i64, pub total: i64, pub aborted: bool }

fn category_id(conn: &Connection, account_id: i64, slug: &str) -> Option<i64> {
    conn.query_row(
        "SELECT id FROM inbox_categories WHERE account_id=?1 AND slug=?2",
        params![account_id, slug],
        |r| r.get(0),
    )
    .ok()
}

fn account_slug(account_id: i64, slug: &str) -> String {
    format!("account-{}-{}", account_id, slug)
}

/// Well-known personal / webmail providers. Mail *from* these domains is
/// almost always a person writing to the user directly, so it is grouped
/// into a single "Personal" bucket rather than one category per provider.
const PERSONAL_PROVIDERS: &[&str] = &[
    "gmail.com", "googlemail.com", "outlook.com", "hotmail.com", "live.com",
    "msn.com", "yahoo.com", "yahoo.co.uk", "icloud.com", "me.com", "mac.com",
    "aol.com", "protonmail.com", "proton.me", "gmx.com", "gmx.net", "web.de",
    "zoho.com", "fastmail.com", "mail.com",
];

/// Extracts the bare email address out of a "Display Name <addr@host>" (or
/// plain `addr@host`) sender string.
fn sender_address(sender: &str) -> String {
    if let (Some(start), Some(end)) = (sender.find('<'), sender.rfind('>')) {
        if end > start {
            return sender[start + 1..end].trim().to_lowercase();
        }
    }
    sender.trim().to_lowercase()
}

/// Extracts the domain (host) part of a sender's email address.
fn sender_domain(sender: &str) -> Option<String> {
    let address = sender_address(sender);
    address.split('@').nth(1).map(|d| d.trim().to_string()).filter(|d| !d.is_empty())
}

/// Reduces a domain such as `notifications.github.com` or `mail.amazon.co.uk`
/// down to the organisation's brand name ("github", "amazon"), which is the
/// part that is actually meaningful to a human reading a category label.
fn domain_brand(domain: &str) -> String {
    const KNOWN_SUFFIXES: &[&str] = &[
        "co.uk", "co.jp", "com.au", "com.br", "co.in", "com.cn", "co.nz",
    ];
    let mut host = domain.to_string();
    for suffix in KNOWN_SUFFIXES {
        if let Some(stripped) = host.strip_suffix(&format!(".{suffix}")) {
            host = stripped.to_string();
            break;
        }
    }
    let mut parts: Vec<&str> = host.split('.').collect();
    // Drop the TLD label (e.g. ".com", ".net", ".org", ...).
    if parts.len() > 1 {
        parts.pop();
    }
    // The brand is normally the *last* remaining label (github.com ->
    // github, notifications.github.com -> github), not the first, since
    // marketing/notification subdomains sit in front of it.
    parts.pop().unwrap_or(domain).to_string()
}

fn slugify(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

enum SenderClass {
    Personal,
    Newsletter,
    Domain(String),
    Unknown,
}

/// Classifies a single message by its sender, using only information that is
/// already stored per-message (no adaptive state, no per-sender cache).
fn classify_sender(sender: &str, list_unsubscribe: &str) -> SenderClass {
    match sender_domain(sender) {
        Some(domain) if PERSONAL_PROVIDERS.contains(&domain.as_str()) => SenderClass::Personal,
        Some(domain) => {
            if !list_unsubscribe.trim().is_empty() {
                SenderClass::Newsletter
            } else {
                SenderClass::Domain(domain)
            }
        }
        None => SenderClass::Unknown,
    }
}

const PALETTE: &[&str] = &["#5c7356", "#6d7fa8", "#b58a4a", "#9a6c9c", "#4f8f8a"];

/// Batch clustering step: looks at the whole inbox once and derives 3-5
/// categories from the sender organisations that actually make up this
/// inbox, rather than guessing at topics from subject-line words. This is
/// far more reliable because "who sent it" is a clean, low-noise signal
/// every mail client already relies on, whereas free-text topic modelling
/// on a handful of words per subject is not.
fn dynamic_categories(conn: &Connection, account_id: i64) -> rusqlite::Result<()> {
    let mut personal_count = 0usize;
    let mut newsletter_count = 0usize;
    let mut domain_counts = std::collections::HashMap::<String, usize>::new();
    {
        let mut stmt = conn.prepare(
            "SELECT sender, COALESCE(list_unsubscribe,'') FROM emails WHERE account_id=?1 AND mailbox='INBOX'",
        )?;
        let rows = stmt.query_map([account_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (sender, list_unsubscribe) = row?;
            match classify_sender(&sender, &list_unsubscribe) {
                SenderClass::Personal => personal_count += 1,
                SenderClass::Newsletter => newsletter_count += 1,
                SenderClass::Domain(domain) => *domain_counts.entry(domain).or_default() += 1,
                SenderClass::Unknown => {}
            }
        }
    }

    // Merge domains that share the same brand (e.g. mail.foo.com and
    // notifications.foo.com both become "foo") before ranking them.
    let mut brand_counts = std::collections::HashMap::<String, usize>::new();
    for (domain, count) in &domain_counts {
        *brand_counts.entry(domain_brand(domain)).or_default() += count;
    }
    let mut brands: Vec<(String, usize)> = brand_counts.into_iter().collect();
    brands.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    conn.execute("UPDATE emails SET category_id=NULL WHERE account_id=?1 AND mailbox='INBOX'", [account_id])?;
    conn.execute("DELETE FROM inbox_categories WHERE account_id=?1", [account_id])?;
    let mut sort_order = 0i64;
    let mut palette_index = 0usize;
    let mut next_color = || {
        let color = PALETTE[palette_index % PALETTE.len()];
        palette_index += 1;
        color
    };

    // Reserve at most 4 organisation categories so there is always room for
    // the fixed Personal/Newsletters/Other buckets, keeping the total in the
    // 3-5 range the feature is scoped to.
    let max_brand_categories = if personal_count > 0 && newsletter_count > 0 { 2 } else { 3 };
    for (brand, count) in brands.iter().take(max_brand_categories) {
        if *count < 2 {
            continue;
        }
        let name = brand
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_default()
            + &brand.chars().skip(1).collect::<String>();
        conn.execute(
            "INSERT INTO inbox_categories (account_id,slug,name,icon,color,sort_order,is_fixed) VALUES (?1,?2,?3,'tag',?4,?5,0)",
            params![account_id, account_slug(account_id, &format!("org-{}", slugify(brand))), name, next_color(), sort_order],
        )?;
        sort_order += 1;
    }
    if personal_count > 0 {
        conn.execute(
            "INSERT INTO inbox_categories (account_id,slug,name,icon,color,sort_order,is_fixed) VALUES (?1,?2,'Personal','user',?3,?4,0)",
            params![account_id, account_slug(account_id, "personal"), next_color(), sort_order],
        )?;
        sort_order += 1;
    }
    if newsletter_count > 0 {
        conn.execute(
            "INSERT INTO inbox_categories (account_id,slug,name,icon,color,sort_order,is_fixed) VALUES (?1,?2,'Newsletters','news',?3,?4,0)",
            params![account_id, account_slug(account_id, "newsletters"), next_color(), sort_order],
        )?;
        sort_order += 1;
    }
    conn.execute(
        "INSERT INTO inbox_categories (account_id,slug,name,icon,color,sort_order,is_fixed) VALUES (?1,?2,'Other','tag','#7b8075',?3,1)",
        params![account_id, account_slug(account_id, "other"), sort_order],
    )?;
    Ok(())
}

/// Steady-state assignment: classify one message's sender and map it to
/// whichever existing category owns that bucket. Never creates or renames
/// categories - if the bucket a message would belong to was not one of the
/// ones discovered during the initial batch pass, it falls back to "Other".
fn choose_dynamic_category(conn: &Connection, account_id: i64, sender: &str, list_unsubscribe: &str) -> Option<i64> {
    let slug = match classify_sender(sender, list_unsubscribe) {
        SenderClass::Personal => "personal".to_string(),
        SenderClass::Newsletter => "newsletters".to_string(),
        SenderClass::Domain(domain) => format!("org-{}", slugify(&domain_brand(&domain))),
        SenderClass::Unknown => return None,
    };
    category_id(conn, account_id, &account_slug(account_id, &slug))
}

pub fn assign_unassigned(conn: &Connection, account_id: i64) -> rusqlite::Result<i64> {
    let initialized: i64 = conn.query_row(
        "SELECT COALESCE(initialized, 0) FROM inbox_smart_state WHERE account_id=?1 AND COALESCE(enabled,1)=1",
        [account_id],
        |r| r.get(0),
    ).unwrap_or(0);
    if initialized == 0 {
        return Ok(0);
    }
    let rows: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id,sender,COALESCE(list_unsubscribe,'') FROM emails
             WHERE account_id=?1 AND mailbox='INBOX' AND category_id IS NULL",
        )?;
        let rows = stmt.query_map([account_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        rows
        .filter_map(Result::ok)
        .collect()
    };
    let mut count = 0;
    for (id, sender, list_unsubscribe) in rows {
        let cid = choose_dynamic_category(conn, account_id, &sender, &list_unsubscribe)
            .or_else(|| category_id(conn, account_id, &account_slug(account_id, "other")));
        if let Some(cid) = cid {
            conn.execute(
                "UPDATE emails SET category_id=?1 WHERE id=?2 AND account_id=?3",
                params![cid, id, account_id],
            )?;
            count += 1;
        }
    }
    Ok(count)
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
    ).unwrap_or(0) != 0)
}

#[tauri::command]
pub async fn categorize_inbox(state: State<'_, Arc<DbState>>) -> Result<CategorizeResult, String> {
    let account_id = get_active_id(&state).await;
    CATEGORIZE_ABORT.store(false, Ordering::SeqCst);
    let conn = state.conn.lock().await;
    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM emails WHERE account_id=?1 AND mailbox='INBOX'",
            [account_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    CATEGORIZE_TOTAL.store(total, Ordering::SeqCst);
    CATEGORIZE_DONE.store(0, Ordering::SeqCst);
    dynamic_categories(&conn, account_id).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO inbox_smart_state (account_id, initialized, enabled) VALUES (?1,0,1)
         ON CONFLICT(account_id) DO UPDATE SET enabled=1",
        [account_id],
    ).map_err(|e| e.to_string())?;
    let mut assigned = 0;
    let rows: Vec<(String,String,String)> = {
        let mut stmt = conn.prepare("SELECT id,sender,COALESCE(list_unsubscribe,'') FROM emails WHERE account_id=?1 AND mailbox='INBOX'").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([account_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e| e.to_string())?;
        rows.filter_map(Result::ok).collect()
    };
    let _ = conn.execute_batch("BEGIN TRANSACTION;");
    for (index, (id, sender, list_unsubscribe)) in rows.into_iter().enumerate() {
        if CATEGORIZE_ABORT.load(Ordering::SeqCst) { break; }
        if let Some(cid) = choose_dynamic_category(&conn, account_id, &sender, &list_unsubscribe).or_else(|| category_id(&conn, account_id, &account_slug(account_id, "other"))) {
            let _ = conn.execute("UPDATE emails SET category_id=?1 WHERE id=?2 AND account_id=?3", params![cid, id, account_id]);
            assigned += 1;
        }
        CATEGORIZE_DONE.fetch_add(1, Ordering::SeqCst);
        if index > 0 && index % 100 == 0 {
            let _ = conn.execute_batch("COMMIT; BEGIN TRANSACTION;");
            tokio::task::yield_now().await;
        }
    }
    let _ = conn.execute_batch("COMMIT;");
    let aborted = CATEGORIZE_ABORT.load(Ordering::SeqCst);
    if aborted { return Ok(CategorizeResult { processed: CATEGORIZE_DONE.load(Ordering::SeqCst), assigned }); }
    conn.execute(
        "INSERT INTO inbox_smart_state (account_id, initialized) VALUES (?1, 1)
         ON CONFLICT(account_id) DO UPDATE SET initialized=1, enabled=1",
        [account_id],
    ).map_err(|e| e.to_string())?;
    Ok(CategorizeResult {
        processed: total,
        assigned,
    })
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
        conn.execute("UPDATE emails SET category_id=?1 WHERE id=?2 AND account_id=?3 AND mailbox='INBOX'", params![cid, id, account_id]).map_err(|e| e.to_string())?;
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
             ON CONFLICT(account_id) DO UPDATE SET enabled=1, initialized=0",
            [account_id],
        ).map_err(|e| e.to_string())?;
    } else {
        // Disabling is a full reset: assignments, renamed categories, and the
        // per-account state must not survive until the next activation.
        conn.execute("UPDATE emails SET category_id=NULL WHERE account_id=?1", [account_id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM inbox_smart_state WHERE account_id=?1", [account_id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM inbox_categories WHERE account_id=?1", [account_id])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_category_emails(
    state: State<'_, Arc<DbState>>,
    slug: String,
) -> Result<Vec<crate::db::Email>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    let mut stmt = conn.prepare(
        "SELECT e.id,e.account_id,e.draft_id,e.thread_id,e.subject,e.sender,e.to_recipients,e.cc_recipients,
         e.snippet,e.body_html,e.attachments_json,e.has_attachments,e.date,e.is_read,e.starred,e.mailbox,e.labels,
         e.internal_ts,e.notified,e.list_unsubscribe,e.unsubscribed,e.bcc_recipients
         FROM emails e JOIN inbox_categories c ON c.id=e.category_id
         WHERE e.account_id=?1 AND e.mailbox='INBOX' AND c.slug=?2 ORDER BY e.internal_ts DESC LIMIT 500"
    ).map_err(|e| e.to_string())?;
    let result = stmt
        .query_map(params![account_id, slug], |r| {
            Ok(crate::db::Email {
                id: r.get(0)?,
                account_id: r.get(1)?,
                draft_id: r.get(2)?,
                thread_id: r.get(3)?,
                subject: r.get(4)?,
                sender: r.get(5)?,
                to_recipients: r.get(6)?,
                cc_recipients: r.get(7)?,
                snippet: r.get(8)?,
                body_html: r.get(9)?,
                attachments_json: r.get(10)?,
                has_attachments: r.get::<_, i32>(11)? != 0,
                date: r.get(12)?,
                is_read: r.get::<_, i32>(13)? != 0,
                starred: r.get::<_, i32>(14)? != 0,
                mailbox: r.get(15)?,
                labels: r.get(16)?,
                internal_ts: r.get(17)?,
                notified: r.get::<_, i32>(18)? != 0,
                list_unsubscribe: r.get(19)?,
                unsubscribed: r.get::<_, i32>(20)? != 0,
                bcc_recipients: r.get(21)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string());
    result
}

#[tauri::command]
pub async fn get_category_threads(
    state: State<'_, Arc<DbState>>,
    slug: String,
) -> Result<Vec<crate::commands::mail::ThreadSummary>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    let sql = "
        SELECT e.thread_id, latest.subject, latest.snippet, latest.date, latest.labels,
               COUNT(e.id), SUM(CASE WHEN e.is_read=0 THEN 1 ELSE 0 END),
               MAX(e.internal_ts), MAX(e.starred), MAX(e.has_attachments),
               GROUP_CONCAT(DISTINCT e.sender)
        FROM emails e
        JOIN inbox_categories c ON c.id=e.category_id
        INNER JOIN emails latest ON latest.id=(
            SELECT id FROM emails
            WHERE thread_id=e.thread_id AND mailbox='INBOX' AND account_id=?1
            ORDER BY internal_ts DESC LIMIT 1
        )
        WHERE e.mailbox='INBOX' AND e.account_id=?1 AND c.slug=?2
        GROUP BY e.thread_id
        ORDER BY MAX(e.internal_ts) DESC LIMIT 500";
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(rusqlite::params![account_id, slug], |row| {
        let unread_count: i64 = row.get(6)?;
        Ok(crate::commands::mail::ThreadSummary {
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
    }).map_err(|e| e.to_string())?;
    rows
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_categories_and_assign() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();

        conn.execute("INSERT OR IGNORE INTO accounts (id, email) VALUES (1, 'user1@example.com'), (2, 'user2@example.com')", []).unwrap();

        conn.execute(
            "INSERT INTO emails (id, account_id, thread_id, subject, sender, mailbox, body_html, date, is_read) VALUES
             ('1', 1, 't1', 'Subj 1', 'Google <no-reply@google.com>', 'INBOX', '', '', 1),
             ('2', 1, 't2', 'Subj 2', 'Friend <friend@gmail.com>', 'INBOX', '', '', 1),
             ('3', 1, 't3', 'Subj 3', 'Promo <news@newsletter.com>', 'INBOX', '', '', 1)",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO emails (id, account_id, thread_id, subject, sender, mailbox, body_html, date, is_read) VALUES
             ('4', 2, 't4', 'Subj 4', 'Github <notifications@github.com>', 'INBOX', '', '', 1)",
            [],
        ).unwrap();

        dynamic_categories(&conn, 1).unwrap();
        dynamic_categories(&conn, 2).unwrap();

        let count_acc1: i64 = conn.query_row("SELECT COUNT(*) FROM inbox_categories WHERE account_id=1", [], |r| r.get(0)).unwrap();
        let count_acc2: i64 = conn.query_row("SELECT COUNT(*) FROM inbox_categories WHERE account_id=2", [], |r| r.get(0)).unwrap();

        assert!(count_acc1 > 0);
        assert!(count_acc2 > 0);

        conn.execute("INSERT INTO inbox_smart_state (account_id, initialized, enabled) VALUES (1, 1, 1), (2, 1, 1)", []).unwrap();
        let assigned1 = assign_unassigned(&conn, 1).unwrap();
        let assigned2 = assign_unassigned(&conn, 2).unwrap();

        assert_eq!(assigned1, 3);
        assert_eq!(assigned2, 1);
    }
}
