use chrono::Local;
use rusqlite::Connection;

use crate::error::ApiResult;

/// 任务 ID：`yyyymmddhhmmss` + `xxxx`（同一秒内递增的后缀，%04d 零填充，超 9999 自然变长；
/// 进入新的一秒后后缀重新从 0000 开始）。
/// 返回的全局 seq 依旧单调递增，供 tasks.seq / position 初始化 / 排序兜底使用。
/// 必须在事务内调用：更新 meta 计数器 → INSERT task（规划 §4.2）。
pub fn next_task_id(tx: &Connection) -> ApiResult<(String, i64)> {
    let ts = Local::now().format("%Y%m%d%H%M%S").to_string();
    next_task_id_at(tx, &ts)
}

fn next_task_id_at(tx: &Connection, ts: &str) -> ApiResult<(String, i64)> {
    let seq: i64 = tx.query_row(
        "SELECT CAST(value AS INTEGER) + 1 FROM meta WHERE key = 'id_seq'",
        [],
        |r| r.get(0),
    )?;
    let (last_ts, last_suffix): (String, i64) = tx.query_row(
        "SELECT (SELECT value FROM meta WHERE key = 'id_ts'),
                CAST((SELECT value FROM meta WHERE key = 'id_suffix') AS INTEGER)",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let suffix = if last_ts == ts { last_suffix + 1 } else { 0 };
    tx.execute(
        "UPDATE meta SET value = CAST(?1 AS TEXT) WHERE key = 'id_seq'",
        [seq],
    )?;
    tx.execute("UPDATE meta SET value = ?1 WHERE key = 'id_ts'", [ts])?;
    tx.execute(
        "UPDATE meta SET value = CAST(?1 AS TEXT) WHERE key = 'id_suffix'",
        [suffix],
    )?;
    Ok((format!("{ts}{suffix:04}"), seq))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn suffix_counts_within_a_second_and_resets_on_the_next() {
        let conn = db::open_memory().unwrap();
        let (id1, seq1) = next_task_id_at(&conn, "20260909120000").unwrap();
        let (id2, seq2) = next_task_id_at(&conn, "20260909120000").unwrap();
        let (id3, seq3) = next_task_id_at(&conn, "20260909120000").unwrap();
        assert_eq!(id1, "202609091200000000");
        assert_eq!(id2, "202609091200000001");
        assert_eq!(id3, "202609091200000002");
        // 全局 seq 与 ID 后缀无关，始终单调递增
        assert_eq!((seq1, seq2, seq3), (1, 2, 3));

        let (id4, seq4) = next_task_id_at(&conn, "20260909120001").unwrap();
        assert_eq!(id4, "202609091200010000");
        assert_eq!(seq4, 4);
    }

    #[test]
    fn suffix_is_not_derived_from_global_seq() {
        let conn = db::open_memory().unwrap();
        // 把全局 seq 拨大：旧逻辑后缀会跟着涨，新逻辑仍从 0000 开始
        conn.execute("UPDATE meta SET value = '42' WHERE key = 'id_seq'", [])
            .unwrap();
        let (id, seq) = next_task_id_at(&conn, "20260909120000").unwrap();
        assert_eq!(id, "202609091200000000");
        assert_eq!(seq, 43);
    }
}
