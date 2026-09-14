pub mod api_agent;
pub mod api_agent_access;
pub mod api_auth;
pub mod api_batch;
pub mod api_skill;
pub mod api_settings;
pub mod api_users;
pub mod api_web;
pub mod auth;
pub mod events;
pub mod finish_notice;
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
    /// 任务进入「待验证」「已完成」的进程内提醒（Tauri 外壳据此弹系统通知）
    pub finish_notices: finish_notice::FinishNoticeBus,
    pub actual_port: RwLock<u16>,
    /// 全局主题变更通道：SSE 下发 theme_changed、Tauri 侧转发给设置窗口。
    pub theme_events: watch::Sender<Option<String>>,
    /// 网页主机设置修改端口/监听范围后，通知 Tauri 外壳重启 HTTP 服务。
    pub settings_restart_events: watch::Sender<u64>,
}

pub type CoreState = Arc<CoreStateInner>;

impl CoreStateInner {
    pub fn new(data_dir: PathBuf, db: rusqlite::Connection, settings: Settings) -> Self {
        // 主机 Agent token 固定不变：已有有效 token 直接复用，仅首次启动（或被吊销后）生成。
        // 重启不再令旧 token 失效，避免本机之外的 Agent 每次都要重新取 token；
        // 需要更换时在设置窗口或网页「我的 Agent 访问」手动重新生成。
        let existing = crate::db::users::active_agent_token(&db, crate::domain::user::HOST_USER_ID)
            .expect("读取主机 Agent token 失败");
        let token = match existing {
            Some(token) => token,
            None => {
                let token = uuid::Uuid::new_v4().to_string();
                crate::db::users::set_agent_token(&db, crate::domain::user::HOST_USER_ID, &token)
                    .expect("初始化主机 Agent token 失败");
                token
            }
        };
        let theme = settings.theme.clone();
        Self {
            db: Mutex::new(db),
            token: tokio::sync::RwLock::new(token),
            settings: RwLock::new(settings),
            data_dir,
            events: EventBus::new(),
            finish_notices: finish_notice::FinishNoticeBus::new(),
            actual_port: RwLock::new(0),
            theme_events: watch::channel(theme).0,
            settings_restart_events: watch::channel(0).0,
        }
    }
}

/// 设置全局主题（设置窗口的 Tauri 命令与网页端 PUT 共用的唯一写入口）：
/// 校验 → 落盘 settings.json → 更新内存 → 通知 SSE 订阅者与设置窗口。
pub fn set_theme(core: &CoreState, theme: &str) -> crate::error::ApiResult<()> {
    use crate::error::ApiError;
    if !crate::settings::THEMES.contains(&theme) {
        return Err(ApiError::unprocessable("主题只能是 light 或 dark"));
    }
    let mut next = core.settings.read().unwrap().clone();
    next.theme = Some(theme.to_string());
    next.save(&paths::settings_path(&core.data_dir))?;
    *core.settings.write().unwrap() = next;
    // watch::send 是异步签名；send_modify 同步完成同样的事（换值 + 唤醒等待者）
    core.theme_events.send_modify(|v| *v = Some(theme.to_string()));
    Ok(())
}

/// 解析查询串：project/type/status/submitter/priority 可重复；status_mode 支持 include/exclude。
pub fn parse_task_filter(raw: Option<&str>) -> ApiResult<TaskFilter> {
    let mut f = TaskFilter::default();
    for (k, v) in url::form_urlencoded::parse(raw.unwrap_or("").as_bytes()) {
        match k.as_ref() {
            "project" => f.project.push(v.into_owned()),
            "type" => f.task_type.push(v.into_owned()),
            "status" => f.status.push(v.into_owned()),
            "status_mode" => match v.as_ref() {
                "include" => f.exclude_status = false,
                "exclude" => f.exclude_status = true,
                _ => {
                    return Err(ApiError::bad_request(
                        "状态筛选方式只能是 include 或 exclude",
                    ))
                }
            },
            "submitter" => f.submitter.push(v.into_owned()),
            "priority" => f.priority.push(v.into_owned()),
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
            "/users/submitter-directory",
            get(api_web::submitter_directory),
        )
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
        // 分组/排序/筛选按账号存在服务端：网页端与应用内窗口读的是同一份
        .route(
            "/me/view-state",
            get(api_web::get_view_state).put(api_web::put_view_state),
        )
        .route("/me/agent-access", get(api_agent_access::get_access))
        .route(
            "/host-settings",
            get(api_settings::get_host_settings).put(api_settings::save_host_settings),
        )
        .route(
            "/me/agent-token",
            axum::routing::post(api_agent_access::regenerate_token)
                .delete(api_agent_access::revoke_token),
        )
        // 设置窗口与网页界面共用的全局主题（写入口，登录后可用）
        .route("/appearance", axum::routing::put(api_web::put_appearance))
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
        )
        // 登录页也要正确着色：主题读取公开，不带任何账号信息
        .route("/appearance", get(api_web::get_appearance));

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
            "/tasks/{id}/priority",
            axum::routing::patch(api_agent::patch_priority),
        )
        .route(
            "/tasks/{id}/description",
            axum::routing::patch(api_agent::patch_description),
        )
        .route("/projects", get(api_agent::list_projects))
        .route("/skill/download", get(api_skill::agent_download))
        .route_layer(middleware::from_fn_with_state(
            core.clone(),
            auth::require_agent_token,
        ))
        // route_layer 之后 merge 的路由不受鉴权中间件约束（axum 0.8 语义）：
        // /help 必须免 token，否则没有 pm-cli、没有 skill 的 Agent 无从得知接入方式。
        .merge(Router::new().route("/help", get(api_agent::help)));

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
        let mut join = self.join;
        // SSE 是无限响应，单靠 graceful shutdown 可能永远等不到连接自行结束。
        // 先给普通请求留出收尾时间，仍未退出就中止服务任务，确保设置保存后的重启能继续。
        if tokio::time::timeout(std::time::Duration::from_millis(500), &mut join)
            .await
            .is_err()
        {
            join.abort();
            let _ = join.await;
        }
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
