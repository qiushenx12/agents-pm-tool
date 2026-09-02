/// 规划 §4.1 固定任务表结构（无泛化建模）
pub const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS tasks (
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
  updated_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_filter ON tasks(project, type, status, submitter);
CREATE INDEX IF NOT EXISTS idx_tasks_created ON tasks(created_at DESC);

CREATE TABLE IF NOT EXISTS projects (
  name       TEXT PRIMARY KEY,
  color      TEXT NOT NULL DEFAULT '#007AFF',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS attachments (
  id          TEXT PRIMARY KEY,
  task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  filename    TEXT NOT NULL,
  stored_path TEXT NOT NULL,
  mime        TEXT,
  size        INTEGER NOT NULL,
  created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_attach_task ON attachments(task_id);

CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#;

pub const SEED: &str = r#"
INSERT OR IGNORE INTO meta(key, value) VALUES ('id_seq', '0');
INSERT OR IGNORE INTO projects(name, color, sort_order, created_at)
VALUES ('agents-pm-tool', '#007AFF', 0, datetime('now', 'localtime'));
"#;
