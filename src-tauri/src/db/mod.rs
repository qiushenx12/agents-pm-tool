pub mod attachments;
pub mod permissions;
pub mod projects;
pub mod schema;
pub mod task_page;
pub mod tasks;
pub mod users;
pub mod view_state;

use std::path::Path;

use rusqlite::Connection;

use crate::error::ApiResult;

const USER_VERSION: i32 = 11;

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
/// v6：tasks 状态 CHECK 增加「取消」（SQLite 不能 ALTER CHECK，需重建表）
/// v7：meta 增加 id_ts / id_suffix（任务 ID 后缀改为同一秒内递增，0000 起）
/// v8：用户、Web 会话、按用户 Agent token 与细粒度权限
/// v9：历史任务回填主机归属；新任务由创建入口写入实际 owner_user_id
/// v10：user_view_state（按用户保存的分组/排序/筛选，供网页端与应用内窗口共用）
/// v11：tasks 增加 priority（高/中/低，默认中；含 CHECK，新列按 NOT NULL DEFAULT 追加）
fn migrate(conn: &Connection) -> ApiResult<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(schema::SCHEMA_V1)?;
        conn.execute_batch(schema::SEED)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    if version < 2 {
        conn.execute_batch("ALTER TABLE projects ADD COLUMN local_path TEXT NOT NULL DEFAULT '';")?;
        conn.pragma_update(None, "user_version", 2)?;
    }
    if version < 3 {
        conn.execute_batch("ALTER TABLE projects ADD COLUMN git_url TEXT NOT NULL DEFAULT '';")?;
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
    if version < 6 {
        // attachments 外键引用 tasks(id)，重建期间必须临时关闭外键，避免 DROP 时级联。
        // PRAGMA foreign_keys 不能在事务内修改，因此在 BEGIN 之前关闭。
        conn.pragma_update(None, "foreign_keys", "OFF")?;
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE tasks_v6 (
               id           TEXT PRIMARY KEY,
               seq          INTEGER NOT NULL UNIQUE,
               project      TEXT NOT NULL,
               type         TEXT NOT NULL CHECK (type IN ('新增需求','优化','BUG')),
               description  TEXT NOT NULL DEFAULT '',
               status       TEXT NOT NULL DEFAULT '未开始'
                            CHECK (status IN ('未开始','进行中','待验证','已完成','验收未通过','验收通过','取消')),
               submitter    TEXT NOT NULL CHECK (submitter IN ('用户','Agent')),
               created_at   TEXT NOT NULL,
               finished_at  TEXT,
               updated_at   TEXT NOT NULL,
               position     REAL NOT NULL DEFAULT 0,
               note         TEXT NOT NULL DEFAULT ''
             );
             INSERT INTO tasks_v6
               SELECT id, seq, project, type, description, status, submitter,
                      created_at, finished_at, updated_at, position, note FROM tasks;
             DROP TABLE tasks;
             ALTER TABLE tasks_v6 RENAME TO tasks;
             CREATE INDEX idx_tasks_filter ON tasks(project, type, status, submitter);
             CREATE INDEX idx_tasks_created ON tasks(created_at DESC);
             COMMIT;",
        )?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "user_version", 6)?;
    }
    if version < 7 {
        // 旧库没有同秒后缀计数器；id_ts 置空使首个新 ID 从 0000 开始。
        // 旧逻辑后缀 = 全局 seq（>= 1），永不产生 0000 后缀，故新旧 ID 不冲突。
        conn.execute_batch(
            "INSERT OR IGNORE INTO meta(key, value) VALUES ('id_ts', ''), ('id_suffix', '0');",
        )?;
        conn.pragma_update(None, "user_version", 7)?;
    }
    if version < 8 {
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE users (
               id            TEXT PRIMARY KEY,
               username      TEXT NOT NULL UNIQUE,
               password_hash TEXT NOT NULL DEFAULT '',
               role          TEXT NOT NULL DEFAULT 'user'
                             CHECK (role IN ('super_admin','admin','user')),
               created_at    TEXT NOT NULL,
               disabled      INTEGER NOT NULL DEFAULT 0 CHECK (disabled IN (0,1))
             );
             CREATE TABLE sessions (
               token      TEXT PRIMARY KEY,
               user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
               created_at TEXT NOT NULL,
               expires_at TEXT NOT NULL
             );
             CREATE INDEX idx_sessions_user ON sessions(user_id);
             CREATE INDEX idx_sessions_expiry ON sessions(expires_at);
             CREATE TABLE user_permissions (
               user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
               project        TEXT NOT NULL,
               field          TEXT NOT NULL,
               allowed_values TEXT,
               PRIMARY KEY (user_id, project, field)
             );
             CREATE INDEX idx_user_permissions_project ON user_permissions(project);
             CREATE TABLE agent_tokens (
               token      TEXT PRIMARY KEY,
               user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
               created_at TEXT NOT NULL,
               revoked    INTEGER NOT NULL DEFAULT 0 CHECK (revoked IN (0,1))
             );
             CREATE INDEX idx_agent_tokens_user ON agent_tokens(user_id);
             ALTER TABLE tasks ADD COLUMN owner_user_id TEXT REFERENCES users(id) ON DELETE SET NULL;
             CREATE INDEX idx_tasks_owner_user ON tasks(owner_user_id);
             PRAGMA user_version = 8;
             COMMIT;",
        )?;
    }
    users::ensure_host(conn)?;
    if version < 8 {
        // 旧版只有全局主机 token，因此存量 Agent 任务归属主机账号。
        conn.execute(
            "UPDATE tasks SET owner_user_id=?1
             WHERE submitter='Agent' AND owner_user_id IS NULL",
            [crate::domain::user::HOST_USER_ID],
        )?;
    }
    if version < 9 {
        // 旧版本没有记录网页任务创建账号；当时的存量记录按单主机模型归属主机。
        conn.execute_batch(
            "BEGIN;
             UPDATE tasks SET owner_user_id='host' WHERE owner_user_id IS NULL;
             PRAGMA user_version = 9;
             COMMIT;",
        )?;
    }
    if version < 10 {
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS user_view_state (
               user_id    TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
               filters    TEXT NOT NULL DEFAULT '',
               updated_at TEXT NOT NULL
             );
             PRAGMA user_version = 10;
             COMMIT;",
        )?;
    }
    if version < 11 {
        // 加列不需要重建表：新列是常量默认值，SQLite 直接回填存量行。
        conn.execute_batch(
            "ALTER TABLE tasks ADD COLUMN priority TEXT NOT NULL DEFAULT '中'
              CHECK (priority IN ('高','中','低'));
             PRAGMA user_version = 11;",
        )?;
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
    fn fresh_database_contains_default_project() {
        let conn = open_memory().unwrap();
        let projects = projects::list(&conn).unwrap();

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "default-project");
    }

    #[test]
    fn fresh_database_contains_user_tables_and_host_account() {
        let conn = open_memory().unwrap();
        for table in ["users", "sessions", "user_permissions", "agent_tokens"] {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing table {table}");
        }
        let host = users::get(&conn, crate::domain::user::HOST_USER_ID).unwrap();
        assert_eq!(host.username, crate::domain::user::DEFAULT_HOST_USERNAME);
        assert_eq!(host.role, "super_admin");
        assert!(!host.disabled);
    }

    #[test]
    fn version_seven_database_migrates_users_and_preserves_tasks() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(schema::SCHEMA_V1).unwrap();
        conn.execute_batch(schema::SEED).unwrap();
        conn.execute_batch(
            "ALTER TABLE projects ADD COLUMN local_path TEXT NOT NULL DEFAULT '';
             ALTER TABLE projects ADD COLUMN git_url TEXT NOT NULL DEFAULT '';
             ALTER TABLE tasks ADD COLUMN position REAL NOT NULL DEFAULT 0;
             ALTER TABLE tasks ADD COLUMN note TEXT NOT NULL DEFAULT '';
             INSERT INTO meta(key, value) VALUES ('id_ts', ''), ('id_suffix', '0');
             INSERT INTO tasks
               (id, seq, project, type, description, status, submitter, created_at, updated_at, position, note)
             VALUES
               ('legacy-v7', 1, 'default-project', '优化', '保留数据', '未开始', 'Agent', '2026-01-01', '2026-01-01', 1, '');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 7).unwrap();

        migrate(&conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE id='legacy-v7'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let owner: Option<String> = conn
            .query_row(
                "SELECT owner_user_id FROM tasks WHERE id='legacy-v7'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(owner.as_deref(), Some("host"));
        assert_eq!(users::get(&conn, "host").unwrap().role, "super_admin");
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, USER_VERSION);
    }

    #[test]
    fn version_eight_database_backfills_only_unowned_tasks_to_host() {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO users(id, username, role, created_at) VALUES ('alice', 'alice', 'user', '2026-01-01')",
            [],
        )
        .unwrap();
        conn.execute_batch(
            "INSERT INTO tasks
               (id, seq, project, type, description, status, submitter, created_at, updated_at, position, note, owner_user_id)
             VALUES
               ('legacy-user', 1, 'default-project', '优化', '旧网页任务', '未开始', '用户', '2026-01-01', '2026-01-01', 1, '', NULL),
               ('owned-user', 2, 'default-project', '优化', '已有归属', '未开始', '用户', '2026-01-02', '2026-01-02', 2, '', 'alice');",
        )
        .unwrap();
        // 回退到 v8 之前；v11 列已存在，需一并去掉以模拟旧库
        conn.execute_batch("PRAGMA user_version = 8; ALTER TABLE tasks DROP COLUMN priority;")
            .unwrap();

        migrate(&conn).unwrap();

        let legacy_owner: String = conn
            .query_row(
                "SELECT owner_user_id FROM tasks WHERE id='legacy-user'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let existing_owner: String = conn
            .query_row(
                "SELECT owner_user_id FROM tasks WHERE id='owned-user'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(legacy_owner, "host");
        assert_eq!(existing_owner, "alice");
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, USER_VERSION);
    }

    #[test]
    fn version_nine_database_gains_user_view_state_table() {
        let conn = open_memory().unwrap();
        // 模拟 v9 旧库：没有按用户保存的视图设置，也没有 v11 的 priority 列
        conn.execute_batch("DROP TABLE user_view_state; PRAGMA user_version = 9; ALTER TABLE tasks DROP COLUMN priority;")
            .unwrap();

        migrate(&conn).unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='user_view_state')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists);
        view_state::put(&conn, "host", r#"{"group_by":"status"}"#).unwrap();
        assert!(view_state::get(&conn, "host").unwrap().is_some());
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, USER_VERSION);
    }

    #[test]
    fn version_ten_database_gains_priority_with_default_medium() {
        let conn = open_memory().unwrap();
        // 模拟 v10 旧库：tasks 表回退到没有 priority 的结构
        conn.execute_batch("PRAGMA foreign_keys=OFF; BEGIN;
             CREATE TABLE tasks_v10 (
               id TEXT PRIMARY KEY, seq INTEGER NOT NULL UNIQUE, project TEXT NOT NULL,
               type TEXT NOT NULL CHECK (type IN ('新增需求','优化','BUG')),
               description TEXT NOT NULL DEFAULT '',
               status TEXT NOT NULL DEFAULT '未开始'
                      CHECK (status IN ('未开始','进行中','待验证','已完成','验收未通过','验收通过','取消')),
               submitter TEXT NOT NULL CHECK (submitter IN ('用户','Agent')),
               created_at TEXT NOT NULL, finished_at TEXT, updated_at TEXT NOT NULL,
               position REAL NOT NULL DEFAULT 0, note TEXT NOT NULL DEFAULT '',
               owner_user_id TEXT REFERENCES users(id) ON DELETE SET NULL
             );
             INSERT INTO tasks_v10
               SELECT id, seq, project, type, description, status, submitter,
                      created_at, finished_at, updated_at, position, note, owner_user_id FROM tasks;
             DROP TABLE tasks;
             ALTER TABLE tasks_v10 RENAME TO tasks;
             PRAGMA user_version = 10; COMMIT; PRAGMA foreign_keys=ON;")
            .unwrap();
        // 旧库里先放一条没有 priority 的存量任务
        conn.execute(
            "INSERT INTO tasks (id, seq, project, type, description, status, submitter, created_at, updated_at, position, note, owner_user_id)
             VALUES ('legacy-p', 99, 'default-project', '优化', '存量任务', '未开始', '用户', '2026-01-01', '2026-01-01', 99, '', 'host')",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        // 存量行回填默认「中」
        let priority: String = conn
            .query_row("SELECT priority FROM tasks WHERE id='legacy-p'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(priority, "中");
        // CHECK 生效：非法值写不进
        assert!(
            conn.execute("UPDATE tasks SET priority='紧急' WHERE id='legacy-p'", [])
                .is_err()
        );
        conn.execute("UPDATE tasks SET priority='高' WHERE id='legacy-p'", [])
            .unwrap();
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, USER_VERSION);
    }

    #[test]
    fn opening_database_repairs_host_role_and_disabled_flag() {
        let conn = open_memory().unwrap();
        conn.execute(
            "UPDATE users SET role='user', disabled=1, password_hash='bad' WHERE id='host'",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let host = users::get(&conn, "host").unwrap();
        assert_eq!(host.role, "super_admin");
        assert!(!host.disabled);
        let password_hash: String = conn
            .query_row(
                "SELECT password_hash FROM users WHERE id='host'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(password_hash.is_empty());
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
        assert!(task_columns(&conn).iter().any(|column| column == "note"));
    }

    #[test]
    fn version_five_database_gains_cancel_status_without_losing_data() {
        let conn = Connection::open_in_memory().unwrap();
        // 模拟 v5 旧库：SCHEMA_V1 + v2~v5 列，但 CHECK 不含「取消」
        conn.execute_batch(
            "CREATE TABLE tasks (
               id           TEXT PRIMARY KEY,
               seq          INTEGER NOT NULL UNIQUE,
               project      TEXT NOT NULL,
               type         TEXT NOT NULL CHECK (type IN ('新增需求','优化','BUG')),
               description  TEXT NOT NULL DEFAULT '',
               status       TEXT NOT NULL DEFAULT '未开始'
                            CHECK (status IN ('未开始','进行中','待验证','已完成','验收未通过','验收通过')),
               submitter    TEXT NOT NULL CHECK (submitter IN ('用户','Agent')),
               created_at   TEXT NOT NULL,
               finished_at  TEXT,
               updated_at   TEXT NOT NULL,
               position     REAL NOT NULL DEFAULT 0,
               note         TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE projects (
               name TEXT PRIMARY KEY, color TEXT NOT NULL DEFAULT '#007AFF',
               sort_order INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL,
               local_path TEXT NOT NULL DEFAULT '', git_url TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE attachments (
               id TEXT PRIMARY KEY,
               task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
               filename TEXT NOT NULL, stored_path TEXT NOT NULL,
               mime TEXT, size INTEGER NOT NULL, created_at TEXT NOT NULL
             );
             CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO meta(key, value) VALUES ('id_seq', '1');
             INSERT INTO projects(name, color, sort_order, created_at)
               VALUES ('p', '#007AFF', 0, '2026-01-01');
             INSERT INTO tasks
               (id, seq, project, type, description, status, submitter, created_at, updated_at, position, note)
             VALUES
               ('t1', 1, 'p', 'BUG', '旧任务', '已完成', '用户', '2026-01-01', '2026-01-02', 1, '备注');
             INSERT INTO attachments(id, task_id, filename, stored_path, size, created_at)
             VALUES ('a1', 't1', 'f.png', 'x/f.png', 10, '2026-01-01');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 5).unwrap();

        migrate(&conn).unwrap();

        // 旧任务与备注保留
        let (status, note): (String, String) = conn
            .query_row("SELECT status, note FROM tasks WHERE id = 't1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((status.as_str(), note.as_str()), ("已完成", "备注"));
        // 附件未被级联删除
        let attachments: i64 = conn
            .query_row("SELECT COUNT(*) FROM attachments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(attachments, 1);
        // 新 CHECK 允许「取消」
        conn.execute("UPDATE tasks SET status = '取消' WHERE id = 't1'", [])
            .unwrap();
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, USER_VERSION);
    }

    #[test]
    fn version_six_database_gains_per_second_id_suffix() {
        let conn = Connection::open_in_memory().unwrap();
        // 模拟 v6 旧库：meta 只有 id_seq，没有 id_ts / id_suffix
        conn.execute_batch(schema::SCHEMA_V1).unwrap();
        conn.execute_batch(schema::SEED).unwrap();
        conn.execute("UPDATE meta SET value = '7' WHERE key = 'id_seq'", [])
            .unwrap();
        conn.pragma_update(None, "user_version", 6).unwrap();

        migrate(&conn).unwrap();

        let (id, seq) = crate::domain::idgen::next_task_id(&conn).unwrap();
        // 后缀从 0000 开始，全局 seq 在旧值上继续递增
        assert!(id.ends_with("0000"));
        assert_eq!(seq, 8);
        let (id2, _) = crate::domain::idgen::next_task_id(&conn).unwrap();
        if id2[..14] == id[..14] {
            assert!(id2.ends_with("0001"));
        } else {
            // 两次调用跨过一秒边界时，后缀重新从 0000 开始
            assert!(id2.ends_with("0000"));
        }
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, USER_VERSION);
    }
}
