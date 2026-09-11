use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use axum::{
    extract::{ConnectInfo, Extension, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::user::{User, HOST_USER_ID},
    error::{ApiError, ApiResult},
    paths,
    settings::Settings,
};

use super::CoreState;

#[derive(Debug, Serialize)]
pub struct ServerStatus {
    pub running: bool,
    pub port: u16,
    pub url: String,
    pub lan_url: String,
    pub data_dir: String,
}

#[derive(Debug, Serialize)]
pub struct HostSettingsResponse {
    pub settings: Settings,
    pub status: ServerStatus,
}

#[derive(Debug, Serialize)]
pub struct SaveHostSettingsResponse {
    pub settings: Settings,
    pub status: ServerStatus,
    pub restarted: bool,
    /// 重启后优先尝试的端口；如被占用，仍可能按既有规则向后顺延。
    pub port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveHostSettingsBody {
    pub port: u16,
    pub autostart: bool,
    pub close_behavior: String,
    pub listen_scope: String,
    pub agent_server_url: String,
}

fn ensure_local_host(user: &User, peer: SocketAddr) -> ApiResult<()> {
    if user.id != HOST_USER_ID || !peer.ip().is_loopback() {
        return Err(ApiError::forbidden("工作区设置仅允许主机账号从本机操作"));
    }
    Ok(())
}

fn lan_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("8.8.8.8", 80)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

fn server_status(core: &CoreState) -> ServerStatus {
    let port = *core.actual_port.read().unwrap();
    let running = port != 0;
    let is_lan = core.settings.read().unwrap().bind_host() == [0, 0, 0, 0];
    ServerStatus {
        running,
        port,
        url: running
            .then(|| format!("http://127.0.0.1:{port}"))
            .unwrap_or_default(),
        lan_url: if running && is_lan {
            lan_ipv4()
                .map(|ip| format!("http://{ip}:{port}"))
                .unwrap_or_default()
        } else {
            String::new()
        },
        data_dir: core.data_dir.display().to_string(),
    }
}

pub async fn get_host_settings(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> ApiResult<Json<HostSettingsResponse>> {
    ensure_local_host(&user, peer)?;
    let settings = core.settings.read().unwrap().clone();
    Ok(Json(HostSettingsResponse {
        settings,
        status: server_status(&core),
    }))
}

pub async fn save_host_settings(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<SaveHostSettingsBody>,
) -> ApiResult<Json<SaveHostSettingsResponse>> {
    ensure_local_host(&user, peer)?;
    let old = core.settings.read().unwrap().clone();
    let next = Settings {
        port: body.port,
        autostart: body.autostart,
        close_behavior: body.close_behavior,
        listen_scope: body.listen_scope,
        agent_server_url: body.agent_server_url.trim().to_string(),
        // 主题有独立写入口，保存表单不能用陈旧值覆盖它。
        theme: old.theme.clone(),
    };
    next.validate().map_err(ApiError::unprocessable)?;
    next.save(&paths::settings_path(&core.data_dir))?;
    *core.settings.write().unwrap() = next.clone();

    let restarted = next.port != old.port || next.bind_host() != old.bind_host();
    if restarted {
        core.settings_restart_events
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    Ok(Json(SaveHostSettingsResponse {
        port: next.port,
        settings: next,
        status: server_status(&core),
        restarted,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: &str) -> User {
        User {
            id: id.into(),
            username: "测试".into(),
            role: "super_admin".into(),
            created_at: String::new(),
            disabled: false,
            is_host: id == HOST_USER_ID,
        }
    }

    #[test]
    fn only_loopback_host_can_manage_settings() {
        assert!(ensure_local_host(&user(HOST_USER_ID), "127.0.0.1:1".parse().unwrap()).is_ok());
        assert!(ensure_local_host(&user("admin"), "127.0.0.1:1".parse().unwrap()).is_err());
        assert!(ensure_local_host(&user(HOST_USER_ID), "192.168.1.20:1".parse().unwrap()).is_err());
    }
}
