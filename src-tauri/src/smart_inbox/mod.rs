mod assign;
mod classify;
mod commands;
mod plan;

use std::sync::atomic::{AtomicBool, AtomicI64};

use serde::Serialize;

pub use assign::assign_unassigned;
pub use commands::*;

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

pub(crate) static CATEGORIZE_ABORT: AtomicBool = AtomicBool::new(false);

pub(crate) static CATEGORIZE_DONE: AtomicI64 = AtomicI64::new(0);

pub(crate) static CATEGORIZE_TOTAL: AtomicI64 = AtomicI64::new(0);

#[derive(Debug, Serialize, Clone)]
pub struct CategorizeProgress { pub processed: i64, pub total: i64, pub aborted: bool }

#[derive(Debug, Serialize, Clone)]
pub struct PlannedCategory {
    pub kind: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub message_count: i64,
}

#[cfg(test)]
mod tests {
    use rusqlite::{params, Connection};

    use super::assign::categorize_all;
    use super::classify::sender_domain;
    use super::plan::plan_categories;
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
