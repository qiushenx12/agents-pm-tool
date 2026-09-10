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
use serde::Serialize;

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
    pub frontend: &'static str,
    pub path: String,
    pub installed: bool,
    pub version: Option<String>,
}

fn skill_roots() -> Vec<(&'static str, PathBuf)> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    [
        ("Codex", home.join(".agents").join("skills")),
        ("Codex（旧版目录）", home.join(".codex").join("skills")),
        ("Claude Code", home.join(".claude").join("skills")),
    ]
    .into_iter()
    .filter(|(_, root)| root.is_dir())
    .collect()
}

pub fn local_skill_targets() -> Vec<SkillTarget> {
    skill_roots()
        .into_iter()
        .map(|(frontend, root)| {
            let directory = root.join(SKILL_NAME);
            let version = std::fs::read_to_string(directory.join("VERSION"))
                .ok()
                .map(|value| value.trim().to_string());
            SkillTarget {
                frontend,
                path: directory.display().to_string(),
                installed: directory.join("SKILL.md").is_file()
                    && directory.join("bin").join("pm-cli.exe").is_file(),
                version,
            }
        })
        .collect()
}

pub fn local_skill_ready() -> bool {
    local_skill_targets()
        .iter()
        .any(|target| target.installed && target.version.as_deref() == Some(VERSION))
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

fn install_into_roots(roots: &[(&'static str, PathBuf)], executable_bytes: &[u8]) -> ApiResult<()> {
    for (_, root) in roots {
        let destination = root.join(SKILL_NAME);
        std::fs::create_dir_all(destination.join("bin"))?;
        std::fs::write(destination.join("SKILL.md"), SKILL_MARKDOWN)?;
        std::fs::write(destination.join("bin").join("pm-cli.exe"), executable_bytes)?;
        std::fs::write(destination.join("VERSION"), format!("{VERSION}\n"))?;
    }
    Ok(())
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
) -> ApiResult<Json<Vec<SkillTarget>>> {
    ensure_local_host(&user, peer)?;
    let executable = find_sidecar()
        .ok_or_else(|| ApiError::not_found("未找到 pm-cli.exe，请先运行 npm run build:cli"))?;
    let executable_bytes = std::fs::read(executable)?;
    let roots = skill_roots();
    if roots.is_empty() {
        return Err(ApiError::not_found(
            "未检测到 Codex 或 Claude Code 的 skill 目录",
        ));
    }
    install_into_roots(&roots, &executable_bytes)?;
    Ok(Json(local_skill_targets()))
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
        let roots = vec![("Codex", temporary.path().join("skills"))];
        install_into_roots(&roots, b"fake executable").unwrap();
        let installed = roots[0].1.join(SKILL_NAME);
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
}
