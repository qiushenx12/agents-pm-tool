pub mod projects;
pub mod schema;
pub mod tasks;

use std::path::Path;

use rusqlite::Connection;

use crate::error::ApiResult;

const USER_VERSION: i32 = 1;

fn configure(conn: &Connection) -> ApiResult<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000_i64)?;
    Ok(())
}

/// user_version 迁移框架：当前仅 v1，后续版本在此追加
fn migrate(conn: &Connection) -> ApiResult<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(schema::SCHEMA_V1)?;
        conn.execute_batch(schema::SEED)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    debug_assert!(version <= USER_VERSION);
    Ok(())
}

pub fn open(path: &Path) -> ApiResult<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

/// 测试用内存库
pub fn open_memory() -> ApiResult<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}
