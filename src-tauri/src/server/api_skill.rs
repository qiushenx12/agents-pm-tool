use std::{
    io::{Cursor, Write},
    net::SocketAddr,
    path::PathBuf,
};

use axum::{
    body::Body,
    extract::{ConnectInfo, Extension},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::user::User,
    error::{ApiError, ApiResult},
};

const SKILL_NAME: &str = "pm-cli";
const SKILL_MARKDOWN: &str = include_str!("../../../pm-cli-skill/SKILL.md");
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn executable_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
}

fn package_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(directory) = executable_dir() {
        candidates.push(directory.join("pm-cli-skill.zip"));
        candidates.push(directory.join("binaries").join("pm-cli-skill.zip"));
        candidates.push(
            directory
                .join("resources")
                .join("binaries")
                .join("pm-cli-skill.zip"),
        );
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join("pm-cli-skill.zip"),
    );
    candidates
}

fn skill_package() -> ApiResult<Vec<u8>> {
    for path in package_candidates() {
        if path.is_file() {
            return std::fs::read(path).map_err(ApiError::from);
        }
    }
    let executable = find_sidecar()
        .ok_or_else(|| ApiError::not_found("未找到 pm-cli.exe，请先运行 npm run build:cli"))?;
    let executable = std::fs::read(executable)?;
    let cursor = Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(cursor);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    archive
        .start_file("pm-cli-skill/SKILL.md", options)
        .map_err(ApiError::internal)?;
    archive.write_all(SKILL_MARKDOWN.as_bytes())?;
    archive
        .start_file("pm-cli-skill/bin/pm-cli.exe", options)
        .map_err(ApiError::internal)?;
    archive.write_all(&executable)?;
    archive
        .start_file("pm-cli-skill/VERSION", options)
        .map_err(ApiError::internal)?;
    archive.write_all(format!("{VERSION}\n").as_bytes())?;
    archive
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(ApiError::internal)
}

fn download_response() -> ApiResult<Response> {
    let bytes = skill_package()?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=pm-cli-skill.zip",
        )
        .header("X-PM-Skill-Version", VERSION)
        .body(Body::from(bytes))
        .map_err(ApiError::internal)?)
}

pub async fn web_download(Extension(_user): Extension<User>) -> ApiResult<Response> {
    download_response()
}

pub async fn agent_download(Extension(_user): Extension<User>) -> ApiResult<Response> {
    download_response()
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillTarget {
    pub frontend_id: &'static str,
    pub frontend: &'static str,
    pub path: String,
    pub installed: bool,
    pub version: Option<String>,
}

type SkillRoot = (&'static str, &'static str, PathBuf);

/// 支持一键安装 skill 的 Agent 前端：前端 id 与展示名。
const FRONTENDS: [(&str, &str); 7] = [
    ("codex", "Codex"),
    ("claude_code", "Claude Code"),
    ("workbuddy", "WorkBuddy"),
    ("opencode", "OpenCode"),
    ("cursor", "Cursor"),
    ("pi", "Pi"),
    ("deepseek_harness", "DeepSeek Harness"),
];

fn frontend_label(frontend_id: &str) -> Option<&'static str> {
    FRONTENDS
        .iter()
        .find(|(id, _)| *id == frontend_id)
        .map(|(_, label)| *label)
}

/// DeepSeek Harness 把全部用户数据收在单一根目录下：`$DSH_HOME`（默认 `~/.dsh`），
/// skill 目录是它下面的 `skills/`。空白的 `$DSH_HOME` 按未设置处理，避免解析到当前工作目录。
fn deepseek_harness_skills_root(home: &std::path::Path, configured: Option<&str>) -> PathBuf {
    configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".dsh"))
        .join("skills")
}

/// 每个前端的 skill 根目录定义，不做存在性过滤。
/// 目录取自各前端官方文档的全局（用户级）skill 位置。
fn all_skill_roots() -> Vec<SkillRoot> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let dsh_home = std::env::var("DSH_HOME").ok();
    vec![
        ("codex", "Codex", home.join(".agents").join("skills")),
        (
            "codex",
            "Codex（旧版目录）",
            home.join(".codex").join("skills"),
        ),
        (
            "claude_code",
            "Claude Code",
            home.join(".claude").join("skills"),
        ),
        (
            "workbuddy",
            "WorkBuddy",
            home.join(".workbuddy").join("skills"),
        ),
        (
            "opencode",
            "OpenCode",
            home.join(".config").join("opencode").join("skills"),
        ),
        ("cursor", "Cursor", home.join(".cursor").join("skills")),
        (
            "pi",
            "Pi",
            home.join(".pi").join("agent").join("skills"),
        ),
        (
            "deepseek_harness",
            "DeepSeek Harness",
            deepseek_harness_skills_root(&home, dsh_home.as_deref()),
        ),
    ]
}

/// 判断某前端是否安装在本机：只要求它的配置目录存在，skill 目录在安装时按需创建，
/// 否则刚装好、还没放过任何 skill 的前端会被误判为「未检测到」。
fn root_is_available(root: &std::path::Path) -> bool {
    root.parent().is_some_and(std::path::Path::is_dir)
}

fn skill_roots() -> Vec<SkillRoot> {
    all_skill_roots()
        .into_iter()
        .filter(|(_, _, root)| root_is_available(root))
        .collect()
}

pub fn local_skill_targets() -> Vec<SkillTarget> {
    skill_roots()
        .into_iter()
        .map(|(frontend_id, frontend, root)| {
            let directory = root.join(SKILL_NAME);
            let version = std::fs::read_to_string(directory.join("VERSION"))
                .ok()
                .map(|value| value.trim().to_string());
            SkillTarget {
                frontend_id,
                frontend,
                path: directory.display().to_string(),
                installed: directory.join("SKILL.md").is_file()
                    && directory.join("bin").join("pm-cli.exe").is_file(),
                version,
            }
        })
        .collect()
}

fn find_sidecar() -> Option<PathBuf> {
    let mut directories = Vec::new();
    if let Some(directory) = executable_dir() {
        directories.push(directory.clone());
        directories.push(directory.join("binaries"));
    }
    directories.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries"));
    directories.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("release"),
    );
    directories.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("debug"),
    );
    for directory in directories {
        let plain = directory.join("pm-cli.exe");
        if plain.is_file() {
            return Some(plain);
        }
        if let Ok(entries) = std::fs::read_dir(&directory) {
            if let Some(path) = entries.flatten().map(|entry| entry.path()).find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("pm-cli-") && name.ends_with(".exe"))
            }) {
                return Some(path);
            }
        }
    }
    None
}

fn ensure_local_host(user: &User, peer: SocketAddr) -> ApiResult<()> {
    if user.id != crate::domain::user::HOST_USER_ID || !peer.ip().is_loopback() {
        return Err(ApiError::forbidden(
            "本机 skill 检测与安装仅允许主机账号从本机操作",
        ));
    }
    Ok(())
}

fn install_into_roots(roots: &[SkillRoot], executable_bytes: &[u8]) -> ApiResult<()> {
    for (_, _, root) in roots {
        let destination = root.join(SKILL_NAME);
        std::fs::create_dir_all(destination.join("bin"))?;
        std::fs::write(destination.join("SKILL.md"), SKILL_MARKDOWN)?;
        std::fs::write(destination.join("bin").join("pm-cli.exe"), executable_bytes)?;
        std::fs::write(destination.join("VERSION"), format!("{VERSION}\n"))?;
    }
    Ok(())
}

fn select_frontend_roots(roots: Vec<SkillRoot>, frontend_id: &str) -> ApiResult<Vec<SkillRoot>> {
    let Some(label) = frontend_label(frontend_id) else {
        let supported = FRONTENDS
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>()
            .join("、");
        return Err(ApiError::unprocessable(format!(
            "frontend 仅支持 {supported}"
        )));
    };
    let selected = roots
        .into_iter()
        .filter(|(id, _, _)| *id == frontend_id)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(ApiError::not_found(format!("未检测到 {label} skill 目录")));
    }
    Ok(selected)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontendBody {
    pub frontend: String,
}

pub async fn local_targets(
    Extension(user): Extension<User>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> ApiResult<Json<Vec<SkillTarget>>> {
    ensure_local_host(&user, peer)?;
    Ok(Json(local_skill_targets()))
}

pub async fn install_local(
    Extension(user): Extension<User>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<FrontendBody>,
) -> ApiResult<Json<Vec<SkillTarget>>> {
    ensure_local_host(&user, peer)?;
    let executable = find_sidecar()
        .ok_or_else(|| ApiError::not_found("未找到 pm-cli.exe，请先运行 npm run build:cli"))?;
    let executable_bytes = std::fs::read(executable)?;
    let roots = select_frontend_roots(skill_roots(), &body.frontend)?;
    install_into_roots(&roots, &executable_bytes)?;
    Ok(Json(local_skill_targets()))
}

#[cfg(target_os = "windows")]
fn open_directory(path: &std::path::Path) -> ApiResult<()> {
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(ApiError::internal)
}

#[cfg(not(target_os = "windows"))]
fn open_directory(_path: &std::path::Path) -> ApiResult<()> {
    Err(ApiError::unprocessable("一键打开目录目前仅支持 Windows"))
}

pub async fn open_local_directory(
    Extension(user): Extension<User>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<FrontendBody>,
) -> ApiResult<StatusCode> {
    ensure_local_host(&user, peer)?;
    let root = select_frontend_roots(skill_roots(), &body.frontend)?
        .into_iter()
        .next()
        .expect("已校验至少有一个目标目录")
        .2;
    let destination = root.join(SKILL_NAME);
    std::fs::create_dir_all(&destination)?;
    open_directory(&destination)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_candidates_include_build_output() {
        assert!(package_candidates().iter().any(|path| {
            path.ends_with(std::path::Path::new("binaries").join("pm-cli-skill.zip"))
        }));
    }

    #[test]
    fn skill_has_portable_required_frontmatter() {
        assert!(SKILL_MARKDOWN.starts_with("---"));
        assert!(SKILL_MARKDOWN.contains("name: pm-cli"));
        assert!(SKILL_MARKDOWN.contains("\ndescription:"));
    }

    #[test]
    fn installer_writes_complete_skill_to_detected_root() {
        let temporary = tempfile::tempdir().unwrap();
        let roots = vec![("codex", "Codex", temporary.path().join("skills"))];
        install_into_roots(&roots, b"fake executable").unwrap();
        let installed = roots[0].2.join(SKILL_NAME);
        assert!(installed.join("SKILL.md").is_file());
        assert_eq!(
            std::fs::read(installed.join("bin").join("pm-cli.exe")).unwrap(),
            b"fake executable"
        );
        assert_eq!(
            std::fs::read_to_string(installed.join("VERSION"))
                .unwrap()
                .trim(),
            VERSION
        );
    }

    #[test]
    fn workbuddy_install_selection_is_supported() {
        let temporary = tempfile::tempdir().unwrap();
        let roots = vec![
            (
                "workbuddy",
                "WorkBuddy",
                temporary.path().join("workbuddy"),
            ),
            ("codex", "Codex", temporary.path().join("codex")),
        ];
        let selected = select_frontend_roots(roots, "workbuddy").unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, "workbuddy");
        install_into_roots(&selected, b"fake executable").unwrap();
        assert!(selected[0]
            .2
            .join(SKILL_NAME)
            .join("SKILL.md")
            .is_file());
    }

    #[test]
    fn frontend_install_selection_is_independent() {
        let temporary = tempfile::tempdir().unwrap();
        let roots = vec![
            ("codex", "Codex", temporary.path().join("codex")),
            (
                "codex",
                "Codex（旧版目录）",
                temporary.path().join("codex-legacy"),
            ),
            (
                "claude_code",
                "Claude Code",
                temporary.path().join("claude"),
            ),
        ];
        let selected = select_frontend_roots(roots, "codex").unwrap();
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().all(|(id, _, _)| *id == "codex"));
        assert!(select_frontend_roots(Vec::new(), "unknown").is_err());
    }

    #[test]
    fn supported_frontends_all_have_skill_roots() {
        let roots = all_skill_roots();
        for (frontend_id, label) in FRONTENDS {
            let matched = roots
                .iter()
                .filter(|(id, _, _)| *id == frontend_id)
                .collect::<Vec<_>>();
            assert!(
                !matched.is_empty(),
                "{frontend_id} 缺少 skill 根目录定义"
            );
            assert!(matched.iter().any(|(_, name, _)| *name == label));
        }
        assert_eq!(roots.len(), 8, "Codex 有两个 skill 目录，其余前端各一个");
    }

    #[test]
    fn deepseek_harness_uses_dsh_home_for_its_skill_root() {
        let home = PathBuf::from("C:\\Users\\tester");
        assert_eq!(
            deepseek_harness_skills_root(&home, None),
            home.join(".dsh").join("skills"),
            "未设置 DSH_HOME 时回落到 ~/.dsh"
        );
        assert_eq!(
            deepseek_harness_skills_root(&home, Some("   ")),
            home.join(".dsh").join("skills"),
            "空白 DSH_HOME 视为未设置"
        );
        assert_eq!(
            deepseek_harness_skills_root(&home, Some(" D:\\dsh-home ")),
            PathBuf::from("D:\\dsh-home").join("skills"),
            "DSH_HOME 覆盖默认根目录，并容忍首尾空白"
        );
    }

    #[test]
    fn deepseek_harness_install_writes_into_dsh_skills_root() {
        let temporary = tempfile::tempdir().unwrap();
        let roots = vec![(
            "deepseek_harness",
            "DeepSeek Harness",
            deepseek_harness_skills_root(temporary.path(), None),
        )];
        let selected = select_frontend_roots(roots, "deepseek_harness").unwrap();
        assert_eq!(selected.len(), 1);
        install_into_roots(&selected, b"fake executable").unwrap();
        let installed = selected[0].2.join(SKILL_NAME);
        assert!(installed.join("SKILL.md").is_file());
        assert_eq!(
            std::fs::read(installed.join("bin").join("pm-cli.exe")).unwrap(),
            b"fake executable"
        );
    }

    #[test]
    fn opencode_cursor_and_pi_install_into_their_native_roots() {
        for frontend_id in ["opencode", "cursor", "pi"] {
            let temporary = tempfile::tempdir().unwrap();
            let roots = vec![(
                frontend_id,
                frontend_label(frontend_id).unwrap(),
                temporary.path().join("skills"),
            )];
            let selected = select_frontend_roots(roots, frontend_id).unwrap();
            assert_eq!(selected.len(), 1);
            install_into_roots(&selected, b"fake executable").unwrap();
            assert!(selected[0]
                .2
                .join(SKILL_NAME)
                .join("SKILL.md")
                .is_file());
            assert!(selected[0]
                .2
                .join(SKILL_NAME)
                .join("bin")
                .join("pm-cli.exe")
                .is_file());
        }
    }

    #[test]
    fn frontend_without_skill_directory_is_still_installable() {
        let temporary = tempfile::tempdir().unwrap();
        let config_root = temporary.path().join("frontend");
        std::fs::create_dir_all(&config_root).unwrap();
        let skills = config_root.join("skills");
        assert!(!skills.is_dir());
        // 前端已安装但还没放过任何 skill：配置目录存在即视为可安装。
        assert!(root_is_available(&skills));
        assert!(!root_is_available(&temporary.path().join("missing").join("skills")));
    }

    #[test]
    fn unknown_frontend_reports_supported_list() {
        let error = select_frontend_roots(Vec::new(), "amp").unwrap_err();
        assert_eq!(error.code, "validation_failed");
        for frontend_id in FRONTENDS.map(|(id, _)| id) {
            assert!(
                error.message.contains(frontend_id),
                "错误信息缺少 {frontend_id}"
            );
        }
    }
}
