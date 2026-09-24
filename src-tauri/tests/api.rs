//! 端到端 API 测试：真实拉起 axum 服务（随机端口），覆盖
//! idgen 并发、状态机/完成时间、Agent 权限收窄、级联重命名与引用保护（规划 §8）。

use std::sync::Arc;

use agents_pm_tool_lib::{db, paths, server, settings::Settings};
use serde_json::{json, Value};

struct TestApp {
    base: String,
    token: String,
    http: reqwest::Client,
    core: server::CoreState,
    _handle: server::ServerHandle,
    _tmp: tempfile::TempDir,
}

async fn spawn_app() -> TestApp {
    spawn_app_with_host([127, 0, 0, 1]).await
}

/// pm-cli 现在随 skill 分发，是一个零依赖的 Node 脚本；测试直接跑它，与用户实际用法一致。
/// 需要用特定解释器时（例如 nvm 环境）可以通过 PM_NODE 指定。
fn run_pm_cli(args: &[&str], env: &[(&str, &str)]) -> std::process::Output {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("仓库根目录")
        .join("pm-cli-skill")
        .join("bin")
        .join("pm-cli.mjs");
    assert!(script.is_file(), "未找到 pm-cli 脚本：{}", script.display());
    let node = std::env::var("PM_NODE").unwrap_or_else(|_| "node".to_string());
    let mut command = std::process::Command::new(node);
    command.arg(&script).args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().unwrap_or_else(|error| {
        panic!("运行 pm-cli 失败（集成测试需要 Node.js 18 或更高版本）：{error}")
    })
}

async fn spawn_app_with_host(bind_host: [u8; 4]) -> TestApp {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().to_path_buf();
    let conn = db::open(&paths::db_path(&data_dir)).unwrap();
    let core = Arc::new(server::CoreStateInner::new(
        data_dir,
        conn,
        Settings::default(),
    ));
    let token = core.token.read().await.clone();
    let handle = server::start_server(core.clone(), 0, bind_host).await.unwrap();
    // 0.0.0.0 绑定时用回环地址访问（测试机本机）
    let base = format!("http://127.0.0.1:{}", handle.port);
    let bootstrap = reqwest::Client::new();
    let login = bootstrap
        .post(format!("{base}/api/web/auth/host-login"))
        .header("X-PM-Client", "web")
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200, "主机测试会话创建失败");
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("X-PM-Client", "web".parse().unwrap());
    headers.insert(reqwest::header::COOKIE, cookie.parse().unwrap());
    let http = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();
    TestApp {
        base,
        token,
        http,
        core,
        _handle: handle,
        _tmp: tmp,
    }
}

#[tokio::test]
async fn web_requires_session_and_csrf_header() {
    let app = spawn_app().await;
    let anonymous = reqwest::Client::new();
    let response = anonymous
        .get(format!("{}/api/web/projects", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);

    let response = anonymous
        .post(format!("{}/api/web/auth/register", app.base))
        .json(&json!({"username":"alice","password":"password-123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn register_login_logout_and_session_expiry() {
    let app = spawn_app().await;
    let raw = reqwest::Client::new();
    let register = raw
        .post(format!("{}/api/web/auth/register", app.base))
        .header("X-PM-Client", "web")
        .json(&json!({"username":"alice","password":"password-123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(register.status(), 201);
    assert_eq!(
        register.json::<Value>().await.unwrap()["user"]["role"],
        "user"
    );

    let wrong = raw
        .post(format!("{}/api/web/auth/login", app.base))
        .header("X-PM-Client", "web")
        .json(&json!({"username":"alice","password":"wrong-password"}))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);

    let login = raw
        .post(format!("{}/api/web/auth/login", app.base))
        .header("X-PM-Client", "web")
        .json(&json!({"username":"alice","password":"password-123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let me = raw
        .get(format!("{}/api/web/auth/me", app.base))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);
    assert_eq!(me.json::<Value>().await.unwrap()["username"], "alice");

    let logout = raw
        .post(format!("{}/api/web/auth/logout", app.base))
        .header("X-PM-Client", "web")
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), 204);
    let expired = raw
        .get(format!("{}/api/web/auth/me", app.base))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(expired.status(), 401);
}

impl TestApp {
    fn agent(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}/api/agent{}", self.base, path))
            .bearer_auth(&self.token)
    }
    fn web(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}/api/web{}", self.base, path))
    }
    fn port(&self) -> u16 {
        self.base.rsplit(':').next().unwrap().parse().unwrap()
    }
}

async fn register_user(app: &TestApp, username: &str) -> (Value, reqwest::Client) {
    let raw = reqwest::Client::new();
    let response = raw
        .post(format!("{}/api/web/auth/register", app.base))
        .header("X-PM-Client", "web")
        .json(&json!({"username": username, "password": "password-123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let cookie = response
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let user = response.json::<Value>().await.unwrap()["user"].clone();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("X-PM-Client", "web".parse().unwrap());
    headers.insert(reqwest::header::COOKIE, cookie.parse().unwrap());
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();
    (user, client)
}

#[tokio::test]
async fn host_settings_are_local_host_only_and_persist_valid_changes() {
    let app = spawn_app().await;

    let response = app
        .web(reqwest::Method::GET, "/host-settings")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(body["settings"]["port"], 17890);
    assert_eq!(body["status"]["running"], true);
    assert_eq!(body["status"]["port"].as_u64(), Some(app._handle.port as u64));
    assert!(body["status"]["data_dir"].as_str().unwrap().len() > 3);

    // 主题由独立入口维护；保存设置表单不能用客户端数据覆盖它。
    let appearance = app
        .web(reqwest::Method::PUT, "/appearance")
        .json(&json!({"theme": "dark"}))
        .send()
        .await
        .unwrap();
    assert_eq!(appearance.status(), 204);
    let saved = app
        .web(reqwest::Method::PUT, "/host-settings")
        .json(&json!({
            "port": 17890,
            "autostart": false,
            "close_behavior": "stop_all",
            "listen_scope": "local",
            "agent_server_url": " http://192.168.1.2:17890 "
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), 200);
    let saved = saved.json::<Value>().await.unwrap();
    assert_eq!(saved["settings"]["autostart"], false);
    assert_eq!(saved["settings"]["theme"], "dark");
    assert_eq!(saved["settings"]["agent_server_url"], "http://192.168.1.2:17890");
    assert_eq!(saved["restarted"], false);

    let persisted = std::fs::read_to_string(app._tmp.path().join("settings.json")).unwrap();
    let persisted: Value = serde_json::from_str(&persisted).unwrap();
    assert_eq!(persisted["close_behavior"], "stop_all");
    assert_eq!(persisted["theme"], "dark");

    let invalid = app
        .web(reqwest::Method::PUT, "/host-settings")
        .json(&json!({
            "port": 80,
            "autostart": true,
            "close_behavior": "keep_service",
            "listen_scope": "local",
            "agent_server_url": ""
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), 422);

    let mut restart_events = app.core.settings_restart_events.subscribe();
    let restart = app
        .web(reqwest::Method::PUT, "/host-settings")
        .json(&json!({
            "port": 17891,
            "autostart": false,
            "close_behavior": "stop_all",
            "listen_scope": "local",
            "agent_server_url": ""
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(restart.status(), 200);
    assert_eq!(restart.json::<Value>().await.unwrap()["restarted"], true);
    tokio::time::timeout(std::time::Duration::from_secs(1), restart_events.changed())
        .await
        .expect("网络设置变化应发出服务重启通知")
        .unwrap();

    let (_, regular_user) = register_user(&app, "settings-user").await;
    let forbidden = regular_user
        .get(format!("{}/api/web/host-settings", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(forbidden.status(), 403);
}

#[tokio::test]
async fn agent_help_and_skill_distribution_are_versioned() {
    let app = spawn_app().await;
    let help = app
        .agent(reqwest::Method::GET, "/help")
        .send()
        .await
        .unwrap();
    assert_eq!(help.status(), 200);
    let help = help.json::<Value>().await.unwrap();
    assert_eq!(help["skill"]["payload"], "/api/agent/skill/payload");
    assert_eq!(help["skill"]["installer"], "/api/agent/skill/install.mjs");
    assert!(help["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["path"] == "/api/agent/tasks/{id}/attachments"));
    assert!(help["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["path"] == "/api/agent/attachments/{id}"));
    assert!(help["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["path"] == "/api/agent/permissions"));
    assert!(help["permissions"].as_array().unwrap().iter().any(|item| {
        let text = item.as_str().unwrap_or_default();
        text.contains("本任务的子任务")
            && text.contains("本任务的父级任务")
            && text.contains("验收通过的各级父任务会回到进行中")
    }));

    // 文件清单：网页端「选择目录」与安装脚本共用这一份，必须包含全部 skill 文件。
    let payload = app
        .agent(reqwest::Method::GET, "/skill/payload")
        .send()
        .await
        .unwrap();
    assert_eq!(payload.status(), 200);
    let payload = payload.json::<Value>().await.unwrap();
    assert_eq!(payload["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(payload["directory"], "pm-cli");
    let paths: Vec<&str> = payload["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    for expected in [
        "SKILL.md",
        "VERSION",
        "bin/pm-cli.mjs",
        "bin/pm-cli.cmd",
        "bin/pm-cli",
    ] {
        assert!(paths.contains(&expected), "清单缺少 {expected}");
    }
    // 各前端的目录提示要有 Windows 与 macOS 两种写法，供用户照着选目录。
    let codex = payload["frontends"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "codex")
        .unwrap();
    assert!(codex["macos_paths"][0]
        .as_str()
        .unwrap()
        .ends_with(".agents/skills"));
    assert!(codex["windows_paths"][0]
        .as_str()
        .unwrap()
        .contains("USERPROFILE"));

    // 安装脚本：把清单内嵌成自包含文件，用户下载后跑一次即可。
    let response = app
        .agent(reqwest::Method::GET, "/skill/install.mjs")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("X-PM-Skill-Version")
            .unwrap()
            .to_str()
            .unwrap(),
        env!("CARGO_PKG_VERSION")
    );
    assert!(response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .contains("javascript"));
    let script = response.text().await.unwrap();
    assert!(
        !script.contains("const PAYLOAD = [];"),
        "安装脚本不能带着空载荷发出去"
    );
    assert!(script.contains("const PAYLOAD = {"));
    assert!(script.contains("bin/pm-cli.mjs"));
    assert!(script.contains("pm-cli-install"));
}

#[tokio::test]
async fn agent_help_is_public_but_other_agent_routes_are_not() {
    let app = spawn_app().await;
    let anonymous = reqwest::Client::new();

    // 没有 pm-cli、没有 skill 的 Agent 必须能免 token 读到接入指引。
    let help = anonymous
        .get(format!("{}/api/agent/help", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(help.status(), 200);
    let help = help.json::<Value>().await.unwrap();
    assert_eq!(help["requires_token"], true);
    assert_eq!(help["unauthenticated_access"][0], "/api/agent/help");
    let bootstrap = &help["bootstrap"];
    assert!(bootstrap["summary"].as_str().unwrap().contains("转告用户"));
    assert!(!bootstrap["ask_the_user"].as_array().unwrap().is_empty());
    assert!(!bootstrap["configure"].as_array().unwrap().is_empty());
    // 工具介绍、权限边界与可直接 config set 的地址都必须免 token 可读——
    // Prompt 已不在本地展开这些内容，全部依赖这里。
    assert!(help["introduction"].as_str().unwrap().contains("pm-cli"));
    assert!(!help["permissions"].as_array().unwrap().is_empty());
    assert!(help["server_url"].as_str().unwrap().starts_with("http://"));

    // skill 分发入口与 /help 同级公开：否则「还没有 skill 的 Agent」无从获取它。
    for path in ["/skill/payload", "/skill/install.mjs"] {
        let response = anonymous
            .get(format!("{}/api/agent{path}", app.base))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{path} 应免 token 可读");
    }

    // 其余 Agent 路由仍必须鉴权，避免公开分发入口时顺手放宽了边界。
    for path in ["/tasks", "/projects", "/permissions"] {
        let response = anonymous
            .get(format!("{}/api/agent{path}", app.base))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401, "{path} 应仍需 token");
    }

    // 带 token 时 /help 内容一致，不影响既有调用方。
    let authenticated = app
        .agent(reqwest::Method::GET, "/help")
        .send()
        .await
        .unwrap();
    assert_eq!(authenticated.status(), 200);
    assert_eq!(
        authenticated.json::<Value>().await.unwrap()["skill"]["installer"],
        "/api/agent/skill/install.mjs"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_cli_uses_environment_connection() {
    let app = spawn_app().await;
    let task = create_task(&app, true, "CLI 提交人展示").await;
    let task_id = task["id"].as_str().unwrap();
    let env = [
        ("PM_SERVER_URL", app.base.as_str()),
        ("PM_AGENT_TOKEN", app.token.as_str()),
    ];
    let output = run_pm_cli(&["projects", "--json"], &env);
    assert!(
        output.status.success(),
        "pm-cli failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let projects: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(projects[0]["name"], "default-project");

    let output = run_pm_cli(&["get", task_id], &env);
    assert!(
        output.status.success(),
        "pm-cli failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("提交人：   Agent（主机）"));
    // 未认领的任务负责人显示占位符
    assert!(String::from_utf8_lossy(&output.stdout).contains("负责人：   —"));

    // Agent 推进状态后自动认领，CLI 详情显示负责人
    let claim = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}/status"))
        .json(&json!({"status": "进行中"}))
        .send()
        .await
        .unwrap();
    assert_eq!(claim.status(), 200);
    let output = run_pm_cli(&["get", task_id], &env);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("负责人：   Agent（主机）"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_can_list_and_download_authorized_attachments_read_only() {
    let app = spawn_app().await;
    let task = create_task(&app, false, "Agent 读取附件").await;
    let task_id = task["id"].as_str().unwrap();
    let fixture = b"agent attachment fixture";
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(fixture.to_vec())
            .file_name("需求说明.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let uploaded = app
        .web(
            reqwest::Method::POST,
            &format!("/tasks/{task_id}/attachments"),
        )
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(uploaded.status(), 201);
    let attachment = uploaded.json::<Value>().await.unwrap();
    let attachment_id = attachment["id"].as_str().unwrap();

    let listed = app
        .agent(
            reqwest::Method::GET,
            &format!("/tasks/{task_id}/attachments"),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    let listed = listed.json::<Value>().await.unwrap();
    assert_eq!(listed[0]["filename"], "需求说明.txt");
    assert_eq!(listed[0]["size"], fixture.len());
    assert!(listed[0].get("stored_path").is_none());

    let downloaded = app
        .agent(
            reqwest::Method::GET,
            &format!("/attachments/{attachment_id}"),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(downloaded.status(), 200);
    assert_eq!(
        downloaded
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .unwrap(),
        "attachment; filename*=UTF-8''%E9%9C%80%E6%B1%82%E8%AF%B4%E6%98%8E.txt"
    );
    assert_eq!(downloaded.bytes().await.unwrap().as_ref(), fixture);

    for request in [
        app.agent(
            reqwest::Method::POST,
            &format!("/tasks/{task_id}/attachments"),
        ),
        app.agent(
            reqwest::Method::DELETE,
            &format!("/attachments/{attachment_id}"),
        ),
    ] {
        assert!(request.send().await.unwrap().status().is_client_error());
    }

    let output_dir = tempfile::tempdir().unwrap();
    let target = output_dir.path().join("cli-downloaded.txt");
    let env = [
        ("PM_SERVER_URL", app.base.as_str()),
        ("PM_AGENT_TOKEN", app.token.as_str()),
    ];
    let list_output = run_pm_cli(&["attachments", task_id, "--json"], &env);
    assert!(
        list_output.status.success(),
        "pm-cli attachments failed: {}",
        String::from_utf8_lossy(&list_output.stderr)
    );
    let cli_attachments: Value = serde_json::from_slice(&list_output.stdout).unwrap();
    assert_eq!(cli_attachments[0]["id"], attachment_id);

    let download_output = run_pm_cli(
        &[
            "download",
            attachment_id,
            "--output",
            target.to_str().unwrap(),
            "--json",
        ],
        &env,
    );
    assert!(
        download_output.status.success(),
        "pm-cli download failed: {}",
        String::from_utf8_lossy(&download_output.stderr)
    );
    assert_eq!(std::fs::read(target).unwrap(), fixture);
}

#[tokio::test]
async fn agent_attachment_reads_follow_user_project_visibility() {
    let app = spawn_app().await;
    let task = create_task(&app, false, "附件项目权限").await;
    let task_id = task["id"].as_str().unwrap();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::text("private attachment").file_name("private.txt"),
    );
    let attachment = app
        .web(
            reqwest::Method::POST,
            &format!("/tasks/{task_id}/attachments"),
        )
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let attachment_id = attachment["id"].as_str().unwrap();

    let (alice, alice_http) = register_user(&app, "alice-attachments").await;
    let alice_id = alice["id"].as_str().unwrap();
    let token_response = alice_http
        .post(format!("{}/api/web/me/agent-token", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(token_response.status(), 200);
    let token = token_response.json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let agent = reqwest::Client::new();

    let denied_list = agent
        .get(format!(
            "{}/api/agent/tasks/{task_id}/attachments",
            app.base
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(denied_list.status(), 403);
    let denied_download = agent
        .get(format!(
            "{}/api/agent/attachments/{attachment_id}",
            app.base
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(denied_download.status(), 403);

    let grant = app
        .web(
            reqwest::Method::PUT,
            &format!("/users/{alice_id}/permissions"),
        )
        .json(&json!({"permissions":[
            {"project":"default-project","field":"project_access","allowed_values":null}
        ]}))
        .send()
        .await
        .unwrap();
    assert_eq!(grant.status(), 200);

    let allowed_list = agent
        .get(format!(
            "{}/api/agent/tasks/{task_id}/attachments",
            app.base
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(allowed_list.status(), 200);
    let allowed_download = agent
        .get(format!(
            "{}/api/agent/attachments/{attachment_id}",
            app.base
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(allowed_download.status(), 200);
    assert_eq!(
        allowed_download.bytes().await.unwrap().as_ref(),
        b"private attachment"
    );
}

#[tokio::test]
async fn ordinary_user_permissions_filter_projects_fields_and_values() {
    let app = spawn_app().await;
    let task = create_task(&app, false, "权限任务").await;
    let task_id = task["id"].as_str().unwrap();
    let (alice, alice_http) = register_user(&app, "alice-permissions").await;
    let alice_id = alice["id"].as_str().unwrap();

    let projects = alice_http
        .get(format!("{}/api/web/projects", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(projects.status(), 200);
    assert!(projects
        .json::<Value>()
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
    let denied = alice_http
        .get(format!("{}/api/web/tasks/{task_id}", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);

    let grant = app
        .web(
            reqwest::Method::PUT,
            &format!("/users/{alice_id}/permissions"),
        )
        .json(&json!({"permissions":[
            {"project":"default-project","field":"project_access","allowed_values":null},
            {"project":"default-project","field":"status","allowed_values":["进行中"]}
        ]}))
        .send()
        .await
        .unwrap();
    assert_eq!(grant.status(), 200);

    let visible = alice_http
        .get(format!("{}/api/web/tasks", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(visible.status(), 200);
    assert_eq!(
        visible
            .json::<Value>()
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let allowed = alice_http
        .patch(format!("{}/api/web/tasks/{task_id}", app.base))
        .json(&json!({"status":"进行中"}))
        .send()
        .await
        .unwrap();
    assert_eq!(allowed.status(), 200);
    let denied_value = alice_http
        .patch(format!("{}/api/web/tasks/{task_id}", app.base))
        .json(&json!({"status":"验收通过"}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied_value.status(), 403);
    let denied_field = alice_http
        .patch(format!("{}/api/web/tasks/{task_id}", app.base))
        .json(&json!({"description":"越权改写"}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied_field.status(), 403);
    let unchanged = app
        .web(reqwest::Method::GET, &format!("/tasks/{task_id}"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(unchanged["description"], "权限任务");
}

#[tokio::test]
async fn user_management_role_matrix_and_host_protection() {
    let app = spawn_app().await;
    let (alice, _alice_http) = register_user(&app, "alice-admin-target").await;
    let (bob, bob_http) = register_user(&app, "bob-admin").await;
    let (charlie, _charlie_http) = register_user(&app, "charlie-admin").await;
    let alice_id = alice["id"].as_str().unwrap();
    let bob_id = bob["id"].as_str().unwrap();
    let charlie_id = charlie["id"].as_str().unwrap();

    let promoted = app
        .web(reqwest::Method::PATCH, &format!("/users/{bob_id}"))
        .json(&json!({"role":"admin"}))
        .send()
        .await
        .unwrap();
    assert_eq!(promoted.status(), 200);
    let promoted_peer = app
        .web(reqwest::Method::PATCH, &format!("/users/{charlie_id}"))
        .json(&json!({"role":"admin"}))
        .send()
        .await
        .unwrap();
    assert_eq!(promoted_peer.status(), 200);

    let listed = bob_http
        .get(format!("{}/api/web/users", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    let listed = listed.json::<Value>().await.unwrap();
    assert!(listed
        .as_array()
        .unwrap()
        .iter()
        .all(|user| user["role"] == "user"));

    let admin_disable_admin = bob_http
        .patch(format!("{}/api/web/users/{charlie_id}", app.base))
        .json(&json!({"disabled":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(admin_disable_admin.status(), 403);

    let admin_role_change = bob_http
        .patch(format!("{}/api/web/users/{alice_id}", app.base))
        .json(&json!({"role":"admin"}))
        .send()
        .await
        .unwrap();
    assert_eq!(admin_role_change.status(), 403);
    let admin_disable_user = bob_http
        .patch(format!("{}/api/web/users/{alice_id}", app.base))
        .json(&json!({"disabled":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(admin_disable_user.status(), 200);

    for (method, body) in [
        (reqwest::Method::PATCH, Some(json!({"role":"user"}))),
        (reqwest::Method::PATCH, Some(json!({"disabled":true}))),
        (reqwest::Method::DELETE, None),
    ] {
        let mut request = app.web(method, "/users/host");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), 403);
    }
    let host_password = reqwest::Client::new()
        .post(format!("{}/api/web/auth/login", app.base))
        .header("X-PM-Client", "web")
        .json(&json!({"username":"主机","password":"password-123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(host_password.status(), 403);
}

#[tokio::test]
async fn user_agent_token_ownership_permissions_and_revocation() {
    let app = spawn_app().await;
    let user_task = create_task(&app, false, "用户任务").await;
    let user_task_id = user_task["id"].as_str().unwrap();
    let (alice, alice_http) = register_user(&app, "alice-agent").await;
    let alice_id = alice["id"].as_str().unwrap();
    let grant = app
        .web(
            reqwest::Method::PUT,
            &format!("/users/{alice_id}/permissions"),
        )
        .json(&json!({"permissions":[
            {"project":"default-project","field":"project_access","allowed_values":null},
            {"project":"default-project","field":"task_create","allowed_values":null},
            {"project":"default-project","field":"type","allowed_values":["BUG"]},
            {"project":"default-project","field":"description","allowed_values":null},
            {"project":"default-project","field":"status","allowed_values":["进行中","待验证","已完成","验收通过"]}
        ]}))
        .send()
        .await
        .unwrap();
    assert_eq!(grant.status(), 200);

    let alice_web_task = alice_http
        .post(format!("{}/api/web/tasks", app.base))
        .json(&json!({
            "project":"default-project",
            "type":"BUG",
            "description":"Alice 用户任务"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(alice_web_task.status(), 201);
    let alice_web_task = alice_web_task.json::<Value>().await.unwrap();
    let alice_web_task_id = alice_web_task["id"].as_str().unwrap();
    assert_eq!(alice_web_task["submitter"], "用户");
    assert_eq!(alice_web_task["submitter_name"], "alice-agent");
    assert_eq!(alice_web_task["owner_user_id"], alice_id);

    let access = alice_http
        .post(format!("{}/api/web/me/agent-token", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(access.status(), 200);
    let token = access.json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let agent = reqwest::Client::new();
    let created = agent
        .post(format!("{}/api/agent/tasks", app.base))
        .bearer_auth(&token)
        .json(&json!({"project":"default-project","type":"BUG","description":"Agent owned"}))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let created = created.json::<Value>().await.unwrap();
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["owner_user_id"], alice_id);
    assert_eq!(created["submitter_name"], "Agent（alice-agent）");

    let own_edit = agent
        .patch(format!("{}/api/agent/tasks/{id}/description", app.base))
        .bearer_auth(&token)
        .json(&json!({"description":"更新后的描述"}))
        .send()
        .await
        .unwrap();
    assert_eq!(own_edit.status(), 200);
    let user_edit = agent
        .patch(format!(
            "{}/api/agent/tasks/{user_task_id}/description",
            app.base
        ))
        .bearer_auth(&token)
        .json(&json!({"description":"不应成功"}))
        .send()
        .await
        .unwrap();
    assert_eq!(user_edit.status(), 403);
    let own_user_edit = agent
        .patch(format!(
            "{}/api/agent/tasks/{alice_web_task_id}/description",
            app.base
        ))
        .bearer_auth(&token)
        .json(&json!({"description":"Agent 不得修改同用户的网页任务"}))
        .send()
        .await
        .unwrap();
    assert_eq!(own_user_edit.status(), 403);
    let acceptance = agent
        .patch(format!("{}/api/agent/tasks/{id}/status", app.base))
        .bearer_auth(&token)
        .json(&json!({"status":"验收通过"}))
        .send()
        .await
        .unwrap();
    assert_eq!(acceptance.status(), 403);

    let revoke = alice_http
        .delete(format!("{}/api/web/me/agent-token", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(revoke.status(), 204);
    let revoked = agent
        .get(format!("{}/api/agent/tasks", app.base))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), 401);
}

#[tokio::test]
async fn agent_permissions_default_and_admin_grants_cover_create_patch_and_legacy_routes() {
    let app = spawn_app().await;
    let user_task = create_task(&app, false, "用户任务").await;
    let user_task_id = user_task["id"].as_str().unwrap();

    let default = app.agent(reqwest::Method::GET, "/permissions").send().await.unwrap();
    assert_eq!(default.status(), 200);
    let default = default.json::<Value>().await.unwrap();
    let project = &default["projects"][0];
    assert_eq!(project["can_create"], true);
    assert_eq!(project["edit_fields"], json!(["description", "status", "priority"]));
    assert_eq!(project["allowed_values"]["status"], json!(["进行中", "待验证", "已完成"]));
    assert_eq!(project["description_scope"], "own_agent");

    let denied_create = app.agent(reqwest::Method::POST, "/tasks")
        .json(&json!({"project":"default-project","type":"BUG","description":"新任务","note":"未授权"}))
        .send().await.unwrap();
    assert_eq!(denied_create.status(), 403);
    let denied_patch = app.agent(reqwest::Method::PATCH, &format!("/tasks/{user_task_id}"))
        .json(&json!({"note":"未授权"})).send().await.unwrap();
    assert_eq!(denied_patch.status(), 403);

    let profile = json!({
        "task_create": true,
        "create_fields": ["note","status","priority","predecessor_task_ids","unlock_task_ids"],
        "edit_fields": ["project","type","description","note","status","priority","predecessor_task_ids","unlock_task_ids"],
        "status_values": ["未开始","进行中","待验证","已完成","验收未通过","验收通过","取消"],
        "description_any_task": true
    });
    let saved = app.web(reqwest::Method::PUT, "/users/host/agent-permissions")
        .json(&json!({"permissions":profile})).send().await.unwrap();
    assert_eq!(saved.status(), 200);
    assert_eq!(saved.json::<Value>().await.unwrap()["permissions"], profile);
    let invalid_profile = app.web(reqwest::Method::PUT, "/users/host/agent-permissions")
        .json(&json!({"permissions": {"task_create":true,"create_fields":["id"],"edit_fields":[],"status_values":[],"description_any_task":false}}))
        .send().await.unwrap();
    assert_eq!(invalid_profile.status(), 422);

    let created = app.agent(reqwest::Method::POST, "/tasks")
        .json(&json!({"project":"default-project","type":"BUG","description":"已授权新任务","note":"可写备注","status":"验收通过","priority":"高"}))
        .send().await.unwrap();
    assert_eq!(created.status(), 201);
    let created = created.json::<Value>().await.unwrap();
    let created_id = created["id"].as_str().unwrap();
    assert_eq!(created["status"], "验收通过");
    assert_eq!(created["note"], "可写备注");
    assert!(created["finished_at"].as_str().is_some());

    let patched = app.agent(reqwest::Method::PATCH, &format!("/tasks/{user_task_id}"))
        .json(&json!({"description":"管理员已授权修改用户任务","note":"已更新","predecessor_task_ids":[created_id]}))
        .send().await.unwrap();
    assert_eq!(patched.status(), 200);
    let patched = patched.json::<Value>().await.unwrap();
    assert_eq!(patched["predecessor_task_ids"], json!([created_id]));
    assert_eq!(patched["description"], "管理员已授权修改用户任务");

    let legacy = app.agent(reqwest::Method::PATCH, &format!("/tasks/{created_id}/status"))
        .json(&json!({"status":"取消"})).send().await.unwrap();
    assert_eq!(legacy.status(), 200);
    let mut restricted = profile.clone();
    restricted["edit_fields"] = json!(["note"]);
    let saved = app.web(reqwest::Method::PUT, "/users/host/agent-permissions")
        .json(&json!({"permissions":restricted})).send().await.unwrap();
    assert_eq!(saved.status(), 200);
    let denied_legacy = app.agent(reqwest::Method::PATCH, &format!("/tasks/{created_id}/status"))
        .json(&json!({"status":"进行中"})).send().await.unwrap();
    assert_eq!(denied_legacy.status(), 403);
    let immutable = app.agent(reqwest::Method::PATCH, &format!("/tasks/{created_id}"))
        .json(&json!({"submitter":"用户"})).send().await.unwrap();
    assert!(immutable.status().is_client_error());
}

#[tokio::test]
async fn agent_permissions_intersect_ordinary_user_fields_and_values() {
    let app = spawn_app().await;
    let (alice, alice_http) = register_user(&app, "alice-agent-grants").await;
    let id = alice["id"].as_str().unwrap();
    let web_permissions = json!([
        {"project":"default-project","field":"project_access","allowed_values":null},
        {"project":"default-project","field":"task_create","allowed_values":null},
        {"project":"default-project","field":"type","allowed_values":["BUG"]},
        {"project":"default-project","field":"description","allowed_values":null},
        {"project":"default-project","field":"status","allowed_values":["进行中"]}
    ]);
    let saved = app.web(reqwest::Method::PUT, &format!("/users/{id}/permissions"))
        .json(&json!({"permissions":web_permissions})).send().await.unwrap();
    assert_eq!(saved.status(), 200);
    let agent_profile = json!({
        "task_create":true,"create_fields":["note","status","priority"],
        "edit_fields":["note","status"],
        "status_values":["进行中","已完成"],"description_any_task":false
    });
    let saved = app.web(reqwest::Method::PUT, &format!("/users/{id}/agent-permissions"))
        .json(&json!({"permissions":agent_profile})).send().await.unwrap();
    assert_eq!(saved.status(), 200);
    let ordinary_admin = alice_http.put(format!("{}/api/web/users/{id}/agent-permissions", app.base))
        .json(&json!({"permissions":agent_profile})).send().await.unwrap();
    assert_eq!(ordinary_admin.status(), 403);
    let token = alice_http.post(format!("{}/api/web/me/agent-token", app.base))
        .send().await.unwrap().json::<Value>().await.unwrap()["token"].as_str().unwrap().to_string();
    let agent = reqwest::Client::new();
    let effective = agent.get(format!("{}/api/agent/permissions", app.base))
        .bearer_auth(&token).send().await.unwrap();
    assert_eq!(effective.status(), 200);
    let effective = effective.json::<Value>().await.unwrap();
    let project = &effective["projects"][0];
    assert_eq!(project["allowed_values"]["status"], json!(["进行中"]));
    assert!(!project["create_fields"].as_array().unwrap().contains(&json!("note")));
    assert!(!project["edit_fields"].as_array().unwrap().contains(&json!("note")));

    let denied = agent.post(format!("{}/api/agent/tasks", app.base)).bearer_auth(&token)
        .json(&json!({"project":"default-project","type":"BUG","description":"任务","note":"越权"}))
        .send().await.unwrap();
    assert_eq!(denied.status(), 403);
    let denied_status = agent.post(format!("{}/api/agent/tasks", app.base)).bearer_auth(&token)
        .json(&json!({"project":"default-project","type":"BUG","description":"任务","status":"已完成"}))
        .send().await.unwrap();
    assert_eq!(denied_status.status(), 403);
    let created = agent.post(format!("{}/api/agent/tasks", app.base)).bearer_auth(&token)
        .json(&json!({"project":"default-project","type":"BUG","description":"任务","status":"进行中"}))
        .send().await.unwrap();
    assert_eq!(created.status(), 201);
    let created = created.json::<Value>().await.unwrap();
    let task_id = created["id"].as_str().unwrap();
    let denied_note = agent.patch(format!("{}/api/agent/tasks/{task_id}", app.base)).bearer_auth(&token)
        .json(&json!({"note":"越权"})).send().await.unwrap();
    assert_eq!(denied_note.status(), 403);
}

// ── 负责人：Agent 认领 + 锁定（任务 202609231147000000） ──────────

#[tokio::test]
async fn agent_claims_task_on_start_and_other_agents_are_locked_out() {
    let app = spawn_app().await;
    let task = create_task(&app, false, "负责人认领").await;
    let task_id = task["id"].as_str().unwrap().to_string();
    // 新任务负责人默认为空
    assert_eq!(task["assignee_user_id"], Value::Null);
    assert_eq!(task["assignee_name"], Value::Null);

    // 网页端推进「未开始 → 进行中」不会写入负责人
    let web_started = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{task_id}"))
        .json(&json!({"status":"进行中"}))
        .send().await.unwrap();
    assert_eq!(web_started.status(), 200);
    assert_eq!(web_started.json::<Value>().await.unwrap()["assignee_user_id"], Value::Null);
    // 退回「未开始」，交给 Agent 认领
    let back = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{task_id}"))
        .json(&json!({"status":"未开始"}))
        .send().await.unwrap();
    assert_eq!(back.status(), 200);

    // 未开始任务上不改状态（仅优先级）不会认领
    let not_claimed = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}/priority"))
        .json(&json!({"priority":"高"}))
        .send().await.unwrap();
    assert_eq!(not_claimed.status(), 200);
    assert_eq!(not_claimed.json::<Value>().await.unwrap()["assignee_user_id"], Value::Null);

    // 主机 Agent 把状态从「未开始」推进到「进行中」→ 自动认领为负责人
    let claimed = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}/status"))
        .json(&json!({"status":"进行中"}))
        .send().await.unwrap();
    assert_eq!(claimed.status(), 200);
    let claimed = claimed.json::<Value>().await.unwrap();
    assert_eq!(claimed["assignee_user_id"], "host");
    assert_eq!(claimed["assignee_name"], "Agent（主机）");

    // 另一个 Agent（alice 的 token）：项目与状态都已授权，仍被负责人锁定
    let (alice, alice_http) = register_user(&app, "alice-claim").await;
    let alice_id = alice["id"].as_str().unwrap().to_string();
    let grant = app
        .web(reqwest::Method::PUT, &format!("/users/{alice_id}/permissions"))
        .json(&json!({"permissions":[
            {"project":"default-project","field":"project_access","allowed_values":null},
            {"project":"default-project","field":"status","allowed_values":["进行中","待验证","已完成"]},
            {"project":"default-project","field":"priority","allowed_values":null}
        ]}))
        .send().await.unwrap();
    assert_eq!(grant.status(), 200);
    let token_response = alice_http
        .post(format!("{}/api/web/me/agent-token", app.base))
        .send().await.unwrap();
    assert_eq!(token_response.status(), 200);
    let alice_token = token_response.json::<Value>().await.unwrap()["token"]
        .as_str().unwrap().to_string();
    let alice_agent = reqwest::Client::new();

    // 其它 Agent 不能修改该任务的任何字段（状态/优先级/通用 PATCH 一律 403）
    for (path, body) in [
        (format!("/tasks/{task_id}/status"), json!({"status":"待验证"})),
        (format!("/tasks/{task_id}/priority"), json!({"priority":"低"})),
        (format!("/tasks/{task_id}"), json!({"status":"待验证","priority":"低"})),
    ] {
        let denied = alice_agent
            .patch(format!("{}/api/agent{path}", app.base))
            .bearer_auth(&alice_token)
            .json(&body)
            .send().await.unwrap();
        assert_eq!(denied.status(), 403, "其它 Agent 修改应被拒绝：{path} {body}");
        assert!(denied.json::<Value>().await.unwrap()["error"]["message"]
            .as_str().unwrap().contains("负责人"));
    }
    // 读取不受锁定影响
    let read = alice_agent
        .get(format!("{}/api/agent/tasks/{task_id}", app.base))
        .bearer_auth(&alice_token)
        .send().await.unwrap();
    assert_eq!(read.status(), 200);
    assert_eq!(read.json::<Value>().await.unwrap()["assignee_name"], "Agent（主机）");

    // 负责人本人可以继续推进
    let own = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}/status"))
        .json(&json!({"status":"待验证"}))
        .send().await.unwrap();
    assert_eq!(own.status(), 200);

    // 网页端用户不受负责人锁定限制
    let web_edit = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{task_id}"))
        .json(&json!({"status":"进行中","note":"网页端继续修改"}))
        .send().await.unwrap();
    assert_eq!(web_edit.status(), 200);

    // 普通网页用户没有 assignee 字段授权 → 不能改派
    let denied_assign = alice_http
        .patch(format!("{}/api/web/tasks/{task_id}", app.base))
        .json(&json!({"assignee_user_id": alice_id}))
        .send().await.unwrap();
    assert_eq!(denied_assign.status(), 403);

    // 不存在的负责人 → 422
    let bad_assignee = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{task_id}"))
        .json(&json!({"assignee_user_id":"ghost"}))
        .send().await.unwrap();
    assert_eq!(bad_assignee.status(), 422);

    // 网页端改派给 alice 的 Agent → 主机 Agent 被锁，alice 的 Agent 可以修改
    let reassigned = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{task_id}"))
        .json(&json!({"assignee_user_id": alice_id}))
        .send().await.unwrap();
    assert_eq!(reassigned.status(), 200);
    let reassigned = reassigned.json::<Value>().await.unwrap();
    assert_eq!(reassigned["assignee_user_id"], json!(alice_id));
    assert_eq!(reassigned["assignee_name"], "Agent（alice-claim）");
    let host_denied = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}/status"))
        .json(&json!({"status":"已完成"}))
        .send().await.unwrap();
    assert_eq!(host_denied.status(), 403);
    let alice_allowed = alice_agent
        .patch(format!("{}/api/agent/tasks/{task_id}/status", app.base))
        .bearer_auth(&alice_token)
        .json(&json!({"status":"已完成"}))
        .send().await.unwrap();
    assert_eq!(alice_allowed.status(), 200);

    // 网页端清空负责人 → 任意 Agent 恢复修改权；任务已不在「未开始」，不会再自动认领
    let cleared = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{task_id}"))
        .json(&json!({"assignee_user_id": null}))
        .send().await.unwrap();
    assert_eq!(cleared.status(), 200);
    assert_eq!(cleared.json::<Value>().await.unwrap()["assignee_user_id"], Value::Null);
    let host_again = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}/priority"))
        .json(&json!({"priority":"中"}))
        .send().await.unwrap();
    assert_eq!(host_again.status(), 200);
    assert_eq!(host_again.json::<Value>().await.unwrap()["assignee_user_id"], Value::Null);

    // Agent 不能直接指定负责人（字段不在 Agent 可编辑集合内，请求体直接拒绝）
    let direct = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}"))
        .json(&json!({"assignee_user_id":"host"}))
        .send().await.unwrap();
    assert!(direct.status().is_client_error());

    // permissions 自述中负责人属于不可变字段
    let permissions = app.agent(reqwest::Method::GET, "/permissions").send().await.unwrap();
    let permissions = permissions.json::<Value>().await.unwrap();
    let immutable = permissions["immutable_fields"].as_array().unwrap();
    assert!(immutable.contains(&json!("assignee_user_id")));
    assert!(immutable.contains(&json!("assignee_name")));
}

#[tokio::test]
async fn assignee_claim_also_covers_cancel_and_description_routes() {
    let app = spawn_app().await;
    let (alice, alice_http) = register_user(&app, "alice-claim2").await;
    let alice_id = alice["id"].as_str().unwrap().to_string();
    let grant = app
        .web(reqwest::Method::PUT, &format!("/users/{alice_id}/permissions"))
        .json(&json!({"permissions":[
            {"project":"default-project","field":"project_access","allowed_values":null},
            {"project":"default-project","field":"task_create","allowed_values":null},
            {"project":"default-project","field":"type","allowed_values":["BUG"]},
            {"project":"default-project","field":"description","allowed_values":null},
            {"project":"default-project","field":"status","allowed_values":["进行中","待验证"]}
        ]}))
        .send().await.unwrap();
    assert_eq!(grant.status(), 200);
    let alice_token = alice_http
        .post(format!("{}/api/web/me/agent-token", app.base))
        .send().await.unwrap()
        .json::<Value>().await.unwrap()["token"].as_str().unwrap().to_string();
    let alice_agent = reqwest::Client::new();

    // alice 的 Agent 创建任务（未开始），随后推进到「进行中」→ 认领
    let created = alice_agent
        .post(format!("{}/api/agent/tasks", app.base))
        .bearer_auth(&alice_token)
        .json(&json!({"project":"default-project","type":"BUG","description":"alice 认领"}))
        .send().await.unwrap();
    assert_eq!(created.status(), 201);
    let task = created.json::<Value>().await.unwrap();
    assert_eq!(task["assignee_user_id"], Value::Null);
    let task_id = task["id"].as_str().unwrap().to_string();
    let started = alice_agent
        .patch(format!("{}/api/agent/tasks/{task_id}", app.base))
        .bearer_auth(&alice_token)
        .json(&json!({"status":"进行中"}))
        .send().await.unwrap();
    assert_eq!(started.status(), 200);
    assert_eq!(started.json::<Value>().await.unwrap()["assignee_user_id"], json!(alice_id));

    // 描述路由（旧版兼容入口）同样被负责人锁定拦住
    let host_desc = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}/description"))
        .json(&json!({"description":"其它 Agent 改描述"}))
        .send().await.unwrap();
    assert_eq!(host_desc.status(), 403);
    let own_desc = alice_agent
        .patch(format!("{}/api/agent/tasks/{task_id}/description", app.base))
        .bearer_auth(&alice_token)
        .json(&json!({"description":"负责人自己改"}))
        .send().await.unwrap();
    assert_eq!(own_desc.status(), 200);

    // 负责人账号被删除 → 负责人置空，任务重新对所有 Agent 开放
    let delete = app
        .web(reqwest::Method::DELETE, &format!("/users/{alice_id}"))
        .send().await.unwrap();
    assert_eq!(delete.status(), 204);
    let reopened = app
        .web(reqwest::Method::GET, &format!("/tasks/{task_id}"))
        .send().await.unwrap().json::<Value>().await.unwrap();
    assert_eq!(reopened["assignee_user_id"], Value::Null);
    let host_ok = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}/status"))
        .json(&json!({"status":"待验证"}))
        .send().await.unwrap();
    assert_eq!(host_ok.status(), 200);
}

#[tokio::test]
async fn host_agent_token_persists_across_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().to_path_buf();

    let first = {
        let conn = db::open(&paths::db_path(&data_dir)).unwrap();
        let core = server::CoreStateInner::new(data_dir.clone(), conn, Settings::default());
        let token = core.token.read().await.clone();
        token
    };

    // 模拟应用重启：同一数据目录重新打开数据库、初始化核心状态。
    let second = {
        let conn = db::open(&paths::db_path(&data_dir)).unwrap();
        let core = server::CoreStateInner::new(data_dir.clone(), conn, Settings::default());
        let token = core.token.read().await.clone();
        token
    };
    assert_eq!(first, second, "主机 Agent token 重启后应保持不变");

    // 只有手动重新生成才会更换，且新 token 同样在重启后保持。
    let regenerated = {
        let conn = db::open(&paths::db_path(&data_dir)).unwrap();
        db::users::regenerate_agent_token(&conn, agents_pm_tool_lib::domain::user::HOST_USER_ID)
            .unwrap()
    };
    assert_ne!(regenerated, second, "手动重新生成应更换主机 token");
    let third = {
        let conn = db::open(&paths::db_path(&data_dir)).unwrap();
        let core = server::CoreStateInner::new(data_dir.clone(), conn, Settings::default());
        let token = core.token.read().await.clone();
        token
    };
    assert_eq!(third, regenerated, "重新生成后的 token 重启后也应保持");
}

async fn create_task(app: &TestApp, via_agent: bool, desc: &str) -> Value {
    let body = json!({
        "project": "default-project",
        "type": "新增需求",
        "description": desc,
    });
    let req = if via_agent {
        app.agent(reqwest::Method::POST, "/tasks")
    } else {
        app.web(reqwest::Method::POST, "/tasks")
    };
    let res = req.json(&body).send().await.unwrap();
    assert_eq!(res.status(), 201, "创建任务失败：{:?}", res.text().await);
    res.json().await.unwrap()
}

async fn task_history(app: &TestApp, id: &str) -> Value {
    let response = app.web(reqwest::Method::GET, &format!("/tasks/{id}/history")).send().await.unwrap();
    assert_eq!(response.status(), 200);
    response.json().await.unwrap()
}

async fn patch_for_undo(app: &TestApp, id: &str, patch: Value) -> String {
    let response = app.web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
        .json(&patch).send().await.unwrap();
    assert_eq!(response.status(), 200, "{:?}", response.text().await);
    response.headers()["X-PM-Undo"].to_str().unwrap().into()
}

#[tokio::test]
async fn undo_repeated_edits_keeps_history_and_detects_conflicts() {
    let app = spawn_app().await;
    let task = create_task(&app, false, "original").await;
    let id = task["id"].as_str().unwrap();
    let first = patch_for_undo(&app, id, json!({"description":"one"})).await;
    let second = patch_for_undo(&app, id, json!({"description":"two"})).await;
    let undo = |op: &str| app.web(reqwest::Method::POST, &format!("/task-operations/{op}/undo"));
    assert_eq!(undo(&first).send().await.unwrap().status(), 409);
    for (op, expected) in [(&second, "one"), (&first, "original")] {
        let response = undo(op).send().await.unwrap();
        assert_eq!(response.status(), 200);
        let restored: Value = response.json().await.unwrap();
        assert_eq!(restored[0]["description"], expected);
    }
    assert_eq!(undo(&first).send().await.unwrap().status(), 409);
    let history = task_history(&app, id).await;
    assert_eq!(history["items"][0]["action"], "undo");
    assert_eq!(history["items"].as_array().unwrap().len(), 5);
    let create_id = history["items"][4]["operation_id"].to_string();
    assert_eq!(undo(&create_id).send().await.unwrap().status(), 422);
    let op = patch_for_undo(&app, id, json!({"priority":"高"})).await;
    assert_eq!(app.agent(reqwest::Method::PATCH, &format!("/tasks/{id}/priority"))
        .json(&json!({"priority":"低"})).send().await.unwrap().status(), 200);
    assert_eq!(undo(&op).send().await.unwrap().status(), 409);
    let agent_op = task_history(&app, id).await["items"][0]["operation_id"].to_string();
    assert_eq!(undo(&agent_op).send().await.unwrap().status(), 403);
}

#[tokio::test]
async fn undo_rechecks_owner_project_fields_and_enum_permissions() {
    let app = spawn_app().await;
    let task = create_task(&app, false, "permissions").await;
    let id = task["id"].as_str().unwrap();
    let op = patch_for_undo(&app, id, json!({"note":"host"})).await;
    let (user, http) = register_user(&app, "undo-user").await;
    let endpoint = |op: &str| format!("{}/api/web/task-operations/{op}/undo", app.base);
    assert_eq!(http.post(endpoint(&op)).send().await.unwrap().status(), 403);
    let grant = |values: Value| app.web(reqwest::Method::PUT, &format!("/users/{}/permissions", user["id"].as_str().unwrap()))
        .json(&json!({"permissions":[
            {"project":"default-project","field":"project_access","allowed_values":null},
            {"project":"default-project","field":"status","allowed_values":values}
        ]}));
    assert_eq!(grant(json!(["进行中"])).send().await.unwrap().status(), 200);
    let response = http.patch(format!("{}/api/web/tasks/{id}", app.base)).json(&json!({"status":"进行中"})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let op = response.headers()["X-PM-Undo"].to_str().unwrap().to_string();
    assert_eq!(http.post(endpoint(&op)).send().await.unwrap().status(), 403);
    assert_eq!(grant(Value::Null).send().await.unwrap().status(), 200);
    assert_eq!(http.post(endpoint(&op)).send().await.unwrap().status(), 200);
    let response = http.patch(format!("{}/api/web/tasks/{id}", app.base)).json(&json!({"status":"进行中"})).send().await.unwrap();
    let op = response.headers()["X-PM-Undo"].to_str().unwrap().to_string();
    assert_eq!(app.web(reqwest::Method::PUT, &format!("/users/{}/permissions", user["id"].as_str().unwrap()))
        .json(&json!({"permissions":[]})).send().await.unwrap().status(), 200);
    assert_eq!(http.post(endpoint(&op)).send().await.unwrap().status(), 403);
}

#[tokio::test]
async fn undo_batch_is_atomic_and_relations_restore_cascaded_status() {
    let app = spawn_app().await;
    let parent = create_task(&app, false, "parent").await;
    let child = create_task(&app, false, "child").await;
    let pid = parent["id"].as_str().unwrap(); let cid = child["id"].as_str().unwrap();
    for id in [cid, pid] { patch_for_undo(&app, id, json!({"status":"已完成"})).await; }
    let op = patch_for_undo(&app, pid, json!({"predecessor_task_ids":[cid]})).await;
    let response = app.web(reqwest::Method::POST, &format!("/task-operations/{op}/undo")).send().await.unwrap();
    assert_eq!(response.status(), 200, "{:?}", response.text().await);
    let restored: Value = response.json().await.unwrap();
    let parent = restored.as_array().unwrap().iter().find(|t| t["id"] == pid).unwrap();
    assert_eq!(parent["status"], "已完成"); assert_eq!(parent["predecessor_task_ids"], json!([]));
    let response = app.web(reqwest::Method::POST, "/tasks/batch")
        .json(&json!({"action":"update","ids":[pid,cid,"missing"],"patch":{"priority":"高"}})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let op = response.headers()["X-PM-Undo"].to_str().unwrap().to_string();
    assert_eq!(response.json::<Value>().await.unwrap()["succeeded"], 2);
    let conflict = patch_for_undo(&app, cid, json!({"priority":"低"})).await;
    assert_eq!(app.web(reqwest::Method::POST, &format!("/task-operations/{op}/undo")).send().await.unwrap().status(), 409);
    let parent: Value = app.web(reqwest::Method::GET, &format!("/tasks/{pid}")).send().await.unwrap().json().await.unwrap();
    assert_eq!(parent["priority"], "高");
    assert_eq!(app.web(reqwest::Method::POST, &format!("/task-operations/{conflict}/undo")).send().await.unwrap().status(), 200);
    assert_eq!(app.web(reqwest::Method::POST, &format!("/task-operations/{op}/undo")).send().await.unwrap().status(), 200);
    // Failed status validation rolls back the entire undo, including its audit record.
    let status_op = patch_for_undo(&app, pid, json!({"status":"进行中"})).await;
    patch_for_undo(&app, cid, json!({"status":"未开始"})).await;
    patch_for_undo(&app, pid, json!({"predecessor_task_ids":[cid]})).await;
    let count = task_history(&app, pid).await["items"].as_array().unwrap().len();
    assert_eq!(app.web(reqwest::Method::POST, &format!("/task-operations/{status_op}/undo")).send().await.unwrap().status(), 422);
    assert_eq!(task_history(&app, pid).await["items"].as_array().unwrap().len(), count);
}

#[tokio::test]
async fn history_pagination_and_reorder_undo_keep_values_and_access_boundaries() {
    let app = spawn_app().await;
    let first = create_task(&app, false, "first").await;
    let second = create_task(&app, false, "second").await;
    let id = first["id"].as_str().unwrap();
    for index in 0..51 { patch_for_undo(&app, id, json!({"note":format!("note-{index}")})).await; }
    let page = task_history(&app, id).await;
    assert_eq!(page["items"].as_array().unwrap().len(), 50);
    let cursor = page["next_before"].as_i64().unwrap();
    let more: Value = app.web(reqwest::Method::GET, &format!("/tasks/{id}/history?before={cursor}"))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(more["items"].as_array().unwrap().len(), 2);
    assert!(more["next_before"].is_null());
    assert!(more["items"][0]["operation_id"].as_i64().unwrap() < cursor);
    let response = app.web(reqwest::Method::POST, &format!("/tasks/{id}/reorder"))
        .json(&json!({"prev_id":second["id"]})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let op = response.headers()["X-PM-Undo"].to_str().unwrap().to_string();
    assert_ne!(response.json::<Value>().await.unwrap()["position"], first["position"]);
    let response = app.web(reqwest::Method::POST, &format!("/task-operations/{op}/undo")).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<Value>().await.unwrap()[0]["position"], first["position"]);
    assert_eq!(app.agent(reqwest::Method::POST, &format!("/task-operations/{op}/undo")).send().await.unwrap().status(), 405);
    let raw = reqwest::Client::new();
    assert_eq!(raw.post(format!("{}/api/web/task-operations/{op}/undo", app.base)).send().await.unwrap().status(), 403);
    assert_eq!(raw.post(format!("{}/api/web/task-operations/{op}/undo", app.base)).header("X-PM-Client", "web").send().await.unwrap().status(), 401);
    let deleted = app.web(reqwest::Method::POST, "/tasks/batch")
        .json(&json!({"action":"delete","ids":[id,"missing"]})).send().await.unwrap();
    assert_eq!(deleted.status(), 200);
    assert!(!deleted.headers().contains_key("X-PM-Undo"));
    assert_eq!(deleted.json::<Value>().await.unwrap()["succeeded"], 1);
}

#[tokio::test]
async fn history_records_web_agent_attachments_and_failed_mutations() {
    let app = spawn_app().await;
    let task = create_task(&app, false, "历史测试").await;
    let id = task["id"].as_str().unwrap();
    let initial = task_history(&app, id).await;
    assert_eq!(initial["items"][0]["action"], "create");
    assert!(initial["items"][0]["changes"][0]["before"].is_null());
    let response = app.web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
        .json(&json!({"description":"修改后", "note":"备注"})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let page = task_history(&app, id).await;
    assert_eq!(page["items"][0]["actor_name"], "主机");
    assert_eq!(page["items"][0]["source"], "web");
    assert_eq!(page["items"][0]["changes"][0], json!({"field":"description","before":"历史测试","after":"修改后"}));
    assert_eq!(page["items"][0]["changes"].as_array().unwrap().len(), 2);
    let denied = app.agent(reqwest::Method::PATCH, &format!("/tasks/{id}/description"))
        .json(&json!({"description":"不允许"})).send().await.unwrap();
    assert_eq!(denied.status(), 403);
    assert_eq!(task_history(&app, id).await["items"].as_array().unwrap().len(), 2);
    assert_eq!(app.agent(reqwest::Method::PATCH, &format!("/tasks/{id}/status"))
        .json(&json!({"status":"进行中"})).send().await.unwrap().status(), 200);
    let page = task_history(&app, id).await;
    assert_eq!(page["items"][0]["source"], "agent");
    assert!(page["items"][0]["changes"].as_array().unwrap().iter().any(|c| c["field"] == "assignee_user_id" && c["after"] == "主机"));
    let form = reqwest::multipart::Form::new().part("file", reqwest::multipart::Part::text("test").file_name("history.txt"));
    let response = app.web(reqwest::Method::POST, &format!("/tasks/{id}/attachments")).multipart(form).send().await.unwrap();
    assert_eq!(response.status(), 201);
    let attachment: Value = response.json().await.unwrap();
    let page = task_history(&app, id).await;
    assert_eq!(page["items"][0]["changes"][0]["after"][0]["filename"], "history.txt");
    assert!(!page.to_string().contains("stored_path"));
    assert_eq!(app.web(reqwest::Method::DELETE, &format!("/attachments/{}", attachment["id"].as_str().unwrap())).send().await.unwrap().status(), 204);
    assert_eq!(task_history(&app, id).await["items"][0]["changes"][0]["after"], json!([]));
}

#[tokio::test]
async fn history_filters_hidden_projects_relations_and_records_cascades() {
    let app = spawn_app().await;
    let parent = create_task(&app, false, "父级").await;
    let child = create_task(&app, false, "子级").await;
    let pid = parent["id"].as_str().unwrap();
    let cid = child["id"].as_str().unwrap();
    for id in [cid, pid] {
        assert_eq!(app.web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
            .json(&json!({"status":"已完成"})).send().await.unwrap().status(), 200);
    }
    assert_eq!(app.web(reqwest::Method::PATCH, &format!("/tasks/{pid}"))
        .json(&json!({"predecessor_task_ids":[cid]})).send().await.unwrap().status(), 200);
    let page = task_history(&app, pid).await;
    let changes = page["items"][0]["changes"].as_array().unwrap();
    assert!(changes.iter().any(|c| c["field"] == "status" && c["after"] == "进行中"));
    assert!(changes.iter().any(|c| c["field"] == "predecessor_task_ids"));
    assert_eq!(task_history(&app, cid).await["items"][0]["changes"][0]["field"], "unlock_task_ids");
    let (user, http) = register_user(&app, "history-reader").await;
    let url = format!("{}/api/web/tasks/{pid}/history", app.base);
    assert_eq!(http.get(&url).send().await.unwrap().status(), 403);
    assert_eq!(reqwest::Client::new().get(&url).send().await.unwrap().status(), 401);
    assert_eq!(app.web(reqwest::Method::POST, "/projects").json(&json!({"name":"hidden"})).send().await.unwrap().status(), 201);
    assert_eq!(app.web(reqwest::Method::PATCH, &format!("/tasks/{cid}"))
        .json(&json!({"project":"hidden", "description":"secret"})).send().await.unwrap().status(), 200);
    assert_eq!(app.web(reqwest::Method::PUT, &format!("/users/{}/permissions", user["id"].as_str().unwrap()))
        .json(&json!({"permissions":[{"project":"default-project","field":"project_access","allowed_values":null}]}))
        .send().await.unwrap().status(), 200);
    let history: Value = http.get(&url).send().await.unwrap().json().await.unwrap();
    assert!(!history.to_string().contains(cid));
    assert_eq!(app.web(reqwest::Method::PATCH, &format!("/tasks/{cid}"))
        .json(&json!({"project":"default-project", "description":"公开"})).send().await.unwrap().status(), 200);
    let history: Value = http.get(format!("{}/api/web/tasks/{cid}/history", app.base)).send().await.unwrap().json().await.unwrap();
    assert!(!history.to_string().contains("secret"));
    assert!(!history.to_string().contains("hidden"));
    assert_eq!(app.web(reqwest::Method::PATCH, "/projects/default-project")
        .json(&json!({"new_name":"renamed"})).send().await.unwrap().status(), 200);
    let history: Value = http.get(&url).send().await.unwrap().json().await.unwrap();
    assert!(history["items"].as_array().unwrap().iter().any(|e| e["action"] == "create"));
}

// ── 基础 CRUD + 筛选 ─────────────────────────────────────

#[tokio::test]
async fn web_crud_and_filter() {
    let app = spawn_app().await;
    let t = create_task(&app, false, "网页创建的任务").await;
    assert_eq!(t["submitter"], "用户");
    assert_eq!(t["submitter_name"], "主机");
    assert_eq!(t["owner_user_id"], "host");
    assert_eq!(t["status"], "未开始");
    assert_eq!(t["note"], "");
    // 未显式给优先级时默认「中」
    assert_eq!(t["priority"], "中");
    assert_eq!(t["id"].as_str().unwrap().len(), 18);

    // 网页端可修改备注，且关键词会匹配备注
    let id = t["id"].as_str().unwrap();
    let res = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
        .json(&json!({"note": "仅备注中的检索词"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(
        res.json::<Value>().await.unwrap()["note"],
        "仅备注中的检索词"
    );

    // 网页创建描述可空
    let res = app
        .web(reqwest::Method::POST, "/tasks")
        .json(&json!({
            "project": "default-project",
            "type": "优化",
            "note": "创建时备注"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    assert_eq!(res.json::<Value>().await.unwrap()["note"], "创建时备注");

    // 筛选
    let res = app
        .web(reqwest::Method::GET, "/tasks?type=优化")
        .send()
        .await
        .unwrap();
    let list: Value = res.json().await.unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["type"], "优化");

    // 关键字匹配 ID、描述与备注
    let kw = url::form_urlencoded::byte_serialize("网页创建".as_bytes()).collect::<String>();
    let res = app
        .web(reqwest::Method::GET, &format!("/tasks?keyword={kw}"))
        .send()
        .await
        .unwrap();
    let list: Value = res.json().await.unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);

    let kw =
        url::form_urlencoded::byte_serialize("仅备注中的检索词".as_bytes()).collect::<String>();
    let res = app
        .web(reqwest::Method::GET, &format!("/tasks?keyword={kw}"))
        .send()
        .await
        .unwrap();
    let list: Value = res.json().await.unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);

    // 删除
    let id = t["id"].as_str().unwrap();
    let res = app
        .web(reqwest::Method::DELETE, &format!("/tasks/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    let res = app
        .web(reqwest::Method::GET, &format!("/tasks?keyword={id}"))
        .send()
        .await
        .unwrap();
    let list: Value = res.json().await.unwrap();
    assert!(list.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn task_dependencies_round_trip_and_gate_agent_start() {
    let app = spawn_app().await;
    let predecessor = create_task(&app, false, "子任务").await;
    let unlocked = create_task(&app, false, "父级任务").await;
    let predecessor_id = predecessor["id"].as_str().unwrap();
    let unlocked_id = unlocked["id"].as_str().unwrap();

    let response = app
        .web(reqwest::Method::POST, "/tasks")
        .json(&json!({
            "project": "default-project",
            "type": "新增需求",
            "description": "受子任务约束的任务",
            "predecessor_task_ids": [predecessor_id],
            "unlock_task_ids": [unlocked_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let task: Value = response.json().await.unwrap();
    let task_id = task["id"].as_str().unwrap();
    assert_eq!(task["predecessor_task_ids"], json!([predecessor_id]));
    assert_eq!(task["unlock_task_ids"], json!([unlocked_id]));

    let predecessor_after: Value = app
        .web(reqwest::Method::GET, &format!("/tasks/{predecessor_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(predecessor_after["unlock_task_ids"], json!([task_id]));
    let unlocked_after: Value = app
        .web(reqwest::Method::GET, &format!("/tasks/{unlocked_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(unlocked_after["predecessor_task_ids"], json!([task_id]));

    let blocked = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}/status"))
        .json(&json!({"status": "进行中"}))
        .send()
        .await
        .unwrap();
    assert_eq!(blocked.status(), 422);
    assert!(blocked.json::<Value>().await.unwrap()["error"]["message"]
        .as_str()
        .unwrap()
        .contains("子任务尚未完成"));

    let pending_review = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{predecessor_id}"))
        .json(&json!({"status": "待验证"}))
        .send()
        .await
        .unwrap();
    assert_eq!(pending_review.status(), 200);
    let still_blocked = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}/status"))
        .json(&json!({"status": "进行中"}))
        .send()
        .await
        .unwrap();
    assert_eq!(still_blocked.status(), 422);

    let completed = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{predecessor_id}"))
        .json(&json!({"status": "已完成"}))
        .send()
        .await
        .unwrap();
    assert_eq!(completed.status(), 200);
    let started = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{task_id}/status"))
        .json(&json!({"status": "进行中"}))
        .send()
        .await
        .unwrap();
    assert_eq!(started.status(), 200);

    let cycle = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{predecessor_id}"))
        .json(&json!({"predecessor_task_ids": [task_id]}))
        .send()
        .await
        .unwrap();
    assert_eq!(cycle.status(), 422);

    let project = app
        .web(reqwest::Method::POST, "/projects")
        .json(&json!({"name": "secret-project"}))
        .send()
        .await
        .unwrap();
    assert_eq!(project.status(), 201);
    let secret = app
        .web(reqwest::Method::POST, "/tasks")
        .json(&json!({
            "project": "secret-project",
            "type": "优化",
            "description": "不可见关联任务"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(secret.status(), 201);
    let secret: Value = secret.json().await.unwrap();
    let secret_id = secret["id"].as_str().unwrap();
    let relinked = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{task_id}"))
        .json(&json!({"unlock_task_ids": [unlocked_id, secret_id]}))
        .send()
        .await
        .unwrap();
    assert_eq!(relinked.status(), 200);

    let (alice, alice_http) = register_user(&app, "dependency-viewer").await;
    let alice_id = alice["id"].as_str().unwrap();
    let permissions = app
        .web(
            reqwest::Method::PUT,
            &format!("/users/{alice_id}/permissions"),
        )
        .json(&json!({"permissions":[
            {"project":"default-project","field":"project_access","allowed_values":null},
            {"project":"default-project","field":"unlock_task_ids","allowed_values":null}
        ]}))
        .send()
        .await
        .unwrap();
    assert_eq!(permissions.status(), 200);
    let visible: Value = alice_http
        .get(format!("{}/api/web/tasks/{task_id}", app.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(visible["unlock_task_ids"], json!([unlocked_id]));
    let alice_patch = alice_http
        .patch(format!("{}/api/web/tasks/{task_id}", app.base))
        .json(&json!({"unlock_task_ids": [unlocked_id]}))
        .send()
        .await
        .unwrap();
    let alice_patch_status = alice_patch.status();
    let alice_patch_body = alice_patch.text().await.unwrap();
    assert_eq!(alice_patch_status, 200, "{alice_patch_body}");
    let host_view: Value = app
        .web(reqwest::Method::GET, &format!("/tasks/{task_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(host_view["unlock_task_ids"], json!([unlocked_id, secret_id]));
}

#[tokio::test]
async fn child_progress_gates_completion_and_reopens_ancestors_through_web_agent_and_batch() {
    let app = spawn_app().await;
    let child = create_task(&app, false, "子任务").await;
    let parent = create_task(&app, false, "父级任务").await;
    let grandparent = create_task(&app, false, "上级任务").await;
    let child_id = child["id"].as_str().unwrap().to_string();
    let parent_id = parent["id"].as_str().unwrap().to_string();
    let grandparent_id = grandparent["id"].as_str().unwrap().to_string();

    for (id, child) in [(&parent_id, &child_id), (&grandparent_id, &parent_id)] {
        let linked = app.web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
            .json(&json!({"predecessor_task_ids":[child]})).send().await.unwrap();
        assert_eq!(linked.status(), 200);
    }
    for id in [&child_id, &parent_id] {
        let completed = app.web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
            .json(&json!({"status":"已完成"})).send().await.unwrap();
        assert_eq!(completed.status(), 200);
    }
    let accepted = app.web(reqwest::Method::PATCH, &format!("/tasks/{grandparent_id}"))
        .json(&json!({"status":"验收通过"})).send().await.unwrap();
    assert_eq!(accepted.status(), 200);

    let regressed = app.agent(reqwest::Method::PATCH, &format!("/tasks/{child_id}/status"))
        .json(&json!({"status":"进行中"})).send().await.unwrap();
    assert_eq!(regressed.status(), 200);
    for id in [&parent_id, &grandparent_id] {
        let after: Value = app.web(reqwest::Method::GET, &format!("/tasks/{id}"))
            .send().await.unwrap().json().await.unwrap();
        assert_eq!(after["status"], "进行中");
    }

    let rejected = app.agent(reqwest::Method::PATCH, &format!("/tasks/{parent_id}/status"))
        .json(&json!({"status":"待验证"})).send().await.unwrap();
    assert_eq!(rejected.status(), 422);
    let after: Value = app.web(reqwest::Method::GET, &format!("/tasks/{parent_id}"))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(after["status"], "进行中");

    let batch = app.web(reqwest::Method::POST, "/tasks/batch")
        .json(&json!({"action":"update","ids":[parent_id,grandparent_id],"patch":{"status":"已完成"}}))
        .send().await.unwrap();
    assert_eq!(batch.status(), 200);
    let batch: Value = batch.json().await.unwrap();
    assert_eq!(batch["failed"], 2);
    assert_eq!(batch["succeeded"], 0);
    assert!(batch["results"].as_array().unwrap().iter().all(|item| {
        item["error"]["message"].as_str().unwrap_or_default().contains("子任务")
    }));

    for (id, status) in [(&child_id, "待验证"), (&parent_id, "待验证"), (&grandparent_id, "验收通过")] {
        let ready = app.web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
            .json(&json!({"status":status})).send().await.unwrap();
        assert_eq!(ready.status(), 200);
    }
    let added = app.web(reqwest::Method::POST, "/tasks")
        .json(&json!({
            "project":"default-project",
            "type":"优化",
            "description":"后来增加的子任务",
            "unlock_task_ids":[parent_id]
        })).send().await.unwrap();
    assert_eq!(added.status(), 201);
    for id in [&parent_id, &grandparent_id] {
        let after: Value = app.web(reqwest::Method::GET, &format!("/tasks/{id}"))
            .send().await.unwrap().json().await.unwrap();
        assert_eq!(after["status"], "进行中");
    }
}

// ── 优先级字段（高/中/低，默认中） ──────────────────────

#[tokio::test]
async fn priority_create_patch_filter_group_and_batch() {
    let app = spawn_app().await;

    // 创建时指定优先级
    let res = app
        .web(reqwest::Method::POST, "/tasks")
        .json(&json!({
            "project": "default-project",
            "type": "新增需求",
            "description": "高优先级任务",
            "priority": "高"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let high = res.json::<Value>().await.unwrap();
    assert_eq!(high["priority"], "高");
    let high_id = high["id"].as_str().unwrap().to_string();

    // 非法优先级 → 422
    let res = app
        .web(reqwest::Method::POST, "/tasks")
        .json(&json!({
            "project": "default-project",
            "type": "新增需求",
            "description": "非法优先级",
            "priority": "紧急"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);

    let low = create_task(&app, false, "默认优先级任务").await;
    let low_id = low["id"].as_str().unwrap().to_string();
    assert_eq!(low["priority"], "中");

    // patch 修改优先级
    let res = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{low_id}"))
        .json(&json!({"priority": "低"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.json::<Value>().await.unwrap()["priority"], "低");
    // patch 非法值 → 422
    let res = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{low_id}"))
        .json(&json!({"priority": "最高"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);

    // 筛选 priority=高
    let res = app
        .web(
            reqwest::Method::GET,
            "/tasks?priority=%E9%AB%98", // 「高」
        )
        .send()
        .await
        .unwrap();
    let list: Value = res.json().await.unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], high_id);

    // 分组 group_by=priority（按 高→中→低 业务顺序）
    let res = app
        .web(reqwest::Method::GET, "/tasks/page?group_by=priority")
        .send()
        .await
        .unwrap();
    let page: Value = res.json().await.unwrap();
    let groups = page["groups"].as_array().unwrap();
    assert_eq!(
        groups
            .iter()
            .map(|g| g["value"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["高", "低"]
    );

    // 排序 sort_by=priority：asc 高在前，desc 低在前
    let res = app
        .web(
            reqwest::Method::GET,
            "/tasks?sort_by=priority&sort_order=asc",
        )
        .send()
        .await
        .unwrap();
    let list: Value = res.json().await.unwrap();
    assert_eq!(list[0]["priority"], "高");
    let res = app
        .web(
            reqwest::Method::GET,
            "/tasks?sort_by=priority&sort_order=desc",
        )
        .send()
        .await
        .unwrap();
    let list: Value = res.json().await.unwrap();
    assert_eq!(list[0]["priority"], "低");

    // 批量修改优先级
    let res = app
        .web(reqwest::Method::POST, "/tasks/batch")
        .json(&json!({
            "action": "update",
            "ids": [high_id, low_id],
            "patch": {"priority": "中"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = res.json::<Value>().await.unwrap();
    assert_eq!(body["succeeded"], 2);
    assert_eq!(body["failed"], 0);
}

#[tokio::test]
async fn agent_can_set_priority_but_stays_within_permissions() {
    let app = spawn_app().await;

    // Agent 创建时带优先级
    let res = app
        .agent(reqwest::Method::POST, "/tasks")
        .json(&json!({
            "project": "default-project",
            "type": "BUG",
            "description": "Agent 高优先级",
            "priority": "高"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let task = res.json::<Value>().await.unwrap();
    assert_eq!(task["priority"], "高");
    let id = task["id"].as_str().unwrap().to_string();

    // Agent patch 优先级
    let res = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{id}/priority"))
        .json(&json!({"priority": "低"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.json::<Value>().await.unwrap()["priority"], "低");

    // 非法值 → 422；缺字段 → 422
    for body in [json!({"priority": "特急"}), json!({})] {
        let res = app
            .agent(reqwest::Method::PATCH, &format!("/tasks/{id}/priority"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 422, "应 422：{body}");
    }

    // Agent list 按优先级筛选
    let res = app
        .agent(reqwest::Method::GET, "/tasks?priority=%E4%BD%8E") // 「低」
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let list: Value = res.json().await.unwrap();
    assert!(list
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["priority"] == "低"));

    // 普通用户的 priority 受字段授权收窄
    let (alice, alice_http) = register_user(&app, "alice-priority").await;
    let alice_id = alice["id"].as_str().unwrap();
    let token_response = alice_http
        .post(format!("{}/api/web/me/agent-token", app.base))
        .send()
        .await
        .unwrap();
    let alice_token = token_response.json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let grant = app
        .web(
            reqwest::Method::PUT,
            &format!("/users/{alice_id}/permissions"),
        )
        .json(&json!({"permissions":[
            {"project":"default-project","field":"project_access","allowed_values":null},
            {"project":"default-project","field":"priority","allowed_values":["中"]}
        ]}))
        .send()
        .await
        .unwrap();
    assert_eq!(grant.status(), 200);

    let alice_agent = reqwest::Client::new();
    // 未授权值 → 403
    let denied = alice_agent
        .patch(format!("{}/api/agent/tasks/{id}/priority", app.base))
        .bearer_auth(&alice_token)
        .json(&json!({"priority": "高"}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    // 授权值 → 200
    let allowed = alice_agent
        .patch(format!("{}/api/agent/tasks/{id}/priority", app.base))
        .bearer_auth(&alice_token)
        .json(&json!({"priority": "中"}))
        .send()
        .await
        .unwrap();
    assert_eq!(allowed.status(), 200);
    // 非法权限值写不进授权
    let bad = app
        .web(
            reqwest::Method::PUT,
            &format!("/users/{alice_id}/permissions"),
        )
        .json(&json!({"permissions":[
            {"project":"default-project","field":"priority","allowed_values":["特急"]}
        ]}))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 422);
}

// ── 手动排序（拖动换序） ─────────────────────────────────

#[tokio::test]
async fn manual_reorder() {
    let app = spawn_app().await;
    let a = create_task(&app, false, "排序A").await;
    let b = create_task(&app, false, "排序B").await;
    let c = create_task(&app, false, "排序C").await;
    let ids = |t: &Value| t["id"].as_str().unwrap().to_string();
    let (a, b, c) = (ids(&a), ids(&b), ids(&c));

    let manual_order = || async {
        let res = app
            .web(reqwest::Method::GET, "/tasks?sort_by=manual&sort_order=asc")
            .send()
            .await
            .unwrap();
        let list: Value = res.json().await.unwrap();
        list.as_array().unwrap().iter().map(ids).collect::<Vec<_>>()
    };

    // 默认按创建顺序
    assert_eq!(manual_order().await, vec![a.clone(), b.clone(), c.clone()]);

    // C 拖到 A、B 之间
    let res = app
        .web(reqwest::Method::POST, &format!("/tasks/{c}/reorder"))
        .json(&json!({"prev_id": a, "next_id": b}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(manual_order().await, vec![a.clone(), c.clone(), b.clone()]);

    // A 拖到末尾（只给 prev）
    let res = app
        .web(reqwest::Method::POST, &format!("/tasks/{a}/reorder"))
        .json(&json!({"prev_id": b}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(manual_order().await, vec![c.clone(), b.clone(), a.clone()]);

    // 邻居不存在 → 404
    let res = app
        .web(reqwest::Method::POST, &format!("/tasks/{a}/reorder"))
        .json(&json!({"prev_id": "missing"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);

    // 以创建时间降序重铺 → 手动顺序重置为创建倒序（切入手动排序时的基线）
    let res = app
        .web(reqwest::Method::POST, "/tasks/rebase-order")
        .json(&json!({"sort_by": "created_at", "sort_order": "desc"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    assert_eq!(manual_order().await, vec![c.clone(), b.clone(), a.clone()]);
}

// ── 状态机与完成时间（规划 §4.3） ─────────────────────────

#[tokio::test]
async fn finished_at_rules() {
    let app = spawn_app().await;
    let t = create_task(&app, false, "状态机测试").await;
    let id = t["id"].as_str().unwrap().to_string();
    assert!(t["finished_at"].is_null());

    let patch = |status: &str| {
        let app = &app;
        let id = id.clone();
        let status = status.to_string();
        async move {
            let res = app
                .web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
                .json(&json!({"status": status}))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            res.json::<Value>().await.unwrap()
        }
    };

    // 进入「待验证」→ 记录完成时间
    let t = patch("待验证").await;
    let f1 = t["finished_at"].as_str().unwrap().to_string();
    // 验收未通过 → 不清空
    let t = patch("验收未通过").await;
    assert_eq!(t["finished_at"].as_str().unwrap(), f1);
    // 回到进行中 → 仍不清空
    let t = patch("进行中").await;
    assert_eq!(t["finished_at"].as_str().unwrap(), f1);
    // 再次待验证 → 刷新（latest-wins，同秒可相等，但语义为重新记录）
    let t = patch("待验证").await;
    let f2 = t["finished_at"].as_str().unwrap().to_string();
    assert!(f2 >= f1);
    // 已完成 → 刷新
    let t = patch("已完成").await;
    assert!(t["finished_at"].as_str().unwrap() >= f2.as_str());
    // 验收通过 → 刷新（终态必须记完成时间）
    let f3 = t["finished_at"].as_str().unwrap().to_string();
    let t = patch("验收通过").await;
    assert!(t["finished_at"].as_str().unwrap() >= f3.as_str());
    // 取消 → 不刷新也不清空完成时间
    let t = patch("取消").await;
    assert_eq!(t["status"].as_str().unwrap(), "取消");
    assert!(t["finished_at"].as_str().is_some());
}

/// 验收通过直达路径：未开始 → 验收通过，完成时间必须有值
#[tokio::test]
async fn finished_at_on_direct_acceptance() {
    let app = spawn_app().await;
    let t = create_task(&app, false, "直接验收通过").await;
    let id = t["id"].as_str().unwrap().to_string();
    assert!(t["finished_at"].is_null());

    let res = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
        .json(&json!({"status": "验收通过"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let t = res.json::<Value>().await.unwrap();
    assert_eq!(t["status"].as_str().unwrap(), "验收通过");
    assert!(
        t["finished_at"].as_str().is_some(),
        "验收通过是终态，必须有完成时间"
    );
}

// ── 任务进入待验证/已完成时的桌面提醒（系统通知数据源） ────────

#[tokio::test]
async fn finish_notice_emitted_on_review_and_done_transitions() {
    let app = spawn_app().await;
    let mut rx = app.core.finish_notices.subscribe();
    let no_notice = |rx: &mut tokio::sync::broadcast::Receiver<
        server::finish_notice::FinishNotice,
    >| {
        assert!(
            rx.try_recv().is_err(),
            "不应触发完成提醒（进入待验证/已完成之外的动作）"
        );
    };

    let t = create_task(&app, false, "完成提醒任务").await;
    let id = t["id"].as_str().unwrap().to_string();

    // 未开始 → 进行中：不提醒
    let res = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
        .json(&json!({"status": "进行中"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    no_notice(&mut rx);

    // 网页端：进行中 → 待验证：提醒一条，带任务摘要
    let res = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
        .json(&json!({"status": "待验证"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let notice = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("进入待验证应收到完成提醒")
        .unwrap();
    assert_eq!(notice.id, id);
    assert_eq!(notice.status, "待验证");
    assert_eq!(notice.project, "default-project");
    assert_eq!(notice.description, "完成提醒任务");
    no_notice(&mut rx);

    // 同状态重复保存：不重复提醒
    let res = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
        .json(&json!({"status": "待验证"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    no_notice(&mut rx);

    // 验收类状态不属于提醒范围
    let res = app
        .web(reqwest::Method::PATCH, &format!("/tasks/{id}"))
        .json(&json!({"status": "验收通过"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    no_notice(&mut rx);

    // Agent 推进到已完成同样提醒
    let res = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{id}/status"))
        .json(&json!({"status": "进行中"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    no_notice(&mut rx);
    let res = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{id}/status"))
        .json(&json!({"status": "已完成"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let notice = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("Agent 推进到已完成应收到完成提醒")
        .unwrap();
    assert_eq!(notice.id, id);
    assert_eq!(notice.status, "已完成");
    no_notice(&mut rx);
}

#[tokio::test]
async fn finish_notice_emitted_for_each_task_in_batch_update() {
    let app = spawn_app().await;
    let mut rx = app.core.finish_notices.subscribe();

    let a = create_task(&app, false, "批量提醒A").await;
    let b = create_task(&app, false, "批量提醒B").await;
    let a = a["id"].as_str().unwrap().to_string();
    let b = b["id"].as_str().unwrap().to_string();

    // 批量改优先级：不提醒
    let res = app
        .web(reqwest::Method::POST, "/tasks/batch")
        .json(&json!({
            "action": "update",
            "ids": [&a, &b],
            "patch": {"priority": "高"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert!(rx.try_recv().is_err(), "批量改优先级不应触发完成提醒");

    // 批量改状态到待验证：每个任务一条提醒
    let res = app
        .web(reqwest::Method::POST, "/tasks/batch")
        .json(&json!({
            "action": "update",
            "ids": [&a, &b],
            "patch": {"status": "待验证"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let mut noticed = Vec::new();
    for _ in 0..2 {
        noticed.push(
            tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .expect("批量进入待验证应每个任务各收到一条提醒")
                .unwrap()
                .id,
        );
    }
    noticed.sort();
    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(noticed, expected);
    assert!(rx.try_recv().is_err(), "不应有多余的完成提醒");
}

// ── Agent 权限收窄（规划 §5.4 验收：越权全部 403/422） ────

#[tokio::test]
async fn agent_permission_matrix() {
    let app = spawn_app().await;

    // 无 token → 401
    let res = app
        .http
        .get(format!("{}/api/agent/tasks", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // Agent 创建：三必填缺一不可
    for body in [
        json!({"type": "BUG", "description": "x"}),
        json!({"project": "default-project", "description": "x"}),
        json!({"project": "default-project", "type": "BUG"}),
    ] {
        let res = app
            .agent(reqwest::Method::POST, "/tasks")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 422, "缺字段应 422：{body}");
    }

    // 不存在的项目 → 422 且 details 列出全部选项
    let res = app
        .agent(reqwest::Method::POST, "/tasks")
        .json(&json!({"project": "不存在", "type": "BUG", "description": "x"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);
    let v: Value = res.json().await.unwrap();
    assert!(v["error"]["details"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p == "default-project"));

    let agent_task = create_task(&app, true, "Agent 的任务").await;
    let agent_id = agent_task["id"].as_str().unwrap().to_string();
    assert_eq!(agent_task["submitter"], "Agent");
    assert_eq!(agent_task["submitter_name"], "Agent（主机）");
    assert_eq!(agent_task["note"], "");

    let user_task = create_task(&app, false, "用户的任务").await;
    let user_id = user_task["id"].as_str().unwrap().to_string();

    // Agent 改状态：进行中/待验证/已完成 OK
    for s in ["进行中", "待验证", "已完成"] {
        let res = app
            .agent(reqwest::Method::PATCH, &format!("/tasks/{agent_id}/status"))
            .json(&json!({"status": s}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200, "Agent 切到 {s} 应允许");
    }

    // Agent 切验收类状态 → 403
    for s in ["验收通过", "验收未通过", "未开始", "取消"] {
        let res = app
            .agent(reqwest::Method::PATCH, &format!("/tasks/{agent_id}/status"))
            .json(&json!({"status": s}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 403, "Agent 切到 {s} 应 403");
    }

    // Agent 改自己创建的任务描述 → OK
    let res = app
        .agent(
            reqwest::Method::PATCH,
            &format!("/tasks/{agent_id}/description"),
        )
        .json(&json!({"description": "改一下"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // 空描述（trim 后为空）→ 422，与 create 校验对齐（review P3-2）
    for body in [
        json!({"description": ""}),
        json!({"description": "   "}),
        json!({}),
    ] {
        let res = app
            .agent(
                reqwest::Method::PATCH,
                &format!("/tasks/{agent_id}/description"),
            )
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 422, "空描述应 422：{body}");
    }

    // Agent 改用户任务的描述 → 403
    let res = app
        .agent(
            reqwest::Method::PATCH,
            &format!("/tasks/{user_id}/description"),
        )
        .json(&json!({"description": "越权"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);

    // Agent 不可改项目/类型/备注（路由不存在 → 4xx，不产生变更）
    let res = app
        .agent(reqwest::Method::PATCH, &format!("/tasks/{agent_id}"))
        .json(&json!({"note": "Agent 越权备注"}))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_client_error());
    let res = app
        .agent(reqwest::Method::GET, &format!("/tasks/{agent_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.json::<Value>().await.unwrap()["note"], "");

    // Agent 不可删任务 → 4xx
    let res = app
        .agent(reqwest::Method::DELETE, &format!("/tasks/{agent_id}"))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_client_error());

    // Agent 项目选项只读：GET OK，POST 不可
    let res = app
        .agent(reqwest::Method::GET, "/projects")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let res = app
        .agent(reqwest::Method::POST, "/projects")
        .json(&json!({"name": "hack"}))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_client_error());
}

// ── ID 生成并发唯一（规划 §8：100 并发无重复） ────────────

#[tokio::test]
async fn idgen_concurrent_unique() {
    let app = spawn_app().await;
    let mut handles = Vec::new();
    for i in 0..100 {
        let base = app.base.clone();
        let token = app.token.clone();
        handles.push(tokio::spawn(async move {
            let res = reqwest::Client::new()
                .post(format!("{base}/api/agent/tasks"))
                .bearer_auth(token)
                .json(&json!({
                    "project": "default-project",
                    "type": "优化",
                    "description": format!("并发 {i}"),
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            res.json::<Value>().await.unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        }));
    }
    let mut ids = Vec::new();
    for h in handles {
        ids.push(h.await.unwrap());
    }
    let mut uniq = ids.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(ids.len(), uniq.len(), "ID 出现重复");
    // seq 连续无回退
    let res = app
        .agent(reqwest::Method::GET, "/tasks?sort_by=seq&sort_order=asc")
        .send()
        .await
        .unwrap();
    let list: Value = res.json().await.unwrap();
    let seqs: Vec<i64> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["seq"].as_i64().unwrap())
        .collect();
    assert_eq!(seqs, (1..=100).collect::<Vec<i64>>());
}

// ── 项目选项：级联重命名 + 引用保护（规划 §5.3） ──────────

#[tokio::test]
async fn project_rename_cascade_and_delete_protection() {
    let app = spawn_app().await;

    // 新建选项
    let res = app
        .web(reqwest::Method::POST, "/projects")
        .json(&json!({"name": "项目B", "color": "#34C759"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);

    let t = create_task(&app, false, "属于 default-project").await;
    let id = t["id"].as_str().unwrap().to_string();

    // 重命名 → 存量任务级联更新
    let res = app
        .web(reqwest::Method::PATCH, "/projects/default-project")
        .json(&json!({"new_name": "pm工具"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let res = app
        .web(reqwest::Method::GET, &format!("/tasks?keyword={id}"))
        .send()
        .await
        .unwrap();
    let list: Value = res.json().await.unwrap();
    assert_eq!(list[0]["project"], "pm工具");

    // 被引用 → 删除 409 并提示数量
    let res = app
        .web(reqwest::Method::DELETE, "/projects/pm工具")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 409);
    let v: Value = res.json().await.unwrap();
    assert_eq!(v["error"]["details"]["ref_count"], 1);

    // 未被引用的可以删
    let res = app
        .web(reqwest::Method::DELETE, "/projects/项目B")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // 重命名为已存在名 → 409
    app.web(reqwest::Method::POST, "/projects")
        .json(&json!({"name": "项目C"}))
        .send()
        .await
        .unwrap();
    let res = app
        .web(reqwest::Method::PATCH, "/projects/项目C")
        .json(&json!({"new_name": "pm工具"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 409);
}

// ── 提交人筛选：大类 + 具体用户名（任务 202609101105430000） ──────────

#[tokio::test]
async fn submitter_filter_supports_categories_and_usernames() {
    let app = spawn_app().await;
    // 主机用户的网页任务 + Agent 任务
    let host_web = create_task(&app, false, "主机网页任务").await;
    let host_agent = create_task(&app, true, "主机 Agent 任务").await;
    let host_web_id = host_web["id"].as_str().unwrap().to_string();
    let host_agent_id = host_agent["id"].as_str().unwrap().to_string();

    // alice 的网页任务（用户提交）
    let (alice, alice_http) = register_user(&app, "alice-filter").await;
    let alice_id = alice["id"].as_str().unwrap().to_string();
    let grant = app
        .web(
            reqwest::Method::PUT,
            &format!("/users/{alice_id}/permissions"),
        )
        .json(&json!({"permissions":[
            {"project":"default-project","field":"project_access","allowed_values":null},
            {"project":"default-project","field":"task_create","allowed_values":null},
            {"project":"default-project","field":"type","allowed_values":["新增需求"]},
            {"project":"default-project","field":"description","allowed_values":null}
        ]}))
        .send()
        .await
        .unwrap();
    assert_eq!(grant.status(), 200);
    let alice_task = alice_http
        .post(format!("{}/api/web/tasks", app.base))
        .json(&json!({
            "project": "default-project",
            "type": "新增需求",
            "description": "alice 的用户任务"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(alice_task.status(), 201);
    let alice_task_id = alice_task.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let list_ids = |path: &str| {
        let app = &app;
        let path = path.to_string();
        async move {
            let res = app
                .web(reqwest::Method::GET, &format!("/tasks?{path}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "list {path} 应成功");
            res.json::<Value>()
                .await
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["id"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        }
    };
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();

    // 大类语义不变
    let ids = list_ids(&format!("submitter={}", enc("用户"))).await;
    assert!(ids.contains(&host_web_id) && ids.contains(&alice_task_id) && !ids.contains(&host_agent_id));
    let ids = list_ids(&format!("submitter={}", enc("Agent"))).await;
    assert!(ids.contains(&host_agent_id) && !ids.contains(&host_web_id));

    // 按具体提交人：裸用户名只命中「用户」提交；`Agent（用户名）` 只命中 Agent 提交
    let ids = list_ids(&format!("submitter={}", enc("主机"))).await;
    assert!(
        ids.contains(&host_web_id) && !ids.contains(&host_agent_id) && !ids.contains(&alice_task_id),
        "裸用户名只应命中该账号作为用户提交的任务"
    );
    let ids = list_ids(&format!("submitter={}", enc("Agent（主机）"))).await;
    assert!(
        ids.contains(&host_agent_id) && !ids.contains(&host_web_id) && !ids.contains(&alice_task_id),
        "Agent（用户名）只应命中该账号作为 Agent 提交的任务"
    );
    let ids = list_ids(&format!("submitter={}", enc("alice-filter"))).await;
    assert!(ids.contains(&alice_task_id) && !ids.contains(&host_web_id));

    // 大类 + 具体提交人混选：并集
    let ids = list_ids(&format!("submitter={}&submitter={}", enc("Agent"), enc("alice-filter"))).await;
    assert!(
        ids.contains(&host_agent_id) && ids.contains(&alice_task_id) && !ids.contains(&host_web_id)
    );

    // 分页接口同样接受具体提交人筛选（与 list 同一 filter_sql）
    let res = app
        .web(
            reqwest::Method::GET,
            &format!("/tasks/page?submitter={}", enc("主机")),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let page: Value = res.json().await.unwrap();
    let ids: Vec<String> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&host_web_id) && !ids.contains(&host_agent_id) && !ids.contains(&alice_task_id));

    // 「未知用户」捞回 owner 置空的历史任务（模拟账号已删除）
    {
        let conn = app.core.db.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET owner_user_id=NULL WHERE id=?1",
            [&alice_task_id],
        )
        .unwrap();
    }
    let ids = list_ids(&format!("submitter={}", enc("未知用户"))).await;
    assert!(ids.contains(&alice_task_id) && !ids.contains(&host_web_id));

    // alice 账号还在但任务 owner 已置空 → 她的「用户」组合消失，只剩「未知用户」；删除账号后也一样
    let res = app
        .web(reqwest::Method::GET, "/users/submitter-directory")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let names: Vec<String> = res
        .json::<Value>()
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        vec![
            "Agent（主机）".to_string(),
            "主机".to_string(),
            "未知用户".to_string()
        ]
    );
    let res = app
        .web(reqwest::Method::DELETE, &format!("/users/{alice_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn submitter_directory_lists_active_task_owners() {
    let app = spawn_app().await;
    let (alice, alice_http) = register_user(&app, "alice-dir").await;
    let alice_id = alice["id"].as_str().unwrap().to_string();
    let grant = app
        .web(
            reqwest::Method::PUT,
            &format!("/users/{alice_id}/permissions"),
        )
        .json(&json!({"permissions":[
            {"project":"default-project","field":"project_access","allowed_values":null},
            {"project":"default-project","field":"task_create","allowed_values":null},
            {"project":"default-project","field":"type","allowed_values":["新增需求"]},
            {"project":"default-project","field":"description","allowed_values":null}
        ]}))
        .send()
        .await
        .unwrap();
    assert_eq!(grant.status(), 200);
    let created = alice_http
        .post(format!("{}/api/web/tasks", app.base))
        .json(&json!({
            "project": "default-project",
            "type": "新增需求",
            "description": "alice 的任务"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    // 主机同时有「用户」任务和 Agent 任务
    create_task(&app, false, "主机的任务").await;
    create_task(&app, true, "主机的 Agent 任务").await;

    // 管理员看全量：每个 (owner, submitter) 组合一条
    let res = app
        .web(reqwest::Method::GET, "/users/submitter-directory")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let names: Vec<String> = res
        .json::<Value>()
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        vec![
            "Agent（主机）".to_string(),
            "alice-dir".to_string(),
            "主机".to_string()
        ]
    );

    // 停用 alice → 她的组合从候选名单消失
    let res = app
        .web(reqwest::Method::PATCH, &format!("/users/{alice_id}"))
        .json(&json!({"disabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let res = app
        .web(reqwest::Method::GET, "/users/submitter-directory")
        .send()
        .await
        .unwrap();
    let names: Vec<String> = res
        .json::<Value>()
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["Agent（主机）".to_string(), "主机".to_string()]
    );
}

// ── 删除任务时清理磁盘附件（review P2-1） ────────────────

#[tokio::test]
async fn delete_task_removes_attachment_files() {
    let app = spawn_app().await;
    let data_dir = app._tmp.path().to_path_buf();
    let t = create_task(&app, false, "带附件的任务").await;
    let id = t["id"].as_str().unwrap().to_string();

    // multipart 上传一个 txt 附件
    let part = reqwest::multipart::Part::text("hello attachment")
        .file_name("备注.txt")
        .mime_str("text/plain")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);
    let res = app
        .web(reqwest::Method::POST, &format!("/tasks/{id}/attachments"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let a: Value = res.json().await.unwrap();
    let stored_rel = a["stored_path"].as_str().unwrap().to_string();
    assert!(data_dir.join(&stored_rel).exists(), "附件应已落盘");

    // 删除任务 → 附件文件一并清理
    let res = app
        .web(reqwest::Method::DELETE, &format!("/tasks/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    assert!(
        !data_dir.join(&stored_rel).exists(),
        "删除任务后磁盘附件应被清理"
    );
}

// ── 静态站点托管 ─────────────────────────────────────────
#[tokio::test]
async fn static_site_serves_index() {
    let app = spawn_app().await;
    let res = app.http.get(&app.base).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("Agents PM Tool") || body.contains("app"));
}

// ── 监听范围（listen_scope）─────────────────────────────────
#[tokio::test]
async fn lan_bind_serves_on_all_interfaces() {
    // 绑定 0.0.0.0 时服务正常响应（测试用回环地址访问本机）
    let app = spawn_app_with_host([0, 0, 0, 0]).await;
    let res = app.http.get(&app.base).send().await.unwrap();
    assert_eq!(res.status(), 200);
    // API 也应可达
    let res = app
        .web(reqwest::Method::GET, "/projects")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ── 全局主题（设置窗口与网页界面共用一份） ────────────────

#[tokio::test]
async fn appearance_read_is_public_and_defaults_to_null() {
    let app = spawn_app().await;
    // 未登录也能读：登录页要靠它着色
    let raw = reqwest::Client::new();
    let res = raw
        .get(format!("{}/api/web/appearance", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.json::<Value>().await.unwrap()["theme"], Value::Null);
}

#[tokio::test]
async fn appearance_write_requires_session_and_client_header() {
    let app = spawn_app().await;
    let raw = reqwest::Client::new();
    // 完全匿名的 PUT（无头无会话）先被 X-PM-Client 检查拦下 → 403
    let res = raw
        .put(format!("{}/api/web/appearance", app.base))
        .json(&json!({"theme": "dark"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);

    // 带自定义头但无会话 → 401（写入口需要登录）
    let res = raw
        .put(format!("{}/api/web/appearance", app.base))
        .header("X-PM-Client", "web")
        .json(&json!({"theme": "dark"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // 有会话但缺 X-PM-Client 头 → 403
    let login = raw
        .post(format!("{}/api/web/auth/host-login", app.base))
        .header("X-PM-Client", "web")
        .send()
        .await
        .unwrap();
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let res = raw
        .put(format!("{}/api/web/appearance", app.base))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&json!({"theme": "dark"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn appearance_put_updates_state_and_persists_to_settings_json() {
    let app = spawn_app().await;
    let data_dir = app._tmp.path().to_path_buf();

    // 非法值 → 422
    let res = app
        .web(reqwest::Method::PUT, "/appearance")
        .json(&json!({"theme": "blue"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);

    // 合法写入 → 204，读回新值，settings.json 落盘
    let res = app
        .web(reqwest::Method::PUT, "/appearance")
        .json(&json!({"theme": "dark"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    let res = app
        .web(reqwest::Method::GET, "/appearance")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.json::<Value>().await.unwrap()["theme"], json!("dark"));

    let saved = std::fs::read_to_string(data_dir.join("settings.json")).unwrap();
    let saved: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(saved["theme"], json!("dark"), "主题应写入 settings.json");
}

#[tokio::test]
async fn sse_broadcasts_theme_changed_with_value() {
    let app = spawn_app().await;
    // 先建立 SSE 订阅，再改主题：应收到带值的 theme_changed 事件
    let mut resp = app
        .http
        .get(format!("{}/api/web/events", app.base))
        .send()
        .await
        .unwrap();

    let res = app
        .web(reqwest::Method::PUT, "/appearance")
        .json(&json!({"theme": "light"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    let deadline = std::time::Duration::from_secs(5);
    let mut got = None;
    while let Ok(Ok(Some(chunk))) = tokio::time::timeout(deadline, resp.chunk()).await {
        let text = String::from_utf8_lossy(&chunk).to_string();
        // SSE 帧：event: theme_changed\ndata: {"theme":"light"}
        if text.contains("theme_changed") {
            got = Some(text);
            break;
        }
    }
    let frame = got.expect("5 秒内应收到 theme_changed 事件");
    assert!(
        frame.contains("\"theme\":\"light\"") || frame.contains("\"theme\": \"light\""),
        "事件应携带主题值：{frame}"
    );
}

#[tokio::test]
async fn stopping_server_is_bounded_even_with_an_open_sse_stream() {
    let app = spawn_app().await;
    let stream = app
        .http
        .get(format!("{}/api/web/events", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(stream.status(), 200);

    let TestApp {
        _handle: handle,
        _tmp: temp,
        core,
        http,
        ..
    } = app;
    let _keep_alive = (stream, temp, core, http);
    tokio::time::timeout(std::time::Duration::from_secs(2), handle.stop())
        .await
        .expect("SSE 长连接不应无限阻塞服务重启");
}

#[tokio::test]
async fn test_instances_never_publish_the_user_level_runtime_pointer() {
    // 用户级运行信息是 pm-cli「零配置发现本机服务」的依据，只有真正的桌面应用实例该写它。
    // 测试用的是临时数据目录，一旦也去写，就会把开发机上真实的连接信息换成测试实例的端口和 token。
    let app = spawn_app().await;
    assert!(
        !app.core.publishes_runtime_pointer(),
        "测试实例不得发布用户级运行信息"
    );
}

/// 直接写 HTTP/1.1 请求，用于精确构造 Host 与转发头。
/// （reqwest 会按 URL 自己填 Host，测不了「客户端实际用哪个地址访问」这件事。）
async fn raw_request(
    base: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(base.trim_start_matches("http://"))
        .await
        .unwrap();
    let mut request = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let (head, body) = response
        .split_once("\r\n\r\n")
        .expect("响应缺少头部与正文的分隔");
    let status: u16 = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("响应缺少状态行");
    (status, body.to_string())
}

async fn raw_get(base: &str, path: &str, headers: &[(&str, &str)]) -> String {
    raw_request(base, "GET", path, headers).await.1
}

async fn help_server_url(app: &TestApp, headers: &[(&str, &str)]) -> String {
    let body = raw_get(&app.base, "/api/agent/help", headers).await;
    let help: Value = serde_json::from_str(&body).unwrap();
    help["server_url"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn host_only_actions_reject_tunnelled_requests() {
    // 隧道/反代的代理进程就装在本机，转发过来的连接对端也是回环。
    // 只看对端 IP 的话，任何拿到隧道地址的人都能以主机（超级管理员）身份登录——
    // 这里把三条拦截条件都钉住。
    let app = spawn_app().await;
    let port = app.port().to_string();

    // 本机直连：放行
    let (status, _) = raw_request(
        &app.base,
        "POST",
        "/api/web/auth/host-login",
        &[
            ("host", &format!("127.0.0.1:{port}")),
            ("x-pm-client", "web"),
        ],
    )
    .await;
    assert_eq!(status, 200, "本机直连应能主机登录");

    // ① 带转发头（ngrok / cloudflared 都会加）→ 拒绝
    let (status, _) = raw_request(
        &app.base,
        "POST",
        "/api/web/auth/host-login",
        &[
            ("host", &format!("127.0.0.1:{port}")),
            ("x-pm-client", "web"),
            ("x-forwarded-for", "1.2.3.4"),
            ("x-forwarded-proto", "https"),
        ],
    )
    .await;
    assert_eq!(status, 403, "经隧道的请求不该拿到主机会话");

    // ② Host 不是回环写法（隧道会用公网域名）→ 拒绝
    let (status, _) = raw_request(
        &app.base,
        "POST",
        "/api/web/auth/host-login",
        &[
            ("host", "abc.ngrok-free.dev"),
            ("x-pm-client", "web"),
        ],
    )
    .await;
    assert_eq!(status, 403, "公网 Host 不该拿到主机会话");

    // ③ 工作区设置同理（带上真实的主机会话，确认拦下来的是「非本机直连」而不是没登录）
    let cookie = app
        .web(reqwest::Method::POST, "/auth/host-login")
        .send()
        .await
        .unwrap()
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let (status, _) = raw_request(
        &app.base,
        "GET",
        "/api/web/host-settings",
        &[
            ("host", &format!("127.0.0.1:{port}")),
            ("x-pm-client", "web"),
            ("cookie", &cookie),
            ("x-forwarded-host", "abc.ngrok-free.dev"),
        ],
    )
    .await;
    assert_eq!(status, 403, "经隧道不该读到工作区设置");
}

#[tokio::test]
async fn suggested_address_follows_the_route_the_caller_used() {
    // 局域网访问的用户该看到局域网地址，走 Tailscale 的用户该看到 Tailscale 地址，
    // 经域名或隧道访问的用户该看到那个域名。
    let app = spawn_app().await;
    *app.core.settings.write().unwrap() = Settings {
        listen_scope: "lan".into(),
        ..Settings::default()
    };

    assert_eq!(
        help_server_url(&app, &[("host", "192.168.1.20:17890")]).await,
        "http://192.168.1.20:17890"
    );
    assert_eq!(
        help_server_url(&app, &[("host", "100.101.102.103:17890")]).await,
        "http://100.101.102.103:17890"
    );
    assert_eq!(
        help_server_url(&app, &[("host", "a-b-c.ngrok-free.dev:17890")]).await,
        "http://a-b-c.ngrok-free.dev:17890"
    );
}

#[tokio::test]
async fn suggested_address_restores_the_forwarded_scheme_and_host() {
    // 隧道对外是 https、转发到本机是 http：只有按转发头还原，Agent 才连得上。
    let app = spawn_app().await;
    *app.core.settings.write().unwrap() = Settings {
        listen_scope: "lan".into(),
        ..Settings::default()
    };

    assert_eq!(
        help_server_url(
            &app,
            &[
                ("host", "a-b-c.ngrok-free.dev"),
                ("x-forwarded-proto", "https"),
            ]
        )
        .await,
        "https://a-b-c.ngrok-free.dev"
    );

    // 有些反代会把 Host 改写成 localhost，原始域名只在 X-Forwarded-Host 里
    assert_eq!(
        help_server_url(
            &app,
            &[
                ("host", "localhost:3010"),
                ("x-forwarded-host", "a-b-c.ngrok-free.dev"),
                ("x-forwarded-proto", "https"),
            ]
        )
        .await,
        "https://a-b-c.ngrok-free.dev"
    );

    // 请求里没有可用地址时（公网 IP 之外，例如畸形 Host 或干脆没带），才轮到自动探测
    let detected = help_server_url(&app, &[("host", "0.0.0.0:17890")]).await;
    assert_ne!(detected, "http://0.0.0.0:17890");
    assert!(detected.starts_with("http://"), "{detected}");
}

#[tokio::test]
async fn configured_address_wins_over_the_request_route() {
    // 手工配置是显式覆盖：留给「Agent 不在浏览页面这台机器上」这类自动识别猜不到的情况。
    let app = spawn_app().await;
    *app.core.settings.write().unwrap() = Settings {
        listen_scope: "lan".into(),
        agent_server_url: "https://pm.example.com".into(),
        ..Settings::default()
    };
    assert_eq!(
        help_server_url(&app, &[("host", "192.168.1.20:17890")]).await,
        "https://pm.example.com"
    );
}

#[tokio::test]
async fn loopback_only_listener_never_advertises_another_interface() {
    // 只监听回环时，即便客户端硬塞一个局域网 Host，也不该把它当成建议地址发出去。
    let app = spawn_app().await; // 默认 listen_scope = local
    assert_eq!(
        help_server_url(&app, &[("host", "192.168.1.20:17890")]).await,
        format!("http://127.0.0.1:{}", app.port())
    );
    assert_eq!(
        help_server_url(&app, &[("host", "pm.example.com:17890")]).await,
        format!("http://127.0.0.1:{}", app.port())
    );
}
