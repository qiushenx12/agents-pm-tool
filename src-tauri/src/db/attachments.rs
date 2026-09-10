use rusqlite::{Connection, Row};

use crate::domain::attachment::Attachment;
use crate::error::{ApiError, ApiResult};

fn from_row(row: &Row<'_>) -> rusqlite::Result<Attachment> {
    Ok(Attachment {
        id: row.get("id")?,
        task_id: row.get("task_id")?,
        filename: row.get("filename")?,
        stored_path: row.get("stored_path")?,
        mime: row.get("mime")?,
        size: row.get("size")?,
        created_at: row.get("created_at")?,
    })
}

pub fn list(conn: &Connection, task_id: &str) -> ApiResult<Vec<Attachment>> {
    let mut statement =
        conn.prepare("SELECT * FROM attachments WHERE task_id = ?1 ORDER BY created_at ASC")?;
    let rows = statement.query_map([task_id], from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get(conn: &Connection, id: &str) -> ApiResult<Attachment> {
    conn.query_row("SELECT * FROM attachments WHERE id = ?1", [id], from_row)
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => ApiError::not_found("附件不存在"),
            other => ApiError::from(other),
        })
}
