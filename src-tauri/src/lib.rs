pub mod db;
pub mod domain;
pub mod error;
pub mod paths; // pm-cli 复用服务发现路径
pub mod server;
pub mod settings;

use std::sync::Mutex;

use serde::Serialize;
use settings::Settings;
use tauri::{Manager, State, WindowEvent};

/// Tauri 托管状态
pub struct AppState {
    pub core: server::CoreState,
    pub server: Mutex<Option<server::ServerHandle>>,
}

#[derive(Serialize)]
struct ServerStatus {
    running: bool,
    port: u16,
    url: String,
    data_dir: String,
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.core.settings.read().unwrap().clone()
}

#[tauri::command]
async fn save_settings(
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    let old_port = state.core.settings.read().unwrap().port;
    settings
        .save(&paths::settings_path(&state.core.data_dir))
        .map_err(|e| e.to_string())?;
    *state.core.settings.write().unwrap() = settings.clone();

    // 端口变化 → 重启服务（规划 §5.6：保存即重启）
    if settings.port != old_port {
        let old = state.server.lock().unwrap().take();
        if let Some(h) = old {
            h.stop().await;
        }
        let handle = server::start_server(state.core.clone(), settings.port)
            .await
            .map_err(|e| e.message)?;
        *state.server.lock().unwrap() = Some(handle);
    }
    Ok(settings)
}

#[tauri::command]
fn get_server_status(state: State<'_, AppState>) -> ServerStatus {
    let guard = state.server.lock().unwrap();
    match guard.as_ref() {
        Some(h) => ServerStatus {
            running: true,
            port: h.port,
            url: format!("http://127.0.0.1:{}", h.port),
            data_dir: state.core.data_dir.display().to_string(),
        },
        None => ServerStatus {
            running: false,
            port: 0,
            url: String::new(),
            data_dir: state.core.data_dir.display().to_string(),
        },
    }
}

#[tauri::command]
async fn regenerate_token(state: State<'_, AppState>) -> Result<(), String> {
    let new_token = uuid::Uuid::new_v4().to_string();
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
    let old = state.server.lock().unwrap().take();
    if let Some(h) = old {
        h.stop().await;
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn open_web_page(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let port = state.server.lock().unwrap().as_ref().map(|h| h.port);
    if let Some(port) = port {
        use tauri_plugin_shell::ShellExt;
        let _ = app
            .shell()
            .open(format!("http://127.0.0.1:{port}"), None);
    }
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
            "tray_show" => show_main_window(app),
            "tray_open_web" => open_web_page(app),
            "tray_quit" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
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
                show_main_window(tray.app_handle());
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
            show_main_window(app);
        }))
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            setup_tray(app)?;
            let data_dir = paths::data_dir().map_err(|e| {
                eprintln!("数据目录初始化失败：{e}");
                e
            })?;
            let db_conn = db::open(&paths::db_path(&data_dir))?;
            let app_settings = Settings::load(&paths::settings_path(&data_dir));
            let core = server::CoreStateInner::new(data_dir, db_conn, app_settings.clone());
            let core = std::sync::Arc::new(core);

            app.manage(AppState {
                core: core.clone(),
                server: Mutex::new(None),
            });

            // 启动内嵌 HTTP 服务（规划 §5.6：autostart 默认开）
            if app_settings.autostart {
                let core = core.clone();
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let port = core.settings.read().unwrap().port;
                    match server::start_server(core.clone(), port).await {
                        Ok(h) => {
                            let actual = h.port;
                            *handle.state::<AppState>().server.lock().unwrap() = Some(h);
                            eprintln!("服务已启动 http://127.0.0.1:{actual}");
                        }
                        Err(e) => eprintln!("服务启动失败：{}", e.message),
                    }
                });
            }

            // 窗口创建完成后显示（frameless 避免白屏闪烁）
            show_main_window(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let close_behavior = window
                    .app_handle()
                    .state::<AppState>()
                    .core
                    .settings
                    .read()
                    .unwrap()
                    .close_behavior
                    .clone();
                if close_behavior == "keep_service" {
                    // 关窗保服务：隐藏窗口而不是退出（规划 §5.6/Phase 3）
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        shutdown_server(&app).await;
                        app.exit(0);
                    });
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_server_status,
            regenerate_token,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Agents PM Tool");
}
