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
        pm-cli config set token <网页签发的 token>\n  \
        pm-cli doctor"
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
    /// 诊断连接配置与连通性（供人和 Agent 排查「连不上 / 没权限」）
    Doctor {
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

/// 连接信息的来源。doctor 用它说明「当前到底用的是哪一份配置」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConnectionSource {
    Environment,
    Config,
    Runtime,
}

impl ConnectionSource {
    fn label(self) -> &'static str {
        match self {
            ConnectionSource::Environment => "环境变量 PM_SERVER_URL / PM_AGENT_TOKEN",
            ConnectionSource::Config => "用户配置文件 cli.json",
            ConnectionSource::Runtime => "本机应用 data/runtime.json",
        }
    }
}

fn resolve_connection(
    runtime_path: &std::path::Path,
    config_path: &std::path::Path,
    env_server_url: Option<String>,
    env_token: Option<String>,
) -> Result<(String, String), String> {
    resolve_connection_detailed(runtime_path, config_path, env_server_url, env_token)
        .map(|(server_url, token, _)| (server_url, token))
}

fn resolve_connection_detailed(
    runtime_path: &std::path::Path,
    config_path: &std::path::Path,
    env_server_url: Option<String>,
    env_token: Option<String>,
) -> Result<(String, String, ConnectionSource), String> {
    match (env_server_url, env_token) {
        (Some(server_url), Some(token))
            if !server_url.trim().is_empty() && !token.trim().is_empty() =>
        {
            return Ok((
                validate_server_url(&server_url)?,
                token.trim().to_string(),
                ConnectionSource::Environment,
            ));
        }
        (Some(server_url), None) => {
            return Err(env_pair_message("PM_SERVER_URL", &server_url, "PM_AGENT_TOKEN"))
        }
        (None, Some(token)) => {
            return Err(env_pair_message("PM_AGENT_TOKEN", &token, "PM_SERVER_URL"))
        }
        (Some(server_url), Some(token)) => {
            return Err(blank_environment_message(&server_url, &token))
        }
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
            ConnectionSource::Config,
        ));
    }

    // 走到这里说明环境变量与用户配置都没有设置。远程用户没有本机应用，
    // runtime_path 不存在是预期情况，应引导其配置连接，而不是让他去排查应用。
    let content = std::fs::read_to_string(runtime_path).map_err(|_| not_configured_message())?;
    let info: RuntimeInfo = serde_json::from_str(&content).map_err(|_| {
        "data/runtime.json 内容损坏，请重启 Agents PM Tool 后重试。".to_string()
    })?;
    Ok((
        format!("http://127.0.0.1:{}", info.port),
        info.token,
        ConnectionSource::Runtime,
    ))
}

/// token 脱敏：保留首尾各 4 位，便于人工核对是否用错 token，又不泄露完整值。
fn mask_token(token: &str) -> String {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= 8 {
        return "*".repeat(chars.len());
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}****{tail}")
}

fn service_down(msg: &str) -> ExitCode {
    eprintln!("错误：{msg}");
    ExitCode::from(EXIT_SERVICE_DOWN)
}

/// 连接失败时的排查提示，点明实际目标地址。
fn unreachable_message(base_url: &str) -> String {
    format!(
        "无法连接 {base_url}。请检查服务是否运行、监听范围是否为局域网、\
         端口是否正确，以及防火墙是否放行"
    )
}

/// 未配置连接时的指引。远程用户（下载 skill ZIP、未安装应用）主要走这条路。
fn not_configured_message() -> String {
    "尚未配置连接。请执行 pm-cli config set server-url <服务地址> 与 \
     pm-cli config set token <token>（token 在网页「我的 Agent 访问」面板签发）；\
     也可改用环境变量 PM_SERVER_URL 与 PM_AGENT_TOKEN（两者必须同时设置）"
        .to_string()
}

/// 两个变量都设了、但至少一个为空。要点名哪个为空，并同样给出 unset 出路。
fn blank_environment_message(server_url: &str, token: &str) -> String {
    let url_blank = server_url.trim().is_empty();
    let token_blank = token.trim().is_empty();
    if url_blank && token_blank {
        return "环境变量 PM_SERVER_URL 与 PM_AGENT_TOKEN 均已设置但为空。请填入有效值；\
                若想改用已保存的配置或本机应用自动发现，请先 unset 两者。"
            .to_string();
    }
    if url_blank {
        return "环境变量 PM_SERVER_URL 已设置但为空。请填入有效的服务地址；\
                若想改用已保存的配置或本机应用自动发现，请先 unset PM_SERVER_URL。"
            .to_string();
    }
    "环境变量 PM_AGENT_TOKEN 已设置但为空。请填入有效 token；\
     若想改用已保存的配置或本机应用自动发现，请先 unset PM_AGENT_TOKEN。"
        .to_string()
}

/// 环境变量只设了其中一个时的指引。要点明缺哪个，并同时给出两条出路：补齐另一个，
/// 或清掉已设的那个、改走用户配置 / 本机自动发现——否则用户会以为只能去配环境变量。
fn env_pair_message(set_name: &str, set_value: &str, missing_name: &str) -> String {
    if set_value.trim().is_empty() {
        return format!(
            "环境变量 {set_name} 已设置但为空，且 {missing_name} 未设置。\
             请为两者都填入有效值；若想改用已保存的配置或本机应用自动发现，请先 unset {set_name}。"
        );
    }
    format!(
        "环境变量必须成对设置：{set_name} 已设置，但 {missing_name} 未设置。\
         请补齐 {missing_name}；若想改用已保存的配置或本机应用自动发现，请先 unset {set_name}。"
    )
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
        .map_err(|message| {
            // 这里是「连接信息没解析出来」，还没发起请求，用户最需要的是诊断入口。
            service_down(&format!(
                "{message}\n运行 pm-cli doctor 可查看当前生效的连接来源与修复步骤"
            ))
        })?;
        Ok(Self {
            http: reqwest::Client::new(),
            base: format!("{server_url}/api/agent"),
            token,
        })
    }

    /// 发起请求并返回状态码与响应体。不打印任何内容，便于 doctor 自行组织输出。
    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<(reqwest::StatusCode, serde_json::Value), String> {
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
            .map_err(|_| unreachable_message(&self.base))?;
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        let value = serde_json::from_str(&text).unwrap_or(json!({ "raw": text }));
        Ok((status, value))
    }

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<(reqwest::StatusCode, serde_json::Value), ExitCode> {
        self.send(method, path, body)
            .await
            .map_err(|message| service_down(&message))
    }

    async fn download(&self, path: &str) -> Result<reqwest::Response, ExitCode> {
        let response = self
            .http
            .get(format!("{}{}", self.base, path))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|_| service_down(&unreachable_message(&self.base)))?;
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
    if status.as_u16() == 401 {
        eprintln!(
            "请在网页「我的 Agent 访问」面板重新生成 token，然后执行 pm-cli config set token <新 token>"
        );
    } else if status.as_u16() == 403 {
        eprintln!("请检查该账号在网页端的项目与字段授权；若账号被停用，需联系管理员恢复。");
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
        t["submitter_name"]
            .as_str()
            .or_else(|| t["submitter"].as_str())
            .unwrap_or(""),
        t["description"].as_str().unwrap_or("").replace('\n', " "),
    );
}

fn print_task_detail(t: &serde_json::Value) {
    println!("ID：       {}", t["id"].as_str().unwrap_or(""));
    println!("项目：     {}", t["project"].as_str().unwrap_or(""));
    println!("类型：     {}", t["type"].as_str().unwrap_or(""));
    println!("状态：     {}", t["status"].as_str().unwrap_or(""));
    println!(
        "提交人：   {}",
        t["submitter_name"]
            .as_str()
            .or_else(|| t["submitter"].as_str())
            .unwrap_or("")
    );
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
        // doctor 必须在建连前执行：连不上时它要给出诊断，而不是直接失败。
        Commands::Doctor { json } => return run_doctor(json).await,
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
        Commands::Doctor { .. } => unreachable!(),
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

/// doctor 的结构化结果：既可人读，也可被 Agent 用 --json 解析。
#[derive(Debug, Serialize)]
struct DoctorReport {
    cli_version: String,
    exe_path: String,
    source: Option<ConnectionSource>,
    server_url: Option<String>,
    token_masked: Option<String>,
    config_path: String,
    runtime_path: String,
    reachable: bool,
    status: Option<u16>,
    visible_projects: Option<usize>,
    skill_version: Option<String>,
    problem: Option<String>,
    hints: Vec<String>,
}

const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// skill 目录布局为 <skill>/bin/pm-cli.exe + <skill>/VERSION，据此读取已安装版本。
fn read_skill_version() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let bin_dir = exe.parent()?;
    if bin_dir.file_name()?.to_str()? != "bin" {
        return None;
    }
    std::fs::read_to_string(bin_dir.parent()?.join("VERSION"))
        .ok()
        .map(|value| value.trim().to_string())
}

fn doctor_base() -> DoctorReport {
    DoctorReport {
        cli_version: CLI_VERSION.to_string(),
        exe_path: std::env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "（未知）".to_string()),
        source: None,
        server_url: None,
        token_masked: None,
        config_path: cli_config_path().display().to_string(),
        runtime_path: runtime_path().display().to_string(),
        reachable: false,
        status: None,
        visible_projects: None,
        skill_version: read_skill_version(),
        problem: None,
        hints: Vec::new(),
    }
}

fn status_hint(status: reqwest::StatusCode) -> &'static str {
    match status.as_u16() {
        401 => "token 无效或已吊销",
        403 => "账号被停用或无权访问",
        _ => "服务返回错误",
    }
}

fn failure_hints(status: reqwest::StatusCode) -> Vec<String> {
    match status.as_u16() {
        401 => vec![
            "在网页「我的 Agent 访问」重新生成 token".to_string(),
            "然后执行 pm-cli config set token <新 token>".to_string(),
        ],
        403 => vec![
            "确认 token 所属账号未被停用".to_string(),
            "确认当前项目已授权给该账号".to_string(),
        ],
        _ => vec!["查看应用日志了解服务端错误详情".to_string()],
    }
}

/// 收集诊断信息。无论结论如何都返回完整报告，由调用方决定打印与退出码。
async fn collect_doctor() -> (DoctorReport, Result<(), ExitCode>) {
    let mut report = doctor_base();
    let config_path = cli_config_path();
    let runtime = runtime_path();

    let resolved = resolve_connection_detailed(
        &runtime,
        &config_path,
        std::env::var("PM_SERVER_URL").ok(),
        std::env::var("PM_AGENT_TOKEN").ok(),
    );

    let (server_url, token, source) = match resolved {
        Ok(values) => values,
        Err(message) => {
            report.problem = Some(message);
            let mut hints = Vec::new();
            // runtime.json 存在却读不出有效内容，说明本机装过应用，最直接的修复是重启应用。
            if runtime.exists() {
                hints.push("重启 Agents PM Tool 桌面应用，应用启动时会重新生成 data/runtime.json".to_string());
            }
            hints.push(format!(
                "pm-cli config set server-url <服务地址>（写入 {}）",
                report.config_path
            ));
            hints.push(
                "pm-cli config set token <token>，token 在网页「我的 Agent 访问」面板签发".to_string(),
            );
            hints.push("或改用环境变量 PM_SERVER_URL 与 PM_AGENT_TOKEN（必须成对设置）".to_string());
            report.hints = hints;
            return (report, Err(ExitCode::from(EXIT_SERVICE_DOWN)));
        }
    };

    report.source = Some(source);
    report.server_url = Some(server_url.clone());
    report.token_masked = Some(mask_token(&token));

    let client = Client {
        http: reqwest::Client::new(),
        base: format!("{server_url}/api/agent"),
        token,
    };

    // 用需要鉴权的 /projects 探测：既验证服务可达，也验证 token 是否有效。
    // （不能用 /api/agent/help —— 它免 token，token 错误时同样返回 200，会把无效 token 误判为正常。）
    let (status, value) = match client.send(reqwest::Method::GET, "/projects", None).await {
        Ok(values) => values,
        Err(message) => {
            report.problem = Some(message);
            report.hints = vec![
                "确认 Agents PM Tool 桌面应用正在运行".to_string(),
                "确认服务的监听范围包含当前访问来源".to_string(),
                "确认端口未被占用且防火墙已放行".to_string(),
            ];
            return (report, Err(ExitCode::from(EXIT_SERVICE_DOWN)));
        }
    };
    report.status = Some(status.as_u16());

    if !status.is_success() {
        report.problem = Some(match value["error"]["message"].as_str() {
            Some(message) => format!("服务返回 HTTP {}：{}", status.as_u16(), message),
            None => format!("服务返回 HTTP {}（{}）", status.as_u16(), status_hint(status)),
        });
        report.hints = failure_hints(status);
        let code = if status.as_u16() == 401 || status.as_u16() == 403 {
            EXIT_VALIDATION
        } else {
            EXIT_SERVICE_DOWN
        };
        return (report, Err(ExitCode::from(code)));
    }
    report.reachable = true;
    report.visible_projects = value.as_array().map(|items| items.len());

    // token 有效但看不到任何项目时，Agent 实际无事可做，这点值得单独提示。
    if report.visible_projects == Some(0) {
        report
            .hints
            .push("当前 token 看不到任何项目，请检查网页端的 Agent 项目授权".to_string());
    }
    if let Some(version) = &report.skill_version {
        if version != CLI_VERSION {
            report.hints.push(format!(
                "skill 版本 {version} 与 pm-cli {CLI_VERSION} 不一致，请在网页「我的 Agent 访问」重新安装 skill"
            ));
        }
    }
    (report, Ok(()))
}

fn print_doctor(report: &DoctorReport) {
    println!("pm-cli 诊断");
    println!("  版本：{}", report.cli_version);
    println!("  可执行文件：{}", report.exe_path);
    println!(
        "  连接来源：{}",
        report
            .source
            .map(ConnectionSource::label)
            .unwrap_or("（尚未配置）")
    );
    println!(
        "  服务地址：{}",
        report.server_url.as_deref().unwrap_or("（尚未配置）")
    );
    println!(
        "  Token：{}",
        report
            .token_masked
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("（尚未配置）")
    );
    println!("  连通性：{}", match (report.reachable, report.status) {
        (true, Some(status)) => match report.visible_projects {
            Some(count) => format!("正常（HTTP {status}，可见 {count} 个项目）"),
            None => format!("正常（HTTP {status}）"),
        },
        (false, Some(status)) => format!("HTTP {status}"),
        _ => "未连接".to_string(),
    });
    if let Some(version) = &report.skill_version {
        println!(
            "  skill 版本：{}{}",
            version,
            if version == &report.cli_version {
                "（与 pm-cli 一致）"
            } else {
                "（与 pm-cli 不一致）"
            }
        );
    }
    match &report.problem {
        Some(problem) => println!("  结论：{problem}"),
        None => println!("  结论：连接正常。"),
    }
    for hint in &report.hints {
        println!("  - {hint}");
    }
}

async fn run_doctor(json_output: bool) -> Result<(), ExitCode> {
    let (report, outcome) = collect_doctor().await;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        print_doctor(&report);
    }
    outcome
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
    fn partial_environment_names_missing_counterpart_and_unset_escape() {
        let temp = tempfile::tempdir().unwrap();
        let missing = |name: &str| temp.path().join(name);
        let error = resolve_connection(
            &missing("missing-runtime.json"),
            &missing("missing-cli.json"),
            Some("http://192.168.1.2:17890".into()),
            None,
        )
        .unwrap_err();
        assert!(error.contains("PM_AGENT_TOKEN"), "应点名缺失的变量：{error}");
        assert!(
            error.contains("unset PM_SERVER_URL"),
            "应给出 unset 后改走其它来源的出路：{error}"
        );

        let error = resolve_connection(
            &missing("missing-runtime.json"),
            &missing("missing-cli.json"),
            None,
            Some("token-only".into()),
        )
        .unwrap_err();
        assert!(error.contains("PM_SERVER_URL"), "应点名缺失的变量：{error}");
        assert!(error.contains("unset PM_AGENT_TOKEN"), "应给出出路：{error}");
    }

    #[test]
    fn blank_environment_value_is_reported_as_blank_not_missing() {
        let temp = tempfile::tempdir().unwrap();
        let missing = |name: &str| temp.path().join(name);

        let error = resolve_connection(
            &missing("missing-runtime.json"),
            &missing("missing-cli.json"),
            Some("   ".into()),
            None,
        )
        .unwrap_err();
        assert!(error.contains("已设置但为空"), "空值不应报成未设置：{error}");

        let error = resolve_connection(
            &missing("missing-runtime.json"),
            &missing("missing-cli.json"),
            Some("   ".into()),
            Some("token".into()),
        )
        .unwrap_err();
        assert!(
            error.contains("PM_SERVER_URL 已设置但为空"),
            "应点名为空的那个：{error}"
        );
        assert!(error.contains("unset PM_SERVER_URL"), "应给出出路：{error}");
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
    fn missing_runtime_reports_configuration_guidance() {
        // 远程用户未配置连接时，runtime.json 不存在是预期情况，
        // 报错必须给出可执行命令，而不是让他去排查一个没装过的应用。
        let temp = tempfile::tempdir().unwrap();
        let error = resolve_connection(
            &temp.path().join("missing-runtime.json"),
            &temp.path().join("missing-cli.json"),
            None,
            None,
        )
        .unwrap_err();
        assert!(
            error.contains("config set server-url") && error.contains("config set token"),
            "未配置时应给出可执行的配置命令，实际：{error}"
        );
        assert!(
            !error.contains("data/runtime.json"),
            "未配置时不应把内部文件名抛给用户，实际：{error}"
        );
    }

    #[test]
    fn corrupt_runtime_is_reported_distinctly() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("runtime.json");
        std::fs::write(&runtime, "{ not json").unwrap();
        let error =
            resolve_connection(&runtime, &temp.path().join("missing-cli.json"), None, None)
                .unwrap_err();
        assert!(
            error.contains("内容损坏"),
            "runtime.json 损坏应单独提示重启应用，实际：{error}"
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
    fn doctor_masks_token_without_exposing_middle() {
        assert_eq!(mask_token(""), "");
        assert_eq!(mask_token("abcd"), "****");
        assert_eq!(mask_token("12345678"), "********");
        assert_eq!(mask_token("1234567890"), "1234****7890");
        let masked = mask_token("0123456789abcdef");
        assert!(!masked.contains("5678"));
        assert!(!masked.contains("0123456789abcdef"));
    }

    #[test]
    fn doctor_reports_which_source_won() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("cli.json");
        save_cli_config(
            &config_path,
            &CliConfig {
                server_url: "https://pm.example.test".into(),
                token: "config-token".into(),
            },
        )
        .unwrap();

        // 环境变量优先，doctor 必须如实报告来源，而不是笼统说「已配置」。
        let (_, _, source) = resolve_connection_detailed(
            &temp.path().join("missing-runtime.json"),
            &config_path,
            Some("http://192.168.1.2:17890".into()),
            Some("env-token".into()),
        )
        .unwrap();
        assert_eq!(source, ConnectionSource::Environment);

        // 无环境变量时落到用户配置。
        let (_, _, source) = resolve_connection_detailed(
            &temp.path().join("missing-runtime.json"),
            &config_path,
            None,
            None,
        )
        .unwrap();
        assert_eq!(source, ConnectionSource::Config);

        // 两者都没有时才用本机 runtime.json。
        let runtime_path = temp.path().join("runtime.json");
        std::fs::write(
            &runtime_path,
            r#"{"port":17890,"token":"runtime-token"}"#,
        )
        .unwrap();
        let (server_url, token, source) = resolve_connection_detailed(
            &runtime_path,
            &temp.path().join("missing-cli.json"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(source, ConnectionSource::Runtime);
        assert_eq!(server_url, "http://127.0.0.1:17890");
        assert_eq!(token, "runtime-token");
    }

    #[test]
    fn doctor_source_labels_are_human_readable() {
        assert!(ConnectionSource::Environment
            .label()
            .contains("PM_SERVER_URL"));
        assert!(ConnectionSource::Config.label().contains("cli.json"));
        assert!(ConnectionSource::Runtime.label().contains("runtime.json"));
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
