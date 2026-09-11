use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_PORT: u16 = 17890;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    /// 全局明暗主题（"light"/"dark"），设置窗口与网页界面共用一份；
    /// None 表示从未显式设置过，各界面跟随各自系统偏好。
    pub theme: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            autostart: true,
            close_behavior: "keep_service".into(),
            listen_scope: "local".into(),
            agent_server_url: String::new(),
            theme: None,
        }
    }
}

/// 主题合法取值（供共享写入口与 API 校验复用）
pub const THEMES: [&str; 2] = ["light", "dark"];

impl Settings {
    /// 设置页与网页主机设置入口共用的校验规则。
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.port < 1024 {
            return Err("服务端口必须在 1024–65535 之间");
        }
        if !["keep_service", "stop_all"].contains(&self.close_behavior.as_str()) {
            return Err("关闭窗口行为无效");
        }
        if !["local", "lan"].contains(&self.listen_scope.as_str()) {
            return Err("访问范围只能是 local 或 lan");
        }
        let address = self.agent_server_url.trim();
        if !address.is_empty()
            && !(address.starts_with("http://") || address.starts_with("https://"))
        {
            return Err("远程 Agent 服务地址必须以 http:// 或 https:// 开头");
        }
        Ok(())
    }

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

    #[test]
    fn old_settings_json_without_theme_parses_as_none() {
        // 主题是后加的字段：旧 settings.json 解析后应为 None（跟随系统），而不是报错
        let s: Settings = serde_json::from_str(
            r#"{"port":17890,"autostart":true,"close_behavior":"keep_service","listen_scope":"local","agent_server_url":""}"#,
        )
        .unwrap();
        assert_eq!(s.theme, None);
    }

    #[test]
    fn theme_roundtrip_and_unknown_value_kept_as_is() {
        let s: Settings = serde_json::from_str(
            r#"{"port":17890,"autostart":true,"close_behavior":"keep_service","listen_scope":"local","agent_server_url":"","theme":"dark"}"#,
        )
        .unwrap();
        assert_eq!(s.theme.as_deref(), Some("dark"));
        // 写入再读出，theme 不丢
        let path = std::env::temp_dir().join(format!(
            "pm-settings-roundtrip-{}.json",
            std::process::id()
        ));
        s.save(&path).unwrap();
        let reloaded = Settings::load(&path);
        assert_eq!(reloaded.theme.as_deref(), Some("dark"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn validates_user_editable_settings() {
        assert!(Settings::default().validate().is_ok());

        let mut invalid = Settings::default();
        invalid.port = 80;
        assert_eq!(invalid.validate(), Err("服务端口必须在 1024–65535 之间"));

        invalid = Settings::default();
        invalid.listen_scope = "internet".into();
        assert_eq!(invalid.validate(), Err("访问范围只能是 local 或 lan"));

        invalid = Settings::default();
        invalid.agent_server_url = "192.168.1.2:17890".into();
        assert_eq!(
            invalid.validate(),
            Err("远程 Agent 服务地址必须以 http:// 或 https:// 开头")
        );
    }
}
