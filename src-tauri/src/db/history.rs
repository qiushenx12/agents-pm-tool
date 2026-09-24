//! Task audit snapshots are captured by SQLite triggers, including cascades.
//! A request supplies the actor once; business data and history commit together.
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;

use crate::{
    domain::{task::now_str, user::User},
    error::{ApiError, ApiResult},
};

pub fn migrate(conn: &Connection) -> ApiResult<()> {
    conn.execute_batch(
        "BEGIN;
         CREATE TABLE task_operations (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           actor_id TEXT NOT NULL, actor_name TEXT NOT NULL,
           source TEXT NOT NULL, action TEXT NOT NULL, created_at TEXT NOT NULL,
           undone INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE task_history (
           operation_id INTEGER NOT NULL REFERENCES task_operations(id) ON DELETE CASCADE,
           task_id TEXT NOT NULL,
           before_json TEXT, after_json TEXT,
           before_project TEXT, after_project TEXT,
           PRIMARY KEY(operation_id, task_id)
         );
         CREATE INDEX idx_task_history_task ON task_history(task_id, operation_id DESC);
         CREATE TABLE task_audit_context (id INTEGER PRIMARY KEY CHECK(id=1), operation_id INTEGER NOT NULL);
         PRAGMA user_version=15;"
    )?;
    // A single view keeps snapshots consistent across direct and cascading changes.
    conn.execute_batch(
        "CREATE VIEW task_audit_snapshot AS SELECT t.id, json_object(
           'project', t.project, 'type', t.type, 'description', t.description,
           'note', t.note, 'status', t.status, 'priority', t.priority,
           'position', t.position, 'finished_at', t.finished_at,
           'assignee_user_id', t.assignee_user_id,
           'assignee_name', (SELECT username FROM users WHERE id=t.assignee_user_id),
           'owner_user_id', t.owner_user_id,
           'owner_name', (SELECT username FROM users WHERE id=t.owner_user_id),
           'predecessor_task_ids', json((SELECT json_group_array(predecessor_task_id) FROM
             (SELECT predecessor_task_id FROM task_dependencies WHERE task_id=t.id ORDER BY predecessor_task_id))),
           'unlock_task_ids', json((SELECT json_group_array(task_id) FROM
             (SELECT task_id FROM task_dependencies WHERE predecessor_task_id=t.id ORDER BY task_id))),
           'attachments', json((SELECT json_group_array(json_object('id', id, 'filename', filename)) FROM
             (SELECT id, filename FROM attachments WHERE task_id=t.id ORDER BY id)))
         ) AS snapshot FROM tasks t;"
    )?;
    for (table, key) in [
        ("tasks", "id"),
        ("attachments", "task_id"),
        ("task_dependencies", "task_id"),
    ] {
        for event in ["INSERT", "UPDATE", "DELETE"] {
            if table == "task_dependencies" && event == "UPDATE" {
                continue;
            }
            for timing in ["BEFORE", "AFTER"] {
                let row = if event == "DELETE" { "OLD" } else { "NEW" };
                let ids = if table == "task_dependencies" {
                    format!("{row}.task_id, {row}.predecessor_task_id")
                } else {
                    format!("{row}.{key}")
                };
                // INSERT has no old task; DELETE has no new task.
                let sql = if table == "tasks" && event == "INSERT" && timing == "BEFORE" {
                    String::new()
                } else if table == "tasks" && event == "DELETE" && timing == "AFTER" {
                    "UPDATE task_history SET after_json=NULL WHERE operation_id=(SELECT operation_id FROM task_audit_context WHERE id=1) AND task_id=OLD.id;".into()
                } else {
                    let before = if table == "tasks" && event == "INSERT" {
                        "NULL"
                    } else {
                        "snapshot"
                    };
                    format!("INSERT INTO task_history(operation_id, task_id, before_json, after_json, before_project, after_project)
                        SELECT (SELECT operation_id FROM task_audit_context WHERE id=1), id, {before}, snapshot, json_extract({before}, '$.project'), json_extract(snapshot, '$.project')
                        FROM task_audit_snapshot WHERE id IN ({ids})
                        ON CONFLICT(operation_id, task_id) DO UPDATE SET after_json=excluded.after_json, after_project=excluded.after_project;")
                };
                if sql.is_empty() {
                    continue;
                }
                conn.execute_batch(&format!(
                    "CREATE TRIGGER audit_{table}_{event}_{timing} {timing} {event} ON {table}
                     WHEN EXISTS(SELECT 1 FROM task_audit_context WHERE id=1) BEGIN {sql} END;"
                ))?;
            }
        }
    }
    conn.execute_batch("CREATE TRIGGER audit_users_DELETE_BEFORE BEFORE DELETE ON users
        WHEN EXISTS(SELECT 1 FROM task_audit_context WHERE id=1) BEGIN
        INSERT INTO task_history(operation_id,task_id,before_json,after_json,before_project,after_project)
          SELECT (SELECT operation_id FROM task_audit_context WHERE id=1), s.id,s.snapshot,s.snapshot,t.project,t.project
          FROM task_audit_snapshot s JOIN tasks t ON t.id=s.id
          WHERE t.owner_user_id=OLD.id OR t.assignee_user_id=OLD.id
          ON CONFLICT(operation_id,task_id) DO NOTHING;
        END;
        COMMIT;")?;
    Ok(())
}

/// Nested business writes use savepoints so even an audit failure rolls back the operation.
pub fn record<T>(
    conn: &mut Connection,
    user: &User,
    source: &str,
    action: &str,
    apply: impl FnOnce(&mut Connection) -> ApiResult<T>,
) -> ApiResult<(T, i64)> {
    conn.execute_batch("SAVEPOINT audit_operation")?;
    let mut guard = AuditRollback {
        conn,
        committed: false,
    };
    let tx = &mut *guard.conn;
    tx.execute("INSERT INTO task_operations(actor_id,actor_name,source,action,created_at) VALUES (?1,?2,?3,?4,?5)",
        params![user.id, user.username, source, action, now_str()])?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO task_audit_context(id,operation_id) VALUES (1,?1)",
        [id],
    )?;
    let result = apply(tx)?;
    tx.execute("DELETE FROM task_audit_context", [])?;
    tx.execute(
        "DELETE FROM task_history WHERE operation_id=?1 AND before_json IS after_json",
        [id],
    )?;
    let changed: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM task_history WHERE operation_id=?1)",
        [id],
        |r| r.get(0),
    )?;
    if !changed {
        tx.execute("DELETE FROM task_operations WHERE id=?1", [id])?;
    }
    tx.execute_batch("RELEASE audit_operation")?;
    guard.committed = true;
    Ok((result, if changed { id } else { 0 }))
}

struct AuditRollback<'a> {
    conn: &'a mut Connection,
    committed: bool,
}
impl Drop for AuditRollback<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self
                .conn
                .execute_batch("ROLLBACK TO audit_operation; RELEASE audit_operation;");
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HistoryChange {
    pub field: String,
    pub before: Value,
    pub after: Value,
}
#[derive(Debug, Serialize)]
pub struct HistoryEntry {
    pub operation_id: i64,
    pub actor_name: String,
    pub source: String,
    pub action: String,
    pub created_at: String,
    pub changes: Vec<HistoryChange>,
}
#[derive(Serialize)]
pub struct HistoryPage {
    pub items: Vec<HistoryEntry>,
    pub next_before: Option<i64>,
}

pub fn parse_snapshot(s: Option<String>) -> ApiResult<Value> {
    s.map(|s| serde_json::from_str(&s).map_err(ApiError::internal))
        .unwrap_or(Ok(Value::Null))
}

pub fn changes(before: &Value, after: &Value) -> Vec<HistoryChange> {
    if before.is_null() || after.is_null() {
        return vec![HistoryChange {
            field: "task".into(),
            before: before.clone(),
            after: after.clone(),
        }];
    }
    [
        "project",
        "type",
        "description",
        "note",
        "status",
        "priority",
        "assignee_user_id",
        "owner_user_id",
        "finished_at",
        "position",
        "predecessor_task_ids",
        "unlock_task_ids",
        "attachments",
    ]
    .into_iter()
    .filter(|field| before[field] != after[field])
    .map(|field| HistoryChange {
        field: field.into(),
        before: before[match field {
            "assignee_user_id" => "assignee_name",
            "owner_user_id" => "owner_name",
            _ => field,
        }]
        .clone(),
        after: after[match field {
            "assignee_user_id" => "assignee_name",
            "owner_user_id" => "owner_name",
            _ => field,
        }]
        .clone(),
    })
    .collect()
}

pub fn list(
    conn: &Connection,
    user: &User,
    id: &str,
    before_id: Option<i64>,
) -> ApiResult<HistoryPage> {
    let task = super::tasks::get(conn, id)?;
    super::permissions::require_project(conn, user, &task.project)?;
    let visible = super::permissions::visible_projects(conn, user)?;
    let mut stmt = conn.prepare("SELECT o.id,o.actor_name,o.source,o.action,o.created_at,h.before_json,h.after_json,h.before_project,h.after_project
        FROM task_history h JOIN task_operations o ON o.id=h.operation_id
        WHERE h.task_id=?1 AND o.id < ?2 ORDER BY o.id DESC LIMIT 51")?;
    let rows = stmt
        .query_map(params![id, before_id.unwrap_or(i64::MAX)], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let next_before = if rows.len() > 50 {
        Some(rows[49].0)
    } else {
        None
    };
    let mut items = Vec::new();
    for (
        operation_id,
        actor_name,
        source,
        action,
        created_at,
        before,
        after,
        before_project,
        after_project,
    ) in rows.into_iter().take(50)
    {
        let mut before = parse_snapshot(before)?;
        let mut after = parse_snapshot(after)?;
        // A task moved from a hidden project must not disclose its earlier contents.
        if let Some(projects) = &visible {
            if [before_project, after_project]
                .iter()
                .flatten()
                .any(|p| !projects.contains(p))
            {
                continue;
            }
        }
        for snapshot in [&mut before, &mut after] {
            if snapshot.is_null() || user.is_admin() {
                continue;
            }
            for field in ["predecessor_task_ids", "unlock_task_ids"] {
                if let Some(ids) = snapshot[field].as_array_mut() {
                    ids.retain(|v| {
                        v.as_str()
                            .and_then(|id| super::tasks::get(conn, id).ok())
                            .is_some_and(|t| {
                                super::permissions::require_project(conn, user, &t.project).is_ok()
                            })
                    });
                }
            }
        }
        let changes = changes(&before, &after);
        if !changes.is_empty() {
            items.push(HistoryEntry {
                operation_id,
                actor_name,
                source,
                action,
                created_at,
                changes,
            });
        }
    }
    Ok(HistoryPage { items, next_before })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, tasks, users};

    #[test]
    fn audit_is_atomic_and_ignores_noops() {
        let mut conn = db::open_memory().unwrap();
        let user = users::get(&conn, "host").unwrap();
        let (task, _) = record(&mut conn, &user, "web", "create", |conn| {
            tasks::create(
                conn,
                &tasks::NewTask {
                    project: "default-project",
                    task_type: "BUG",
                    description: "初始",
                    note: "",
                    submitter: "用户",
                    owner_user_id: Some("host"),
                    priority: None,
                    predecessor_task_ids: &[],
                    unlock_task_ids: &[],
                },
            )
        })
        .unwrap();
        let page = list(&conn, &user, &task.id, None).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].changes[0].field, "task");
        assert!(page.items[0].changes[0].before.is_null());
        let error: ApiResult<((), i64)> = record(&mut conn, &user, "web", "update", |conn| {
            tasks::patch(
                conn,
                &task.id,
                &tasks::TaskPatch {
                    note: Some("不应保存".into()),
                    ..Default::default()
                },
            )?;
            Err(ApiError::conflict("模拟失败"))
        });
        assert!(error.is_err());
        assert_eq!(tasks::get(&conn, &task.id).unwrap().note, "");
        assert_eq!(list(&conn, &user, &task.id, None).unwrap().items.len(), 1);
        let (_, id) = record(&mut conn, &user, "web", "update", |conn| {
            tasks::patch(conn, &task.id, &tasks::TaskPatch::default())
        })
        .unwrap();
        assert_eq!(id, 0);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM task_audit_context", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
