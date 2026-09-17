//! pm-cli skill 的分发。
//!
//! skill 只有一份实现：`pm-cli-skill/` 目录被内嵌进二进制，安装 = 把这份文件清单写到目标目录。
//! 三种出口共用同一份清单，避免「网页装的」和「脚本装的」不一致：
//!
//! - `payload`：文件清单 JSON，供网页端「选择目录」直接写入用户选中的目录；
//! - `installer`：把清单内嵌进一个自包含安装脚本，供用户下载后跑一次，或用一行命令
//!   拉下来直接执行（浏览器无法指定下载位置，也无法在局域网 HTTP 页面里写本地目录）；
//! - `install_local` / `install_into_roots`：主机账号在本机时由服务端直接落盘。
//!
//! 前两个免 token，与 `/api/agent/help` 同级：skill 内容不含任何机密，而
//! 「还没有 skill 的 Agent」本身就需要先拿到它。

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    extract::{ConnectInfo, Extension},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::user::User,
    error::{ApiError, ApiResult},
};

const SKILL_NAME: &str = "pm-cli";
const VERSION: &str = env!("CARGO_PKG_VERSION");

const SKILL_MARKDOWN: &str = include_str!("../../../pm-cli-skill/SKILL.md");
const SKILL_CLI: &str = include_str!("../../../pm-cli-skill/bin/pm-cli.mjs");
const SKILL_CMD: &str = include_str!("../../../pm-cli-skill/bin/pm-cli.cmd");
const SKILL_SH: &str = include_str!("../../../pm-cli-skill/bin/pm-cli");
const INSTALLER_TEMPLATE: &str = include_str!("../../../pm-cli-skill/installer.mjs");

/// 安装脚本模板里的锚点：分发时替换成真实文件清单。
/// 改模板时必须同步这里，`installer_template_has_payload_anchor` 测试会挡住改名。
const PAYLOAD_ANCHOR: &str = "const PAYLOAD = [];";

// -------------------------------------------------------------- 前端目录定义

struct RootDef {
    /// 相对用户主目录的 skills 根目录，用 `/` 分隔。
    relative: &'static str,
    /// 展示名覆盖；默认用前端名（Codex 的历史目录需要额外标注）。
    label: Option<&'static str>,
}

struct FrontendDef {
    id: &'static str,
    label: &'static str,
    roots: &'static [RootDef],
    /// 支持用环境变量整目录覆盖的前端（DeepSeek Harness 的 `DSH_HOME`）。
    env_home: Option<&'static str>,
    env_home_subpath: Option<&'static str>,
    /// 给用户看的补充说明。
    note: Option<&'static str>,
}

const fn root(relative: &'static str) -> RootDef {
    RootDef {
        relative,
        label: None,
    }
}

const fn labeled_root(relative: &'static str, label: &'static str) -> RootDef {
    RootDef {
        relative,
        label: Some(label),
    }
}

/// 支持安装 skill 的 Agent 前端，以及各自的全局（用户级）skill 目录。
/// **前端清单只维护在这里**；新增前端时还需同步 `src/shared/types.ts` 的
/// `LocalSkillFrontendId`、`FrontendLogo.vue` 与 grid.css 的配色 token。
const FRONTENDS: [FrontendDef; 7] = [
    FrontendDef {
        id: "codex",
        label: "Codex",
        roots: &[
            root(".agents/skills"),
            labeled_root(".codex/skills", "Codex（旧版目录）"),
        ],
        env_home: None,
        env_home_subpath: None,
        note: None,
    },
    FrontendDef {
        id: "claude_code",
        label: "Claude Code",
        roots: &[root(".claude/skills")],
        env_home: None,
        env_home_subpath: None,
        note: None,
    },
    FrontendDef {
        id: "workbuddy",
        label: "WorkBuddy",
        roots: &[root(".workbuddy/skills")],
        env_home: None,
        env_home_subpath: None,
        note: None,
    },
    FrontendDef {
        id: "opencode",
        label: "OpenCode",
        roots: &[root(".config/opencode/skills")],
        env_home: None,
        env_home_subpath: None,
        note: None,
    },
    FrontendDef {
        id: "cursor",
        label: "Cursor",
        roots: &[root(".cursor/skills")],
        env_home: None,
        env_home_subpath: None,
        note: None,
    },
    FrontendDef {
        id: "pi",
        label: "Pi",
        roots: &[root(".pi/agent/skills")],
        env_home: None,
        env_home_subpath: None,
        note: None,
    },
    FrontendDef {
        id: "deepseek_harness",
        label: "DeepSeek Harness",
        roots: &[root(".dsh/skills")],
        env_home: Some("DSH_HOME"),
        env_home_subpath: Some("skills"),
        note: Some("设置了 DSH_HOME 时，改用该目录下的 skills 子目录"),
    },
];

fn frontend(id: &str) -> Option<&'static FrontendDef> {
    FRONTENDS.iter().find(|item| item.id == id)
}

/// 该前端在本机的 skills 根目录（按声明顺序，不做存在性过滤）。
fn frontend_roots(def: &FrontendDef) -> Vec<PathBuf> {
    let home = dirs::home_dir();
    let overridden = def
        .env_home
        .and_then(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    def.roots
        .iter()
        .map(|item| match (&overridden, def.env_home_subpath) {
            (Some(base), Some(subpath)) => PathBuf::from(base).join(subpath),
            _ => home
                .clone()
                .unwrap_or_default()
                .join(item.relative.replace('/', std::path::MAIN_SEPARATOR_STR)),
        })
        .collect()
}

fn root_label(def: &FrontendDef, index: usize) -> &'static str {
    def.roots
        .get(index)
        .and_then(|item| item.label)
        .unwrap_or(def.label)
}

type SkillRoot = (&'static str, &'static str, PathBuf);

/// 每个前端的 skill 根目录定义，不做存在性过滤。
fn all_skill_roots() -> Vec<SkillRoot> {
    let mut roots = Vec::new();
    for def in FRONTENDS.iter() {
        for (index, directory) in frontend_roots(def).into_iter().enumerate() {
            roots.push((def.id, root_label(def, index), directory));
        }
    }
    roots
}

/// 判断某前端是否装在本机：只要求它的配置目录存在，skill 目录在安装时按需创建，
/// 否则刚装好、还没放过任何 skill 的前端会被误判为「未检测到」。
fn root_is_available(root: &Path) -> bool {
    root.parent().is_some_and(Path::is_dir)
}

fn skill_roots() -> Vec<SkillRoot> {
    all_skill_roots()
        .into_iter()
        .filter(|(_, _, root)| root_is_available(root))
        .collect()
}

/// 展示用的目录提示：Windows 与 macOS 两种写法，供用户照着选目录。
fn platform_paths(def: &FrontendDef) -> (Vec<String>, Vec<String>) {
    let overridden = def
        .env_home
        .is_some_and(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()));
    // 目录被环境变量整目录覆盖时，静态提示不再准确，直接说明由环境变量决定。
    if overridden {
        if let (Some(name), Some(subpath)) = (def.env_home, def.env_home_subpath) {
            let hint = format!("{name} 下的 {subpath} 子目录");
            return (vec![hint.clone()], vec![hint]);
        }
    }
    let windows = def
        .roots
        .iter()
        .map(|item| format!("%USERPROFILE%\\{}", item.relative.replace('/', "\\")))
        .collect();
    let macos = def
        .roots
        .iter()
        .map(|item| format!("~/{}", item.relative))
        .collect();
    (windows, macos)
}

// -------------------------------------------------------------- skill 文件清单

/// 一个待写入的文件。`path` 是 skill 目录内的相对路径，用 `/` 分隔。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    pub path: String,
    pub content: String,
    /// 需要可执行位（只在 POSIX 平台有意义）。
    #[serde(default)]
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadRoot {
    pub relative: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadFrontend {
    pub id: String,
    pub label: String,
    pub roots: Vec<PayloadRoot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_home: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_home_subpath: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub windows_paths: Vec<String>,
    pub macos_paths: Vec<String>,
}

/// 网页端「选择目录」与安装脚本共用的一份完整载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPayload {
    pub name: String,
    pub version: String,
    /// 安装到目标 skills 根目录下时创建的目录名。
    pub directory: String,
    pub frontends: Vec<PayloadFrontend>,
    pub files: Vec<SkillFile>,
}

fn skill_files() -> Vec<SkillFile> {
    vec![
        SkillFile {
            path: "SKILL.md".into(),
            content: SKILL_MARKDOWN.to_string(),
            executable: false,
        },
        SkillFile {
            path: "VERSION".into(),
            content: format!("{VERSION}\n"),
            executable: false,
        },
        SkillFile {
            path: "bin/pm-cli.mjs".into(),
            content: SKILL_CLI.to_string(),
            executable: false,
        },
        SkillFile {
            path: "bin/pm-cli.cmd".into(),
            content: SKILL_CMD.to_string(),
            executable: false,
        },
        SkillFile {
            path: "bin/pm-cli".into(),
            content: SKILL_SH.to_string(),
            executable: true,
        },
    ]
}

pub fn skill_payload() -> SkillPayload {
    SkillPayload {
        name: SKILL_NAME.to_string(),
        version: VERSION.to_string(),
        directory: SKILL_NAME.to_string(),
        frontends: FRONTENDS
            .iter()
            .map(|def| {
                let (windows_paths, macos_paths) = platform_paths(def);
                PayloadFrontend {
                    id: def.id.to_string(),
                    label: def.label.to_string(),
                    roots: def
                        .roots
                        .iter()
                        .enumerate()
                        .map(|(index, item)| PayloadRoot {
                            relative: item.relative.to_string(),
                            label: root_label(def, index).to_string(),
                        })
                        .collect(),
                    env_home: def.env_home.map(str::to_string),
                    env_home_subpath: def.env_home_subpath.map(str::to_string),
                    note: def.note.map(str::to_string),
                    windows_paths,
                    macos_paths,
                }
            })
            .collect(),
        files: skill_files(),
    }
}

// -------------------------------------------------------------------- 分发出口

/// 文件清单：供网页端「选择目录」把 skill 直接写进用户选中的目录。
pub async fn payload() -> Json<SkillPayload> {
    Json(skill_payload())
}

/// 自包含安装脚本：把文件清单内嵌进去，用户拿到后跑一次即可。
fn installer_script() -> String {
    let json = serde_json::to_string(&skill_payload()).unwrap_or_else(|_| "null".to_string());
    // U+2028/U+2029 在旧解析器里等价换行，嵌进字符串字面量会破坏脚本，先转义掉。
    let json = json
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    INSTALLER_TEMPLATE.replacen(PAYLOAD_ANCHOR, &format!("const PAYLOAD = {json};"), 1)
}

/// 安装脚本。用附件形式返回，浏览器会直接存成文件，curl 也能管道给 node 执行。
pub async fn installer() -> ApiResult<Response> {
    let body = installer_script();
    if body.contains(PAYLOAD_ANCHOR) {
        // 锚点没被替换掉：说明模板改了写法。宁可报错也不发一个空载荷的脚本出去。
        return Err(ApiError::internal(
            "安装脚本模板与载荷锚点不匹配，请检查 pm-cli-skill/installer.mjs",
        ));
    }
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/javascript; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=pm-cli-install.mjs",
        )
        .header("X-PM-Skill-Version", VERSION)
        .body(Body::from(body))
        .map_err(ApiError::internal)?)
}

// -------------------------------------------------------------------- 本机安装

#[derive(Debug, Clone, Serialize)]
pub struct SkillTarget {
    pub frontend_id: &'static str,
    pub frontend: &'static str,
    pub path: String,
    pub installed: bool,
    pub version: Option<String>,
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
                installed: directory.join("bin").join("pm-cli.mjs").is_file(),
                version,
            }
        })
        .collect()
}

/// 写入前先清掉旧版痕迹：旧的可执行文件，以及为绕过「找不到运行信息」而人为
/// 建立的 `bin/data` 目录（现在运行信息放在用户级位置，不再需要）。
/// 只删空目录或目录联接，真实存在内容的目录不动。
fn clean_legacy(directory: &Path) {
    let _ = std::fs::remove_file(directory.join("bin").join("pm-cli.exe"));
    let legacy_data = directory.join("bin").join("data");
    if legacy_data.is_dir() {
        let _ = std::fs::remove_dir(&legacy_data);
    }
}

fn write_file(destination: &Path, file: &SkillFile) -> ApiResult<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(destination, file.content.as_bytes())?;
    #[cfg(unix)]
    if file.executable {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn install_into_roots(roots: &[SkillRoot], files: &[SkillFile]) -> ApiResult<()> {
    for (_, _, root) in roots {
        let destination = root.join(SKILL_NAME);
        std::fs::create_dir_all(&destination)?;
        for file in files {
            let relative = file.path.replace('/', std::path::MAIN_SEPARATOR_STR);
            write_file(&destination.join(relative), file)?;
        }
        clean_legacy(&destination);
    }
    Ok(())
}

fn ensure_local_host(user: &User, peer: SocketAddr, headers: &HeaderMap) -> ApiResult<()> {
    if user.id != crate::domain::user::HOST_USER_ID {
        return Err(ApiError::forbidden(
            "本机 skill 检测与安装仅允许主机账号操作",
        ));
    }
    // 回环对端不足以证明是本机：隧道/反代的代理进程就在本机（见 auth::require_direct_local）。
    super::auth::require_direct_local(peer, headers)?;
    Ok(())
}

fn select_frontend_roots(roots: Vec<SkillRoot>, frontend_id: &str) -> ApiResult<Vec<SkillRoot>> {
    let Some(def) = frontend(frontend_id) else {
        let supported = FRONTENDS
            .iter()
            .map(|item| item.id)
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
        return Err(ApiError::not_found(format!(
            "未检测到 {} 的 skill 目录",
            def.label
        )));
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
    headers: HeaderMap,
) -> ApiResult<Json<Vec<SkillTarget>>> {
    ensure_local_host(&user, peer, &headers)?;
    Ok(Json(local_skill_targets()))
}

pub async fn install_local(
    Extension(user): Extension<User>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<FrontendBody>,
) -> ApiResult<Json<Vec<SkillTarget>>> {
    ensure_local_host(&user, peer, &headers)?;
    let roots = select_frontend_roots(skill_roots(), &body.frontend)?;
    install_into_roots(&roots, &skill_files())?;
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
    Err(ApiError::unprocessable(
        "一键打开目录目前仅支持 Windows 桌面版",
    ))
}

pub async fn open_local_directory(
    Extension(user): Extension<User>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<FrontendBody>,
) -> ApiResult<StatusCode> {
    ensure_local_host(&user, peer, &headers)?;
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

    fn test_roots(directory: &Path) -> Vec<SkillRoot> {
        vec![("codex", "Codex", directory.to_path_buf())]
    }

    #[test]
    fn payload_carries_every_skill_file() {
        let payload = skill_payload();
        let paths: Vec<&str> = payload.files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "SKILL.md",
                "VERSION",
                "bin/pm-cli.mjs",
                "bin/pm-cli.cmd",
                "bin/pm-cli"
            ]
        );
        assert_eq!(payload.directory, SKILL_NAME);
        assert_eq!(payload.version, VERSION);
        let sh = payload
            .files
            .iter()
            .find(|file| file.path == "bin/pm-cli")
            .expect("POSIX 包装脚本必须在清单里");
        assert!(sh.executable, "POSIX 包装脚本需要可执行位");
        assert!(sh.content.starts_with("#!/bin/sh"));
    }

    #[test]
    fn skill_has_portable_required_frontmatter() {
        assert!(SKILL_MARKDOWN.starts_with("---"));
        assert!(SKILL_MARKDOWN.contains("name: pm-cli"));
        assert!(SKILL_MARKDOWN.contains("\ndescription:"));
    }

    #[test]
    fn skill_version_file_matches_cargo_version() {
        let bundled = include_str!("../../../pm-cli-skill/VERSION").trim();
        assert_eq!(
            bundled, VERSION,
            "pm-cli-skill/VERSION 与 Cargo.toml 版本不一致：请在 build.py 同步或手动更正"
        );
    }

    #[test]
    fn installer_template_has_payload_anchor() {
        assert!(
            INSTALLER_TEMPLATE.contains(PAYLOAD_ANCHOR),
            "安装脚本改了锚点写法，需同步 PAYLOAD_ANCHOR，否则会发出空载荷脚本"
        );
    }

    #[test]
    fn installer_script_embeds_the_whole_payload() {
        let script = installer_script();
        assert!(!script.contains(PAYLOAD_ANCHOR), "锚点必须已被替换");
        assert!(script.contains("const PAYLOAD = {"));
        for file in skill_files() {
            assert!(
                script.contains(&file.path),
                "安装脚本缺少 {} 的条目",
                file.path
            );
        }
    }

    #[test]
    fn installer_writes_complete_skill_to_detected_root() {
        let temporary = tempfile::tempdir().unwrap();
        let roots = test_roots(&temporary.path().join("skills"));
        install_into_roots(&roots, &skill_files()).unwrap();
        let installed = roots[0].2.join(SKILL_NAME);
        assert!(installed.join("SKILL.md").is_file());
        assert!(installed.join("bin").join("pm-cli.mjs").is_file());
        assert!(installed.join("bin").join("pm-cli.cmd").is_file());
        assert_eq!(
            std::fs::read_to_string(installed.join("VERSION"))
                .unwrap()
                .trim(),
            VERSION
        );
    }

    #[test]
    fn reinstall_keeps_legacy_data_with_real_content_but_drops_exe() {
        let temporary = tempfile::tempdir().unwrap();
        let roots = test_roots(&temporary.path().join("skills"));
        let installed = roots[0].2.join(SKILL_NAME);
        std::fs::create_dir_all(installed.join("bin").join("data")).unwrap();
        std::fs::write(installed.join("bin").join("pm-cli.exe"), b"old").unwrap();
        std::fs::write(installed.join("bin").join("data").join("keep.txt"), b"keep").unwrap();

        install_into_roots(&roots, &skill_files()).unwrap();

        assert!(
            !installed.join("bin").join("pm-cli.exe").exists(),
            "旧版可执行文件必须清掉，避免新旧混用"
        );
        assert!(
            installed.join("bin").join("data").join("keep.txt").is_file(),
            "有真实内容的 bin/data 不能被删"
        );
    }

    #[test]
    fn reinstall_clears_empty_legacy_data_directory() {
        let temporary = tempfile::tempdir().unwrap();
        let roots = test_roots(&temporary.path().join("skills"));
        let installed = roots[0].2.join(SKILL_NAME);
        std::fs::create_dir_all(installed.join("bin").join("data")).unwrap();

        install_into_roots(&roots, &skill_files()).unwrap();

        assert!(
            !installed.join("bin").join("data").exists(),
            "空的 bin/data（目录联接残留）应被清掉"
        );
    }

    #[test]
    fn installed_detection_requires_the_script() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("skills").join(SKILL_NAME);
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("SKILL.md"), b"doc").unwrap();
        std::fs::write(root.join("VERSION"), b"1.0.0\n").unwrap();
        std::fs::write(root.join("bin").join("pm-cli.exe"), b"old").unwrap();
        let targets = vec![(
            "codex",
            "Codex",
            temporary.path().join("skills"),
            root.clone(),
        )];
        // 只有 SKILL.md 时不算装好：判定必须看 bin/pm-cli.mjs
        assert!(!root.join("bin").join("pm-cli.mjs").exists());
        std::fs::write(root.join("bin").join("pm-cli.mjs"), b"// cli").unwrap();
        assert!(root.join("bin").join("pm-cli.mjs").is_file());
        assert_eq!(targets.len(), 1);
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
        install_into_roots(&selected, &skill_files()).unwrap();
        assert!(selected[0]
            .2
            .join(SKILL_NAME)
            .join("bin")
            .join("pm-cli.mjs")
            .is_file());
    }

    #[test]
    fn supported_frontends_all_have_skill_roots() {
        let roots = all_skill_roots();
        for def in FRONTENDS.iter() {
            let matched = roots
                .iter()
                .filter(|(id, _, _)| *id == def.id)
                .collect::<Vec<_>>();
            assert!(!matched.is_empty(), "{} 缺少 skill 根目录定义", def.id);
            assert!(matched.iter().any(|(_, name, _)| *name == def.label));
        }
        assert_eq!(roots.len(), 8, "Codex 有两个 skill 目录，其余前端各一个");
    }

    #[test]
    fn deepseek_harness_uses_dsh_home_for_its_skill_root() {
        let def = frontend("deepseek_harness").unwrap();
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

        std::env::remove_var("DSH_HOME");
        assert_eq!(frontend_roots(def), vec![home.join(".dsh").join("skills")]);

        std::env::set_var("DSH_HOME", "   ");
        assert_eq!(
            frontend_roots(def),
            vec![home.join(".dsh").join("skills")],
            "空白 DSH_HOME 视为未设置"
        );

        let custom = tempfile::tempdir().unwrap();
        std::env::set_var("DSH_HOME", custom.path());
        assert_eq!(frontend_roots(def), vec![custom.path().join("skills")]);
        std::env::remove_var("DSH_HOME");
    }

    #[test]
    fn platform_paths_are_rendered_for_both_systems() {
        std::env::remove_var("DSH_HOME");
        let codex = frontend("codex").unwrap();
        let (windows, macos) = platform_paths(codex);
        assert_eq!(macos[0], "~/.agents/skills");
        assert_eq!(macos[1], "~/.codex/skills");
        assert_eq!(windows[0], "%USERPROFILE%\\.agents\\skills");

        let payload = skill_payload();
        let entry = payload
            .frontends
            .iter()
            .find(|item| item.id == "claude_code")
            .unwrap();
        assert_eq!(entry.macos_paths, vec!["~/.claude/skills"]);
        assert_eq!(entry.roots.len(), 1);
        assert_eq!(entry.roots[0].label, "Claude Code");
    }

    #[test]
    fn frontend_without_skill_directory_is_still_installable() {
        let temporary = tempfile::tempdir().unwrap();
        let config_root = temporary.path().join("frontend");
        std::fs::create_dir_all(&config_root).unwrap();
        let skills = config_root.join("skills");
        assert!(!skills.is_dir());
        assert!(root_is_available(&skills));
        assert!(!root_is_available(&temporary.path().join("missing").join("skills")));
    }

    #[test]
    fn opencode_cursor_and_pi_install_into_their_native_roots() {
        for frontend_id in ["opencode", "cursor", "pi"] {
            let temporary = tempfile::tempdir().unwrap();
            let roots = vec![(
                frontend_id,
                frontend(frontend_id).unwrap().label,
                temporary.path().join("skills"),
            )];
            let selected = select_frontend_roots(roots, frontend_id).unwrap();
            install_into_roots(&selected, &skill_files()).unwrap();
            assert!(selected[0]
                .2
                .join(SKILL_NAME)
                .join("bin")
                .join("pm-cli.mjs")
                .is_file());
        }
    }

    #[test]
    fn unknown_frontend_reports_supported_list() {
        let error = select_frontend_roots(Vec::new(), "amp").unwrap_err();
        assert_eq!(error.code, "validation_failed");
        for def in FRONTENDS.iter() {
            assert!(
                error.message.contains(def.id),
                "错误信息缺少 {}",
                def.id
            );
        }
    }
}
