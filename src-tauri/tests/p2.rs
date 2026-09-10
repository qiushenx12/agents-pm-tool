use agents_pm_tool_lib::{
    db::{
        self,
        task_page::{self, PageOptions},
        tasks,
    },
    server,
    settings::Settings,
};
use serde_json::{json, Value};
use std::sync::Arc;

fn seed(conn: &mut rusqlite::Connection, count: usize) -> Vec<String> {
    (0..count)
        .map(|index| {
            let task = tasks::create(
                conn,
                &tasks::NewTask {
                    project: "default-project",
                    task_type: if index % 2 == 0 { "优化" } else { "BUG" },
                    description: &format!("验收任务 {index}"),
                    note: "",
                    submitter: "用户",
                    owner_user_id: None,
                },
            )
            .unwrap();
            task.id
        })
        .collect()
}
#[test]
fn pages_are_stable_complete_and_clamped() {
    let mut conn = db::open_memory().unwrap();
    let ids = seed(&mut conn, 205);
    let filter = tasks::TaskFilter {
        sort_by: Some("created_at".into()),
        sort_order: Some("asc".into()),
        ..Default::default()
    };
    let mut all = Vec::new();
    for page in 1..=3 {
        let result = task_page::list_page(
            &conn,
            &filter,
            &PageOptions {
                page,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(result.total, 205);
        assert!(result.items.len() <= 100);
        all.extend(result.items.into_iter().map(|t| t.id));
    }
    assert_eq!(all, ids);
    let last = task_page::list_page(
        &conn,
        &filter,
        &PageOptions {
            page: 999,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(last.page, 3);
    assert_eq!(last.items.len(), 5);
}
#[test]
fn grouping_counts_all_filtered_records_and_anchor_obeys_filters() {
    let mut conn = db::open_memory().unwrap();
    let ids = seed(&mut conn, 205);
    let options = PageOptions {
        page_size: 50,
        group_by: Some("type".into()),
        anchor_id: Some(ids[204].clone()),
        ..Default::default()
    };
    let all = task_page::list_page(&conn, &tasks::TaskFilter::default(), &options).unwrap();
    assert_eq!(all.groups.iter().map(|g| g.count).sum::<i64>(), 205);
    assert!(all
        .groups
        .iter()
        .any(|g| g.value == "优化" && g.count == 103));
    assert_eq!(all.anchor_found, Some(true));
    assert!(all.items.iter().any(|t| t.id == ids[204]));
    let filtered = tasks::TaskFilter {
        task_type: vec!["BUG".into()],
        ..Default::default()
    };
    let result = task_page::list_page(&conn, &filtered, &options).unwrap();
    assert_eq!(result.total, 102);
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.anchor_found, Some(false));
    assert!(result.items.iter().all(|t| t.task_type == "BUG"));
}
#[test]
fn page_options_reject_invalid_limits_and_sql_fields() {
    for query in [
        "page=0",
        "page=-1",
        "page_size=0",
        "page_size=201",
        "page=abc",
        "group_by=id",
        "group_by=status%3BDROP+TABLE+tasks",
    ] {
        assert!(PageOptions::parse(Some(query)).is_err(), "{query}");
    }
    assert!(PageOptions::parse(Some("page=3&page_size=100&group_by=status")).is_ok());
}
#[tokio::test]
async fn batch_reports_partial_success_and_preserves_agent_boundaries() {
    let tmp = tempfile::tempdir().unwrap();
    let mut conn = db::open_memory().unwrap();
    let ids = seed(&mut conn, 3);
    let core = Arc::new(server::CoreStateInner::new(
        tmp.path().into(),
        conn,
        Settings::default(),
    ));
    let token = core.token.read().await.clone();
    let handle = server::start_server(core, 0, [127, 0, 0, 1]).await.unwrap();
    let base = format!("http://127.0.0.1:{}", handle.port);
    let bootstrap = reqwest::Client::new();
    let login = bootstrap
        .post(format!("{base}/api/web/auth/host-login"))
        .header("X-PM-Client", "web")
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
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("X-PM-Client", "web".parse().unwrap());
    headers.insert(reqwest::header::COOKIE, cookie.parse().unwrap());
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();
    let response = client.post(format!("{base}/api/web/tasks/batch")).json(&json!({
        "action":"update", "ids":[ids[0], "missing", ids[1], ids[0]], "patch":{"status":"进行中"}
    })).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["succeeded"], 2);
    assert_eq!(body["failed"], 1);
    assert_eq!(body["results"].as_array().unwrap().len(), 3);
    assert_eq!(body["results"][1]["error"]["code"], "not_found");
    let page: Value = client
        .get(format!(
            "{base}/api/web/tasks/page?status=进行中&page_size=1"
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page["total"], 2);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    let task: Value = client
        .get(format!("{base}/api/web/tasks/{}", ids[0]))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(task["status"], "进行中");
    let old: Value = client
        .get(format!("{base}/api/web/tasks"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(old.as_array().unwrap().len(), 3);
    let agent = client
        .post(format!("{base}/api/agent/tasks/batch"))
        .bearer_auth(token)
        .json(&json!({"action":"delete","ids":[ids[0]]}))
        .send()
        .await
        .unwrap();
    assert!(!agent.status().is_success());
    for invalid in [
        json!({"action":"update","ids":[ids[0]],"patch":{}}),
        json!({"action":"delete","ids":[]}),
        json!({"action":"delete","ids":vec![ids[0].clone();501]}),
    ] {
        assert!(!client
            .post(format!("{base}/api/web/tasks/batch"))
            .json(&invalid)
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
    }
    let upload: Value = client
        .post(format!("{base}/api/web/tasks/{}/attachments", ids[0]))
        .multipart(
            reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::bytes(b"batch attachment fixture".to_vec())
                    .file_name("fixture.txt"),
            ),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let attachment_path = tmp.path().join(upload["stored_path"].as_str().unwrap());
    assert!(attachment_path.exists());
    let deletion: Value = client
        .post(format!("{base}/api/web/tasks/batch"))
        .json(&json!({"action":"delete","ids":[ids[0], "missing"]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(deletion["succeeded"], 1);
    assert_eq!(deletion["failed"], 1);
    assert!(
        !attachment_path.exists(),
        "batch deletion must also clean up attachment files"
    );
    assert_eq!(
        client
            .get(format!("{base}/api/web/tasks/{}", ids[0]))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    handle.stop().await;
}
