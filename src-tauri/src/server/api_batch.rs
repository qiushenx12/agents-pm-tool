use crate::{
    db::{permissions, tasks},
    domain::task::Task,
    domain::user::User,
    error::{ApiError, ApiResult},
    server::CoreState,
};
use axum::{
    extract::{Extension, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchPatch {
    pub project: Option<String>,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum BatchRequest {
    Update { ids: Vec<String>, patch: BatchPatch },
    Delete { ids: Vec<String> },
}
#[derive(Serialize)]
pub struct BatchError {
    code: &'static str,
    message: String,
}
#[derive(Serialize)]
pub struct BatchItem {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<Task>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<BatchError>,
}
#[derive(Serialize)]
pub struct BatchResponse {
    results: Vec<BatchItem>,
    succeeded: usize,
    failed: usize,
}

pub async fn batch_tasks(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Json(body): Json<BatchRequest>,
) -> ApiResult<Json<BatchResponse>> {
    let (ids, patch) = match body {
        BatchRequest::Update { ids, patch } => {
            if patch.project.is_none()
                && patch.task_type.is_none()
                && patch.status.is_none()
                && patch.priority.is_none()
            {
                return Err(ApiError::bad_request("请选择要批量修改的字段"));
            }
            (ids, Some(patch))
        }
        BatchRequest::Delete { ids } => (ids, None),
    };
    if ids.is_empty() || ids.len() > 500 || ids.iter().any(|id| id.trim().is_empty()) {
        return Err(ApiError::bad_request("每次批量操作需选择 1–500 条任务"));
    }
    let mut seen = HashSet::new();
    let ids = ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect::<Vec<_>>();
    let mut conn = core.db.lock().unwrap();
    let mut results = Vec::with_capacity(ids.len());
    let mut cleanup = Vec::new();
    let mut notices = Vec::new();
    let mut succeeded = 0;
    for id in ids {
        let result: ApiResult<Option<Task>> = (|| {
            let current = tasks::get(&conn, &id)?;
            match &patch {
                Some(patch) => {
                    let mut fields = Vec::new();
                    if patch.project.is_some() {
                        fields.push(("project", None));
                    }
                    if let Some(value) = patch.task_type.as_deref() {
                        fields.push(("type", Some(value)));
                    }
                    if let Some(value) = patch.status.as_deref() {
                        fields.push(("status", Some(value)));
                    }
                    if let Some(value) = patch.priority.as_deref() {
                        fields.push(("priority", Some(value)));
                    }
                    permissions::require_fields(&conn, &user, &current.project, &fields)?;
                    if let Some(project) = patch.project.as_deref() {
                        permissions::require_project(&conn, &user, project)?;
                    }
                    tasks::patch(
                        &mut conn,
                        &id,
                        &tasks::TaskPatch {
                            project: patch.project.clone(),
                            task_type: patch.task_type.clone(),
                            status: patch.status.clone(),
                            priority: patch.priority.clone(),
                            description: None,
                            note: None,
                            predecessor_task_ids: None,
                            unlock_task_ids: None,
                        },
                    )
                    .map(|task| {
                        if let Some(notice) =
                            super::finish_notice::notice_on_finish(&current, &task)
                        {
                            notices.push(notice);
                        }
                        Some(task)
                    })
                }
                None => {
                    permissions::require_field(
                        &conn,
                        &user,
                        &current.project,
                        "task_delete",
                        None,
                    )?;
                    tasks::remove(&conn, &id).map(|paths| {
                        cleanup.extend(paths);
                        None
                    })
                }
            }
        })();
        match result {
            Ok(task) => {
                succeeded += 1;
                results.push(BatchItem {
                    id,
                    task,
                    error: None,
                });
            }
            Err(error) => results.push(BatchItem {
                id,
                task: None,
                error: Some(BatchError {
                    code: error.code,
                    message: error.message,
                }),
            }),
        }
    }
    let visible = permissions::visible_projects(&conn, &user)?;
    for item in &mut results {
        if let Some(task) = item.task.as_mut() {
            tasks::retain_visible_dependencies(
                &conn,
                std::slice::from_mut(task),
                visible.as_deref(),
            )?;
        }
    }
    drop(conn);
    for notice in notices {
        core.finish_notices.notify(notice);
    }
    for path in cleanup {
        if let Err(error) = std::fs::remove_file(core.data_dir.join(&path)) {
            eprintln!("清理附件文件失败 {path}: {error}");
        }
    }
    if succeeded > 0 {
        core.events.notify();
    }
    let failed = results.len() - succeeded;
    Ok(Json(BatchResponse {
        results,
        succeeded,
        failed,
    }))
}
