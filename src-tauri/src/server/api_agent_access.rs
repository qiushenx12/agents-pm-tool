use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::{
    db::users,
    domain::user::{User, HOST_USER_ID},
    error::ApiResult,
    server::{self, CoreState},
};

#[derive(Debug, Serialize)]
pub struct AgentAccess {
    pub server_url: String,
    pub token: Option<String>,
    pub access_instructions: String,
    pub skill_ready: bool,
}

fn lan_ipv4() -> Option<std::net::Ipv4Addr> {
    let socket = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("8.8.8.8", 80)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

fn server_url(core: &CoreState, user: &User) -> String {
    let port = *core.actual_port.read().unwrap();
    let settings = core.settings.read().unwrap().clone();
    if user.id == HOST_USER_ID || settings.listen_scope != "lan" {
        return format!("http://127.0.0.1:{port}");
    }
    let configured = settings.agent_server_url;
    if !configured.trim().is_empty() {
        return configured.trim_end_matches('/').to_string();
    }
    lan_ipv4()
        .map(|ip| format!("http://{ip}:{port}"))
        .unwrap_or_else(|| format!("http://127.0.0.1:{port}"))
}

pub async fn get_access(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
) -> ApiResult<Json<AgentAccess>> {
    let token = {
        let connection = core.db.lock().unwrap();
        users::active_agent_token(&connection, &user.id)?
    };
    let server_url = server_url(&core, &user);
    let access_instructions = if user.id == HOST_USER_ID {
        "Agents PM Tool 是本地任务管理工具，Agent 通过受限客户端 pm-cli 读取和推进任务。请先确保桌面应用正在运行；安装版会注册 pm-cli 到用户 PATH，并自动读取实际端口和临时 token，无需手动配置。安装或升级后需重新打开终端/Agent 前端。".to_string()
    } else {
        format!(
            "Agents PM Tool 位于远程主机。请设置 PM_SERVER_URL={server_url} 与网页中签发的 PM_AGENT_TOKEN，或运行 pm-cli config set server-url {server_url} 和 pm-cli config set token <token>。"
        )
    };
    Ok(Json(AgentAccess {
        server_url,
        token,
        access_instructions,
        skill_ready: user.id == HOST_USER_ID && super::api_skill::local_skill_ready(),
    }))
}

pub async fn regenerate_token(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
) -> ApiResult<Json<AgentAccess>> {
    let token = {
        let connection = core.db.lock().unwrap();
        users::regenerate_agent_token(&connection, &user.id)?
    };
    if user.id == HOST_USER_ID {
        *core.token.write().await = token.clone();
        let port = *core.actual_port.read().unwrap();
        server::write_runtime_json(&core, port).await?;
    }
    let server_url = server_url(&core, &user);
    Ok(Json(AgentAccess {
        access_instructions: if user.id == HOST_USER_ID {
            "本机 pm-cli 将自动读取更新后的 token。".into()
        } else {
            format!("请将 PM_SERVER_URL 设为 {server_url}，并将 PM_AGENT_TOKEN 更新为新 token。")
        },
        server_url,
        token: Some(token),
        skill_ready: user.id == HOST_USER_ID && super::api_skill::local_skill_ready(),
    }))
}

pub async fn revoke_token(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
) -> ApiResult<impl IntoResponse> {
    let connection = core.db.lock().unwrap();
    users::revoke_agent_tokens(&connection, &user.id)?;
    Ok(StatusCode::NO_CONTENT)
}
