use axum::{
    extract::{Extension, Path, RawQuery, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::db::{permissions, projects, tasks};
use crate::domain::task as domain;
use crate::domain::user::User;
use crate::error::{ApiError, ApiResult};
use crate::server::{parse_task_filter, CoreState};

/// /api/agent/*：权限收窄全部在服务端强制（规划 §5.4），CLI 只是 HTTP 薄客户端。

pub async fn list_tasks(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    RawQuery(q): RawQuery,
) -> ApiResult<impl IntoResponse> {
    let mut filter = parse_task_filter(q.as_deref())?;
    let conn = core.db.lock().unwrap();
    filter.visible_projects = permissions::visible_projects(&conn, &user)?;
    Ok(Json(tasks::list(&conn, &filter)?))
}

pub async fn get_task(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    let task = tasks::get(&conn, &id)?;
    permissions::require_project(&conn, &user, &task.project)?;
    Ok(Json(task))
}

#[derive(Deserialize)]
pub struct AgentCreateBody {
    pub project: Option<String>,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub description: Option<String>,
}

/// Agent 创建：项目/类型/描述三必填，submitter 强制 Agent，status 固定未开始
pub async fn create_task(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Json(body): Json<AgentCreateBody>,
) -> ApiResult<impl IntoResponse> {
    let project = body
        .project
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::unprocessable("项目为必填项（--project）"))?;
    let task_type = body
        .task_type
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::unprocessable("任务类型为必填项（--type）"))?;
    let description = body
        .description
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::unprocessable("Agent 创建任务必须填写描述（--description）"))?;

    let mut conn = core.db.lock().unwrap();
    permissions::require_fields(
        &conn,
        &user,
        project.trim(),
        &[
            ("task_create", None),
            ("type", Some(task_type.trim())),
            ("description", None),
        ],
    )?;
    let task = tasks::create(
        &mut conn,
        &tasks::NewTask {
            project: project.trim(),
            task_type: task_type.trim(),
            description: description.trim(),
            note: "",
            submitter: "Agent",
            owner_user_id: Some(&user.id),
        },
    )?;
    drop(conn);
    core.events.notify();
    Ok((StatusCode::CREATED, Json(task)))
}

#[derive(Deserialize)]
pub struct AgentStatusBody {
    pub status: Option<String>,
}

/// Agent 仅可切到 进行中/待验证/已完成（验收类状态留给用户）
pub async fn patch_status(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<AgentStatusBody>,
) -> ApiResult<impl IntoResponse> {
    let status = body
        .status
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::unprocessable("缺少 status"))?;
    if !domain::is_valid_status(&status) {
        return Err(ApiError::unprocessable(format!(
            "状态不合法：{status}，合法取值：{}",
            domain::STATUSES.join(" / ")
        )));
    }
    if !domain::is_agent_status(&status) {
        return Err(ApiError::forbidden(format!(
            "Agent 无权切换到「{status}」（仅可切到：{}），验收由用户完成",
            domain::AGENT_STATUSES.join(" / ")
        )));
    }

    let mut conn = core.db.lock().unwrap();
    let current = tasks::get(&conn, &id)?;
    permissions::require_field(&conn, &user, &current.project, "status", Some(&status))?;
    let task = tasks::patch(
        &mut conn,
        &id,
        &tasks::TaskPatch {
            status: Some(status),
            ..Default::default()
        },
    )?;
    drop(conn);
    core.events.notify();
    Ok(Json(task))
}

#[derive(Deserialize)]
pub struct AgentDescriptionBody {
    pub description: Option<String>,
}

/// Agent 只能改 submitter=Agent 的任务描述（用户创建的任务 CLI 不可碰）
pub async fn patch_description(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<AgentDescriptionBody>,
) -> ApiResult<impl IntoResponse> {
    // 与 create 对齐：描述 trim 后不能为空（不允许借 describe 清空描述）
    let description = body
        .description
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::unprocessable("描述不能为空（--description）"))?;

    let mut conn = core.db.lock().unwrap();
    let current = tasks::get(&conn, &id)?;
    permissions::require_field(&conn, &user, &current.project, "description", None)?;
    let owns_task = current.owner_user_id.as_deref() == Some(user.id.as_str());
    if !owns_task {
        return Err(ApiError::forbidden(
            "该任务不是当前用户的 Agent 创建，不能修改其描述",
        ));
    }
    let task = tasks::patch(
        &mut conn,
        &id,
        &tasks::TaskPatch {
            description: Some(description),
            ..Default::default()
        },
    )?;
    drop(conn);
    core.events.notify();
    Ok(Json(task))
}

/// 项目选项只读（规划 §5.3：Agent 只能消费，不可增删改）
pub async fn list_projects(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    let mut result = projects::list(&conn)?;
    if let Some(visible) = permissions::visible_projects(&conn, &user)? {
        result.retain(|project| visible.contains(&project.name));
    }
    Ok(Json(result))
}

pub async fn help(Extension(_user): Extension<User>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "Agents PM Tool Agent API",
        "authentication": "Authorization: Bearer <PM_AGENT_TOKEN>",
        "commands": [
            {"method":"GET", "path":"/api/agent/tasks", "description":"查看与筛选任务"},
            {"method":"GET", "path":"/api/agent/tasks/{id}", "description":"查看任务详情"},
            {"method":"POST", "path":"/api/agent/tasks", "description":"创建 Agent 任务"},
            {"method":"PATCH", "path":"/api/agent/tasks/{id}/status", "description":"推进状态"},
            {"method":"PATCH", "path":"/api/agent/tasks/{id}/description", "description":"修改 Agent 创建任务的描述"},
            {"method":"GET", "path":"/api/agent/projects", "description":"只读查看项目"},
            {"method":"GET", "path":"/api/agent/skill/download", "description":"下载匹配服务端版本的 pm-cli-skill"}
        ],
        "configuration": {
            "environment": ["PM_SERVER_URL", "PM_AGENT_TOKEN"],
            "example": "PM_SERVER_URL=http://192.168.1.10:17890; PM_AGENT_TOKEN=<token>"
        },
        "skill_download": "/api/agent/skill/download"
    }))
}
