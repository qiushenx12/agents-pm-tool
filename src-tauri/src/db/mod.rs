pub mod projects;
pub mod schema;
pub mod task_page;
pub mod tasks;

use std::path::Path;

use rusqlite::Connection;

use crate::error::ApiResult;

const USER_VERSION: i32 = 4;

fn configure(conn: &Connection) -> ApiResult<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000_i64)?;
    Ok(())
}

/// user_version 迁移框架：后续版本在此追加
/// v2：projects 增加 local_path（项目本地路径，供网页端记录/跳转）
/// v3：projects 增加 git_url（远端仓库地址，供 Agent 查询）
/// v4：tasks 增加 position（手动排序位置，实数中点插入；初始 = seq）
fn migrate(conn: &Connection) -> ApiResult<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(schema::SCHEMA_V1)?;
        conn.execute_batch(schema::SEED)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    if version < 2 {
        conn.execute_batch(
            "ALTER TABLE projects ADD COLUMN local_path TEXT NOT NULL DEFAULT '';",
        )?;
        conn.pragma_update(None, "user_version", 2)?;
    }
    if version < 3 {
        conn.execute_batch(
            "ALTER TABLE projects ADD COLUMN git_url TEXT NOT NULL DEFAULT '';",
        )?;
        conn.pragma_update(None, "user_version", 3)?;
    }
    if version < 4 {
        conn.execute_batch(
            "ALTER TABLE tasks ADD COLUMN position REAL NOT NULL DEFAULT 0;
             UPDATE tasks SET position = seq;",
        )?;
        conn.pragma_update(None, "user_version", 4)?;
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
