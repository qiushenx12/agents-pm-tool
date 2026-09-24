use axum::{
    body::Body,
    extract::{Extension, Path, RawQuery, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::db::{agent_permissions, attachments, permissions, projects, tasks};
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
    let visible = permissions::visible_projects(&conn, &user)?;
    filter.visible_projects = visible.clone();
    let mut result = tasks::list(&conn, &filter)?;
    tasks::retain_visible_dependencies(&conn, &mut result, visible.as_deref())?;
    Ok(Json(result))
}

pub async fn get_task(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    let mut task = tasks::get(&conn, &id)?;
    permissions::require_project(&conn, &user, &task.project)?;
    let visible = permissions::visible_projects(&conn, &user)?;
    tasks::retain_visible_dependencies(&conn, std::slice::from_mut(&mut task), visible.as_deref())?;
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
#[serde(deny_unknown_fields)]
pub struct AgentCreateBody {
    pub project: Option<String>,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub description: Option<String>,
    pub note: Option<String>,
    pub status: Option<String>,
    /// 可选；缺省为「中」
    pub priority: Option<String>,
    pub predecessor_task_ids: Option<Vec<String>>,
    pub unlock_task_ids: Option<Vec<String>>,
}

/// Agent 创建：项目/类型/描述三必填，submitter 固定 Agent；其它字段逐项授权。
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
    if !domain::is_valid_task_type(task_type.trim()) {
        return Err(ApiError::unprocessable(format!(
            "任务类型不合法：{task_type}"
        )));
    }
    let description = body
        .description
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::unprocessable("Agent 创建任务必须填写描述（--description）"))?;
    let priority = body
        .priority
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());
    if let Some(priority) = &priority {
        if !domain::is_valid_priority(priority) {
            return Err(ApiError::unprocessable(format!(
                "优先级不合法：{priority}，合法取值：{}",
                domain::PRIORITIES.join(" / ")
            )));
        }
    }
    if let Some(status) = body.status.as_deref() {
        if !domain::is_valid_status(status) {
            return Err(ApiError::unprocessable(format!("状态不合法：{status}")));
        }
    }

    let mut conn = core.db.lock().unwrap();
    let agent = agent_permissions::get(&conn, &user.id)?;
    if !agent.task_create {
        return Err(ApiError::forbidden("Agent 无权创建任务"));
    }
    let mut fields = vec![
        ("task_create", None),
        ("type", Some(task_type.trim())),
        ("description", None),
    ];
    if let Some(priority) = priority.as_deref() {
        agent.require_create_field("priority")?;
        fields.push(("priority", Some(priority)));
    }
    if body.note.is_some() {
        agent.require_create_field("note")?;
        fields.push(("note", None));
    }
    if let Some(status) = body.status.as_deref() {
        agent.require_create_field("status")?;
        agent.require_value("status", status)?;
        fields.push(("status", Some(status)));
    }
    if body.predecessor_task_ids.is_some() {
        agent.require_create_field("predecessor_task_ids")?;
        fields.push(("predecessor_task_ids", None));
    }
    if body.unlock_task_ids.is_some() {
        agent.require_create_field("unlock_task_ids")?;
        fields.push(("unlock_task_ids", None));
    }
    permissions::require_fields(&conn, &user, project.trim(), &fields)?;
    super::api_web::require_related_projects(
        &conn,
        &user,
        body.predecessor_task_ids.as_deref().unwrap_or_default(),
    )?;
    super::api_web::require_related_projects(
        &conn,
        &user,
        body.unlock_task_ids.as_deref().unwrap_or_default(),
    )?;
    let (mut task, _) = crate::db::history::record(&mut conn, &user, "agent", "create", |conn| {
        tasks::create_with_status(
            conn,
            &tasks::NewTask {
                project: project.trim(),
                task_type: task_type.trim(),
                description: description.trim(),
                note: body.note.as_deref().unwrap_or(""),
                submitter: "Agent",
                owner_user_id: Some(&user.id),
                priority: priority.as_deref(),
                predecessor_task_ids: body.predecessor_task_ids.as_deref().unwrap_or_default(),
                unlock_task_ids: body.unlock_task_ids.as_deref().unwrap_or_default(),
            },
            body.status.as_deref(),
        )
    })?;
    let visible = permissions::visible_projects(&conn, &user)?;
    tasks::retain_visible_dependencies(&conn, std::slice::from_mut(&mut task), visible.as_deref())?;
    drop(conn);
    let mut before = task.clone();
    before.status = "未开始".into();
    if let Some(notice) = super::finish_notice::notice_on_finish(&before, &task) {
        core.finish_notices.notify(notice);
    }
    core.events.notify();
    Ok((StatusCode::CREATED, Json(task)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPatchBody {
    pub project: Option<String>,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub description: Option<String>,
    pub note: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub predecessor_task_ids: Option<Vec<String>>,
    pub unlock_task_ids: Option<Vec<String>>,
}

async fn apply_patch(
    core: CoreState,
    user: User,
    id: String,
    mut patch: tasks::TaskPatch,
) -> ApiResult<Json<domain::Task>> {
    for (field, value, valid) in [
        (
            "任务类型",
            patch.task_type.as_deref(),
            domain::is_valid_task_type as fn(&str) -> bool,
        ),
        ("状态", patch.status.as_deref(), domain::is_valid_status),
        (
            "优先级",
            patch.priority.as_deref(),
            domain::is_valid_priority,
        ),
    ] {
        if let Some(value) = value {
            if !valid(value) {
                return Err(ApiError::unprocessable(format!("{field}不合法：{value}")));
            }
        }
    }
    if let Some(description) = patch.description.as_mut() {
        if description.trim().is_empty() {
            return Err(ApiError::unprocessable("描述不能为空（--description）"));
        }
        *description = description.trim().to_string();
    }
    let mut conn = core.db.lock().unwrap();
    let current = tasks::get(&conn, &id)?;
    // 负责人锁定：任务被某个 Agent 认领后，其它 Agent 一律不能再修改（含状态/优先级/描述等所有字段）；
    // 只有网页端用户可按自身权限继续修改或改派。负责人账号被删除时外键置空，任务自动重新开放。
    if let Some(assignee) = current.assignee_user_id.as_deref() {
        if assignee != user.id {
            return Err(ApiError::forbidden(format!(
                "该任务的负责人是{}，其它 Agent 不能修改",
                current.assignee_name.as_deref().unwrap_or("其他 Agent")
            )));
        }
    }
    let agent = agent_permissions::get(&conn, &user.id)?;
    let mut fields = Vec::new();
    for (field, present, value) in [
        ("project", patch.project.is_some(), None),
        (
            "type",
            patch.task_type.is_some(),
            patch.task_type.as_deref(),
        ),
        ("description", patch.description.is_some(), None),
        ("note", patch.note.is_some(), None),
        ("status", patch.status.is_some(), patch.status.as_deref()),
        (
            "priority",
            patch.priority.is_some(),
            patch.priority.as_deref(),
        ),
        (
            "predecessor_task_ids",
            patch.predecessor_task_ids.is_some(),
            None,
        ),
        ("unlock_task_ids", patch.unlock_task_ids.is_some(), None),
    ] {
        if present {
            agent.require_edit_field(field)?;
            if let Some(value) = value {
                agent.require_value(field, value)?;
            }
            fields.push((field, value));
        }
    }
    if fields.is_empty() {
        return Err(ApiError::bad_request("请选择要修改的任务字段"));
    }
    permissions::require_fields(&conn, &user, &current.project, &fields)?;
    if patch.description.is_some() && !agent.description_any_task {
        let owns_task = current.submitter == "Agent"
            && current.owner_user_id.as_deref() == Some(user.id.as_str());
        if !owns_task {
            return Err(ApiError::forbidden(
                "该任务不是当前用户的 Agent 创建，不能修改其描述",
            ));
        }
    }
    if let Some(project) = patch.project.as_deref() {
        permissions::require_project(&conn, &user, project)?;
    }
    for ids in [
        patch.predecessor_task_ids.as_deref(),
        patch.unlock_task_ids.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        super::api_web::require_related_projects(&conn, &user, ids)?;
    }
    super::api_web::preserve_hidden_relationships(
        &conn,
        &user,
        &current.predecessor_task_ids,
        &mut patch.predecessor_task_ids,
    )?;
    super::api_web::preserve_hidden_relationships(
        &conn,
        &user,
        &current.unlock_task_ids,
        &mut patch.unlock_task_ids,
    )?;
    // 认领规则：Agent 把任务从「未开始」推进到其它任意状态时，自动成为该任务负责人。
    // 已有负责人时上面已通过锁定校验（只可能是自己），此处不重复写入。
    let agent_claims = current.status == "未开始"
        && current.assignee_user_id.is_none()
        && patch
            .status
            .as_deref()
            .is_some_and(|status| status != "未开始");
    if agent_claims {
        patch.assignee_user_id = Some(Some(user.id.clone()));
    }
    let (mut task, _) = crate::db::history::record(&mut conn, &user, "agent", "update", |conn| {
        tasks::patch(conn, &id, &patch)
    })?;
    let visible = permissions::visible_projects(&conn, &user)?;
    tasks::retain_visible_dependencies(&conn, std::slice::from_mut(&mut task), visible.as_deref())?;
    drop(conn);
    if let Some(notice) = super::finish_notice::notice_on_finish(&current, &task) {
        core.finish_notices.notify(notice);
    }
    core.events.notify();
    Ok(Json(task))
}

pub async fn patch_task(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<AgentPatchBody>,
) -> ApiResult<impl IntoResponse> {
    apply_patch(core, user, id, tasks::TaskPatch {
        project: body.project,
        task_type: body.task_type,
        description: body.description,
        note: body.note,
        status: body.status,
        priority: body.priority,
        // 负责人不开放给 Agent 直接修改：只能由认领规则或网页端写入。
        assignee_user_id: None,
        predecessor_task_ids: body.predecessor_task_ids,
        unlock_task_ids: body.unlock_task_ids,
    }).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentStatusBody {
    pub status: Option<String>,
}

/// 兼容旧 CLI；实际权限与通用 PATCH 完全一致。
pub async fn patch_status(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<AgentStatusBody>,
) -> ApiResult<impl IntoResponse> {
    let status = body.status.filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::unprocessable("缺少 status"))?;
    apply_patch(core, user, id, tasks::TaskPatch {
        status: Some(status), ..Default::default()
    }).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDescriptionBody {
    pub description: Option<String>,
}

/// 兼容旧 CLI；描述范围可由管理员配置。
pub async fn patch_description(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<AgentDescriptionBody>,
) -> ApiResult<impl IntoResponse> {
    let description = body.description
        .ok_or_else(|| ApiError::unprocessable("描述不能为空（--description）"))?;
    apply_patch(core, user, id, tasks::TaskPatch {
        description: Some(description), ..Default::default()
    }).await
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

/// 当前 token 的有效能力：Agent 配置与网页项目/字段授权取交集，不泄露不可见项目。
pub async fn get_permissions(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    let agent = agent_permissions::get(&conn, &user.id)?;
    let visible = permissions::visible_projects(&conn, &user)?;
    let mut available = projects::list(&conn)?;
    if let Some(visible) = visible {
        available.retain(|project| visible.contains(&project.name));
    }
    let mut result = Vec::new();
    for project in available {
        let name = project.name;
        let allowed = |field: &str, value: Option<&str>| {
            permissions::require_field(&conn, &user, &name, field, value).is_ok()
        };
        let type_values = domain::TASK_TYPES.iter().filter(|value| allowed("type", Some(value)))
            .copied().collect::<Vec<_>>();
        let priority_values = domain::PRIORITIES.iter().filter(|value| allowed("priority", Some(value)))
            .copied().collect::<Vec<_>>();
        let status_values = agent.status_values.iter()
            .filter(|value| allowed("status", Some(value)))
            .cloned().collect::<Vec<_>>();
        let can_create = agent.task_create && allowed("task_create", None)
            && !type_values.is_empty() && allowed("description", None);
        let mut create_fields = Vec::new();
        if can_create {
            create_fields.extend(["project", "type", "description"]);
            for field in agent_permissions::CREATE_FIELDS {
                let available = match field {
                    "status" => !status_values.is_empty(),
                    "priority" => !priority_values.is_empty(),
                    _ => allowed(field, None),
                };
                if agent.allows_create_field(field) && available {
                    create_fields.push(field);
                }
            }
        }
        let edit_fields = agent_permissions::EDIT_FIELDS.iter()
            .filter(|field| {
                agent.allows_edit_field(field) && match **field {
                    "status" => !status_values.is_empty(),
                    "type" => !type_values.is_empty(),
                    "priority" => !priority_values.is_empty(),
                    _ => allowed(field, None),
                }
            })
            .copied().collect::<Vec<_>>();
        result.push(serde_json::json!({
            "project": name,
            "can_create": can_create,
            "create_fields": create_fields,
            "edit_fields": edit_fields,
            "allowed_values": {
                "type": type_values,
                "status": status_values,
                "priority": priority_values,
            },
            "description_scope": if agent.description_any_task { "any" } else { "own_agent" },
        }));
    }
    Ok(Json(serde_json::json!({
        "projects": result,
        "immutable_fields": ["id", "seq", "submitter", "submitter_name", "created_at", "finished_at", "updated_at", "position", "owner_user_id", "assignee_user_id", "assignee_name", "attachment_count"],
        "read_access": "已授权项目中的任务、项目与附件可读；附件上传/删除和任务删除仍仅限网页端",
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPriorityBody {
    pub priority: Option<String>,
}

/// 兼容旧 CLI；实际权限与通用 PATCH 完全一致。
pub async fn patch_priority(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<AgentPriorityBody>,
) -> ApiResult<impl IntoResponse> {
    let priority = body.priority.filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::unprocessable("缺少 priority"))?;
    apply_patch(core, user, id, tasks::TaskPatch {
        priority: Some(priority), ..Default::default()
    }).await
}

/// 接口自述。**无需 token**：没有 pm-cli、没有 skill 的 Agent 也能访问它，
/// 从中得知「要让用户做什么」。真正读写任务仍需 token。
pub async fn help(State(core): State<CoreState>, headers: HeaderMap) -> Json<serde_json::Value> {
    // 用调用方这次实际走通的路由回地址：局域网来的给局域网地址，tailnet 来的给 Tailscale 地址。
    let server_url = super::api_agent_access::suggested_server_url(&core, &headers);
    Json(serde_json::json!({
        "name": "Agents PM Tool Agent API",
        "introduction": "Agents PM Tool 是本地任务管理工具，Agent 通过 pm-cli 或本接口（HTTP + Bearer token）读取任务，并在当前权限内创建或修改任务。pm-cli 是 pm-cli-skill 里的一个 Node 脚本（需要 Node.js 18 或更高版本），装到本机 Agent 前端的 skills 目录后即可使用；完整用法见 skill 目录里的 SKILL.md。",
        "server_url": server_url,
        "authentication": "Authorization: Bearer <PM_AGENT_TOKEN>",
        "requires_token": true,
        "runtime_requirement": "pm-cli 需要 Node.js 18 或更高版本；没有 Node.js 时可直接调用本接口里的 HTTP 端点。",
        "bootstrap": {
            "summary": "本接口无需 token 即可访问；但读取或推进任务必须带 token。token 由用户在网页端签发，Agent 无法自行获取 —— 手上没有 token 时，请把下面这几步转告用户，拿到之后再继续。",
            "ask_the_user": [
                "打开 Agents PM Tool 网页端（你会看到一个任务工作台）。",
                "点左侧的「我的 Agent 访问」，在弹出的「我的账号与 Agent 访问」里找到「Agent 连接凭据」卡片。",
                "卡片里有一行「Agent token」：如果显示「尚未签发或已吊销」，先点下面的「生成 token」；然后点这一行右侧的复制按钮。",
                "把复制到的 token（以及卡片里的「服务地址」）发给我，我就用它们连接。也可以你自己在本机终端执行下面 configuration 里的三条命令。"
            ],
            "configure": [
                "pm-cli config set server-url <服务地址>",
                "pm-cli config set token <token>",
                "pm-cli doctor"
            ],
            "if_user_prefers_one_command": [
                "在 Agent 所在电脑上执行一条命令即可：装好 pm-cli skill（检测到的前端全部安装）并写好上面的连接配置。",
                "Windows PowerShell：$OutputEncoding=[Text.UTF8Encoding]::new($false); irm <服务地址>/api/agent/skill/install.mjs | node --input-type=module - --all --server-url <服务地址> --token <token>",
                "macOS / Linux：curl -fsSL <服务地址>/api/agent/skill/install.mjs | node --input-type=module - --all --server-url <服务地址> --token <token>",
                "需要 Node.js 18 或更高版本；命令里的 `-` 是「程序从标准输入读」，不能省。",
                "Windows 上那串 $OutputEncoding 是给 PowerShell 5.1 补的：它往管道写非 ASCII 默认用 ASCII 编码，会把脚本里的中文变成问号；这一段必须是不带 BOM 的 UTF-8，否则 node 会因开头的 BOM 报语法错误。",
                "服务地址若是 ngrok 这类隧道域名，PowerShell 自带的浏览器 UA 会被隧道挡成一张 HTML 提示页（node 报 `Unexpected identifier 'are'`），给 irm 补 -Headers @{\"ngrok-skip-browser-warning\"=\"1\"} 即可跳过；局域网与 Tailscale 地址不需要。网页端「我的 Agent 访问」里复制的命令已自动带上。"
            ],
            "if_pm_cli_missing": [
                "先确认本机有 Node.js 18 或更高版本。",
                "按上面的 if_user_prefers_one_command 执行一条命令即可装好 skill 与连接；不带 --token 时只会装 skill，连接需要另配。",
                "要先看看会装到哪里：把该命令末尾的 --all 换成 --list 只做检测，换成 --dir <目录> 可指定目录。",
                "或直接用 HTTP：curl -H \"Authorization: Bearer <token>\" <服务地址>/api/agent/tasks"
            ]
        },
        "commands": [
            {"method":"GET", "path":"/api/agent/tasks", "description":"查看与筛选任务"},
            {"method":"GET", "path":"/api/agent/tasks/{id}", "description":"查看任务详情"},
            {"method":"GET", "path":"/api/agent/tasks/{id}/attachments", "description":"列出任务附件（只读）"},
            {"method":"GET", "path":"/api/agent/attachments/{id}", "description":"下载附件（只读）"},
            {"method":"POST", "path":"/api/agent/tasks", "description":"创建 Agent 任务"},
            {"method":"PATCH", "path":"/api/agent/tasks/{id}", "description":"按授权修改可编辑字段"},
            {"method":"PATCH", "path":"/api/agent/tasks/{id}/status", "description":"兼容旧客户端的状态修改入口"},
            {"method":"PATCH", "path":"/api/agent/tasks/{id}/priority", "description":"兼容旧客户端的优先级修改入口"},
            {"method":"PATCH", "path":"/api/agent/tasks/{id}/description", "description":"兼容旧客户端的描述修改入口"},
            {"method":"GET", "path":"/api/agent/projects", "description":"只读查看项目"},
            {"method":"GET", "path":"/api/agent/permissions", "description":"查看当前 token 的有效项目、创建和字段修改权限"}
        ],
        "configuration": {
            "environment": ["PM_SERVER_URL", "PM_AGENT_TOKEN"],
            "example": "PM_SERVER_URL=http://192.168.1.10:17890; PM_AGENT_TOKEN=<token>",
            "note": "本机装有 Agents PM Tool 时不需要配置：应用会把端口与 token 写到你电脑上固定的一处位置，pm-cli 自动读取。"
        },
        "skill": {
            "payload": "/api/agent/skill/payload",
            "installer": "/api/agent/skill/install.mjs",
            "installer_example": format!("curl -fsSL {server_url}/api/agent/skill/install.mjs | node --input-type=module - --all")
        },
        "unauthenticated_access": [
            "/api/agent/help",
            "/api/agent/skill/payload",
            "/api/agent/skill/install.mjs"
        ],
        "permissions": [
            "可以查看/筛选已授权项目中的任务和项目，并查看或下载其附件。",
            "创建与修改能力由管理员配置的 Agent 权限和账号项目/字段授权共同决定；GET /api/agent/permissions 返回当前 token 的有效权限。",
            "默认权限与旧版一致：可创建任务、改状态为进行中/待验证/已完成、改优先级、修改自己 Agent 创建任务的非空描述。管理员可以额外授予其它可编辑字段或状态值。",
            "任务关系：predecessor_task_ids=本任务的子任务；unlock_task_ids=本任务的父级任务。从未开始或取消启动为进行中时，子任务须全部已完成或验收通过；进入待验证、已完成或验收通过时，子任务须全部待验证、已完成或验收通过。子任务回退或新增时，处于待验证、已完成、验收通过的各级父任务会回到进行中。",
            "负责人：Agent 把任务从「未开始」推进到其它任意状态时自动成为该任务的负责人；已有负责人的任务只能被该负责人（同一账号的 Agent）修改，其它 Agent 的修改会被拒绝（HTTP 403），网页端用户不受此限制。",
            "ID、提交人、时间戳、负责人等不可变字段不能由客户端设置；删除任务、上传或删除附件、管理项目、直接读写 SQLite 始终不开放给 Agent。"
        ]
    }))
}
