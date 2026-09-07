use serde::{Deserialize, Serialize};

/// 扩展名白名单（规划 Phase 3，共 15 个），单文件 ≤ 200MB
pub const ALLOWED_EXTENSIONS: [&str; 15] = [
    "png", "jpg", "jpeg", "gif", "webp", "mp4", "mov", "doc", "docx", "ppt", "pptx", "md", "txt",
    "pdf", "xlsx",
];
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
        && ALLOWED_EXTENSIONS.contains(&ext.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist() {
        // 与规划 Phase 3 对齐：png/jpg/jpeg/gif/webp/mp4/mov/doc/docx/ppt/pptx/md/txt/pdf/xlsx
        assert_eq!(ALLOWED_EXTENSIONS.len(), 15);
        assert!(is_allowed("截图.PNG"));
        assert!(is_allowed("demo.mp4"));
        assert!(is_allowed("表.xlsx"));
        assert!(!is_allowed("evil.exe"));
        assert!(!is_allowed("无扩展名"));
    }
}
