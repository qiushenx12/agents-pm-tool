use axum::{
    extract::{Path, RawQuery, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::db::{projects, tasks};
use crate::domain::task as domain;
use crate::error::{ApiError, ApiResult};
use crate::server::{parse_task_filter, CoreState};

/// /api/agent/*：权限收窄全部在服务端强制（规划 §5.4），CLI 只是 HTTP 薄客户端。

pub async fn list_tasks(
    State(core): State<CoreState>,
    RawQuery(q): RawQuery,
) -> ApiResult<impl IntoResponse> {
    let filter = parse_task_filter(q.as_deref())?;
    let conn = core.db.lock().unwrap();
    Ok(Json(tasks::list(&conn, &filter)?))
}

pub async fn get_task(
    State(core): State<CoreState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    Ok(Json(tasks::get(&conn, &id)?))
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
    let task = tasks::create(
        &mut conn,
        &tasks::NewTask {
            project: project.trim(),
            task_type: task_type.trim(),
            description: description.trim(),
            submitter: "Agent",
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
    Path(id): Path<String>,
    Json(body): Json<AgentDescriptionBody>,
) -> ApiResult<impl IntoResponse> {
    let description = body.description.unwrap_or_default();

    let mut conn = core.db.lock().unwrap();
    let current = tasks::get(&conn, &id)?;
    if current.submitter != "Agent" {
        return Err(ApiError::forbidden(
            "该任务由用户创建，Agent 无权修改其描述",
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
pub async fn list_projects(State(core): State<CoreState>) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    Ok(Json(projects::list(&conn)?))
}
