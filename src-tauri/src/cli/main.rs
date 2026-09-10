//! pm-cli：Agent 的命令行入口（规划 §5.5）。
//! 只是 HTTP 薄客户端：本机读 runtime.json，远程读环境变量或用户配置，再调用 /api/agent/*。
//! 无任何本地 DB 访问能力。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;

const EXIT_SERVICE_DOWN: u8 = 3;
const EXIT_VALIDATION: u8 = 2;

#[derive(Parser)]
#[command(
    name = "pm-cli",
    version,
    about = "Agents PM Tool 命令行（供 Agent 使用）",
    after_help = "服务发现：远程环境变量 > 用户配置 > <exe 同目录>/data/runtime.json。\n\
        退出码：0 成功；2 参数/校验失败（stderr 给出中文原因与合法取值）；3 服务配置或连接失败。\n\
        权限边界：Agent 可只读列出和下载可见任务附件；不可修改项目/类型、不可删任务、不可上传或删除附件、\n\
        不可切验收类状态（验收通过/未通过），\n\
        只能修改自己（submitter=Agent）创建的任务描述；项目选项仅可只读（projects 子命令）。\n\n\
        示例：\n  \
        pm-cli create --project default-project --type BUG --description \"登录页白屏\"\n  \
        pm-cli list --status 进行中 --json\n  \
        pm-cli attachments 202609021050340001\n  \
        pm-cli download <附件ID> --output 需求说明.pdf\n  \
        pm-cli status 202609021050340001 --to 待验证\n  \
        pm-cli config set server-url http://192.168.1.10:17890\n  \
        pm-cli config set token <网页签发的 token>"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 列出任务（支持筛选）
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long = "type")]
        task_type: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        submitter: Option<String>,
        #[arg(long)]
        keyword: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// 查看单个任务详情
    Get {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// 列出任务附件（只读）
    Attachments {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// 下载单个附件（只读；默认使用原文件名且不覆盖已有文件）
    Download {
        id: String,
        #[arg(short, long, value_name = "文件路径")]
        output: Option<PathBuf>,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
    },
    /// 创建任务（项目/类型/描述三必填；状态固定「未开始」）
    Create {
        #[arg(long)]
        project: Option<String>,
        #[arg(long = "type")]
        task_type: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// 修改任务状态（仅可切到：进行中/待验证/已完成）
    Status {
        id: String,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// 修改任务描述（仅限 Agent 自己创建的任务）
    Describe {
        id: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// 列出项目选项（只读）
    Projects {
        #[arg(long)]
        json: bool,
    },
    /// 配置远程服务（写入用户级配置文件）
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// 设置 server-url 或 token
    Set { key: String, value: String },
    /// 查看当前远程配置（token 会脱敏）
    Show,
}

#[derive(Deserialize)]
struct RuntimeInfo {
    port: u16,
    token: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct CliConfig {
    server_url: String,
    token: String,
}

fn runtime_path() -> PathBuf {
    // 与 app 同规则：<exe 同目录>/data/runtime.json（dev 模式回退项目根）
    agents_pm_tool_lib::paths::runtime_path(&agents_pm_tool_lib::paths::data_dir_lossy())
}

fn cli_config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("agents-pm-tool").join("cli.json")
}

fn load_cli_config(path: &std::path::Path) -> Result<CliConfig, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            serde_json::from_str(&content).map_err(|_| format!("{} 内容损坏", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(CliConfig::default()),
        Err(error) => Err(format!("读取 {} 失败：{error}", path.display())),
    }
}

fn save_cli_config(path: &std::path::Path, config: &CliConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败：{error}"))?;
    }
    let content = serde_json::to_string_pretty(config).map_err(|error| error.to_string())?;
    std::fs::write(path, format!("{content}\n")).map_err(|error| format!("写入配置失败：{error}"))
}

fn validate_server_url(value: &str) -> Result<String, String> {
    let value = value.trim().trim_end_matches('/');
    let parsed = url::Url::parse(value).map_err(|_| "server-url 不是有效 URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("server-url 必须是包含主机名的 http/https URL".into());
    }
    Ok(value.to_string())
}

fn resolve_connection(
    runtime_path: &std::path::Path,
    config_path: &std::path::Path,
    env_server_url: Option<String>,
    env_token: Option<String>,
) -> Result<(String, String), String> {
    match (env_server_url, env_token) {
        (Some(server_url), Some(token))
            if !server_url.trim().is_empty() && !token.trim().is_empty() =>
        {
            return Ok((validate_server_url(&server_url)?, token.trim().to_string()));
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err("PM_SERVER_URL 与 PM_AGENT_TOKEN 必须同时设置".into());
        }
        (Some(_), Some(_)) => return Err("远程服务地址与 token 不能为空".into()),
        (None, None) => {}
    }

    let config = load_cli_config(config_path)?;
    if !config.server_url.trim().is_empty() || !config.token.trim().is_empty() {
        if config.server_url.trim().is_empty() || config.token.trim().is_empty() {
            return Err(format!(
                "远程配置不完整，请使用 pm-cli config set 补齐 server-url 与 token（{}）",
                config_path.display()
            ));
        }
        return Ok((
            validate_server_url(&config.server_url)?,
            config.token.trim().to_string(),
        ));
    }

    let content = std::fs::read_to_string(runtime_path)
        .map_err(|_| "未找到 data/runtime.json".to_string())?;
    let info: RuntimeInfo =
        serde_json::from_str(&content).map_err(|_| "data/runtime.json 内容损坏".to_string())?;
    Ok((format!("http://127.0.0.1:{}", info.port), info.token))
}

fn service_down(msg: &str) -> ExitCode {
    eprintln!("错误：{msg}。请检查 Agents PM Tool 是否运行，以及服务地址和 token 配置");
    ExitCode::from(EXIT_SERVICE_DOWN)
}

struct Client {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Client {
    fn new() -> Result<Self, ExitCode> {
        let (server_url, token) = resolve_connection(
            &runtime_path(),
            &cli_config_path(),
            std::env::var("PM_SERVER_URL").ok(),
            std::env::var("PM_AGENT_TOKEN").ok(),
        )
        .map_err(|message| service_down(&message))?;
        Ok(Self {
            http: reqwest::Client::new(),
            base: format!("{server_url}/api/agent"),
            token,
        })
    }

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<(reqwest::StatusCode, serde_json::Value), ExitCode> {
        let mut req = self
            .http
            .request(method, format!("{}{}", self.base, path))
            .bearer_auth(&self.token);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req
            .send()
            .await
            .map_err(|_| service_down("无法连接 Agents PM Tool 服务"))?;
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        let value = serde_json::from_str(&text).unwrap_or(json!({ "raw": text }));
        Ok((status, value))
    }

    async fn download(&self, path: &str) -> Result<reqwest::Response, ExitCode> {
        let response = self
            .http
            .get(format!("{}{}", self.base, path))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|_| service_down("无法连接 Agents PM Tool 服务"))?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let value = serde_json::from_str(&text).unwrap_or(json!({ "raw": text }));
        Err(fail_from_server(status, &value))
    }
}

fn fail_from_server(status: reqwest::StatusCode, v: &serde_json::Value) -> ExitCode {
    let msg = v["error"]["message"].as_str().unwrap_or("未知错误");
    eprintln!("错误：{msg}");
    if let Some(details) = v["error"]["details"].as_object() {
        if let Some(projects) = details.get("projects").and_then(|p| p.as_array()) {
            let names: Vec<&str> = projects.iter().filter_map(|p| p.as_str()).collect();
            eprintln!("当前项目选项：{}", names.join("、"));
        }
    }
    if status.as_u16() == 401 || status.as_u16() == 403 {
        ExitCode::from(EXIT_VALIDATION)
    } else if status.is_client_error() {
        ExitCode::from(EXIT_VALIDATION)
    } else {
        ExitCode::FAILURE
    }
}

fn print_task(t: &serde_json::Value) {
    println!(
        "{}\t[{}]\t{}\t{}\t{}\t{}",
        t["id"].as_str().unwrap_or(""),
        t["project"].as_str().unwrap_or(""),
        t["type"].as_str().unwrap_or(""),
        t["status"].as_str().unwrap_or(""),
        t["submitter"].as_str().unwrap_or(""),
        t["description"].as_str().unwrap_or("").replace('\n', " "),
    );
}

fn print_task_detail(t: &serde_json::Value) {
    println!("ID：       {}", t["id"].as_str().unwrap_or(""));
    println!("项目：     {}", t["project"].as_str().unwrap_or(""));
    println!("类型：     {}", t["type"].as_str().unwrap_or(""));
    println!("状态：     {}", t["status"].as_str().unwrap_or(""));
    println!("提交人：   {}", t["submitter"].as_str().unwrap_or(""));
    println!("创建时间： {}", t["created_at"].as_str().unwrap_or(""));
    println!("完成时间： {}", t["finished_at"].as_str().unwrap_or("—"));
    println!("描述：     {}", t["description"].as_str().unwrap_or(""));
    println!("备注：     {}", t["note"].as_str().unwrap_or(""));
    println!(
        "附件：     {} 个",
        t["attachment_count"].as_i64().unwrap_or(0)
    );
}

fn print_attachment(attachment: &serde_json::Value) {
    println!(
        "{}\t{}\t{} bytes\t{}",
        attachment["id"].as_str().unwrap_or(""),
        attachment["filename"].as_str().unwrap_or(""),
        attachment["size"].as_i64().unwrap_or(0),
        attachment["created_at"].as_str().unwrap_or(""),
    );
}

fn filename_from_content_disposition(value: &str) -> Option<String> {
    let encoded = value
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("filename*=UTF-8''"))?;
    let query = format!("filename={encoded}");
    url::form_urlencoded::parse(query.as_bytes())
        .find_map(|(key, value)| (key == "filename").then(|| value.into_owned()))
}

fn safe_download_filename(filename: &str, fallback: &str) -> String {
    let leaf = filename.rsplit(['/', '\\']).next().unwrap_or("");
    let sanitized = leaf
        .chars()
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches([' ', '.']);
    if sanitized.is_empty() || matches!(sanitized, "." | "..") {
        fallback.to_string()
    } else {
        sanitized.to_string()
    }
}

fn download_target(output: Option<PathBuf>, filename: &str) -> PathBuf {
    match output {
        Some(path) if path.is_dir() => path.join(filename),
        Some(path) => path,
        None => PathBuf::from(filename),
    }
}

fn write_download(path: &Path, bytes: &[u8], force: bool) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if !parent.is_dir() {
            return Err(format!("输出目录不存在：{}", parent.display()));
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            format!("文件已存在：{}（如需覆盖请添加 --force）", path.display())
        } else {
            format!("无法写入 {}：{error}", path.display())
        }
    })?;
    file.write_all(bytes)
        .map_err(|error| format!("写入 {} 失败：{error}", path.display()))
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

async fn run(cli: Cli) -> Result<(), ExitCode> {
    let command = match cli.command {
        Commands::Config { command } => return handle_config(command),
        command => command,
    };
    let client = Client::new()?;

    match command {
        Commands::List {
            project,
            task_type,
            status,
            submitter,
            keyword,
            json,
        } => {
            let mut qs = Vec::new();
            let mut push = |k: &str, v: &Option<String>| {
                if let Some(v) = v {
                    qs.push(format!(
                        "{k}={}",
                        url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>()
                    ));
                }
            };
            push("project", &project);
            push("type", &task_type);
            push("status", &status);
            push("submitter", &submitter);
            push("keyword", &keyword);
            let path = if qs.is_empty() {
                "/tasks".into()
            } else {
                format!("/tasks?{}", qs.join("&"))
            };

            let (status, v) = client.call(reqwest::Method::GET, &path, None).await?;
            if !status.is_success() {
                return Err(fail_from_server(status, &v));
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            } else if let Some(arr) = v.as_array() {
                if arr.is_empty() {
                    println!("（无匹配任务）");
                }
                for t in arr {
                    print_task(t);
                }
            }
            Ok(())
        }

        Commands::Get { id, json } => {
            let (status, v) = client
                .call(reqwest::Method::GET, &format!("/tasks/{id}"), None)
                .await?;
            if !status.is_success() {
                return Err(fail_from_server(status, &v));
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            } else {
                print_task_detail(&v);
            }
            Ok(())
        }

        Commands::Attachments { id, json } => {
            let (status, value) = client
                .call(
                    reqwest::Method::GET,
                    &format!("/tasks/{id}/attachments"),
                    None,
                )
                .await?;
            if !status.is_success() {
                return Err(fail_from_server(status, &value));
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            } else if let Some(attachments) = value.as_array() {
                if attachments.is_empty() {
                    println!("（该任务暂无附件）");
                }
                for attachment in attachments {
                    print_attachment(attachment);
                }
            }
            Ok(())
        }

        Commands::Download {
            id,
            output,
            force,
            json,
        } => {
            let response = client.download(&format!("/attachments/{id}")).await?;
            let server_filename = response
                .headers()
                .get(reqwest::header::CONTENT_DISPOSITION)
                .and_then(|value| value.to_str().ok())
                .and_then(filename_from_content_disposition)
                .unwrap_or_else(|| id.clone());
            let filename = safe_download_filename(&server_filename, &id);
            let bytes = response
                .bytes()
                .await
                .map_err(|_| service_down("附件下载中断"))?;
            let target = download_target(output, &filename);
            write_download(&target, &bytes, force).map_err(|message| {
                eprintln!("错误：{message}");
                ExitCode::from(EXIT_VALIDATION)
            })?;
            let displayed_path = target.canonicalize().unwrap_or(target);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": id,
                        "filename": filename,
                        "path": displayed_path.to_string_lossy(),
                        "size": bytes.len(),
                    }))
                    .unwrap()
                );
            } else {
                println!("附件已下载：{}", displayed_path.display());
            }
            Ok(())
        }

        Commands::Create {
            project,
            task_type,
            description,
            json,
        } => {
            for (name, val) in [
                ("--project", &project),
                ("--type", &task_type),
                ("--description", &description),
            ] {
                if val.is_none() {
                    eprintln!("错误：{name} 为必填项（Agent 创建任务时项目/类型/描述三必填）");
                    return Err(ExitCode::from(EXIT_VALIDATION));
                }
            }
            let body = json!({
                "project": project.unwrap(),
                "type": task_type.unwrap(),
                "description": description.unwrap(),
            });
            let (status, v) = client
                .call(reqwest::Method::POST, "/tasks", Some(body))
                .await?;
            if !status.is_success() {
                return Err(fail_from_server(status, &v));
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            } else {
                println!("已创建任务：");
                print_task_detail(&v);
            }
            Ok(())
        }

        Commands::Status { id, to, json } => {
            let Some(to) = to else {
                eprintln!("错误：--to 为必填项（可切到：进行中/待验证/已完成）");
                return Err(ExitCode::from(EXIT_VALIDATION));
            };
            let (status, v) = client
                .call(
                    reqwest::Method::PATCH,
                    &format!("/tasks/{id}/status"),
                    Some(json!({ "status": to })),
                )
                .await?;
            if !status.is_success() {
                return Err(fail_from_server(status, &v));
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            } else {
                println!(
                    "任务 {} 状态已更新为「{}」",
                    v["id"].as_str().unwrap_or(&id),
                    v["status"].as_str().unwrap_or("")
                );
            }
            Ok(())
        }

        Commands::Describe {
            id,
            description,
            json,
        } => {
            let Some(description) = description else {
                eprintln!("错误：--description 为必填项");
                return Err(ExitCode::from(EXIT_VALIDATION));
            };
            let (status, v) = client
                .call(
                    reqwest::Method::PATCH,
                    &format!("/tasks/{id}/description"),
                    Some(json!({ "description": description })),
                )
                .await?;
            if !status.is_success() {
                return Err(fail_from_server(status, &v));
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            } else {
                println!("任务 {} 描述已更新", v["id"].as_str().unwrap_or(&id));
            }
            Ok(())
        }

        Commands::Projects { json } => {
            let (status, v) = client.call(reqwest::Method::GET, "/projects", None).await?;
            if !status.is_success() {
                return Err(fail_from_server(status, &v));
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            } else if let Some(arr) = v.as_array() {
                for p in arr {
                    println!("{}", p["name"].as_str().unwrap_or(""));
                    let local_path = p["local_path"].as_str().unwrap_or("");
                    let git_url = p["git_url"].as_str().unwrap_or("");
                    if !local_path.is_empty() {
                        println!("  本地路径：{local_path}");
                    }
                    if !git_url.is_empty() {
                        println!("  Git 地址：{git_url}");
                    }
                }
            }
            Ok(())
        }
        Commands::Config { .. } => unreachable!(),
    }
}

fn handle_config(command: ConfigCommand) -> Result<(), ExitCode> {
    let path = cli_config_path();
    let mut config = load_cli_config(&path).map_err(|message| {
        eprintln!("错误：{message}");
        ExitCode::from(EXIT_VALIDATION)
    })?;
    match command {
        ConfigCommand::Set { key, value } => match key.as_str() {
            "server-url" => {
                config.server_url = validate_server_url(&value).map_err(|message| {
                    eprintln!("错误：{message}");
                    ExitCode::from(EXIT_VALIDATION)
                })?;
            }
            "token" => {
                if value.trim().is_empty() {
                    eprintln!("错误：token 不能为空");
                    return Err(ExitCode::from(EXIT_VALIDATION));
                }
                config.token = value.trim().to_string();
            }
            _ => {
                eprintln!("错误：配置项仅支持 server-url 或 token");
                return Err(ExitCode::from(EXIT_VALIDATION));
            }
        },
        ConfigCommand::Show => {
            println!(
                "server-url：{}",
                if config.server_url.is_empty() {
                    "（未设置）"
                } else {
                    &config.server_url
                }
            );
            println!(
                "token：{}",
                if config.token.is_empty() {
                    "（未设置）"
                } else {
                    "********"
                }
            );
            println!("配置文件：{}", path.display());
            return Ok(());
        }
    }
    save_cli_config(&path, &config).map_err(|message| {
        eprintln!("错误：{message}");
        ExitCode::from(EXIT_VALIDATION)
    })?;
    println!("配置已保存：{}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_remote_mode_has_highest_priority() {
        let temp = tempfile::tempdir().unwrap();
        let result = resolve_connection(
            &temp.path().join("missing-runtime.json"),
            &temp.path().join("missing-cli.json"),
            Some("http://192.168.1.2:17890/".into()),
            Some("secret".into()),
        )
        .unwrap();
        assert_eq!(result, ("http://192.168.1.2:17890".into(), "secret".into()));
    }

    #[test]
    fn config_remote_mode_precedes_local_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("cli.json");
        save_cli_config(
            &config_path,
            &CliConfig {
                server_url: "https://pm.example.test".into(),
                token: "remote-token".into(),
            },
        )
        .unwrap();
        let result = resolve_connection(
            &temp.path().join("missing-runtime.json"),
            &config_path,
            None,
            None,
        )
        .unwrap();
        assert_eq!(result.0, "https://pm.example.test");
        assert_eq!(result.1, "remote-token");
    }

    #[test]
    fn local_runtime_remains_the_zero_config_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("runtime.json");
        std::fs::write(&runtime, r#"{"port":17901,"token":"local-token"}"#).unwrap();
        let result =
            resolve_connection(&runtime, &temp.path().join("missing-cli.json"), None, None)
                .unwrap();
        assert_eq!(
            result,
            ("http://127.0.0.1:17901".into(), "local-token".into())
        );
    }

    #[test]
    fn attachment_filename_is_decoded_and_sanitized() {
        let filename = filename_from_content_disposition(
            "attachment; filename*=UTF-8''%E9%9C%80%E6%B1%82%2B%E8%AF%B4%E6%98%8E.pdf",
        )
        .unwrap();
        assert_eq!(filename, "需求+说明.pdf");
        assert_eq!(
            safe_download_filename("../危险?.txt", "fallback"),
            "危险_.txt"
        );
        assert_eq!(safe_download_filename("..", "fallback"), "fallback");
    }

    #[test]
    fn attachment_download_does_not_overwrite_by_default() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("附件.txt");
        write_download(&target, b"first", false).unwrap();
        assert!(write_download(&target, b"second", false)
            .unwrap_err()
            .contains("--force"));
        assert_eq!(std::fs::read(&target).unwrap(), b"first");
        write_download(&target, b"second", true).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"second");
    }
}
