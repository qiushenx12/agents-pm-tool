use axum::{
    body::Body,
    extract::{Multipart, Path, RawQuery, State},
    http::{header, StatusCode},
    response::{sse::KeepAlive, IntoResponse, Response, Sse},
    Json,
};
use serde::Deserialize;

use crate::db::{projects, tasks};
use crate::domain::attachment as attach;
use crate::domain::task::now_str;
use crate::error::{ApiError, ApiResult};
use crate::server::{parse_task_filter, CoreState};

// ── 任务 ─────────────────────────────────────────────────

pub async fn list_tasks(
    State(core): State<CoreState>,
    RawQuery(q): RawQuery,
) -> ApiResult<impl IntoResponse> {
    let filter = parse_task_filter(q.as_deref())?;
    let conn = core.db.lock().unwrap();
    Ok(Json(tasks::list(&conn, &filter)?))
}

#[derive(Deserialize)]
pub struct CreateTaskBody {
    pub project: Option<String>,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub description: Option<String>,
}

pub async fn create_task(
    State(core): State<CoreState>,
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
    let task = tasks::create(
        &mut conn,
        &tasks::NewTask {
            project: project.trim(),
            task_type: task_type.trim(),
            description: body.description.as_deref().unwrap_or(""),
            submitter: "用户", // 网页端固定（规划 §4.3）
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
    pub status: Option<String>,
}

pub async fn patch_task(
    State(core): State<CoreState>,
    Path(id): Path<String>,
    Json(body): Json<PatchTaskBody>,
) -> ApiResult<impl IntoResponse> {
    let mut conn = core.db.lock().unwrap();
    let task = tasks::patch(
        &mut conn,
        &id,
        &tasks::TaskPatch {
            project: body.project,
            task_type: body.task_type,
            description: body.description,
            status: body.status,
        },
    )?;
    drop(conn);
    core.events.notify();
    Ok(Json(task))
}

pub async fn delete_task(
    State(core): State<CoreState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
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

// ── 项目选项 ─────────────────────────────────────────────

pub async fn list_projects(State(core): State<CoreState>) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    Ok(Json(projects::list(&conn)?))
}

#[derive(Deserialize)]
pub struct CreateProjectBody {
    pub name: String,
    pub color: Option<String>,
}

pub async fn create_project(
    State(core): State<CoreState>,
    Json(body): Json<CreateProjectBody>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    let p = projects::create(&conn, &body.name, body.color.as_deref())?;
    drop(conn);
    core.events.notify();
    Ok((StatusCode::CREATED, Json(p)))
}

#[derive(Deserialize)]
pub struct PatchProjectBody {
    pub new_name: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i64>,
}

pub async fn patch_project(
    State(core): State<CoreState>,
    Path(name): Path<String>,
    Json(body): Json<PatchProjectBody>,
) -> ApiResult<impl IntoResponse> {
    let mut conn = core.db.lock().unwrap();
    let p = projects::patch(
        &mut conn,
        &name,
        &projects::ProjectPatch {
            new_name: body.new_name,
            color: body.color,
            sort_order: body.sort_order,
        },
    )?;
    drop(conn);
    core.events.notify();
    Ok(Json(p))
}

pub async fn delete_project(
    State(core): State<CoreState>,
    Path(name): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    projects::remove(&conn, &name)?;
    drop(conn);
    core.events.notify();
    Ok(StatusCode::NO_CONTENT)
}

// ── 附件（仅网页端，规划 §5.4） ───────────────────────────

pub async fn list_attachments(
    State(core): State<CoreState>,
    Path(task_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let conn = core.db.lock().unwrap();
    tasks::get(&conn, &task_id)?; // 任务不存在 → 404
    let mut stmt = conn.prepare(
        "SELECT * FROM attachments WHERE task_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([task_id], |r| {
        Ok(attach::Attachment {
            id: r.get("id")?,
            task_id: r.get("task_id")?,
            filename: r.get("filename")?,
            stored_path: r.get("stored_path")?,
            mime: r.get("mime")?,
            size: r.get("size")?,
            created_at: r.get("created_at")?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(Json(out))
}

pub async fn upload_attachment(
    State(core): State<CoreState>,
    Path(task_id): Path<String>,
    mut multipart: Multipart,
) -> ApiResult<impl IntoResponse> {
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
    let ext = filename.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let stored_rel = format!("attachments/{id}.{ext}");
    let stored_abs = core.data_dir.join(&stored_rel);

    std::fs::create_dir_all(stored_abs.parent().unwrap())?;
    std::fs::write(&stored_abs, &data)?;

    let mime = mime_guess::from_path(&filename).first().map(|m| m.to_string());
    let conn = core.db.lock().unwrap();
    let result = (|| -> ApiResult<attach::Attachment> {
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

fn load_attachment(core: &CoreState, id: &str) -> ApiResult<attach::Attachment> {
    let conn = core.db.lock().unwrap();
    conn.query_row(
        "SELECT * FROM attachments WHERE id = ?1",
        [id],
        |r| {
            Ok(attach::Attachment {
                id: r.get("id")?,
                task_id: r.get("task_id")?,
                filename: r.get("filename")?,
                stored_path: r.get("stored_path")?,
                mime: r.get("mime")?,
                size: r.get("size")?,
                created_at: r.get("created_at")?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => ApiError::not_found("附件不存在"),
        other => ApiError::from(other),
    })
}

pub async fn download_attachment(
    State(core): State<CoreState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let a = load_attachment(&core, &id)?;
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
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let a = load_attachment(&core, &id)?;
    let conn = core.db.lock().unwrap();
    conn.execute("DELETE FROM attachments WHERE id = ?1", [id])?;
    drop(conn);
    let _ = std::fs::remove_file(core.data_dir.join(&a.stored_path));
    core.events.notify();
    Ok(StatusCode::NO_CONTENT)
}

// ── SSE ──────────────────────────────────────────────────

pub async fn events(State(core): State<CoreState>) -> impl IntoResponse {
    let mut rx = core.events.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(()) => yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().event("tasks_changed").data("{}")),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}
