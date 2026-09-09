//! 10 万行冒烟数据灌入（规划 Phase 4）：直接走 db 层批量插入。
//! 用法：PM_DATA_DIR=<数据目录> cargo run --release --example seed -- 100000

use std::sync::Arc;
use std::time::Instant;

use agents_pm_tool_lib::{db, paths, server, settings::Settings};

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);

    let data_dir = paths::data_dir().expect("数据目录初始化失败");
    let conn = db::open(&paths::db_path(&data_dir)).expect("数据库初始化失败");
    let core = Arc::new(server::CoreStateInner::new(
        data_dir,
        conn,
        Settings::default(),
    ));

    let types = ["新增需求", "优化", "BUG"];
    let statuses = ["未开始", "进行中", "待验证", "已完成", "验收未通过", "验收通过"];
    let submitters = ["用户", "Agent"];

    let t0 = Instant::now();
    let mut conn = core.db.lock().unwrap();

    // 先清空再灌，保证冒烟可重复
    conn.execute("DELETE FROM tasks", []).unwrap();

    let tx = conn.transaction().unwrap();
    {
        let mut seq: i64 = tx
            .query_row("SELECT CAST(value AS INTEGER) FROM meta WHERE key='id_seq'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let mut stmt = tx
            .prepare(
                "INSERT INTO tasks (id, seq, project, type, description, status, submitter, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            )
            .unwrap();
        for i in 0..n {
            seq += 1;
            let id = format!("2026090{}12000{}{:04}", i % 9 + 1, i % 60, seq % 10000);
            stmt.execute(rusqlite::params![
                format!("{id}{seq:05}"),
                seq,
                "default-project",
                types[i % 3],
                format!("冒烟任务 #{i}：模拟描述文本，包含关键字 登录 支付 报表 导出 {}", i % 97),
                statuses[i % 6],
                submitters[i % 2],
                format!("2026-08-{:02} 10:{:02}:{:02}", i % 28 + 1, i % 60, i % 60),
            ])
            .unwrap();
        }
        tx.execute(
            "UPDATE meta SET value = CAST(?1 AS TEXT) WHERE key = 'id_seq'",
            [seq],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    println!("已插入 {n} 条任务，耗时 {:?}", t0.elapsed());
}
