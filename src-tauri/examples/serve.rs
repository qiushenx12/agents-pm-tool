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
    let core = Arc::new(server::CoreStateInner::new(data_dir, conn, settings));
    let handle = server::start_server(core, port).await.expect("服务启动失败");
    println!("服务已启动 http://127.0.0.1:{}（Ctrl+C 停止）", handle.port);
    // 挂起直到 Ctrl+C
    tokio::signal::ctrl_c().await.ok();
    handle.stop().await;
}
