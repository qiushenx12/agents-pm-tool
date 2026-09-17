//! 本机网络地址探测。
//!
//! 桌面设置窗口、网页主机设置、Agent 接入说明都要回答「这部机器可以从哪里访问」，
//! 此前每个调用点各自复制了一份 `lan_ipv4`；集中到这里，避免继续分叉出更多副本。

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 本机对外局域网 IPv4。
///
/// UDP connect 并不真的发包，只是让内核按路由表选出一块对外网卡，再读它的地址；
/// 比枚举网卡更贴近「别人能不能连到我」这个问题的答案。
pub fn lan_ipv4() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("8.8.8.8", 80)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

/// Tailscale（以及自建 Headscale 等同一方案的组网）固定从 100.64.0.0/10 分配 IPv4。
///
/// 用网段校验而不是「命令成功就算数」：命令输出被别的提示信息污染时，
/// 也不至于把一个无关地址摆到界面上。
fn is_tailnet_ipv4(ip: Ipv4Addr) -> bool {
    let [first, second, ..] = ip.octets();
    first == 100 && (64..128).contains(&second)
}

const TAILSCALE_CLI: &str = "tailscale";

/// `tailscale` 命令行可能不在 PATH 上（Windows 由安装器写入系统 PATH；
/// macOS 的图形版把它放在 app 包里，GUI 启动的应用看不到 Homebrew 路径）。
fn tailscale_commands() -> Vec<PathBuf> {
    let mut commands = vec![PathBuf::from(TAILSCALE_CLI)];
    #[cfg(target_os = "windows")]
    commands.push(PathBuf::from(r"C:\Program Files\Tailscale\tailscale.exe"));
    #[cfg(target_os = "macos")]
    {
        commands.push(PathBuf::from(
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        ));
        commands.push(PathBuf::from("/opt/homebrew/bin/tailscale"));
        commands.push(PathBuf::from("/usr/local/bin/tailscale"));
    }
    commands
}

/// 从 `tailscale ip -4` 的输出里取第一个合法地址。
fn parse_first_ipv4(stdout: &str) -> Option<Ipv4Addr> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find_map(|line| line.parse::<Ipv4Addr>().ok())
        .filter(|ip| is_tailnet_ipv4(*ip))
}

fn probe(command: &Path) -> Option<Ipv4Addr> {
    let output = std::process::Command::new(command)
        .args(["ip", "-4"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_first_ipv4(&String::from_utf8_lossy(&output.stdout))
}

/// 探测要起一个子进程，而状态接口每次打开设置面板都会被调用，缓存一下免得反复起进程。
const CACHE_TTL: Duration = Duration::from_secs(10);

fn cache() -> &'static Mutex<Option<(Instant, Option<Ipv4Addr>)>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, Option<Ipv4Addr>)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// 本机的 Tailscale IPv4；没装、没登录或服务没起来时返回 None。
///
/// 判据是「现在有没有一个能用的 Tailscale 地址」，而不是「有没有装」——
/// 装了但没上线时给出一行连不通的地址，比不显示更误导。
pub fn tailscale_ipv4() -> Option<Ipv4Addr> {
    if let Ok(guard) = cache().lock() {
        if let Some((at, cached)) = *guard {
            if at.elapsed() < CACHE_TTL {
                return cached;
            }
        }
    }
    let probed = tailscale_commands()
        .iter()
        .find_map(|command| probe(command));
    if let Ok(mut guard) = cache().lock() {
        *guard = Some((Instant::now(), probed));
    }
    probed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailnet_range_matches_100_64_slash_10_only() {
        assert!(!is_tailnet_ipv4("100.63.255.255".parse().unwrap()));
        assert!(is_tailnet_ipv4("100.64.0.0".parse().unwrap()));
        assert!(is_tailnet_ipv4("100.101.102.103".parse().unwrap()));
        assert!(is_tailnet_ipv4("100.127.255.255".parse().unwrap()));
        assert!(!is_tailnet_ipv4("100.128.0.0".parse().unwrap()));
        // 常见局域网地址不能被误认成 Tailscale
        assert!(!is_tailnet_ipv4("192.168.1.20".parse().unwrap()));
        assert!(!is_tailnet_ipv4("10.0.0.5".parse().unwrap()));
    }

    #[test]
    fn parse_takes_first_address_and_skips_noise() {
        assert_eq!(
            parse_first_ipv4("100.101.102.103\n"),
            Some("100.101.102.103".parse().unwrap())
        );
        // 前面可能有提示行，也可能同时输出 IPv6
        assert_eq!(
            parse_first_ipv4("# tailscale is not running\n100.64.0.7\nfd7a::1\n"),
            Some("100.64.0.7".parse().unwrap())
        );
        assert_eq!(parse_first_ipv4("   \n\n"), None);
        assert_eq!(parse_first_ipv4("not an address"), None);
        // 地址合法但不在 Tailscale 网段：宁可不显示，也不给错地址
        assert_eq!(parse_first_ipv4("192.168.1.20\n"), None);
    }

    #[test]
    fn probe_missing_command_is_none() {
        let missing = std::env::temp_dir().join("pm-netinfo-definitely-missing-cli");
        assert_eq!(probe(&missing), None);
    }

    #[test]
    fn lan_ipv4_never_panics() {
        // 具体地址取决于运行环境（CI/沙箱里可能没有对外网卡），这里只要求不 panic。
        let _ = lan_ipv4();
    }
}
