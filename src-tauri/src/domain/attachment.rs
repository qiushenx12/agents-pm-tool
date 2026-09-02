use serde::{Deserialize, Serialize};

/// 扩展名白名单（规划 Phase 3），单文件 ≤ 200MB
pub const ALLOWED_EXTENSIONS: [&str; 14] = [
    "png", "jpg", "jpeg", "gif", "webp", "mp4", "mov", "doc", "docx", "ppt", "pptx", "md", "txt",
    "pdf", // xlsx 见下行，共 15 个
];
pub const EXTRA_EXTENSIONS: [&str; 1] = ["xlsx"];
pub const MAX_SIZE: u64 = 200 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub task_id: String,
    pub filename: String,
    pub stored_path: String,
    pub mime: Option<String>,
    pub size: i64,
    pub created_at: String,
}

pub fn is_allowed(filename: &str) -> bool {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    !ext.is_empty()
        && ext != filename.to_ascii_lowercase()
        && (ALLOWED_EXTENSIONS.contains(&ext.as_str()) || EXTRA_EXTENSIONS.contains(&ext.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist() {
        assert!(is_allowed("截图.PNG"));
        assert!(is_allowed("demo.mp4"));
        assert!(is_allowed("表.xlsx"));
        assert!(!is_allowed("evil.exe"));
        assert!(!is_allowed("无扩展名"));
    }
}
