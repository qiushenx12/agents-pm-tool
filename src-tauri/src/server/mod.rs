pub mod api_agent;
pub mod api_agent_access;
pub mod api_auth;
pub mod api_batch;
pub mod api_skill;
pub mod api_users;
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
    pub actual_port: RwLock<u16>,
}

pub type CoreState = Arc<CoreStateInner>;

impl CoreStateInner {
    pub fn new(data_dir: PathBuf, db: rusqlite::Connection, settings: Settings) -> Self {
        let token = uuid::Uuid::new_v4().to_string();
        // runtime.json 中的本机 token 归属内置主机账号；每次启动令旧 token 失效。
        crate::db::users::set_agent_token(&db, crate::domain::user::HOST_USER_ID, &token)
            .expect("初始化主机 Agent token 失败");
        Self {
            db: Mutex::new(db),
            token: tokio::sync::RwLock::new(token),
            settings: RwLock::new(settings),
            data_dir,
            events: EventBus::new(),
            actual_port: RwLock::new(0),
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
    let protected_web = Router::new()
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
            "/tasks/{id}/reorder",
            axum::routing::post(api_web::reorder_task),
        )
        .route(
            "/tasks/rebase-order",
            axum::routing::post(api_web::rebase_order),
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
        .route("/pick-folder", axum::routing::post(api_web::pick_folder))
        .route(
            "/attachments/{id}",
            get(api_web::download_attachment).delete(api_web::delete_attachment),
        )
        .route("/events", get(api_web::events))
        .route("/skill/download", get(api_skill::web_download))
        .route("/local-skills", get(api_skill::local_targets))
        .route(
            "/local-skills/install",
            axum::routing::post(api_skill::install_local),
        )
        .route(
            "/local-skills/open",
            axum::routing::post(api_skill::open_local_directory),
        )
        .route("/users", get(api_users::list_users))
        .route(
            "/users/{id}",
            axum::routing::patch(api_users::patch_user).delete(api_users::delete_user),
        )
        .route(
            "/users/{id}/permissions",
            get(api_users::get_permissions).put(api_users::put_permissions),
        )
        .route("/auth/me", get(api_auth::me))
        .route("/auth/logout", axum::routing::post(api_auth::logout))
        .route("/me/agent-access", get(api_agent_access::get_access))
        .route(
            "/me/agent-token",
            axum::routing::post(api_agent_access::regenerate_token)
                .delete(api_agent_access::revoke_token),
        )
        .route_layer(middleware::from_fn_with_state(
            core.clone(),
            auth::require_web_auth,
        ));

    let public_web = Router::new()
        .route("/auth/register", axum::routing::post(api_auth::register))
        .route("/auth/login", axum::routing::post(api_auth::login))
        .route(
            "/auth/host-login",
            axum::routing::post(api_auth::host_login),
        );

    let web = Router::new()
        .merge(public_web)
        .merge(protected_web)
        .route_layer(middleware::from_fn(auth::require_web_client_header));

    let agent = Router::new()
        .route(
            "/tasks",
            get(api_agent::list_tasks).post(api_agent::create_task),
        )
        .route("/tasks/{id}", get(api_agent::get_task))
        .route("/tasks/{id}/attachments", get(api_agent::list_attachments))
        .route("/attachments/{id}", get(api_agent::download_attachment))
        .route(
            "/tasks/{id}/status",
            axum::routing::patch(api_agent::patch_status),
        )
        .route(
            "/tasks/{id}/description",
            axum::routing::patch(api_agent::patch_description),
        )
        .route("/projects", get(api_agent::list_projects))
        .route("/help", get(api_agent::help))
        .route("/skill/download", get(api_skill::agent_download))
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
pub(crate) async fn write_runtime_json(core: &CoreState, port: u16) -> ApiResult<()> {
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
    *core.actual_port.write().unwrap() = port;

    write_runtime_json(&core, port).await?;

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let app = build_router(core);

    let join = tokio::spawn(async move {
        let server = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
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
