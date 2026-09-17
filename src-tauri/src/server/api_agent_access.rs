use axum::{
    extract::{Extension, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use std::net::IpAddr;

use crate::{
    db::users,
    domain::user::{User, HOST_USER_ID},
    error::ApiResult,
    netinfo,
    server::{self, CoreState},
};

#[derive(Debug, Serialize)]
pub struct AgentAccess {
    pub server_url: String,
    pub token: Option<String>,
    pub access_instructions: String,
}

/// 拆出 Host 里的地址部分与显式端口：`192.168.1.20:17890`、`[::1]:17890`、`box.local`。
fn split_host(raw: &str) -> Option<(&str, Option<u16>)> {
    if let Some(rest) = raw.strip_prefix('[') {
        let (ip, tail) = rest.split_once(']')?;
        let port = match tail.strip_prefix(':') {
            Some(port) => Some(port.parse().ok()?),
            None => None,
        };
        Some((ip, port))
    } else if let Some((host, port)) = raw.split_once(':') {
        Some((host, Some(port.parse().ok()?)))
    } else {
        Some((raw, None))
    }
}

/// 取首个逗号分隔的值：反代链路里这类头可能被追加成 `a, b`。
fn first_header_value<'a>(
    headers: &'a HeaderMap,
    name: impl axum::http::header::AsHeaderName,
) -> Option<&'a str> {
    headers
        .get(name)?
        .to_str()
        .ok()?
        .split(',')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// 客户端的访问协议。
///
/// 隧道与反向代理（ngrok、Cloudflare Tunnel、Tailscale Funnel…）对外是 https，
/// 转发到本机是明文 http，原始协议只在 `X-Forwarded-Proto` 里。不看这个头，
/// 就会把 `https://域名` 报成 `http://域名`，照着填的 Agent 直接连不上。
fn scheme_from_request(headers: &HeaderMap) -> &'static str {
    match first_header_value(headers, "x-forwarded-proto") {
        Some(value) if value.eq_ignore_ascii_case("https") => "https",
        _ => "http",
    }
}

/// 只放行 URL 里不会变味的字符。挡掉 `/ ? # @ 空格` 这些——
/// 它们会让拼出来的地址指向别处，或者把后面的端口、路径吃掉。
fn is_hostname(host: &str) -> bool {
    host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

/// 调用方实际用来访问本服务的地址。
///
/// 客户端用哪个地址连上来就回哪个：局域网访问的拿到局域网地址，走 Tailscale 的拿到
/// Tailscale 地址，经域名或隧道访问的拿到那个域名——比在服务端猜一块网卡准确得多。
///
/// 地址按调用方给的写法原样保留：带端口就带端口，不带端口就不带
/// （隧道/反代对外的 443 本来就不写端口，拿本地端口去补只会得到一个连不上的地址）。
///
/// 反代/隧道可能把 Host 改写成 `localhost:端口`，原始域名留在 `X-Forwarded-Host` 里，
/// 所以那个头优先；协议同理看 `X-Forwarded-Proto`。
fn address_from_request(headers: &HeaderMap) -> Option<String> {
    let raw = first_header_value(headers, "x-forwarded-host")
        .or_else(|| first_header_value(headers, header::HOST))?;
    let (host, explicit_port) = split_host(raw)?;
    let host = host.trim_end_matches('.');
    if host.is_empty() {
        return None;
    }
    let address = match host.parse::<IpAddr>() {
        Ok(ip) => {
            // 0.0.0.0 / :: 是「监听全部」的写法，不是能连接过去的地址
            if ip.is_unspecified() {
                return None;
            }
            match ip {
                IpAddr::V4(ip) => ip.to_string(),
                IpAddr::V6(ip) => format!("[{ip}]"),
            }
        }
        Err(_) => {
            if !is_hostname(host) {
                return None;
            }
            host.to_string()
        }
    };
    let authority = match explicit_port {
        // 端口 0 是无效写法（split_host 会把非法端口解析成 None 之外的 0）
        Some(0) => return None,
        Some(port) => format!("{address}:{port}"),
        None => address,
    };
    Some(format!("{}://{authority}", scheme_from_request(headers)))
}

fn server_url(core: &CoreState, user: &User, headers: &HeaderMap) -> String {
    // 主机账号的 Agent 一定运行在本机，loopback 永远可达，也不受客户端来源影响。
    if user.id == HOST_USER_ID {
        let port = *core.actual_port.read().unwrap();
        return format!("http://127.0.0.1:{port}");
    }
    suggested_server_url(core, headers)
}

/// 建议给调用方的服务地址，按可靠程度排序：
///
/// 1. 用户手工配置的 `agent_server_url`——显式指定就是压过一切自动判断，
///    留给「Agent 不在浏览页面这台机器上」这类自动识别猜不到的情况；
/// 2. 请求自己带的路由信息——客户端拿哪个地址连上来就给哪个：局域网访问的给局域网地址，
///    走 Tailscale 的给 Tailscale 地址，经域名/隧道访问的给那个域名（按转发头还原 https）；
/// 3. 按监听范围自动探测。
pub(crate) fn suggested_server_url(core: &CoreState, headers: &HeaderMap) -> String {
    let port = *core.actual_port.read().unwrap();
    let settings = core.settings.read().unwrap().clone();
    if settings.listen_scope != "lan" {
        // 只监听回环：除了本机没别的路可走，也就没必要看请求头。
        return format!("http://127.0.0.1:{port}");
    }
    let configured = settings.agent_server_url;
    if !configured.trim().is_empty() {
        return configured.trim_end_matches('/').to_string();
    }
    if let Some(url) = address_from_request(headers) {
        return url;
    }
    netinfo::lan_ipv4()
        .map(|ip| format!("http://{ip}:{port}"))
        .unwrap_or_else(|| format!("http://127.0.0.1:{port}"))
}

pub async fn get_access(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    headers: HeaderMap,
) -> ApiResult<Json<AgentAccess>> {
    let token = {
        let connection = core.db.lock().unwrap();
        users::active_agent_token(&connection, &user.id)?
    };
    let server_url = server_url(&core, &user, &headers);
    let access_instructions = if user.id == HOST_USER_ID {
        "Agents PM Tool 是本地任务管理工具，Agent 通过受限客户端 pm-cli 读取和推进任务。\
         请先确保桌面应用正在运行，然后在下面把 pm-cli skill 安装到本机的 Agent 前端\
         （需要 Node.js 18 或更高版本，未检测到的前端需先安装该前端）。\
         本机安装后不需要任何连接配置：应用会把端口与 token 写到你电脑上固定的一处位置，\
         pm-cli 自动读取。安装或升级后请重新打开终端或 Agent 前端。"
            .to_string()
    } else {
        format!(
            "Agents PM Tool 位于远程主机，Agent 通过受限客户端 pm-cli 读取和推进任务。\
             pm-cli 是 pm-cli-skill 里的一个 Node 脚本（需要 Node.js 18 或更高版本）：\
             在下面选择目标的前端 skill 目录直接写入，或下载安装脚本在 Agent 所在电脑上运行一次。\
             远程环境不会自动读取端口与 token，需要手动配置一次：\
             设置 PM_SERVER_URL={server_url} 与网页中签发的 PM_AGENT_TOKEN，\
             或运行 pm-cli config set server-url {server_url} 和 pm-cli config set token <token>，\
             再用 pm-cli doctor 确认连通。\
             （上面的服务地址按你当前的访问方式给出：走局域网就显示局域网地址，\
             走 Tailscale 就显示 Tailscale 地址，经域名或隧道访问就显示那个地址，\
             换一种方式访问本页即可拿到另一个。）"
        )
    };
    Ok(Json(AgentAccess {
        server_url,
        token,
        access_instructions,
    }))
}

pub async fn regenerate_token(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    headers: HeaderMap,
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
    let server_url = server_url(&core, &user, &headers);
    Ok(Json(AgentAccess {
        access_instructions: if user.id == HOST_USER_ID {
            "本机 pm-cli 将自动读取更新后的 token。".into()
        } else {
            format!("请将 PM_SERVER_URL 设为 {server_url}，并将 PM_AGENT_TOKEN 更新为新 token。")
        },
        server_url,
        token: Some(token),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            let name: axum::http::HeaderName = name.parse().unwrap();
            headers.insert(name, value.parse().unwrap());
        }
        headers
    }

    fn host(value: &str) -> HeaderMap {
        headers(&[("host", value)])
    }

    #[test]
    fn keeps_the_route_the_caller_actually_used() {
        // 局域网进来的给局域网地址
        assert_eq!(
            address_from_request(&host("192.168.1.20:17890")),
            Some("http://192.168.1.20:17890".into())
        );
        // tailnet 进来的给 Tailscale 地址
        assert_eq!(
            address_from_request(&host("100.101.102.103:17890")),
            Some("http://100.101.102.103:17890".into())
        );
        // 端口映射场景照抄对方用的端口
        assert_eq!(
            address_from_request(&host("192.168.1.20:8080")),
            Some("http://192.168.1.20:8080".into())
        );
        assert_eq!(
            address_from_request(&host("localhost:17890")),
            Some("http://localhost:17890".into())
        );
        assert_eq!(
            address_from_request(&host("127.0.0.1:17890")),
            Some("http://127.0.0.1:17890".into())
        );
    }

    #[test]
    fn supports_domains_and_restores_the_forwarded_scheme() {
        // 经域名访问（ngrok / Cloudflare Tunnel / 自建反代…）：域名原样给回
        assert_eq!(
            address_from_request(&host("a-b-c.ngrok-free.dev:17890")),
            Some("http://a-b-c.ngrok-free.dev:17890".into())
        );
        // 隧道对外是 https、转发到本机是 http，必须按 X-Forwarded-Proto 还原
        assert_eq!(
            address_from_request(&headers(&[
                ("host", "a-b-c.ngrok-free.dev"),
                ("x-forwarded-proto", "https"),
            ])),
            // 不带端口就不补：对外就是 443，补上本地端口只会连不上
            Some("https://a-b-c.ngrok-free.dev".into())
        );
        assert_eq!(
            address_from_request(&headers(&[
                ("host", "a-b-c.ngrok-free.dev:443"),
                ("x-forwarded-proto", "https"),
            ])),
            Some("https://a-b-c.ngrok-free.dev:443".into())
        );
        // 未知协议取值不认，回落 http
        assert_eq!(
            address_from_request(&headers(&[
                ("host", "pm.example.com:17890"),
                ("x-forwarded-proto", "gopher"),
            ])),
            Some("http://pm.example.com:17890".into())
        );
    }

    #[test]
    fn prefers_the_host_the_proxy_preserved() {
        // 反代可能把 Host 改写成 localhost:端口，原始域名留在 X-Forwarded-Host
        assert_eq!(
            address_from_request(&headers(&[
                ("host", "localhost:3010"),
                ("x-forwarded-host", "a-b-c.ngrok-free.dev"),
                ("x-forwarded-proto", "https"),
            ])),
            Some("https://a-b-c.ngrok-free.dev".into())
        );
        // 逗号分隔的链路取第一个
        assert_eq!(
            address_from_request(&headers(&[
                ("host", "127.0.0.1:3010"),
                ("x-forwarded-host", "a.example.com, b.example.com"),
            ])),
            Some("http://a.example.com".into())
        );
    }

    #[test]
    fn rejects_hosts_that_cannot_be_used_as_an_address() {
        assert_eq!(address_from_request(&HeaderMap::new()), None);
        // 端口写坏
        assert_eq!(address_from_request(&host("192.168.1.20:abc")), None);
        assert_eq!(address_from_request(&host("192.168.1.20:0")), None);
        // 监听全部用的写法，不是能连过去的地址
        assert_eq!(address_from_request(&host("0.0.0.0:17890")), None);
        assert_eq!(address_from_request(&host("[::]:17890")), None);
        // 会让拼出来的地址跑偏的字符
        for bad in [
            "pm.example.com/path",
            "pm.example.com?x=1",
            "user@pm.example.com",
            "pm example.com",
            "-pm.example.com",
            "pm..example.com",
        ] {
            assert_eq!(
                address_from_request(&host(bad)),
                None,
                "{bad} 不该被当成可用地址"
            );
        }
    }
}
