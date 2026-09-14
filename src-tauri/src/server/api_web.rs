use axum::{
    body::Body,
    extract::{Extension, Multipart, Path, RawQuery, State},
    http::{header, StatusCode},
    response::{sse::KeepAlive, IntoResponse, Response, Sse},
    Json,
};
use serde::Deserialize;

pub use super::api_batch::batch_tasks;
use crate::db::{attachments, permissions, projects, tasks, users};
use crate::domain::attachment as attach;
use crate::domain::task::now_str;
use crate::domain::user::User;
use crate::error::{ApiError, ApiResult};
use crate::server::{parse_task_filter, CoreState};

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

pub async fn page_tasks(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    RawQuery(query): RawQuery,
) -> ApiResult<impl IntoResponse> {
    let mut filter = parse_task_filter(query.as_deref())?;
    let options = crate::db::task_page::PageOptions::parse(query.as_deref())?;
    let conn = core.db.lock().unwrap();
    filter.visible_projects = permissions::visible_projects(&conn, &user)?;
    Ok(Json(crate::db::task_page::list_page(
        &conn, &filter, &options,
    )?))
}

// ── 任务 ─────────────────────────────────────────────────

/// 提交人筛选的用户名候选：未停用、且在可见项目内出现过任务归属的用户。
/// 不做管理员收窄——普通用户也只能看到「可见项目里出现过任务」的名字。
pub async fn submitter_directory(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    let visible = permissions::visible_projects(&conn, &user)?;
    Ok(Json(users::list_submitter_names(
        &conn,
        visible.as_deref(),
    )?))
}

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

#[derive(Deserialize)]
pub struct CreateTaskBody {
    pub project: Option<String>,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub description: Option<String>,
    pub note: Option<String>,
    pub priority: Option<String>,
}

pub async fn create_task(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Json(body): Json<CreateTaskBody>,
) -> ApiResult<impl IntoResponse> {
    let project = body
        .project
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::unprocessable("项目为必填项"))?;
    let task_type = body
        .task_type
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::unprocessable("任务类型为必填项"))?;

    let mut conn = core.db.lock().unwrap();
    let mut fields = vec![("task_create", None), ("type", Some(task_type.trim()))];
    if body.description.is_some() {
        fields.push(("description", None));
    }
    if body.note.is_some() {
        fields.push(("note", None));
    }
    if let Some(value) = body.priority.as_deref() {
        fields.push(("priority", Some(value)));
    }
    permissions::require_fields(&conn, &user, project.trim(), &fields)?;
    let task = tasks::create(
        &mut conn,
        &tasks::NewTask {
            project: project.trim(),
            task_type: task_type.trim(),
            description: body.description.as_deref().unwrap_or(""),
            note: body.note.as_deref().unwrap_or(""),
            submitter: "用户", // 网页端固定（规划 §4.3）
            owner_user_id: Some(&user.id),
            priority: body.priority.as_deref(),
        },
    )?;
    drop(conn);
    core.events.notify();
    Ok((StatusCode::CREATED, Json(task)))
}

#[derive(Deserialize)]
pub struct PatchTaskBody {
    pub project: Option<String>,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub description: Option<String>,
    pub note: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
}

pub async fn patch_task(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<PatchTaskBody>,
) -> ApiResult<impl IntoResponse> {
    let mut conn = core.db.lock().unwrap();
    let current = tasks::get(&conn, &id)?;
    let mut fields = Vec::new();
    if body.project.is_some() {
        fields.push(("project", None));
    }
    if let Some(value) = body.task_type.as_deref() {
        fields.push(("type", Some(value)));
    }
    if body.description.is_some() {
        fields.push(("description", None));
    }
    if body.note.is_some() {
        fields.push(("note", None));
    }
    if let Some(value) = body.status.as_deref() {
        fields.push(("status", Some(value)));
    }
    if let Some(value) = body.priority.as_deref() {
        fields.push(("priority", Some(value)));
    }
    permissions::require_fields(&conn, &user, &current.project, &fields)?;
    if let Some(project) = body.project.as_deref() {
        permissions::require_project(&conn, &user, project)?;
    }
    let task = tasks::patch(
        &mut conn,
        &id,
        &tasks::TaskPatch {
            project: body.project,
            task_type: body.task_type,
            description: body.description,
            note: body.note,
            status: body.status,
            priority: body.priority,
        },
    )?;
    drop(conn);
    if let Some(notice) = super::finish_notice::notice_on_finish(&current, &task) {
        core.finish_notices.notify(notice);
    }
    core.events.notify();
    Ok(Json(task))
}

pub async fn delete_task(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    let task = tasks::get(&conn, &id)?;
    permissions::require_field(&conn, &user, &task.project, "task_delete", None)?;
    let attach_paths = tasks::remove(&conn, &id)?;
    drop(conn);
    // 级联删除的附件行已清除，这里清理磁盘文件；失败仅告警不回滚（规划：删除任务仅网页端）
    for rel in &attach_paths {
        if let Err(e) = std::fs::remove_file(core.data_dir.join(rel)) {
            eprintln!("清理附件文件失败 {rel}：{e}");
        }
    }
    core.events.notify();
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ReorderTaskBody {
    pub prev_id: Option<String>,
    pub next_id: Option<String>,
}

/// 手动排序：把任务移到 prev_id/next_id 之间（sort_by=manual 时前端拖拽调用）
pub async fn reorder_task(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<ReorderTaskBody>,
) -> ApiResult<impl IntoResponse> {
    let mut conn = core.db.lock().unwrap();
    let task = tasks::get(&conn, &id)?;
    permissions::require_field(&conn, &user, &task.project, "reorder", None)?;
    for neighbor in [body.prev_id.as_deref(), body.next_id.as_deref()]
        .into_iter()
        .flatten()
    {
        let neighbor = tasks::get(&conn, neighbor)?;
        permissions::require_project(&conn, &user, &neighbor.project)?;
    }
    let task = tasks::reorder(
        &mut conn,
        &id,
        body.prev_id.as_deref(),
        body.next_id.as_deref(),
    )?;
    drop(conn);
    core.events.notify();
    Ok(Json(task))
}

#[derive(Deserialize)]
pub struct RebaseOrderBody {
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

/// 以指定排序重铺手动位置：切入手动排序时以当前视图为基线（视觉顺序不变）
pub async fn rebase_order(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Json(body): Json<RebaseOrderBody>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    match permissions::visible_projects(&conn, &user)? {
        None => {
            tasks::rebase_positions(&conn, body.sort_by.as_deref(), body.sort_order.as_deref())?
        }
        Some(projects) => {
            for project in &projects {
                permissions::require_field(&conn, &user, project, "reorder", None)?;
            }
            tasks::rebase_positions_for_projects(
                &conn,
                body.sort_by.as_deref(),
                body.sort_order.as_deref(),
                &projects,
            )?;
        }
    }
    drop(conn);
    core.events.notify();
    Ok(StatusCode::NO_CONTENT)
}

// ── 项目选项 ─────────────────────────────────────────────

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

#[derive(Deserialize)]
pub struct CreateProjectBody {
    pub name: String,
    pub color: Option<String>,
    pub local_path: Option<String>,
    pub git_url: Option<String>,
}

pub async fn create_project(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Json(body): Json<CreateProjectBody>,
) -> ApiResult<impl IntoResponse> {
    if !user.is_admin() {
        return Err(ApiError::forbidden("仅管理员可以创建项目"));
    }
    let conn = core.db.lock().unwrap();
    let p = projects::create(
        &conn,
        &body.name,
        body.color.as_deref(),
        body.local_path.as_deref(),
        body.git_url.as_deref(),
    )?;
    drop(conn);
    core.events.notify();
    Ok((StatusCode::CREATED, Json(p)))
}

#[derive(Deserialize)]
pub struct PatchProjectBody {
    pub new_name: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i64>,
    pub local_path: Option<String>,
    pub git_url: Option<String>,
}

pub async fn patch_project(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(name): Path<String>,
    Json(body): Json<PatchProjectBody>,
) -> ApiResult<impl IntoResponse> {
    if !user.is_admin() {
        return Err(ApiError::forbidden("仅管理员可以修改项目"));
    }
    let mut conn = core.db.lock().unwrap();
    let p = projects::patch(
        &mut conn,
        &name,
        &projects::ProjectPatch {
            new_name: body.new_name,
            color: body.color,
            sort_order: body.sort_order,
            local_path: body.local_path,
            git_url: body.git_url,
        },
    )?;
    drop(conn);
    core.events.notify();
    Ok(Json(p))
}

/// 弹系统文件夹选择框（对话框开在本机服务端），返回所选路径；用户取消 → 204
pub async fn pick_folder(Extension(user): Extension<User>) -> ApiResult<Response> {
    if !user.is_admin() {
        return Err(ApiError::forbidden("仅管理员可以选择主机上的项目目录"));
    }
    let folder = rfd::AsyncFileDialog::new()
        .set_title("选择项目本地路径")
        .pick_folder()
        .await;
    match folder {
        Some(f) => Ok(Json(serde_json::json!({
            "path": f.path().display().to_string()
        }))
        .into_response()),
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

pub async fn delete_project(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(name): Path<String>,
) -> ApiResult<impl IntoResponse> {
    if !user.is_admin() {
        return Err(ApiError::forbidden("仅管理员可以删除项目"));
    }
    let conn = core.db.lock().unwrap();
    projects::remove(&conn, &name)?;
    drop(conn);
    core.events.notify();
    Ok(StatusCode::NO_CONTENT)
}

// ── 附件（网页端可管理；Agent 仅开放只读接口） ──────────────

pub async fn list_attachments(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(task_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    let task = tasks::get(&conn, &task_id)?; // 任务不存在 → 404
    permissions::require_project(&conn, &user, &task.project)?;
    Ok(Json(attachments::list(&conn, &task_id)?))
}

pub async fn upload_attachment(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(task_id): Path<String>,
    mut multipart: Multipart,
) -> ApiResult<impl IntoResponse> {
    {
        let conn = core.db.lock().unwrap();
        let task = tasks::get(&conn, &task_id)?;
        permissions::require_field(&conn, &user, &task.project, "attachment_upload", None)?;
    }
    let mut filename: Option<String> = None;
    let mut data: Vec<u8> = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            filename = field.file_name().map(|s| s.to_string());
            data = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad_request(format!("读取上传内容失败：{e}")))?
                .to_vec();
        }
    }

    let filename = filename.ok_or_else(|| ApiError::bad_request("缺少上传文件（字段名 file）"))?;
    if data.is_empty() {
        return Err(ApiError::bad_request("上传内容为空"));
    }
    if data.len() as u64 > attach::MAX_SIZE {
        return Err(ApiError::unprocessable("单文件大小不能超过 200MB"));
    }
    if !attach::is_allowed(&filename) {
        return Err(ApiError::unprocessable(format!(
            "不支持的文件类型，允许：{}",
            attach::ALLOWED_EXTENSIONS.join("/")
        )));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let stored_rel = format!("attachments/{id}.{ext}");
    let stored_abs = core.data_dir.join(&stored_rel);

    std::fs::create_dir_all(stored_abs.parent().unwrap())?;
    std::fs::write(&stored_abs, &data)?;

    let mime = mime_guess::from_path(&filename)
        .first()
        .map(|m| m.to_string());
    let conn = core.db.lock().unwrap();
    let result =
        (|| -> ApiResult<attach::Attachment> {
            tasks::get(&conn, &task_id)?;
            let now = now_str();
            conn.execute(
            "INSERT INTO attachments (id, task_id, filename, stored_path, mime, size, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, task_id, filename, stored_rel, mime, data.len() as i64, now],
        )?;
            Ok(attach::Attachment {
                id,
                task_id,
                filename,
                stored_path: stored_rel,
                mime,
                size: data.len() as i64,
                created_at: now,
            })
        })();
    drop(conn);

    match result {
        Ok(a) => {
            core.events.notify();
            Ok((StatusCode::CREATED, Json(a)))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&stored_abs);
            Err(e)
        }
    }
}

pub async fn download_attachment(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let a = {
        let conn = core.db.lock().unwrap();
        attachments::get(&conn, &id)?
    };
    {
        let conn = core.db.lock().unwrap();
        let task = tasks::get(&conn, &a.task_id)?;
        permissions::require_project(&conn, &user, &task.project)?;
    }
    let bytes = std::fs::read(core.data_dir.join(&a.stored_path))
        .map_err(|_| ApiError::not_found("附件文件已丢失"))?;
    let mime = a
        .mime
        .clone()
        .unwrap_or_else(|| "application/octet-stream".into());
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(
            header::CONTENT_DISPOSITION,
            format!(
                "inline; filename*=UTF-8''{}",
                url::form_urlencoded::byte_serialize(a.filename.as_bytes()).collect::<String>()
            ),
        )
        .body(Body::from(bytes))
        .unwrap())
}

pub async fn delete_attachment(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let a = {
        let conn = core.db.lock().unwrap();
        attachments::get(&conn, &id)?
    };
    let conn = core.db.lock().unwrap();
    let task = tasks::get(&conn, &a.task_id)?;
    permissions::require_field(&conn, &user, &task.project, "attachment_delete", None)?;
    conn.execute("DELETE FROM attachments WHERE id = ?1", [id])?;
    drop(conn);
    let _ = std::fs::remove_file(core.data_dir.join(&a.stored_path));
    core.events.notify();
    Ok(StatusCode::NO_CONTENT)
}

// ── 视图设置（分组/排序/筛选，按登录账号在服务端保存） ──────

/// 视图设置体积上限：只是几个筛选值，超过说明客户端在塞别的东西
const VIEW_STATE_MAX: usize = 4096;

pub async fn get_view_state(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    let saved = crate::db::view_state::get(&conn, &user.id)?;
    // 存的是前端结构，服务端不解释语义；坏了就按「没保存过」处理，别让客户端起不来。
    let filters = saved
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .filter(|value| value.is_object());
    Ok(Json(serde_json::json!({ "filters": filters })))
}

#[derive(Deserialize)]
pub struct ViewStateBody {
    pub filters: serde_json::Value,
}

pub async fn put_view_state(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Json(body): Json<ViewStateBody>,
) -> ApiResult<impl IntoResponse> {
    if !body.filters.is_object() {
        return Err(ApiError::unprocessable("视图设置格式不正确"));
    }
    let raw = serde_json::to_string(&body.filters).map_err(ApiError::internal)?;
    if raw.len() > VIEW_STATE_MAX {
        return Err(ApiError::unprocessable("视图设置过大"));
    }
    let conn = core.db.lock().unwrap();
    crate::db::view_state::put(&conn, &user.id, &raw)?;
    Ok(StatusCode::NO_CONTENT)
}

// ── SSE ──────────────────────────────────────────────────

pub async fn events(
    State(core): State<CoreState>,
    Extension(_user): Extension<User>,
) -> impl IntoResponse {
    let mut rx = core.events.subscribe();
    let mut theme_rx = core.theme_events.subscribe();
    let stream = async_stream::stream! {
        loop {
            // 任务与主题共用一条 SSE 连接：任务变化仍是空信号（不夹内容），
            // 主题变化带值下发，前端免二次 GET。
            tokio::select! {
                r = rx.recv() => match r {
                    Ok(()) => yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().event("tasks_changed").data("{}")),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                r = theme_rx.changed() => match r {
                    Ok(()) => {
                        let theme = theme_rx.borrow_and_update().clone();
                        let payload = serde_json::json!({ "theme": theme });
                        yield Ok(axum::response::sse::Event::default().event("theme_changed").data(payload.to_string()));
                    }
                    Err(_) => break,
                },
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ── 全局主题（设置窗口与网页界面共用） ────────────────────

/// 公开端点（登录页也要着色）：只回一个主题值，不含账号信息
pub async fn get_appearance(State(core): State<CoreState>) -> ApiResult<impl IntoResponse> {
    let theme = core.settings.read().unwrap().theme.clone();
    Ok(Json(serde_json::json!({ "theme": theme })))
}

#[derive(Deserialize)]
pub struct AppearanceBody {
    pub theme: String,
}

pub async fn put_appearance(
    State(core): State<CoreState>,
    Json(body): Json<AppearanceBody>,
) -> ApiResult<impl IntoResponse> {
    super::set_theme(&core, &body.theme)?;
    Ok(StatusCode::NO_CONTENT)
}
