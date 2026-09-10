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

fn require_loopback(peer: SocketAddr) -> ApiResult<()> {
    if peer.ip().is_loopback() {
        Ok(())
    } else {
        Err(ApiError::forbidden("主机登录仅允许从本机访问"))
    }
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
) -> ApiResult<Response> {
    require_loopback(peer)?;
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
        assert!(require_loopback("127.0.0.1:1234".parse().unwrap()).is_ok());
        let error = require_loopback("192.168.1.20:1234".parse().unwrap()).unwrap_err();
        assert_eq!(error.status, StatusCode::FORBIDDEN);
    }
}
