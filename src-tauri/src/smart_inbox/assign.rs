use std::sync::atomic::Ordering;

use rusqlite::{params, Connection};

use super::classify::{classify_sender, domain_brand, slugify, SenderClass};
use super::plan::{account_slug, dynamic_categories};
use super::{CategorizeResult, CATEGORIZE_ABORT, CATEGORIZE_DONE, CATEGORIZE_TOTAL};

pub(crate) struct Assigner<'a> {
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

pub(crate) type PendingRow = (String, String, String, String);

pub(crate) fn pending_rows(conn: &Connection, account_id: i64, only_unassigned: bool) -> rusqlite::Result<Vec<PendingRow>> {
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

pub(crate) fn categorize_all(conn: &Connection, account_id: i64) -> rusqlite::Result<CategorizeResult> {
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
