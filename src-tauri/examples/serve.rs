//! 开发用无头服务：不开 Tauri 窗口，直接拉起 axum 服务，便于联调 pm-cli。
//! 用法：cargo run --example serve  （可用 PM_DATA_DIR 指定数据目录）

use std::sync::Arc;

use agents_pm_tool_lib::{db, paths, server, settings::Settings};

#[tokio::main]
async fn main() {
    let data_dir = paths::data_dir().expect("数据目录初始化失败");
    let conn = db::open(&paths::db_path(&data_dir)).expect("数据库初始化失败");
    let settings = Settings::load(&paths::settings_path(&data_dir));
    let port = settings.port;
    let bind_host = settings.bind_host();
    let core = Arc::new(server::CoreStateInner::new(data_dir, conn, settings));
    let handle = server::start_server(core, port, bind_host)
        .await
        .expect("服务启动失败");
    let scope = if bind_host == [0, 0, 0, 0] {
        "0.0.0.0（局域网可达）"
    } else {
        "127.0.0.1（仅本机）"
    };
    println!(
        "服务已启动，监听 {}:{} {}（Ctrl+C 停止）",
        std::net::Ipv4Addr::from(bind_host),
        handle.port,
        scope
    );
    // 挂起直到 Ctrl+C
    tokio::signal::ctrl_c().await.ok();
    handle.stop().await;
}
