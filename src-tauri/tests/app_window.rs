//! 「进入应用」窗口的窗口级行为：用 Tauri 的 mock runtime 真建一次窗口，
//! 锁住 label / 标题 / 指向本机服务地址，以及重复点击不叠加窗口、端口变更后跟随新地址。
//!
//! 注：`build.rs` 给测试目标补了 Common-Controls v6 的清单依赖，
//! 否则链接了托盘代码的测试进程会因 comctl32 退回 5.82 而启动即失败。

use agents_pm_tool_lib::focus_or_create_app_window;
use agents_pm_tool_lib::server::CoreStateInner;
use agents_pm_tool_lib::settings::Settings;
use agents_pm_tool_lib::window_state::{self, GeometryTracker, WindowGeometry};
use agents_pm_tool_lib::AppState;
use std::sync::Mutex;
use tauri::Manager;

fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
    tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap()
}

/// 带托管状态的 app：几何记忆要读数据目录，测试里给一个临时目录。
fn mock_app_with_data_dir(data_dir: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
    let app = mock_app();
    let core = std::sync::Arc::new(CoreStateInner::new(
        data_dir.to_path_buf(),
        agents_pm_tool_lib::db::open_memory().unwrap(),
        Settings::default(),
    ));
    app.manage(AppState {
        core,
        server: Mutex::new(None),
        app_window: Mutex::new(GeometryTracker::default()),
    });
    app
}

#[test]
fn creates_a_window_pointing_at_the_local_server() {
    let app = mock_app();
    focus_or_create_app_window(app.handle(), 17890).unwrap();

    let window = app
        .get_webview_window("web")
        .expect("应创建 label 为 web 的应用窗口");
    assert_eq!(window.url().unwrap().as_str(), "http://127.0.0.1:17890/");
    // 标题由 builder 设置，mock runtime 不保存，故不断言（真机上是「Agents PM Tool」）。
}

#[test]
fn reuses_the_existing_window_instead_of_stacking_a_second_one() {
    let app = mock_app();
    for _ in 0..3 {
        focus_or_create_app_window(app.handle(), 17890).unwrap();
    }

    let web_windows = app
        .webview_windows()
        .keys()
        .filter(|label| label.as_str() == "web")
        .count();
    assert_eq!(web_windows, 1);
}

#[test]
fn follows_the_port_after_the_service_restarts() {
    let app = mock_app();
    focus_or_create_app_window(app.handle(), 17890).unwrap();
    focus_or_create_app_window(app.handle(), 17901).unwrap();

    let window = app.get_webview_window("web").unwrap();
    assert_eq!(window.url().unwrap().as_str(), "http://127.0.0.1:17901/");
}

#[test]
fn keeps_in_page_navigation_within_the_same_origin() {
    let app = mock_app();
    focus_or_create_app_window(app.handle(), 17890).unwrap();
    let window = app.get_webview_window("web").unwrap();
    window
        .navigate("http://127.0.0.1:17890/settings".parse().unwrap())
        .unwrap();

    // 同源：不该被拉回首页
    focus_or_create_app_window(app.handle(), 17890).unwrap();
    assert_eq!(
        window.url().unwrap().as_str(),
        "http://127.0.0.1:17890/settings",
    );
}

#[test]
fn reopens_with_the_geometry_saved_on_close() {
    let dir = tempfile::tempdir().unwrap();
    let saved = WindowGeometry {
        x: 140.0,
        y: 96.0,
        width: 1440.0,
        height: 900.0,
        maximized: false,
    };
    window_state::save(dir.path(), &saved).unwrap();

    let app = mock_app_with_data_dir(dir.path());
    focus_or_create_app_window(app.handle(), 17890).unwrap();

    // 保存的几何能正常走完还原流程（mock runtime 不保存窗口尺寸，真机行为另测）
    assert!(app.get_webview_window("web").is_some());
    // 且已按它预热：用户什么都没动就关窗，也能原样存回去
    let primed = app
        .state::<AppState>()
        .app_window
        .lock()
        .unwrap()
        .snapshot()
        .expect("按保存的几何打开后应已预热");
    assert_eq!((primed.x, primed.y), (140.0, 96.0));
    assert_eq!((primed.width, primed.height), (1440.0, 900.0));
    assert!(!primed.maximized);
}

#[test]
fn clamps_a_geometry_from_another_display_instead_of_failing() {
    let dir = tempfile::tempdir().unwrap();
    // 上次在副屏上、且比最小尺寸还小 —— 换到只有主屏的机器上也不能打不开
    window_state::save(
        dir.path(),
        &WindowGeometry {
            x: -9000.0,
            y: -9000.0,
            width: 320.0,
            height: 200.0,
            maximized: true,
        },
    )
    .unwrap();

    let app = mock_app_with_data_dir(dir.path());
    focus_or_create_app_window(app.handle(), 17890).unwrap();

    assert!(app.get_webview_window("web").is_some());
}
