use std::net::{IpAddr, SocketAddr};

use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, Method},
    middleware::Next,
    response::Response,
};

use crate::db::users;
use crate::domain::user::User;
use crate::error::ApiError;
use crate::server::CoreState;

pub const SESSION_COOKIE: &str = "pm_session";
pub const WEB_CLIENT_HEADER: &str = "x-pm-client";

/// 代理与隧道一定会加上的头；真正的本机直连不会带任何一个。
const FORWARDING_HEADERS: [&str; 5] = [
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "forwarded",
    "x-real-ip",
];

/// `Host` 是不是回环写法（本机浏览器直接访问就是 `127.0.0.1:端口` / `localhost:端口`）。
fn host_header_is_loopback(headers: &HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let host = host.trim();
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""),
        None => host.split(':').next().unwrap_or(""),
    };
    name.eq_ignore_ascii_case("localhost")
        || name
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

/// 判定「确实是本机直连」，供只允许主机在本机进行的操作使用（主机登录、工作区设置、本机 skill 安装）。
///
/// **只看对端 IP 是不够的**：隧道和反向代理（ngrok、Cloudflare Tunnel、Tailscale Funnel…）的
/// 代理进程就装在同一台机器上，它转发过来的连接对端同样是 127.0.0.1。实测把服务经 ngrok
/// 暴露出去后，从公网 `POST /api/web/auth/host-login` 能直接拿到主机（超级管理员）会话。
///
/// 所以三个条件必须同时成立：
/// 1. 对端是回环；
/// 2. `Host` 是回环写法；
/// 3. 请求里没有任何转发头。
pub fn require_direct_local(peer: SocketAddr, headers: &HeaderMap) -> Result<(), ApiError> {
    if !peer.ip().is_loopback() {
        return Err(ApiError::forbidden("仅允许从本机访问"));
    }
    if !host_header_is_loopback(headers)
        || FORWARDING_HEADERS
            .iter()
            .any(|name| headers.contains_key(*name))
    {
        return Err(ApiError::forbidden(
            "检测到请求来自隧道或反向代理，已拒绝：请在本机用 http://127.0.0.1:<端口> 直接访问",
        ));
    }
    Ok(())
}

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
