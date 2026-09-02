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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            autostart: true,
            close_behavior: "keep_service".into(),
        }
    }
}

impl Settings {
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
