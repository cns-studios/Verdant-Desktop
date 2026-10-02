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

const PERSONAL_PROVIDERS: &[&str] = &[
    "gmail.com", "googlemail.com", "outlook.com", "hotmail.com", "live.com",
    "msn.com", "yahoo.com", "yahoo.co.uk", "icloud.com", "me.com", "mac.com",
    "aol.com", "protonmail.com", "proton.me", "gmx.com", "gmx.net", "web.de",
    "zoho.com", "fastmail.com", "mail.com",
];

fn sender_address(sender: &str) -> String {
    if let (Some(start), Some(end)) = (sender.find('<'), sender.rfind('>')) {
        if end > start {
            return sender[start + 1..end].trim().to_lowercase();
        }
    }
    sender.trim().to_lowercase()
}

fn sender_domain(sender: &str) -> Option<String> {
    let address = sender_address(sender);
    address
        .rsplit_once('@')
        .map(|(_, d)| d.trim().trim_end_matches('.').to_string())
        .filter(|d| !d.is_empty() && d.contains('.'))
        .filter(|d| d.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.'))
}

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
    if parts.len() > 1 {
        parts.pop();
    }
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
    Domain { domain: String, bulk: bool },
    Unknown,
}

fn classify_sender(sender: &str, list_unsubscribe: &str) -> SenderClass {
    match sender_domain(sender) {
        Some(domain) if PERSONAL_PROVIDERS.contains(&domain.as_str()) => SenderClass::Personal,
        Some(domain) => SenderClass::Domain { domain, bulk: !list_unsubscribe.trim().is_empty() },
        None => SenderClass::Unknown,
    }
}

const PALETTE: &[&str] = &["#5c7356", "#6d7fa8", "#b58a4a", "#9a6c9c", "#4f8f8a"];

#[derive(Debug, Serialize, Clone)]
pub struct PlannedCategory {
    pub kind: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub message_count: i64,
}

fn plan_categories(conn: &Connection, account_id: i64) -> rusqlite::Result<Vec<PlannedCategory>> {
    let mut total = 0usize;
    let mut personal_count = 0usize;
    let mut brand_counts = std::collections::HashMap::<String, (usize, usize)>::new();
    {
        let mut stmt = conn.prepare(
            "SELECT sender, COALESCE(list_unsubscribe,'') FROM emails WHERE account_id=?1 AND mailbox='INBOX'",
        )?;
        let rows = stmt.query_map([account_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (sender, list_unsubscribe) = row?;
            total += 1;
            match classify_sender(&sender, &list_unsubscribe) {
                SenderClass::Personal => personal_count += 1,
                SenderClass::Domain { domain, bulk } => {
                    let entry = brand_counts.entry(domain_brand(&domain)).or_default();
                    entry.0 += 1;
                    if bulk {
                        entry.1 += 1;
                    }
                }
                SenderClass::Unknown => {}
            }
        }
    }
    let mut brands: Vec<(String, (usize, usize))> = brand_counts.into_iter().collect();
    brands.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
    let eligible = brands.iter().take_while(|(_, (total, _))| *total >= 2).count();

    let bulk_left = |chosen: usize| brands.iter().skip(chosen).map(|(_, (_, bulk))| bulk).sum::<usize>();
    let mut chosen = eligible.min(2);
    if eligible > 2 && (personal_count == 0 || bulk_left(3) == 0) {
        chosen = 3;
    }
    let newsletter_count = bulk_left(chosen);

    let mut palette = PALETTE.iter().cycle();
    let mut plan = Vec::new();
    let mut assigned = 0usize;
    for (brand, (count, _)) in brands.iter().take(chosen) {
        let name = brand
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_default()
            + &brand.chars().skip(1).collect::<String>();
        plan.push(PlannedCategory {
            kind: format!("org-{}", slugify(brand)),
            name,
            icon: "tag".into(),
            color: palette.next().unwrap().to_string(),
            message_count: *count as i64,
        });
        assigned += count;
    }
    if personal_count > 0 {
        plan.push(PlannedCategory {
            kind: "personal".into(),
            name: "Personal".into(),
            icon: "user".into(),
            color: palette.next().unwrap().to_string(),
            message_count: personal_count as i64,
        });
        assigned += personal_count;
    }
    if newsletter_count > 0 {
        plan.push(PlannedCategory {
            kind: "newsletters".into(),
            name: "Newsletters".into(),
            icon: "news".into(),
            color: palette.next().unwrap().to_string(),
            message_count: newsletter_count as i64,
        });
        assigned += newsletter_count;
    }
    plan.push(PlannedCategory {
        kind: "other".into(),
        name: "Other".into(),
        icon: "tag".into(),
        color: "#7b8075".into(),
        message_count: total.saturating_sub(assigned) as i64,
    });
    Ok(plan)
}

fn dynamic_categories(conn: &Connection, account_id: i64) -> rusqlite::Result<()> {
    let plan = plan_categories(conn, account_id)?;
    conn.execute("UPDATE emails SET category_id=NULL WHERE account_id=?1", [account_id])?;
    conn.execute("DELETE FROM inbox_categories WHERE account_id=?1", [account_id])?;
    for (sort_order, c) in plan.iter().enumerate() {
        conn.execute(
            "INSERT INTO inbox_categories (account_id,slug,name,icon,color,sort_order,is_fixed) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                account_id, account_slug(account_id, &c.kind), c.name, c.icon, c.color,
                sort_order as i64, (c.kind == "other") as i64,
            ],
        )?;
    }
    Ok(())
}

#[tauri::command]
pub async fn preview_inbox_categories(state: State<'_, Arc<DbState>>) -> Result<Vec<PlannedCategory>, String> {
    let account_id = get_active_id(&state).await;
    let conn = state.conn.lock().await;
    plan_categories(&conn, account_id).map_err(|e| e.to_string())
}

struct Assigner<'a> {
    conn: &'a Connection,
    account_id: i64,
    slugs: std::collections::HashMap<String, i64>,
    threads: std::collections::HashMap<String, i64>,
}

impl<'a> Assigner<'a> {
    fn new(conn: &'a Connection, account_id: i64) -> rusqlite::Result<Self> {
        let mut stmt = conn.prepare("SELECT slug, id FROM inbox_categories WHERE account_id=?1")?;
        let slugs = stmt
            .query_map([account_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Self { conn, account_id, slugs, threads: Default::default() })
    }

    fn slug(&self, slug: &str) -> Option<i64> {
        self.slugs.get(&account_slug(self.account_id, slug)).copied()
    }

    fn thread_category(&mut self, thread_id: &str) -> Option<i64> {
        if let Some(cid) = self.threads.get(thread_id) {
            return Some(*cid);
        }
        let cid: Option<i64> = self.conn.query_row(
            "SELECT e.category_id FROM emails e JOIN inbox_categories c ON c.id=e.category_id
             WHERE e.account_id=?1 AND e.thread_id=?2 AND e.mailbox='INBOX' AND c.account_id=?1
             ORDER BY e.internal_ts ASC LIMIT 1",
            params![self.account_id, thread_id],
            |r| r.get(0),
        ).ok();
        if let Some(cid) = cid {
            self.threads.insert(thread_id.to_string(), cid);
        }
        cid
    }

    fn choose(&mut self, thread_id: &str, sender: &str, list_unsubscribe: &str) -> Option<i64> {
        let cid = self.thread_category(thread_id).or_else(|| match classify_sender(sender, list_unsubscribe) {
            SenderClass::Personal => self.slug("personal"),
            SenderClass::Domain { domain, bulk } => self
                .slug(&format!("org-{}", slugify(&domain_brand(&domain))))
                .or_else(|| if bulk { self.slug("newsletters") } else { None }),
            SenderClass::Unknown => None,
        }).or_else(|| self.slug("other"))?;
        self.threads.insert(thread_id.to_string(), cid);
        Some(cid)
    }
}

type PendingRow = (String, String, String, String);

fn pending_rows(conn: &Connection, account_id: i64, only_unassigned: bool) -> rusqlite::Result<Vec<PendingRow>> {
    let sql = if only_unassigned {
        "SELECT id,thread_id,sender,COALESCE(list_unsubscribe,'') FROM emails
         WHERE account_id=?1 AND mailbox='INBOX' AND (category_id IS NULL
            OR category_id NOT IN (SELECT id FROM inbox_categories WHERE account_id=?1))
         ORDER BY internal_ts ASC"
    } else {
        "SELECT id,thread_id,sender,COALESCE(list_unsubscribe,'') FROM emails
         WHERE account_id=?1 AND mailbox='INBOX' ORDER BY internal_ts ASC"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([account_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    rows.collect()
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
    let rows = pending_rows(conn, account_id, true)?;
    let mut assigner = Assigner::new(conn, account_id)?;
    let mut count = 0;
    for (id, thread_id, sender, list_unsubscribe) in rows {
        if let Some(cid) = assigner.choose(&thread_id, &sender, &list_unsubscribe) {
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

fn categorize_all(conn: &Connection, account_id: i64) -> rusqlite::Result<CategorizeResult> {
    let tx = conn.unchecked_transaction()?;
    dynamic_categories(&tx, account_id)?;
    let rows = pending_rows(&tx, account_id, false)?;
    let total = rows.len() as i64;
    CATEGORIZE_TOTAL.store(total, Ordering::SeqCst);

    let mut assigner = Assigner::new(&tx, account_id)?;
    let mut assigned = 0;
    for (id, thread_id, sender, list_unsubscribe) in rows {
        if CATEGORIZE_ABORT.load(Ordering::SeqCst) {
            return Ok(CategorizeResult { processed: CATEGORIZE_DONE.load(Ordering::SeqCst), assigned: 0 });
        }
        if let Some(cid) = assigner.choose(&thread_id, &sender, &list_unsubscribe) {
            tx.execute("UPDATE emails SET category_id=?1 WHERE id=?2 AND account_id=?3", params![cid, id, account_id])?;
            assigned += 1;
        }
        CATEGORIZE_DONE.fetch_add(1, Ordering::SeqCst);
    }
    drop(assigner);
    tx.execute(
        "INSERT INTO inbox_smart_state (account_id, initialized, enabled) VALUES (?1, 1, 1)
         ON CONFLICT(account_id) DO UPDATE SET initialized=1, enabled=1",
        [account_id],
    )?;
    tx.commit()?;
    Ok(CategorizeResult { processed: total, assigned })
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

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        conn.execute("INSERT OR IGNORE INTO accounts (id, email) VALUES (1, 'user1@example.com'), (2, 'user2@example.com')", []).unwrap();
        conn
    }

    fn insert(conn: &Connection, id: &str, account: i64, thread: &str, sender: &str, unsub: &str, ts: i64) {
        conn.execute(
            "INSERT INTO emails (id, account_id, thread_id, subject, sender, mailbox, body_html, date, is_read, list_unsubscribe, internal_ts)
             VALUES (?1, ?2, ?3, 'Subject', ?4, 'INBOX', '', '', 1, ?5, ?6)",
            params![id, account, thread, sender, unsub, ts],
        ).unwrap();
    }

    fn category_of(conn: &Connection, id: &str) -> Option<String> {
        conn.query_row(
            "SELECT c.slug FROM emails e JOIN inbox_categories c ON c.id=e.category_id WHERE e.id=?1",
            [id],
            |r| r.get(0),
        ).ok()
    }

    #[test]
    fn categories_are_per_account() {
        let conn = setup();
        insert(&conn, "1", 1, "t1", "Google <no-reply@google.com>", "", 1);
        insert(&conn, "2", 1, "t2", "Friend <friend@gmail.com>", "", 2);
        insert(&conn, "3", 1, "t3", "Promo <news@newsletter.com>", "<https://x>", 3);
        insert(&conn, "4", 2, "t4", "Github <notifications@github.com>", "", 4);

        categorize_all(&conn, 1).unwrap();
        categorize_all(&conn, 2).unwrap();

        let count = |account: i64| conn.query_row(
            "SELECT COUNT(*) FROM inbox_categories WHERE account_id=?1", [account], |r| r.get::<_, i64>(0),
        ).unwrap();
        assert!(count(1) > 0);
        assert!(count(2) > 0);
        assert!(category_of(&conn, "1").unwrap().starts_with("account-1-"));
        assert!(category_of(&conn, "4").unwrap().starts_with("account-2-"));
    }

    #[test]
    fn bulk_mail_from_a_top_sender_gets_its_own_category() {
        let conn = setup();
        for i in 0..4 {
            insert(&conn, &format!("gh{i}"), 1, &format!("gh{i}"), "GitHub <notifications@github.com>", "<mailto:u@github.com>", i);
        }
        insert(&conn, "n1", 1, "n1", "Shop <deals@shop.example>", "<https://unsub>", 10);
        categorize_all(&conn, 1).unwrap();
        assert_eq!(category_of(&conn, "gh0").as_deref(), Some("account-1-org-github"));
        assert_eq!(category_of(&conn, "n1").as_deref(), Some("account-1-newsletters"));
    }

    #[test]
    fn replies_stay_in_their_conversations_category() {
        let conn = setup();
        insert(&conn, "a", 1, "thread", "GitHub <notifications@github.com>", "", 1);
        insert(&conn, "b", 1, "other", "GitHub <notifications@github.com>", "", 2);
        categorize_all(&conn, 1).unwrap();
        insert(&conn, "c", 1, "thread", "Friend <friend@gmail.com>", "", 3);
        assign_unassigned(&conn, 1).unwrap();
        assert_eq!(category_of(&conn, "c"), category_of(&conn, "a"));
    }

    #[test]
    fn dangling_assignments_are_repaired() {
        let conn = setup();
        insert(&conn, "a", 1, "t", "GitHub <notifications@github.com>", "", 1);
        insert(&conn, "b", 1, "u", "GitHub <notifications@github.com>", "", 2);
        categorize_all(&conn, 1).unwrap();
        conn.execute("UPDATE emails SET category_id=99999 WHERE id='a'", []).unwrap();
        assert_eq!(assign_unassigned(&conn, 1).unwrap(), 1);
        assert!(category_of(&conn, "a").is_some());
    }

    #[test]
    fn preview_matches_what_is_created_and_changes_nothing() {
        let conn = setup();
        for i in 0..3 {
            insert(&conn, &format!("gh{i}"), 1, &format!("gh{i}"), "GitHub <n@github.com>", "", i);
        }
        insert(&conn, "p", 1, "p", "Friend <f@gmail.com>", "", 5);
        insert(&conn, "x", 1, "x", "Solo <s@solo.example>", "", 6);
        let plan = plan_categories(&conn, 1).unwrap();
        let kinds: Vec<_> = plan.iter().map(|c| (c.kind.as_str(), c.message_count)).collect();
        assert_eq!(kinds, vec![("org-github", 3), ("personal", 1), ("other", 1)]);
        let stored: i64 = conn.query_row("SELECT COUNT(*) FROM inbox_categories", [], |r| r.get(0)).unwrap();
        assert_eq!(stored, 0);
        categorize_all(&conn, 1).unwrap();
        assert_eq!(category_of(&conn, "x").as_deref(), Some("account-1-other"));
    }

    #[test]
    fn hostile_sender_domains_are_ignored() {
        assert_eq!(sender_domain("x <a@<img src=x onerror=alert(1)>.com>"), None);
        assert_eq!(sender_domain("A <a@Mail.Example.COM>").as_deref(), Some("mail.example.com"));
        assert_eq!(sender_domain("plain@host"), None);
    }

    #[test]
    fn nothing_is_assigned_before_first_analysis() {
        let conn = setup();
        insert(&conn, "a", 1, "t", "GitHub <notifications@github.com>", "", 1);
        assert_eq!(assign_unassigned(&conn, 1).unwrap(), 0);
    }
}
