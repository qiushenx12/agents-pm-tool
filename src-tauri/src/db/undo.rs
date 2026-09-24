//! Undo consumes only server-owned snapshots and rechecks current Web permissions.
use super::{history, permissions, tasks};
use crate::{
    domain::{task::Task, user::User},
    error::{ApiError, ApiResult},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

const EDITABLE: [&str; 10] = [
    "project",
    "type",
    "description",
    "note",
    "status",
    "priority",
    "assignee_user_id",
    "predecessor_task_ids",
    "unlock_task_ids",
    "position",
];

fn strings(value: &Value) -> ApiResult<Vec<String>> {
    serde_json::from_value(value.clone()).map_err(ApiError::internal)
}

pub struct UndoResult {
    pub before: Vec<Task>,
    pub tasks: Vec<Task>,
}

pub fn apply(conn: &mut Connection, user: &User, operation_id: i64) -> ApiResult<UndoResult> {
    let operation: Option<(String, String, String, bool)> = conn
        .query_row(
            "SELECT actor_id,source,action,undone FROM task_operations WHERE id=?1",
            [operation_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((actor, source, action, undone)) = operation else {
        return Err(ApiError::not_found("操作记录不存在"));
    };
    if actor != user.id || source != "web" {
        return Err(ApiError::forbidden("只能撤销自己在网页端的操作"));
    }
    if undone {
        return Err(ApiError::conflict("该操作已经撤销"));
    }
    if !matches!(action.as_str(), "update" | "batch_update" | "reorder") {
        return Err(ApiError::unprocessable("此操作不支持撤销"));
    }
    let rows = {
        let mut stmt = conn.prepare("SELECT task_id,before_json,after_json FROM task_history WHERE operation_id=?1 ORDER BY task_id")?;
        let rows = stmt.query_map([operation_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let mut plans = Vec::new();
    let mut previous = Vec::new();
    for (id, before, after) in rows {
        let before = history::parse_snapshot(before)?;
        let after = history::parse_snapshot(after)?;
        if before.is_null() || after.is_null() {
            return Err(ApiError::conflict("任务已创建或删除，不能撤销此操作"));
        }
        let task =
            tasks::get(conn, &id).map_err(|_| ApiError::conflict("任务已被删除，无法撤销"))?;
        permissions::require_project(conn, user, &task.project)?;
        let current: String = conn.query_row(
            "SELECT snapshot FROM task_audit_snapshot WHERE id=?1",
            [&id],
            |r| r.get(0),
        )?;
        let current = history::parse_snapshot(Some(current))?;
        let mut patch = tasks::TaskPatch::default();
        let mut position = None;
        for field in EDITABLE {
            if before[field] == after[field] {
                continue;
            }
            if current[field] != after[field] {
                return Err(ApiError::conflict(
                    "相关字段已被其他操作修改，未撤销；请刷新后检查任务",
                ));
            }
            let permission = match field {
                "assignee_user_id" => "assignee",
                "position" => "reorder",
                _ => field,
            };
            let target = if matches!(field, "status" | "type" | "priority") {
                before[field].as_str()
            } else {
                None
            };
            permissions::require_field(conn, user, &task.project, permission, target)?;
            let text = || before[field].as_str().unwrap_or_default().to_string();
            match field {
                "project" => {
                    permissions::require_project(conn, user, &text())?;
                    patch.project = Some(text());
                }
                "type" => patch.task_type = Some(text()),
                "description" => patch.description = Some(text()),
                "note" => patch.note = Some(text()),
                "status" => patch.status = Some(text()),
                "priority" => patch.priority = Some(text()),
                "assignee_user_id" => {
                    patch.assignee_user_id = Some(before[field].as_str().map(str::to_string))
                }
                "predecessor_task_ids" | "unlock_task_ids" => {
                    // Restoring relationships may not reveal or clear inaccessible edges.
                    for related in strings(&before[field])?
                        .into_iter()
                        .chain(strings(&after[field])?)
                    {
                        let related = tasks::get(conn, &related)
                            .map_err(|_| ApiError::conflict("关联任务已被删除，无法撤销"))?;
                        permissions::require_project(conn, user, &related.project)?;
                    }
                    if field == "predecessor_task_ids" {
                        patch.predecessor_task_ids = Some(strings(&before[field])?);
                    } else {
                        patch.unlock_task_ids = Some(strings(&before[field])?);
                    }
                }
                "position" => position = before[field].as_f64(),
                _ => unreachable!(),
            }
        }
        previous.push(task);
        plans.push((id, patch, position));
    }
    let (result, _) = history::record(conn, user, "web", "undo", |conn| {
        let mut statuses = Vec::new();
        let ids: Vec<String> = plans.iter().map(|(id, _, _)| id.clone()).collect();
        for (id, mut patch, position) in plans {
            if let Some(status) = patch.status.take() {
                statuses.push((id.clone(), status));
            }
            tasks::patch(conn, &id, &patch)?;
            if let Some(position) = position {
                conn.execute(
                    "UPDATE tasks SET position=?2 WHERE id=?1",
                    params![id, position],
                )?;
            }
        }
        // Restore children before parents; status gates and completion timestamps stay authoritative.
        while !statuses.is_empty() {
            let mut next = None;
            for (index, (id, _)) in statuses.iter().enumerate() {
                let task = tasks::get(conn, id)?;
                if !statuses
                    .iter()
                    .any(|(child, _)| task.predecessor_task_ids.contains(child))
                {
                    next = Some(index);
                    break;
                }
            }
            let index = next.ok_or_else(|| ApiError::conflict("任务关系已改变，无法撤销"))?;
            let (id, status) = statuses.remove(index);
            tasks::patch(
                conn,
                &id,
                &tasks::TaskPatch {
                    status: Some(status),
                    ..Default::default()
                },
            )?;
        }
        conn.execute(
            "UPDATE task_operations SET undone=1 WHERE id=?1",
            [operation_id],
        )?;
        ids.iter()
            .map(|id| tasks::get(conn, id))
            .collect::<ApiResult<Vec<_>>>()
    })?;
    Ok(UndoResult {
        before: previous,
        tasks: result,
    })
}
