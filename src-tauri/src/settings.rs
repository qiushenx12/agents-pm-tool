use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_PORT: u16 = 17890;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// HTTP 服务端口
    pub port: u16,
    /// 启动 app 后自动开启服务
    pub autostart: bool,
    /// 关窗行为：keep_service（保持服务）/ stop_all（一并停止）
    pub close_behavior: String,
    /// 监听范围：local（127.0.0.1，仅本机）/ lan（0.0.0.0，局域网可达）
    pub listen_scope: String,
    /// 远程 Agent 建议连接地址；空值表示自动探测局域网 IPv4。
    pub agent_server_url: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            autostart: true,
            close_behavior: "keep_service".into(),
            listen_scope: "local".into(),
            agent_server_url: String::new(),
        }
    }
}

impl Settings {
    /// 绑定地址（listen_scope=lan → 0.0.0.0，否则 127.0.0.1）
    pub fn bind_host(&self) -> [u8; 4] {
        if self.listen_scope == "lan" {
            [0, 0, 0, 0]
        } else {
            [127, 0, 0, 1]
        }
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let s = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_host_scope_mapping() {
        let mut s = Settings::default();
        assert_eq!(s.bind_host(), [127, 0, 0, 1], "默认 local");
        s.listen_scope = "lan".into();
        assert_eq!(s.bind_host(), [0, 0, 0, 0]);
        s.listen_scope = "local".into();
        assert_eq!(s.bind_host(), [127, 0, 0, 1]);
        // 未知值兜底回 local（安全默认）
        s.listen_scope = "garbage".into();
        assert_eq!(s.bind_host(), [127, 0, 0, 1]);
    }

    #[test]
    fn old_settings_json_defaults_to_local() {
        // 旧版 settings.json 没有 listen_scope 字段，serde(default) 应兜底为 local
        let s: Settings = serde_json::from_str(
            r#"{"port":17890,"autostart":true,"close_behavior":"keep_service"}"#,
        )
        .unwrap();
        assert_eq!(s.listen_scope, "local");
        assert_eq!(s.agent_server_url, "");
        assert_eq!(s.bind_host(), [127, 0, 0, 1]);
    }
}
