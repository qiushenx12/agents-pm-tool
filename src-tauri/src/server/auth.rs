use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::error::ApiError;
use crate::server::CoreState;

/// Agent token 校验中间层：Authorization: Bearer <token>（规划 §5.7）
pub async fn require_agent_token(
    State(core): State<CoreState>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = header.strip_prefix("Bearer ").unwrap_or("");

    let expected = core.token.read().await;
    if token.is_empty() || token != expected.as_str() {
        return Err(ApiError::unauthorized(
            "Agent token 缺失或不正确，请检查 data/runtime.json",
        ));
    }
    drop(expected);
    Ok(next.run(req).await)
}
