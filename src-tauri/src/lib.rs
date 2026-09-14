pub mod db;
pub mod domain;
pub mod error;
pub mod paths; // pm-cli 复用服务发现路径
pub mod server;
pub mod settings;
pub mod window_state;

use std::sync::Mutex;

use serde::Serialize;
use settings::Settings;
use tauri::{Emitter, Manager, State, WindowEvent};

/// Tauri 托管状态
pub struct AppState {
    pub core: server::CoreState,
    pub server: Mutex<Option<server::ServerHandle>>,
    /// 串行化启动、停止与重启，避免自动启动和设置页按钮同时拉起两个服务。
    pub server_transition: tokio::sync::Mutex<()>,
    /// 应用内网页窗口的几何记忆（关闭时落盘，见 window_state）
    pub app_window: Mutex<window_state::GeometryTracker>,
    /// 当前停留的桌面界面；退出时写入 window-state.json，供下次启动恢复。
    pub active_view: Mutex<window_state::ActiveView>,
}

#[derive(Serialize)]
struct ServerStatus {
    running: bool,
    port: u16,
    url: String,
    /// LAN 模式下的局域网访问地址（供其他设备访问）；local 模式为空
    lan_url: String,
    data_dir: String,
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.core.settings.read().unwrap().clone()
}

#[derive(Serialize)]
struct SaveSettingsResult {
    settings: Settings,
    /// 端口变化导致服务重启（前端提示语据此区分，review P3-3）
    restarted: bool,
    /// 重启后的实际端口（可能因占用顺延）
    port: u16,
}

/// 应用内网页窗口的 label 与标题（「进入应用」打开的窗口）
const APP_WINDOW_LABEL: &str = "web";
const APP_WINDOW_TITLE: &str = "Agents PM Tool";

/// 正在监听的本机服务端口；服务未运行时为 None
fn server_port(app: &tauri::AppHandle) -> Option<u16> {
    let state = app.state::<AppState>();
    let port = state.server.lock().unwrap().as_ref().map(|h| h.port);
    port
}

/// 本机服务的访问地址；服务未运行时为 None
fn server_url(app: &tauri::AppHandle) -> Option<String> {
    server_port(app).map(|port| format!("http://127.0.0.1:{port}"))
}

/// 调用方须先持有 `server_transition`。
async fn start_embedded_server(state: &AppState) -> Result<u16, String> {
    if let Some(port) = state
        .server
        .lock()
        .unwrap()
        .as_ref()
        .map(|handle| handle.port)
    {
        return Ok(port);
    }
    let (port, bind_host) = {
        let settings = state.core.settings.read().unwrap();
        (settings.port, settings.bind_host())
    };
    let handle = server::start_server(state.core.clone(), port, bind_host)
        .await
        .map_err(|error| error.message)?;
    let actual = handle.port;
    *state.server.lock().unwrap() = Some(handle);
    Ok(actual)
}

/// 调用方须先持有 `server_transition`。返回停止前是否确有服务在运行。
async fn stop_embedded_server(state: &AppState) -> bool {
    let old = state.server.lock().unwrap().take();
    let was_running = old.is_some();
    if let Some(handle) = old {
        handle.stop().await;
    }
    *state.core.actual_port.write().unwrap() = 0;
    match std::fs::remove_file(paths::runtime_path(&state.core.data_dir)) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => eprintln!("清理服务运行信息失败：{error}"),
    }
    was_running
}

fn app_window_url(port: u16) -> Result<tauri::Url, String> {
    format!("http://127.0.0.1:{port}")
        .parse()
        .map_err(|e| format!("地址无效：{e}"))
}

/// 全局主题（settings.theme）→ 应用窗口原生顶栏的明暗。
/// Windows 上经 DWM 沉浸式深色模式生效；None = 跟随系统。
fn native_title_bar_theme(theme: Option<&str>) -> Option<tauri::Theme> {
    match theme {
        Some("light") => Some(tauri::Theme::Light),
        Some("dark") => Some(tauri::Theme::Dark),
        _ => None,
    }
}

/// 让已打开的应用窗口跟上服务端口——端口变更会重启服务，旧地址随即失效。
/// 只比较 origin：网页端自身可能改过路径，不该被强行拉回首页。
fn sync_app_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>, port: u16) {
    let Some(window) = app.get_webview_window(APP_WINDOW_LABEL) else {
        return;
    };
    let Ok(target) = app_window_url(port) else {
        return;
    };
    let same_origin = window
        .url()
        .map(|current| {
            current.scheme() == target.scheme()
                && current.host_str() == target.host_str()
                && current.port() == target.port()
        })
        .unwrap_or(false);
    if !same_origin {
        let _ = window.navigate(target);
    }
}

/// 应用窗口已存在则聚焦（并纠正地址），否则按本机服务地址新建一个。
/// 公开是为了让 `tests/app_window.rs` 能用 mock runtime 直接驱动窗口创建。
pub fn focus_or_create_app_window<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    port: u16,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(APP_WINDOW_LABEL) {
        sync_app_window(app, port);
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }
    // 上次关闭时的位置与大小（含"当时是最大化"的标记）
    let geometry = app
        .try_state::<AppState>()
        .and_then(|state| window_state::load(&state.core.data_dir));
    // 原生顶栏跟随全局主题（settings.theme），None = 跟随系统
    let theme = app
        .try_state::<AppState>()
        .and_then(|state| state.core.settings.read().unwrap().theme.clone());
    let builder = tauri::WebviewWindowBuilder::new(
        app,
        APP_WINDOW_LABEL,
        tauri::WebviewUrl::External(app_window_url(port)?),
    )
    .title(APP_WINDOW_TITLE)
    // Windows 上 Tauri 的原生文件拖放会截获 WebView 的 HTML5 DragEvent；
    // 附件上传依赖 dataTransfer.files，因此应用工作台必须让事件交给前端。
    .disable_drag_drop_handler()
    .theme(native_title_bar_theme(theme.as_deref()))
    .min_inner_size(window_state::MIN_WIDTH, window_state::MIN_HEIGHT)
    .inner_size(window_state::DEFAULT_WIDTH, window_state::DEFAULT_HEIGHT)
    // 先摆好再显示：越界的位置会被夹回工作区，省得窗口先闪一下再跳
    .visible(false);
    let builder = match geometry {
        Some(geometry) => builder
            .position(geometry.x, geometry.y)
            .maximized(geometry.maximized),
        None => builder.center(),
    };
    let window = builder
        .build()
        .map_err(|e| format!("打开应用窗口失败：{e}"))?;
    if let (Some(state), Some(geometry)) = (app.try_state::<AppState>(), geometry) {
        let fitted = window_state::apply_geometry(&window, geometry);
        // 先按这份几何预热：用户什么都没动就关窗时，也能原样存回去
        state.app_window.lock().unwrap().prime(fitted);
    }
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

/// 记录应用窗口当前几何：正常态更新位置大小，最大化只更新标记（见 window_state）
fn track_app_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(window) = app.get_webview_window(APP_WINDOW_LABEL) else {
        return;
    };
    let Some((geometry, placement)) = window_state::capture(&window) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    state.app_window.lock().unwrap().observe(geometry, placement);
}

/// 落盘应用窗口几何：关窗时保存，退出应用（托盘退出 / 关闭任一桌面窗口）时也补一次。
/// 窗口已经销毁就只用内存里最后一次观测到的值，因此不会因为读不到窗口而丢状态。
fn persist_app_window_geometry<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    track_app_window(app);
    let snapshot = state.app_window.lock().unwrap().snapshot();
    if let Some(geometry) = snapshot {
        if let Err(e) = window_state::save(&state.core.data_dir, &geometry) {
            eprintln!("保存应用窗口位置失败：{e}");
        }
    }
}

/// 退出前把当前界面与工作区窗口几何一起持久化。
fn persist_desktop_state<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    persist_app_window_geometry(app);
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let active_view = *state.active_view.lock().unwrap();
    if let Err(error) = window_state::save_active_view(&state.core.data_dir, active_view) {
        eprintln!("保存应用界面状态失败：{error}");
    }
}

#[tauri::command]
async fn save_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    mut settings: Settings,
) -> Result<SaveSettingsResult, String> {
    let old = state.core.settings.read().unwrap().clone();
    // 设置表单只管端口/行为；主题由专门入口维护，保存表单时不能把它冲掉
    settings.theme = old.theme.clone();
    settings.validate().map_err(str::to_string)?;
    settings
        .save(&paths::settings_path(&state.core.data_dir))
        .map_err(|e| e.to_string())?;
    *state.core.settings.write().unwrap() = settings.clone();

    // 端口或监听范围变化 → 重启服务（规划 §5.6：保存即重启）
    let mut restarted = false;
    if settings.port != old.port || settings.bind_host() != old.bind_host() {
        let _transition = state.server_transition.lock().await;
        // 手动停止后仍允许修改并保存连接设置，但不能因此意外重新启动服务。
        if stop_embedded_server(&state).await {
            start_embedded_server(&state).await?;
            restarted = true;
        }
    }
    let port = state
        .server
        .lock()
        .unwrap()
        .as_ref()
        .map(|h| h.port)
        .unwrap_or(0);
    if restarted {
        sync_app_window(&app, port);
    }
    Ok(SaveSettingsResult {
        settings,
        restarted,
        port,
    })
}

/// 本机局域网 IPv4（UDP connect 不真发包，只是让内核选出对外网卡）
fn lan_ipv4() -> Option<std::net::Ipv4Addr> {
    let s = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    s.connect(("8.8.8.8", 80)).ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(v4) if !v4.is_loopback() => Some(v4),
        _ => None,
    }
}

#[tauri::command]
fn get_server_status(state: State<'_, AppState>) -> ServerStatus {
    current_server_status(&state)
}

fn current_server_status(state: &AppState) -> ServerStatus {
    let guard = state.server.lock().unwrap();
    let is_lan = state.core.settings.read().unwrap().bind_host() == [0, 0, 0, 0];
    match guard.as_ref() {
        Some(h) => ServerStatus {
            running: true,
            port: h.port,
            url: format!("http://127.0.0.1:{}", h.port),
            lan_url: if is_lan {
                lan_ipv4()
                    .map(|ip| format!("http://{ip}:{}", h.port))
                    .unwrap_or_default()
            } else {
                String::new()
            },
            data_dir: state.core.data_dir.display().to_string(),
        },
        None => ServerStatus {
            running: false,
            port: 0,
            url: String::new(),
            lan_url: String::new(),
            data_dir: state.core.data_dir.display().to_string(),
        },
    }
}

/// 设置页手动启动或停止内嵌服务；返回操作后的真实状态供界面立即刷新。
#[tauri::command]
async fn set_server_running(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    running: bool,
) -> Result<ServerStatus, String> {
    {
        let _transition = state.server_transition.lock().await;
        if running {
            let port = start_embedded_server(&state).await?;
            sync_app_window(&app, port);
        } else {
            stop_embedded_server(&state).await;
        }
    }

    if !running {
        // 服务停止后不保留一个必然断线的工作区窗口；当前控制入口仍停留在设置页。
        persist_app_window_geometry(&app);
        if let Some(window) = app.get_webview_window(APP_WINDOW_LABEL) {
            let _ = window.destroy();
        }
        show_settings_view(&app);
    }
    Ok(current_server_status(&state))
}

#[tauri::command]
async fn regenerate_token(state: State<'_, AppState>) -> Result<(), String> {
    let new_token = uuid::Uuid::new_v4().to_string();
    {
        let connection = state.core.db.lock().unwrap();
        db::users::set_agent_token(&connection, domain::user::HOST_USER_ID, &new_token)
            .map_err(|error| error.message)?;
    }
    *state.core.token.write().await = new_token;
    // 立即写 runtime.json，让 CLI 用上新 token（规划 §5.6）
    let port = state.server.lock().unwrap().as_ref().map(|h| h.port);
    if let Some(port) = port {
        let info = serde_json::json!({
            "port": port,
            "token": state.core.token.read().await.clone(),
            "pid": std::process::id(),
        });
        std::fs::write(
            paths::runtime_path(&state.core.data_dir),
            serde_json::to_string_pretty(&info).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn shutdown_server(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let _transition = state.server_transition.lock().await;
    stop_embedded_server(&state).await;
}

fn show_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn active_view<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> window_state::ActiveView {
    app.try_state::<AppState>()
        .map(|state| *state.active_view.lock().unwrap())
        .unwrap_or_default()
}

fn set_active_view<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    active_view: window_state::ActiveView,
) {
    if let Some(state) = app.try_state::<AppState>() {
        *state.active_view.lock().unwrap() = active_view;
    }
}

fn show_settings_view<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    show_main_window(app);
    set_active_view(app, window_state::ActiveView::Settings);
}

fn show_workspace_view<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    port: u16,
) -> Result<(), String> {
    focus_or_create_app_window(app, port)?;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    set_active_view(app, window_state::ActiveView::Workspace);
    Ok(())
}

/// 恢复上次退出时停留的界面。工作区依赖内嵌服务；服务不可用时回退到设置页。
/// 公开是为了让窗口级测试覆盖真实的启动选择逻辑。
pub fn restore_active_view<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    port: Option<u16>,
) -> Result<(), String> {
    if active_view(app) == window_state::ActiveView::Workspace {
        if let Some(port) = port {
            if let Err(error) = show_workspace_view(app, port) {
                show_settings_view(app);
                return Err(error);
            }
            return Ok(());
        }
    }
    show_settings_view(app);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowCloseAction {
    /// 设置窗口在保留服务时隐藏，以便从托盘再次打开。
    HideWindow,
    /// 工作台在保留服务时正常关闭；服务与托盘继续运行。
    CloseWindow,
    /// 当前设置要求退出应用，并在退出前停止内嵌服务。
    StopAll,
}

/// “关闭窗口时”同时约束设置窗口和应用内工作台。
/// 未知设置值沿用原有的安全行为：按 stop_all 处理，避免意外驻留后台。
fn window_close_action(window_label: &str, close_behavior: &str) -> WindowCloseAction {
    if !matches!(window_label, "main" | APP_WINDOW_LABEL) {
        return WindowCloseAction::CloseWindow;
    }
    if close_behavior != "keep_service" {
        return WindowCloseAction::StopAll;
    }
    if window_label == "main" {
        WindowCloseAction::HideWindow
    } else {
        WindowCloseAction::CloseWindow
    }
}

/// 从应用内工作台返回设置窗口：先记录并关闭 web 窗口，再显示设置窗口。
/// 公开是为了让窗口级测试使用 Tauri mock runtime 验证切换行为。
pub fn show_settings_and_close_app_window<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(), String> {
    persist_app_window_geometry(app);
    if let Some(window) = app.get_webview_window(APP_WINDOW_LABEL) {
        window
            .destroy()
            .map_err(|error| format!("关闭应用窗口失败：{error}"))?;
    }
    show_settings_view(app);
    Ok(())
}

fn open_web_page(app: &tauri::AppHandle) {
    if let Some(url) = server_url(app) {
        use tauri_plugin_opener::OpenerExt;
        let _ = app.opener().open_url(url, None::<&str>);
    }
}

/// 「进入应用」：在应用内打开网页同款的界面。
/// 已打开则聚焦，不重复创建；地址随端口变化时自动纠正。
#[tauri::command]
async fn open_app_window(app: tauri::AppHandle) -> Result<(), String> {
    let port = server_port(&app).ok_or("服务未运行，无法进入应用")?;
    show_workspace_view(&app, port)
}

/// 应用内工作台的“设置”：关闭工作台窗口并回到桌面设置窗口。
#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    show_settings_and_close_app_window(&app)
}

/// 设置全局明暗主题：设置窗口切换时调用，网页端 PUT appearance 走同一份状态。
#[tauri::command]
async fn set_theme(state: State<'_, AppState>, theme: String) -> Result<(), String> {
    server::set_theme(&state.core, &theme).map_err(|e| e.message)
}

/// 系统托盘：打开设置 / 打开网页 / 退出（停止服务）
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = MenuItem::with_id(app, "tray_show", "打开设置", true, None::<&str>)?;
    let open_web = MenuItem::with_id(app, "tray_open_web", "打开网页", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray_quit", "退出（停止服务）", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &open_web, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Agents PM Tool")
        .icon(app.default_window_icon().unwrap().clone())
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_show" => show_settings_view(app),
            "tray_open_web" => open_web_page(app),
            "tray_quit" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    // 先记录当前界面与应用窗口几何，再停服务退出
                    persist_desktop_state(&app);
                    shutdown_server(&app).await;
                    app.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_settings_view(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例：重复启动时聚焦已有窗口，避免起第二个服务（端口顺延导致双实例）
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = restore_active_view(app, server_port(app));
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            setup_tray(app)?;
            let data_dir = paths::data_dir().map_err(|e| {
                eprintln!("数据目录初始化失败：{e}");
                e
            })?;
            let db_conn = db::open(&paths::db_path(&data_dir))?;
            let app_settings = Settings::load(&paths::settings_path(&data_dir));
            let startup_view = window_state::load_active_view(&data_dir);
            let core = server::CoreStateInner::new(data_dir, db_conn, app_settings.clone());
            let core = std::sync::Arc::new(core);

            app.manage(AppState {
                core: core.clone(),
                server: Mutex::new(None),
                server_transition: tokio::sync::Mutex::new(()),
                app_window: Mutex::new(window_state::GeometryTracker::default()),
                active_view: Mutex::new(startup_view),
            });

            // 网页端改主题（PUT appearance）→ 转发给设置窗口实时换肤
            {
                let app = app.handle().clone();
                let core = core.clone();
                tauri::async_runtime::spawn(async move {
                    let mut rx = core.theme_events.subscribe();
                    loop {
                        if rx.changed().await.is_err() {
                            break;
                        }
                        let theme = rx.borrow_and_update().clone();
                        let _ = app.emit("theme-changed", theme.clone());
                        // 应用窗口的原生顶栏也跟着换（DWM 沉浸式深色模式）
                        if let Some(window) = app.get_webview_window(APP_WINDOW_LABEL) {
                            let _ =
                                window.set_theme(native_title_bar_theme(theme.as_deref()));
                        }
                    }
                });
            }

            // 主机网页设置修改端口或监听范围后，在当前请求完成响应之后重启服务。
            // graceful shutdown 会等待触发这次重启的 handler 返回，避免保存响应被截断。
            {
                let app = app.handle().clone();
                let core = core.clone();
                tauri::async_runtime::spawn(async move {
                    let mut rx = core.settings_restart_events.subscribe();
                    loop {
                        if rx.changed().await.is_err() {
                            break;
                        }
                        let state = app.state::<AppState>();
                        let _transition = state.server_transition.lock().await;
                        stop_embedded_server(&state).await;
                        match start_embedded_server(&state).await {
                            Ok(actual) => {
                                sync_app_window(&app, actual);
                                eprintln!("服务已按网页设置重启 http://127.0.0.1:{actual}");
                            }
                            Err(error) => eprintln!("按网页设置重启服务失败：{error}"),
                        }
                    }
                });
            }

            // 启动内嵌 HTTP 服务（规划 §5.6：autostart 默认开）
            if app_settings.autostart {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    let _transition = state.server_transition.lock().await;
                    match start_embedded_server(&state).await {
                        Ok(actual) => {
                            eprintln!("服务已启动 http://127.0.0.1:{actual}");
                            // 只有上次停留在工作区、且启动期间没有切回设置页时才直接恢复。
                            if active_view(&handle) == window_state::ActiveView::Workspace {
                                if let Err(error) = restore_active_view(&handle, Some(actual)) {
                                    eprintln!("恢复应用界面失败：{error}");
                                }
                            }
                        }
                        Err(error) => {
                            eprintln!("服务启动失败：{error}");
                            if active_view(&handle) == window_state::ActiveView::Workspace {
                                show_settings_view(&handle);
                            }
                        }
                    }
                });
            }

            // 设置页可立即显示；工作区则等内嵌服务成功后直接恢复，避免先闪出设置页。
            if !app_settings.autostart || startup_view == window_state::ActiveView::Settings {
                show_settings_view(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // 若设置页与工作区因托盘操作短暂同时存在，以最后获得焦点的界面为准。
            if matches!(event, WindowEvent::Focused(true)) {
                match window.label() {
                    "main" => {
                        set_active_view(window.app_handle(), window_state::ActiveView::Settings)
                    }
                    APP_WINDOW_LABEL => {
                        set_active_view(window.app_handle(), window_state::ActiveView::Workspace)
                    }
                    _ => {}
                }
            }
            // 应用内网页窗口：位置/大小/最大化状态随时记，关窗时落盘
            if window.label() == APP_WINDOW_LABEL {
                match event {
                    WindowEvent::Resized(_) | WindowEvent::Moved(_) => {
                        track_app_window(window.app_handle())
                    }
                    WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed => {
                        persist_app_window_geometry(window.app_handle())
                    }
                    _ => {}
                }
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if !matches!(window.label(), "main" | APP_WINDOW_LABEL) {
                    return;
                }
                let close_behavior = window
                    .app_handle()
                    .state::<AppState>()
                    .core
                    .settings
                    .read()
                    .unwrap()
                    .close_behavior
                    .clone();
                match window_close_action(window.label(), &close_behavior) {
                    WindowCloseAction::HideWindow => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    WindowCloseAction::CloseWindow => {
                        // 工作台正常关闭；进程和服务继续由托盘承载。
                    }
                    WindowCloseAction::StopAll => {
                        api.prevent_close();
                        let app = window.app_handle().clone();
                        tauri::async_runtime::spawn(async move {
                            persist_desktop_state(&app);
                            shutdown_server(&app).await;
                            app.exit(0);
                        });
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            set_theme,
            get_server_status,
            set_server_running,
            regenerate_token,
            open_app_window,
            open_settings_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Agents PM Tool");
}

#[cfg(test)]
mod tests {
    use super::{native_title_bar_theme, window_close_action, WindowCloseAction};

    #[test]
    fn maps_global_theme_to_native_title_bar() {
        assert_eq!(
            native_title_bar_theme(Some("light")),
            Some(tauri::Theme::Light)
        );
        assert_eq!(
            native_title_bar_theme(Some("dark")),
            Some(tauri::Theme::Dark)
        );
        // 未设主题 / 异常值都退回跟随系统
        assert_eq!(native_title_bar_theme(None), None);
        assert_eq!(native_title_bar_theme(Some("system")), None);
    }

    #[test]
    fn close_behavior_applies_to_both_desktop_windows() {
        assert_eq!(
            window_close_action("main", "keep_service"),
            WindowCloseAction::HideWindow
        );
        assert_eq!(
            window_close_action("web", "keep_service"),
            WindowCloseAction::CloseWindow
        );
        assert_eq!(
            window_close_action("main", "stop_all"),
            WindowCloseAction::StopAll
        );
        assert_eq!(
            window_close_action("web", "stop_all"),
            WindowCloseAction::StopAll
        );
    }

    #[test]
    fn unknown_close_behavior_does_not_leave_the_app_running() {
        assert_eq!(
            window_close_action("web", "unexpected"),
            WindowCloseAction::StopAll
        );
    }
}
