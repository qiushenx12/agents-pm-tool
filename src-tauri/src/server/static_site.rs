use axum::{
    body::Body,
    extract::Path,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

/// 把前端 dist/ 打进 exe，单文件分发（规划 §2）
#[derive(RustEmbed)]
#[folder = "../dist/"]
struct DistAssets;

fn serve_asset(path: &str) -> Response {
    match DistAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.into_owned()))
                .unwrap()
        }
        None => (StatusCode::NOT_FOUND, "页面不存在").into_response(),
    }
}

/// SPA：/assets 等真实文件直接返回，其余路径回退 index.html
pub async fn index(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if !path.is_empty() && DistAssets::get(path).is_some() {
        return serve_asset(path);
    }
    serve_asset("index.html")
}

pub async fn asset(Path(path): Path<String>) -> Response {
    // {*path} 通配符只捕获 /assets/ 之后的部分；rust-embed 的 key 带 assets/ 前缀，需补回
    serve_asset(&format!("assets/{path}"))
}

/// 调试用：列出嵌入的全部文件
pub fn debug_list_files() -> Vec<String> {
    DistAssets::iter().map(|c| c.to_string()).collect()
}
