use axum::{
    extract::{Request, State},
    http::{header, Method},
    middleware::Next,
    response::Response,
};

use crate::db::users;
use crate::domain::user::User;
use crate::error::ApiError;
use crate::server::CoreState;

pub const SESSION_COOKIE: &str = "pm_session";
pub const WEB_CLIENT_HEADER: &str = "x-pm-client";

fn cookie_value<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|item| {
                let (key, value) = item.trim().split_once('=')?;
                (key == name).then_some(value)
            })
        })
}

/// Cookie 会话配合自定义头阻止跨站表单、图片等发起写请求。
pub async fn require_web_client_header(req: Request, next: Next) -> Result<Response, ApiError> {
    if !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS)
        && req
            .headers()
            .get(WEB_CLIENT_HEADER)
            .and_then(|value| value.to_str().ok())
            != Some("web")
    {
        return Err(ApiError::forbidden(
            "写请求缺少 X-PM-Client: web，请刷新页面后重试",
        ));
    }
    Ok(next.run(req).await)
}

pub async fn require_web_auth(
    State(core): State<CoreState>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let token = cookie_value(&req, SESSION_COOKIE)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| ApiError::unauthorized("请先登录"))?
        .to_string();
    let user = {
        let connection = core.db.lock().unwrap();
        users::authenticate_session(&connection, &token)?
    };
    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

pub fn authenticated_user(request: &Request) -> Result<&User, ApiError> {
    request
        .extensions()
        .get::<User>()
        .ok_or_else(|| ApiError::unauthorized("请先登录"))
}

pub fn session_token(request: &Request) -> Option<&str> {
    cookie_value(request, SESSION_COOKIE)
}

/// Agent token 校验中间层：Authorization: Bearer <token>（规划 §5.7）
pub async fn require_agent_token(
    State(core): State<CoreState>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = header.strip_prefix("Bearer ").unwrap_or("");

    if token.is_empty() {
        return Err(ApiError::unauthorized("Agent token 缺失"));
    }
    let user = {
        let connection = core.db.lock().unwrap();
        users::authenticate_agent_token(&connection, token)?
    };
    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}
