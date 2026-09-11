//! 按用户保存的表格视图设置（项目/类型/状态/提交人、关键词、排序、分组）。
//!
//! 为什么放在服务端而不是浏览器：网页端与「进入应用」的内嵌窗口用的是两套
//! WebView 数据目录，localStorage 互不可见。想让两端打开时看到同一份分组、
//! 排序、筛选，就必须把这份状态放到两边都能读到的服务端。
//!
//! 只存当前视图，不存命名的筛选方案（那份仍在各客户端 localStorage 里）。

use rusqlite::{Connection, OptionalExtension};

use crate::domain::task::now_str;
use crate::error::ApiResult;

/// 读取该用户上次保存的视图设置（JSON 原文）；从未保存过时为 None
pub fn get(conn: &Connection, user_id: &str) -> ApiResult<Option<String>> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT filters FROM user_view_state WHERE user_id = ?1",
            [user_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(stored.filter(|raw| !raw.is_empty()))
}

/// 覆盖保存（按用户唯一）；同一用户反复切换筛选只留最后一份
pub fn put(conn: &Connection, user_id: &str, filters: &str) -> ApiResult<()> {
    conn.execute(
        "INSERT INTO user_view_state (user_id, filters, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(user_id) DO UPDATE SET filters = excluded.filters, updated_at = excluded.updated_at",
        rusqlite::params![user_id, filters, now_str()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn keeps_one_row_per_user_and_returns_the_latest() {
        let conn = db::open_memory().unwrap();
        assert_eq!(get(&conn, "host").unwrap(), None);

        put(&conn, "host", r#"{"status":["进行中"]}"#).unwrap();
        put(&conn, "host", r#"{"group_by":"status"}"#).unwrap();

        assert_eq!(
            get(&conn, "host").unwrap().as_deref(),
            Some(r#"{"group_by":"status"}"#)
        );
        let rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM user_view_state WHERE user_id='host'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn scopes_view_state_to_the_signed_in_user() {
        let conn = db::open_memory().unwrap();
        conn.execute(
            "INSERT INTO users(id, username, role, created_at) VALUES ('alice','alice','user','2026-01-01')",
            [],
        )
        .unwrap();
        put(&conn, "host", r#"{"keyword":"主机"}"#).unwrap();

        assert_eq!(get(&conn, "alice").unwrap(), None);
        assert!(get(&conn, "host").unwrap().is_some());
    }

    #[test]
    fn treats_an_empty_payload_as_no_saved_view() {
        let conn = db::open_memory().unwrap();
        put(&conn, "host", "").unwrap();

        assert_eq!(get(&conn, "host").unwrap(), None);
    }
}
