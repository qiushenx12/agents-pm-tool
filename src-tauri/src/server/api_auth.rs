use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Extension, Request, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::users,
    domain::user::{User, HOST_USER_ID},
    error::{ApiError, ApiResult},
    server::{auth, CoreState},
};

#[derive(Debug, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
struct SessionResponse {
    user: User,
}

fn session_cookie(token: &str) -> String {
    format!(
        "{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        auth::SESSION_COOKIE,
        users::SESSION_DAYS * 24 * 60 * 60
    )
}

fn clear_session_cookie() -> &'static str {
    "pm_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"
}

fn login_response(user: User, token: String, status: StatusCode) -> ApiResult<Response> {
    let mut response = (status, Json(SessionResponse { user })).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie(&token)).map_err(ApiError::internal)?,
    );
    Ok(response)
}

pub async fn register(
    State(core): State<CoreState>,
    Json(body): Json<Credentials>,
) -> ApiResult<Response> {
    let connection = core.db.lock().unwrap();
    let user = users::create(&connection, &body.username, &body.password)?;
    let token = users::create_session(&connection, &user.id)?;
    login_response(user, token, StatusCode::CREATED)
}

pub async fn login(
    State(core): State<CoreState>,
    Json(body): Json<Credentials>,
) -> ApiResult<Response> {
    let connection = core.db.lock().unwrap();
    let user = users::authenticate_password(&connection, &body.username, &body.password)?;
    let token = users::create_session(&connection, &user.id)?;
    login_response(user, token, StatusCode::OK)
}

pub async fn host_login(
    State(core): State<CoreState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Response> {
    auth::require_direct_local(peer, &headers)?;
    let connection = core.db.lock().unwrap();
    let user = users::get(&connection, HOST_USER_ID)?;
    let token = users::create_session(&connection, &user.id)?;
    login_response(user, token, StatusCode::OK)
}

pub async fn me(Extension(user): Extension<User>) -> Json<User> {
    Json(user)
}

pub async fn logout(State(core): State<CoreState>, request: Request) -> ApiResult<Response> {
    if let Some(token) = auth::session_token(&request) {
        let connection = core.db.lock().unwrap();
        users::delete_session(&connection, token)?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(clear_session_cookie()),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cookie_is_http_only_and_lax() {
        let cookie = session_cookie("secret");
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Max-Age=604800"));
    }

    #[test]
    fn host_login_rejects_non_loopback_peer() {
        let headers = request_headers("127.0.0.1:17890");
        assert!(auth::require_direct_local("127.0.0.1:1234".parse().unwrap(), &headers).is_ok());
        let error =
            auth::require_direct_local("192.168.1.20:1234".parse().unwrap(), &headers).unwrap_err();
        assert_eq!(error.status, StatusCode::FORBIDDEN);
    }

    fn request_headers(host: &str) -> axum::http::HeaderMap {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::HOST, host.parse().unwrap());
        headers
    }

    #[test]
    fn host_login_rejects_tunnelled_requests_from_loopback() {
        // 隧道/反代的代理进程就在本机，对端也是回环：只看 IP 会把主机身份放到公网上。
        let peer: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        // ① Host 是公网域名（ngrok 这类隧道会校验 Host，攻击者也塞不进去）
        assert!(auth::require_direct_local(peer, &request_headers("abc.ngrok-free.dev")).is_err());
        // ② 带转发头（客户端自己也能加，加了只会更容易被拒）
        for name in ["x-forwarded-for", "x-forwarded-host", "x-forwarded-proto", "forwarded"] {
            let mut headers = request_headers("127.0.0.1:17890");
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                "1.2.3.4".parse().unwrap(),
            );
            assert!(
                auth::require_direct_local(peer, &headers).is_err(),
                "{name} 应该让请求被判为非本机直连"
            );
        }
        // ③ 缺 Host 也当不满足（HTTP/1.1 必须有 Host）
        assert!(auth::require_direct_local(peer, &axum::http::HeaderMap::new()).is_err());
        // 回环 Host 的几种写法都认
        for host in ["127.0.0.1:17890", "localhost:17890", "[::1]:17890"] {
            assert!(
                auth::require_direct_local(peer, &request_headers(host)).is_ok(),
                "{host} 应被认作本机直连"
            );
        }
    }
}
