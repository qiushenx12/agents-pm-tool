//! pm-cli：Agent 的命令行入口（规划 §5.5）。
//! 只是 HTTP 薄客户端：读 data/runtime.json 拿 port+token，调用 /api/agent/*。
//! 无任何本地 DB 访问能力。

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde::Deserialize;
use serde_json::json;

const EXIT_SERVICE_DOWN: u8 = 3;
const EXIT_VALIDATION: u8 = 2;

#[derive(Parser)]
#[command(
    name = "pm-cli",
    version,
    about = "Agents PM Tool 命令行（供 Agent 使用）",
    after_help = "服务发现：读取 <exe 同目录>/data/runtime.json（port + token），无需配置。\n\
        退出码：0 成功；2 参数/校验失败（stderr 给出中文原因与合法取值）；3 服务未启动。\n\
        权限边界：Agent 不可修改项目/类型、不可删任务、不可操作附件、不可切验收类状态（验收通过/未通过），\n\
        只能修改自己（submitter=Agent）创建的任务描述；项目选项仅可只读（projects 子命令）。\n\n\
        示例：\n  \
        pm-cli create --project agents-pm-tool --type BUG --description \"登录页白屏\"\n  \
        pm-cli list --status 进行中 --json\n  \
        pm-cli status 202609021050340001 --to 待验证"
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
}

#[derive(Deserialize)]
struct RuntimeInfo {
    port: u16,
    token: String,
}

fn runtime_path() -> PathBuf {
    // 与 app 同规则：<exe 同目录>/data/runtime.json（dev 模式回退项目根）
    agents_pm_tool_lib::paths::runtime_path(&agents_pm_tool_lib::paths::data_dir_lossy())
}

fn service_down(msg: &str) -> ExitCode {
    eprintln!("错误：{msg}，服务未启动，请先打开 Agents PM Tool");
    ExitCode::from(EXIT_SERVICE_DOWN)
}

struct Client {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Client {
    fn new() -> Result<Self, ExitCode> {
        let path = runtime_path();
        let content = std::fs::read_to_string(&path)
            .map_err(|_| service_down("未找到 data/runtime.json"))?;
        let info: RuntimeInfo = serde_json::from_str(&content)
            .map_err(|_| service_down("data/runtime.json 内容损坏"))?;
        Ok(Self {
            http: reqwest::Client::new(),
            base: format!("http://127.0.0.1:{}/api/agent", info.port),
            token: info.token,
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
            .map_err(|_| service_down("无法连接本地服务"))?;
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        let value = serde_json::from_str(&text).unwrap_or(json!({ "raw": text }));
        Ok((status, value))
    }
}

fn fail_from_server(status: reqwest::StatusCode, v: &serde_json::Value) -> ExitCode {
    let msg = v["error"]["message"]
        .as_str()
        .unwrap_or("未知错误");
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
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

async fn run(cli: Cli) -> Result<(), ExitCode> {
    let client = Client::new()?;

    match cli.command {
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
            let (status, v) = client.call(reqwest::Method::POST, "/tasks", Some(body)).await?;
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
                }
            }
            Ok(())
        }
    }
}
