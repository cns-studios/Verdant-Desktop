use rusqlite::{params, Connection, Result};

use super::Email;

pub const EMAIL_COLUMNS: &str = "id,account_id,draft_id,thread_id,subject,sender,to_recipients,cc_recipients,\
    snippet,body_html,attachments_json,has_attachments,date,is_read,starred,mailbox,labels,internal_ts,notified,\
    list_unsubscribe,unsubscribed,bcc_recipients";

pub fn clear_account_emails(conn: &Connection, account_id: i64) -> Result<()> {
    conn.execute("DELETE FROM emails WHERE account_id = ?1", params![account_id])?;
    Ok(())
}

pub fn map_email_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Email> {
    Ok(Email {
        id: row.get(0)?,
        account_id: row.get(1)?,
        draft_id: row.get(2)?,
        thread_id: row.get(3)?,
        subject: row.get(4)?,
        sender: row.get(5)?,
        to_recipients: row.get(6)?,
        cc_recipients: row.get(7)?,
        snippet: row.get(8)?,
        body_html: row.get(9)?,
        attachments_json: row.get(10)?,
        has_attachments: row.get::<_, i32>(11)? != 0,
        date: row.get(12)?,
        is_read: row.get::<_, i32>(13)? != 0,
        starred: row.get::<_, i32>(14)? != 0,
        mailbox: row.get(15)?,
        labels: row.get(16)?,
        internal_ts: row.get(17)?,
        notified: row.get::<_, i32>(18)? != 0,
        list_unsubscribe: row.get(19)?,
        unsubscribed: row.get::<_, i32>(20)? != 0,
        bcc_recipients: row.get(21)?,
    })
}
