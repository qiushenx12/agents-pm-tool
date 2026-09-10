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

/// data_dir 的宽松版本（供 pm-cli：目录不存在时照样返回路径，由读取方报错）。
///
/// 只读语义：**不创建目录、不写探测文件**。pm-cli 只读取连接信息，不应在
/// 自身所在目录（例如用户 skill 目录下的 `bin/`）留下空的 `data/` 痕迹。
pub fn data_dir_lossy() -> PathBuf {
    if let Ok(custom) = std::env::var("PM_DATA_DIR") {
        return PathBuf::from(custom);
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    if cfg!(debug_assertions) {
        // dev：exe 位于 src-tauri/target/debug/，上溯 4 级 = 项目根
        exe.parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.join("data"))
            .unwrap_or_else(|| PathBuf::from("data"))
    } else {
        exe.parent()
            .map(|p| p.join("data"))
            .unwrap_or_else(|| PathBuf::from("data"))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_lossy_does_not_create_directories() {
        // 本测试独占 PM_DATA_DIR，用唯一子路径避免与其他用例相互干扰。
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join("never-created");
        std::env::set_var("PM_DATA_DIR", &target);
        let resolved = data_dir_lossy();
        std::env::remove_var("PM_DATA_DIR");

        assert_eq!(resolved, target);
        assert!(
            !target.exists(),
            "data_dir_lossy 是只读解析，不应创建目录或写入探测文件"
        );
    }
}
