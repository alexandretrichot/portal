use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;

const APPLE_EPOCH_OFFSET: i64 = 978307200;

fn apple_time_to_datetime(apple_time: i64) -> DateTime<Utc> {
    let unix_timestamp = if apple_time > 1_000_000_000_000_000_000 {
        (apple_time / 1_000_000_000) + APPLE_EPOCH_OFFSET
    } else {
        apple_time + APPLE_EPOCH_OFFSET
    };
    DateTime::from_timestamp(unix_timestamp, 0).unwrap_or_default()
}

#[derive(Debug, Serialize)]
pub struct Conversation {
    pub chat_id: String,
    pub display_name: Option<String>,
    pub participants: Vec<String>,
    pub last_message_date: Option<String>,
    pub is_group: bool,
}

#[derive(Debug, Serialize)]
pub struct Message {
    pub id: i64,
    pub text: Option<String>,
    pub is_from_me: bool,
    pub sender: Option<String>,
    pub date: String,
    pub date_read: Option<String>,
    pub attachments: Vec<String>,
}

pub fn list_conversations(db_path: &str, limit: usize) -> Result<Vec<Conversation>> {
    let conn = Connection::open(db_path)?;

    let mut stmt = conn.prepare(
        r#"
        SELECT
            c.chat_identifier,
            c.display_name,
            c.ROWID,
            MAX(m.date) as last_date
        FROM chat c
        LEFT JOIN chat_message_join cmj ON c.ROWID = cmj.chat_id
        LEFT JOIN message m ON cmj.message_id = m.ROWID
        GROUP BY c.ROWID
        ORDER BY last_date DESC
        LIMIT ?
        "#,
    )?;

    let mut conversations = Vec::new();

    let rows = stmt.query_map([limit], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<i64>>(3)?,
        ))
    })?;

    for row in rows {
        let (chat_identifier, display_name, chat_rowid, last_date) = row?;

        let participants = get_chat_participants(&conn, chat_rowid)?;
        let is_group = participants.len() > 1 || chat_identifier.starts_with("chat");

        conversations.push(Conversation {
            chat_id: chat_identifier,
            display_name,
            participants,
            last_message_date: last_date.map(|d| apple_time_to_datetime(d).to_rfc3339()),
            is_group,
        });
    }

    Ok(conversations)
}

fn get_chat_participants(conn: &Connection, chat_rowid: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT h.id
        FROM handle h
        JOIN chat_handle_join chj ON h.ROWID = chj.handle_id
        WHERE chj.chat_id = ?
        "#,
    )?;

    let participants: Vec<String> = stmt
        .query_map([chat_rowid], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(participants)
}

pub fn get_messages(db_path: &str, chat_id: &str, limit: usize) -> Result<Vec<Message>> {
    let conn = Connection::open(db_path)?;

    let mut stmt = conn.prepare(
        r#"
        SELECT
            m.ROWID,
            m.text,
            m.is_from_me,
            h.id as sender,
            m.date,
            m.date_read
        FROM message m
        JOIN chat_message_join cmj ON m.ROWID = cmj.message_id
        JOIN chat c ON cmj.chat_id = c.ROWID
        LEFT JOIN handle h ON m.handle_id = h.ROWID
        WHERE c.chat_identifier = ?
        ORDER BY m.date DESC
        LIMIT ?
        "#,
    )?;

    let mut messages = Vec::new();

    let rows = stmt.query_map(rusqlite::params![chat_id, limit], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, i32>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<i64>>(5)?,
        ))
    })?;

    for row in rows {
        let (id, text, is_from_me, sender, date, date_read) = row?;

        let attachments = get_message_attachments(&conn, id)?;

        messages.push(Message {
            id,
            text,
            is_from_me: is_from_me == 1,
            sender,
            date: apple_time_to_datetime(date).to_rfc3339(),
            date_read: date_read.map(|d| apple_time_to_datetime(d).to_rfc3339()),
            attachments,
        });
    }

    messages.reverse();
    Ok(messages)
}

fn get_message_attachments(conn: &Connection, message_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT a.filename
        FROM attachment a
        JOIN message_attachment_join maj ON a.ROWID = maj.attachment_id
        WHERE maj.message_id = ?
        "#,
    )?;

    let attachments: Vec<String> = stmt
        .query_map([message_id], |row| row.get::<_, Option<String>>(0))?
        .filter_map(|r| r.ok())
        .flatten()
        .collect();

    Ok(attachments)
}

pub fn search_messages(db_path: &str, query: &str, limit: usize) -> Result<Vec<Message>> {
    let conn = Connection::open(db_path)?;

    let mut stmt = conn.prepare(
        r#"
        SELECT
            m.ROWID,
            m.text,
            m.is_from_me,
            h.id as sender,
            m.date,
            m.date_read
        FROM message m
        LEFT JOIN handle h ON m.handle_id = h.ROWID
        WHERE m.text LIKE ?
        ORDER BY m.date DESC
        LIMIT ?
        "#,
    )?;

    let search_pattern = format!("%{}%", query);
    let mut messages = Vec::new();

    let rows = stmt.query_map(rusqlite::params![search_pattern, limit], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, i32>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<i64>>(5)?,
        ))
    })?;

    for row in rows {
        let (id, text, is_from_me, sender, date, date_read) = row?;

        messages.push(Message {
            id,
            text,
            is_from_me: is_from_me == 1,
            sender,
            date: apple_time_to_datetime(date).to_rfc3339(),
            date_read: date_read.map(|d| apple_time_to_datetime(d).to_rfc3339()),
            attachments: vec![],
        });
    }

    Ok(messages)
}
