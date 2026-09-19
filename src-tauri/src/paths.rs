use std::path::PathBuf;

/// 数据目录：发布版 = exe 同目录/data（macOS 除外，见 release_data_dir）；开发模式 = 项目根/data。
/// 探测不可写时明确报错，不静默回退（规划 §7）。
pub fn data_dir() -> std::io::Result<PathBuf> {
    // 显式覆盖（测试 / 无头服务模式）
    if let Ok(custom) = std::env::var("PM_DATA_DIR") {
        let dir = PathBuf::from(custom);
        std::fs::create_dir_all(&dir)?;
        return Ok(dir);
    }
    let dir = if cfg!(debug_assertions) {
        // dev：exe 位于 src-tauri/target/debug/，上溯 4 级 = 项目根
        let exe = std::env::current_exe()?;
        exe.parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
            .join("data")
    } else {
        release_data_dir()?
    };

    std::fs::create_dir_all(&dir)?;
    // 可写性探测
    let probe = dir.join(".write_probe");
    std::fs::write(&probe, b"ok").map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!(
                "数据目录 {} 不可写，请以「当前用户」模式重新安装：{e}",
                dir.display()
            ),
        )
    })?;
    let _ = std::fs::remove_file(&probe);
    Ok(dir)
}

/// Windows / Linux 发布版：保持「exe 同目录 data/」的便携式布局。
#[cfg(not(target_os = "macos"))]
fn release_data_dir() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(exe
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "无法定位 exe 目录"))?
        .join("data"))
}

/// macOS 发布版：可执行文件在 .app 包内（Contents/MacOS/），包目录签名后即只读，
/// 数据必须落到用户目录 `~/Library/Application Support/agents-pm-tool/data`。
#[cfg(target_os = "macos")]
fn release_data_dir() -> std::io::Result<PathBuf> {
    dirs::data_dir()
        .map(|base| base.join("agents-pm-tool").join("data"))
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "无法定位用户数据目录"))
}

pub fn db_path(data: &std::path::Path) -> PathBuf {
    data.join("pm.db")
}

pub fn settings_path(data: &std::path::Path) -> PathBuf {
    data.join("settings.json")
}

pub fn runtime_path(data: &std::path::Path) -> PathBuf {
    data.join("runtime.json")
}

/// 用户级配置目录：Windows `%APPDATA%\agents-pm-tool`、macOS
/// `~/Library/Application Support/agents-pm-tool`、其它 `$XDG_CONFIG_HOME|~/.config/agents-pm-tool`。
///
/// pm-cli 侧用完全相同的规则解析（见 `pm-cli-skill/bin/pm-cli.mjs` 的 `userConfigDir`），
/// 改动这里必须同步过去，否则两端会指向不同目录。
pub fn user_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|base| base.join("agents-pm-tool"))
}

/// 用户级运行信息：应用额外把「当前端口 + 主机 token」写到这里，
/// 让装在任意位置（各前端 skill 目录）的 pm-cli 都能零配置发现本机服务 ——
/// 不再依赖「pm-cli 与应用同目录」这一前提。
pub fn user_runtime_path() -> Option<PathBuf> {
    user_config_dir().map(|dir| dir.join("runtime.json"))
}

/// 应用内网页窗口的位置与大小（见 window_state 模块）
pub fn window_state_path(data: &std::path::Path) -> PathBuf {
    data.join("window-state.json")
}

pub fn attachments_dir(data: &std::path::Path) -> PathBuf {
    data.join("attachments")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_runtime_path_sits_next_to_cli_config() {
        let Some(directory) = user_config_dir() else {
            return; // 极端环境取不到用户目录时不强求
        };
        assert!(directory.ends_with("agents-pm-tool"));
        assert_eq!(
            user_runtime_path(),
            Some(directory.join("runtime.json")),
            "运行信息与 pm-cli 的用户配置必须落在同一个目录"
        );
    }
}
