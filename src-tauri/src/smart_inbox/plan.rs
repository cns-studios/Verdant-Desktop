use rusqlite::{params, Connection};

use super::classify::{classify_sender, domain_brand, slugify, SenderClass};
use super::PlannedCategory;

pub(crate) const PALETTE: &[&str] = &["#5c7356", "#6d7fa8", "#b58a4a", "#9a6c9c", "#4f8f8a"];

pub(crate) fn account_slug(account_id: i64, slug: &str) -> String {
    format!("account-{}-{}", account_id, slug)
}

pub(crate) fn plan_categories(conn: &Connection, account_id: i64) -> rusqlite::Result<Vec<PlannedCategory>> {
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

pub(crate) fn dynamic_categories(conn: &Connection, account_id: i64) -> rusqlite::Result<()> {
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
