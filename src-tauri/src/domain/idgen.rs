use chrono::Local;
use rusqlite::Connection;

use crate::error::ApiResult;

/// 任务 ID：`yyyymmdd` + `hhmmss` + `xxxx`（全局序号 %04d 零填充，超 9999 自然变长）。
/// 必须在事务内调用：id_seq+1 → UPDATE meta → INSERT task（规划 §4.2）。
pub fn next_task_id(tx: &Connection) -> ApiResult<(String, i64)> {
    let seq: i64 = tx.query_row(
        "SELECT CAST(value AS INTEGER) + 1 FROM meta WHERE key = 'id_seq'",
        [],
        |r| r.get(0),
    )?;
    tx.execute(
        "UPDATE meta SET value = CAST(?1 AS TEXT) WHERE key = 'id_seq'",
        [seq],
    )?;
    let ts = Local::now().format("%Y%m%d%H%M%S");
    Ok((format!("{ts}{seq:04}"), seq))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn ids_are_sequential_and_monotonic() {
        let conn = db::open_memory().unwrap();
        let (id1, seq1) = next_task_id(&conn).unwrap();
        let (id2, seq2) = next_task_id(&conn).unwrap();
        assert_eq!(seq1, 1);
        assert_eq!(seq2, 2);
        assert!(id1.ends_with("0001"));
        assert!(id2.ends_with("0002"));
        assert!(id1.len() >= 18);
    }
}
