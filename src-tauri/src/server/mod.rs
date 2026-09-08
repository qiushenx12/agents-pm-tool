pub mod api_agent;
pub mod api_batch;
pub mod api_web;
pub mod auth;
pub mod events;
pub mod static_site;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use axum::{middleware, routing::get, Router};
use serde::Serialize;
use tokio::sync::watch;

use crate::db::tasks::TaskFilter;
use crate::error::{ApiError, ApiResult};
use crate::paths;
use crate::settings::Settings;
use events::EventBus;

/// 全应用共享状态（axum handler 与 Tauri command 共用）
pub struct CoreStateInner {
    pub db: Mutex<rusqlite::Connection>,
    pub token: tokio::sync::RwLock<String>,
    pub settings: RwLock<Settings>,
    pub data_dir: PathBuf,
    pub events: EventBus,
}

pub type CoreState = Arc<CoreStateInner>;

impl CoreStateInner {
    pub fn new(data_dir: PathBuf, db: rusqlite::Connection, settings: Settings) -> Self {
        Self {
            db: Mutex::new(db),
            token: tokio::sync::RwLock::new(uuid::Uuid::new_v4().to_string()),
            settings: RwLock::new(settings),
            data_dir,
            events: EventBus::new(),
        }
    }
}

/// 解析查询串：project/type/status/submitter 可重复，keyword、sort_by、sort_order 单值
pub fn parse_task_filter(raw: Option<&str>) -> ApiResult<TaskFilter> {
    let mut f = TaskFilter::default();
    for (k, v) in url::form_urlencoded::parse(raw.unwrap_or("").as_bytes()) {
        match k.as_ref() {
            "project" => f.project.push(v.into_owned()),
            "type" => f.task_type.push(v.into_owned()),
            "status" => f.status.push(v.into_owned()),
            "submitter" => f.submitter.push(v.into_owned()),
            "keyword" => f.keyword = Some(v.into_owned()),
            "sort_by" => f.sort_by = Some(v.into_owned()),
            "sort_order" => f.sort_order = Some(v.into_owned()),
            _ => {}
        }
    }
    Ok(f)
}

fn build_router(core: CoreState) -> Router {
    let web = Router::new()
        .route(
            "/tasks",
            get(api_web::list_tasks).post(api_web::create_task),
        )
        .route("/tasks/page", get(api_web::page_tasks))
        .route("/tasks/batch", axum::routing::post(api_web::batch_tasks))
        .route(
            "/tasks/{id}",
            get(api_web::get_task)
                .patch(api_web::patch_task)
                .delete(api_web::delete_task),
        )
        .route(
            "/tasks/{id}/attachments",
            get(api_web::list_attachments).post(api_web::upload_attachment),
        )
        .route(
            "/projects",
            get(api_web::list_projects).post(api_web::create_project),
        )
        .route(
            "/projects/{name}",
            axum::routing::patch(api_web::patch_project).delete(api_web::delete_project),
        )
        .route(
            "/attachments/{id}",
            get(api_web::download_attachment).delete(api_web::delete_attachment),
        )
        .route("/events", get(api_web::events));

    let agent = Router::new()
        .route(
            "/tasks",
            get(api_agent::list_tasks).post(api_agent::create_task),
        )
        .route("/tasks/{id}", get(api_agent::get_task))
        .route(
            "/tasks/{id}/status",
            axum::routing::patch(api_agent::patch_status),
        )
        .route(
            "/tasks/{id}/description",
            axum::routing::patch(api_agent::patch_description),
        )
        .route("/projects", get(api_agent::list_projects))
        .route_layer(middleware::from_fn_with_state(
            core.clone(),
            auth::require_agent_token,
        ));

    Router::new()
        .nest("/api/web", web)
        .nest("/api/agent", agent)
        // 附件上传 200MB 上限
        .layer(axum::extract::DefaultBodyLimit::max(
            crate::domain::attachment::MAX_SIZE as usize + 1024 * 1024,
        ))
        .fallback(get(static_site::index))
        .route("/assets/{*path}", get(static_site::asset))
        .with_state(core)
}

pub struct ServerHandle {
    pub port: u16,
    shutdown: watch::Sender<bool>,
    join: tokio::task::JoinHandle<()>,
}

impl ServerHandle {
    pub async fn stop(self) {
        let _ = self.shutdown.send(true);
        let _ = self.join.await;
    }
}

#[derive(Serialize)]
struct RuntimeInfo {
    port: u16,
    token: String,
    pid: u32,
}

/// 写 data/runtime.json 供 pm-cli 服务发现（规划 §5.5）
async fn write_runtime_json(core: &CoreState, port: u16) -> ApiResult<()> {
    let info = RuntimeInfo {
        port,
        token: core.token.read().await.clone(),
        pid: std::process::id(),
    };
    let s = serde_json::to_string_pretty(&info).map_err(ApiError::internal)?;
    std::fs::write(paths::runtime_path(&core.data_dir), s)?;
    Ok(())
}

/// 启动 axum 服务；端口占用时自动顺延（最多 +20，规划 §7）
/// `bind_host` 来自 Settings::bind_host()：127.0.0.1 仅本机 / 0.0.0.0 局域网可达
pub async fn start_server(
    core: CoreState,
    preferred_port: u16,
    bind_host: [u8; 4],
) -> ApiResult<ServerHandle> {
    let mut last_err: Option<std::io::Error> = None;
    let mut listener = None;
    let mut port = preferred_port;

    for offset in 0..=20u16 {
        port = preferred_port.saturating_add(offset);
        let addr = SocketAddr::from((bind_host, port));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => {
                listener = Some(l);
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }

    let listener = listener.ok_or_else(|| {
        ApiError::internal(format!(
            "端口 {preferred_port} 起连续 20 个端口均被占用：{}",
            last_err.map(|e| e.to_string()).unwrap_or_default()
        ))
    })?;
    // preferred_port = 0 时取 OS 分配的实际端口（测试用）
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(port);

    write_runtime_json(&core, port).await?;

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let app = build_router(core);

    let join = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_rx.changed().await;
        });
        if let Err(e) = server.await {
            eprintln!("axum 服务异常退出：{e}");
        }
    });

    Ok(ServerHandle {
        port,
        shutdown: shutdown_tx,
        join,
    })
}
