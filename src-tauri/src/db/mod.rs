pub mod projects;
pub mod schema;
pub mod task_page;
pub mod tasks;

use std::path::Path;

use rusqlite::Connection;

use crate::error::ApiResult;

const USER_VERSION: i32 = 5;

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
/// v5：tasks 增加 note（用户维护的备注）
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
    if version < 5 {
        conn.execute_batch("ALTER TABLE tasks ADD COLUMN note TEXT NOT NULL DEFAULT '';")?;
        conn.pragma_update(None, "user_version", 5)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn task_columns(conn: &Connection) -> Vec<String> {
        let mut stmt = conn.prepare("PRAGMA table_info(tasks)").unwrap();
        stmt.query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn fresh_database_contains_note_column() {
        let conn = open_memory().unwrap();
        assert!(task_columns(&conn).iter().any(|column| column == "note"));
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, USER_VERSION);
    }

    #[test]
    fn version_four_database_migrates_existing_tasks_with_empty_note() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(schema::SCHEMA_V1).unwrap();
        conn.execute_batch(
            "ALTER TABLE projects ADD COLUMN local_path TEXT NOT NULL DEFAULT '';
             ALTER TABLE projects ADD COLUMN git_url TEXT NOT NULL DEFAULT '';
             ALTER TABLE tasks ADD COLUMN position REAL NOT NULL DEFAULT 0;
             INSERT INTO tasks
               (id, seq, project, type, description, status, submitter, created_at, updated_at, position)
             VALUES
               ('legacy', 1, 'legacy-project', '优化', '旧任务', '未开始', '用户', '2026-01-01', '2026-01-01', 1);",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 4).unwrap();

        migrate(&conn).unwrap();

        let note: String = conn
            .query_row("SELECT note FROM tasks WHERE id = 'legacy'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(note, "");
        assert_eq!(task_columns(&conn).last().map(String::as_str), Some("note"));
    }
}
