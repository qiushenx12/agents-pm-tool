use axum::{
    body::Body,
    extract::{Extension, Path, RawQuery, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::db::{attachments, permissions, projects, tasks};
use crate::domain::attachment::Attachment;
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

#[derive(Serialize)]
pub struct AgentAttachment {
    pub id: String,
    pub task_id: String,
    pub filename: String,
    pub mime: Option<String>,
    pub size: i64,
    pub created_at: String,
}

impl From<Attachment> for AgentAttachment {
    fn from(value: Attachment) -> Self {
        Self {
            id: value.id,
            task_id: value.task_id,
            filename: value.filename,
            mime: value.mime,
            size: value.size,
            created_at: value.created_at,
        }
    }
}

/// Agent 只读列出可见任务的附件，不暴露服务端 stored_path。
pub async fn list_attachments(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(task_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    let task = tasks::get(&conn, &task_id)?;
    permissions::require_project(&conn, &user, &task.project)?;
    let result = attachments::list(&conn, &task_id)?
        .into_iter()
        .map(AgentAttachment::from)
        .collect::<Vec<_>>();
    Ok(Json(result))
}

/// Agent 只读下载可见任务的附件；上传和删除仍不开放路由。
pub async fn download_attachment(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let attachment = {
        let conn = core.db.lock().unwrap();
        let attachment = attachments::get(&conn, &id)?;
        let task = tasks::get(&conn, &attachment.task_id)?;
        permissions::require_project(&conn, &user, &task.project)?;
        attachment
    };
    let bytes = std::fs::read(core.data_dir.join(&attachment.stored_path))
        .map_err(|_| ApiError::not_found("附件文件已丢失"))?;
    let mime = attachment
        .mime
        .as_deref()
        .unwrap_or("application/octet-stream");
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "private, no-store")
        .header(
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename*=UTF-8''{}",
                url::form_urlencoded::byte_serialize(attachment.filename.as_bytes())
                    .collect::<String>()
            ),
        )
        .body(Body::from(bytes))
        .map_err(ApiError::internal)
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
    let owns_task =
        current.submitter == "Agent" && current.owner_user_id.as_deref() == Some(user.id.as_str());
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

/// 接口自述。**无需 token**：没有 pm-cli、没有 skill 的 Agent 也能访问它，
/// 从中得知「要让用户做什么」。真正读写任务仍需 token。
pub async fn help(State(core): State<CoreState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "Agents PM Tool Agent API",
        "introduction": "Agents PM Tool 是本地任务管理工具，Agent 通过受限客户端 pm-cli 或本接口（HTTP + Bearer token）读取和推进任务。pm-cli 由 pm-cli-skill 提供：可在网页端「我的 Agent 访问」下载 ZIP，解压到 Codex、Claude Code 或 WorkBuddy 的 skills 目录后即可使用，完整用法见同目录的 SKILL.md。",
        "server_url": super::api_agent_access::reachable_server_url(&core),
        "authentication": "Authorization: Bearer <PM_AGENT_TOKEN>",
        "requires_token": true,
        "bootstrap": {
            "summary": "本接口无需 token 即可访问；但读取或推进任务必须带 token。token 由已登录用户在网页端签发，Agent 无法自行获取，请把下列步骤转告用户。",
            "ask_the_user": [
                "打开 Agents PM Tool 网页端，进入「我的 Agent 访问」。",
                "复制面板上的「服务地址」，并生成（或复制）Agent token。",
                "把服务地址与 token 提供给 Agent；或由用户在本机执行下面的配置命令。"
            ],
            "configure": [
                "pm-cli config set server-url <服务地址>",
                "pm-cli config set token <token>",
                "pm-cli doctor"
            ],
            "if_pm_cli_missing": [
                "由用户在网页端「我的 Agent 访问」下载 pm-cli-skill，解压到 Codex、Claude Code 或 WorkBuddy 的 skills 目录。",
                "或直接用 curl：<curl> -H \"Authorization: Bearer <token>\" <服务地址>/api/agent/tasks"
            ]
        },
        "commands": [
            {"method":"GET", "path":"/api/agent/tasks", "description":"查看与筛选任务"},
            {"method":"GET", "path":"/api/agent/tasks/{id}", "description":"查看任务详情"},
            {"method":"GET", "path":"/api/agent/tasks/{id}/attachments", "description":"列出任务附件（只读）"},
            {"method":"GET", "path":"/api/agent/attachments/{id}", "description":"下载附件（只读）"},
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
        "unauthenticated_access": ["/api/agent/help"],
        "skill_download": "/api/agent/skill/download",
        "permissions": [
            "可以查看/筛选任务、只读查看项目、创建任务，并查看和下载已授权项目中的任务附件。",
            "只能把任务状态改为进行中、待验证或已完成。",
            "只能修改由 Agent 创建的任务描述，且描述不能为空。",
            "不能设置验收状态，不能修改项目、类型或用户创建的任务描述，也不能删除任务、上传或删除附件、直接读写 SQLite。"
        ]
    }))
}
