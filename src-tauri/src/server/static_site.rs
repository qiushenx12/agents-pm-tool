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

/// 静态资源的缓存策略。
///
/// - `assets/` 下是 Vite 产物，文件名带内容哈希，改名即换内容，可以长期缓存；
/// - 其余（`index.html` 与 SPA 回退）必须每次都重新取：它引用的是哈希文件名，
///   一旦被缓存，重新安装后浏览器还会拿着旧壳子去要已被替换掉的老资源，
///   表现就是「明明重新打包了，界面还是旧的」。
fn cache_control_for(path: &str) -> &'static str {
    if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

fn serve_asset(path: &str) -> Response {
    match DistAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::CACHE_CONTROL, cache_control_for(path))
                .body(Body::from(content.data.into_owned()))
                .unwrap()
        }
        None => (StatusCode::NOT_FOUND, "页面不存在").into_response(),
    }
}

/// SPA：/assets 等真实文件直接返回，其余路径回退 index.html
///
/// `/api/*` 是例外：这类路径要么由具体 handler 接住，要么就是**已删除或拼错的接口**。
/// 回退成 index.html 会让调用方拿到 200 + HTML（旧版 Agent 会以为调用成功，然后解析 JSON 失败），
/// 所以这里明确给一个 JSON 404。
pub async fn index(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if !path.is_empty() && DistAssets::get(path).is_some() {
        return serve_asset(path);
    }
    if path == "api" || path.starts_with("api/") {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": {
                    "code": "not_found",
                    "message": "接口不存在",
                    "details": {},
                }
            })),
        )
            .into_response();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spa_shell_is_never_cached_but_hashed_assets_are() {
        assert_eq!(cache_control_for("index.html"), "no-cache");
        assert_eq!(cache_control_for("config.html"), "no-cache");
        assert_eq!(
            cache_control_for("assets/index-abc12345.js"),
            "public, max-age=31536000, immutable"
        );
        // 兜底：整段缓存策略里必须有一条禁止缓存 HTML 壳子的分支。
        assert!(!cache_control_for("index.html").contains("max-age"));
    }

    #[tokio::test]
    async fn unknown_api_paths_are_json_404_instead_of_the_spa_shell() {
        // 已删除/拼错的接口必须明确报错：回退成 index.html 会让旧版调用方
        // 拿到 200 + HTML，误以为成功。
        let response = index("/api/agent/skill/download".parse().unwrap()).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default(),
            "application/json"
        );

        let response = index("/api".parse().unwrap()).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // 非接口路径仍然是 SPA 回退
        let response = index("/tasks/202609171230110000".parse().unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn embedded_shell_references_hashed_assets() {
        let html = DistAssets::get("index.html").expect("dist/index.html 应已嵌入");
        let text = String::from_utf8_lossy(&html.data).to_string();
        assert!(text.contains("/assets/"), "index.html 应引用 /assets/ 下的哈希资源");
        assert!(
            cache_control_for("assets/anything").contains("immutable"),
            "哈希资源应可长期缓存"
        );
    }
}
