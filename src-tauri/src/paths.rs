use std::path::PathBuf;

/// 数据目录：发布版 = exe 同目录/data；开发模式 = 项目根/data。
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
        let exe = std::env::current_exe()?;
        exe.parent()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "无法定位 exe 目录"))?
            .join("data")
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

/// data_dir 的宽松版本（供 pm-cli：目录不存在时照样返回路径，由读取方报错）
pub fn data_dir_lossy() -> PathBuf {
    data_dir().unwrap_or_else(|_| {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        exe.parent()
            .map(|p| p.join("data"))
            .unwrap_or_else(|| PathBuf::from("data"))
    })
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

pub fn attachments_dir(data: &std::path::Path) -> PathBuf {
    data.join("attachments")
}
