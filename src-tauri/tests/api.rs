//! 端到端 API 测试：真实拉起 axum 服务（随机端口），覆盖
//! idgen 并发、状态机/完成时间、Agent 权限收窄、级联重命名与引用保护（规划 §8）。

use std::sync::Arc;

use agents_pm_tool_lib::{db, paths, server, settings::Settings};
use serde_json::{json, Value};

struct TestApp {
    base: String,
    token: String,
    http: reqwest::Client,
    _handle: server::ServerHandle,
    _tmp: tempfile::TempDir,
}

async fn spawn_app() -> TestApp {
    spawn_app_with_host([127, 0, 0, 1]).await
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
    let token = "test-token".to_string();
    *core.token.write().await = token.clone();
    let handle = server::start_server(core, 0, bind_host).await.unwrap();
    // 0.0.0.0 绑定时用回环地址访问（测试机本机）
    let base = format!("http://127.0.0.1:{}", handle.port);
    TestApp {
        base,
        token,
        http: reqwest::Client::new(),
        _handle: handle,
        _tmp: tmp,
    }
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
}

async fn create_task(app: &TestApp, via_agent: bool, desc: &str) -> Value {
    let body = json!({
        "project": "agents-pm-tool",
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

// ── 基础 CRUD + 筛选 ─────────────────────────────────────

#[tokio::test]
async fn web_crud_and_filter() {
    let app = spawn_app().await;
    let t = create_task(&app, false, "网页创建的任务").await;
    assert_eq!(t["submitter"], "用户");
    assert_eq!(t["status"], "未开始");
    assert_eq!(t["note"], "");
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
            "project": "agents-pm-tool",
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
        list.as_array()
            .unwrap()
            .iter()
            .map(ids)
            .collect::<Vec<_>>()
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
        json!({"project": "agents-pm-tool", "description": "x"}),
        json!({"project": "agents-pm-tool", "type": "BUG"}),
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
        .any(|p| p == "agents-pm-tool"));

    let agent_task = create_task(&app, true, "Agent 的任务").await;
    let agent_id = agent_task["id"].as_str().unwrap().to_string();
    assert_eq!(agent_task["submitter"], "Agent");
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
    for s in ["验收通过", "验收未通过", "未开始"] {
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
    for body in [json!({"description": ""}), json!({"description": "   "}), json!({})] {
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
    let res = app.agent(reqwest::Method::GET, "/projects").send().await.unwrap();
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
                    "project": "agents-pm-tool",
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

    let t = create_task(&app, false, "属于 agents-pm-tool").await;
    let id = t["id"].as_str().unwrap().to_string();

    // 重命名 → 存量任务级联更新
    let res = app
        .web(
            reqwest::Method::PATCH,
            "/projects/agents-pm-tool",
        )
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
    let res = app.web(reqwest::Method::GET, "/projects").send().await.unwrap();
    assert_eq!(res.status(), 200);
}
