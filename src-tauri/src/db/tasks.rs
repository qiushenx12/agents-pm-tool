use rusqlite::{params, Connection};

use crate::domain::idgen;
use crate::domain::task::{self, Task};
use crate::error::{ApiError, ApiResult};

#[derive(Debug, Default)]
pub struct TaskFilter {
    pub project: Vec<String>,
    pub task_type: Vec<String>,
    pub status: Vec<String>,
    pub submitter: Vec<String>,
    pub keyword: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

pub(super) fn row_to_task(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get("id")?,
        seq: r.get("seq")?,
        project: r.get("project")?,
        task_type: r.get("type")?,
        description: r.get("description")?,
        status: r.get("status")?,
        submitter: r.get("submitter")?,
        created_at: r.get("created_at")?,
        finished_at: r.get("finished_at")?,
        updated_at: r.get("updated_at")?,
        attachment_count: r.get("attachment_count")?,
    })
}

pub(super) const SELECT_TASKS: &str = r#"
SELECT t.*, (SELECT COUNT(*) FROM attachments a WHERE a.task_id = t.id) AS attachment_count
FROM tasks t
"#;

pub(super) fn filter_sql(f: &TaskFilter) -> (String, Vec<String>) {
    let mut conds = Vec::new();
    let mut values = Vec::new();
    for (column, items) in [
        ("project", &f.project),
        ("type", &f.task_type),
        ("status", &f.status),
        ("submitter", &f.submitter),
    ] {
        if !items.is_empty() {
            let marks = vec!["?"; items.len()].join(",");
            conds.push(format!("t.{column} IN ({marks})"));
            values.extend(items.iter().cloned());
        }
    }
    if let Some(keyword) = f
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        conds.push("(t.id LIKE ? OR t.description LIKE ?)".into());
        values.push(format!("%{keyword}%"));
        values.push(format!("%{keyword}%"));
    }
    (
        if conds.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conds.join(" AND "))
        },
        values,
    )
}

pub(super) fn sort_sql(f: &TaskFilter) -> String {
    let column = match f.sort_by.as_deref() {
        Some("seq") => "t.seq",
        Some("updated_at") => "t.updated_at",
        Some("finished_at") => "t.finished_at",
        _ => "t.created_at",
    };
    let direction = if f.sort_order.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };
    format!("{column} {direction}, t.seq {direction}")
}

pub fn list(conn: &Connection, f: &TaskFilter) -> ApiResult<Vec<Task>> {
    let (conditions, values) = filter_sql(f);
    let mut stmt = conn.prepare(&format!(
        "{SELECT_TASKS}{conditions} ORDER BY {}",
        sort_sql(f)
    ))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), row_to_task)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get(conn: &Connection, id: &str) -> ApiResult<Task> {
    conn.query_row(
        &format!("{SELECT_TASKS} WHERE t.id = ?1"),
        params![id],
        row_to_task,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => ApiError::not_found(format!("任务 {id} 不存在")),
        other => ApiError::from(other),
    })
}

pub struct NewTask<'a> {
    pub project: &'a str,
    pub task_type: &'a str,
    pub description: &'a str,
    pub submitter: &'a str,
}

/// 校验 + 创建（ID 生成与插入在同一事务，规划 §4.2）
pub fn create(conn: &mut Connection, n: &NewTask) -> ApiResult<Task> {
    validate_project_exists(conn, n.project)?;
    if !task::is_valid_task_type(n.task_type) {
        return Err(ApiError::unprocessable(format!(
            "任务类型不合法：{}，合法取值：{}",
            n.task_type,
            task::TASK_TYPES.join(" / ")
        )));
    }

    let now = task::now_str();
    let tx = conn.transaction()?;
    let (id, seq) = idgen::next_task_id(&tx)?;
    tx.execute(
        "INSERT INTO tasks (id, seq, project, type, description, status, submitter, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, '未开始', ?6, ?7, ?7)",
        params![id, seq, n.project, n.task_type, n.description, n.submitter, now],
    )?;
    tx.commit()?;
    get(conn, &id)
}

pub fn validate_project_exists(conn: &Connection, project: &str) -> ApiResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE name = ?1)",
        params![project],
        |r| r.get(0),
    )?;
    if !exists {
        let options = super::projects::list(conn)?
            .into_iter()
            .map(|p| p.name)
            .collect::<Vec<_>>();
        return Err(ApiError::unprocessable(format!("项目「{project}」不存在"))
            .with_details(serde_json::json!({ "projects": options })));
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct TaskPatch {
    pub project: Option<String>,
    pub task_type: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
}

/// 网页端全量修改。状态走 transition() 统一入口（完成时间规则 §4.3）。
pub fn patch(conn: &mut Connection, id: &str, p: &TaskPatch) -> ApiResult<Task> {
    let current = get(conn, id)?;

    if let Some(project) = &p.project {
        validate_project_exists(conn, project)?;
    }
    if let Some(t) = &p.task_type {
        if !task::is_valid_task_type(t) {
            return Err(ApiError::unprocessable(format!(
                "任务类型不合法：{t}，合法取值：{}",
                task::TASK_TYPES.join(" / ")
            )));
        }
    }
    if let Some(s) = &p.status {
        if !task::is_valid_status(s) {
            return Err(ApiError::unprocessable(format!(
                "状态不合法：{s}，合法取值：{}",
                task::STATUSES.join(" / ")
            )));
        }
    }

    let project = p.project.as_deref().unwrap_or(&current.project);
    let task_type = p.task_type.as_deref().unwrap_or(&current.task_type);
    let description = p.description.as_deref().unwrap_or(&current.description);
    let status = p.status.as_deref().unwrap_or(&current.status);
    let finished_at = task::transition(status, current.finished_at.clone());
    let now = task::now_str();

    conn.execute(
        "UPDATE tasks SET project = ?2, type = ?3, description = ?4, status = ?5,
            finished_at = ?6, updated_at = ?7 WHERE id = ?1",
        params![
            id,
            project,
            task_type,
            description,
            status,
            finished_at,
            now
        ],
    )?;
    get(conn, id)
}

/// 删除任务（attachments 行随 ON DELETE CASCADE 清除）。
/// 返回被级联删除的附件 stored_path 清单，由调用方清理磁盘文件（DB 层不碰文件系统）。
pub fn remove(conn: &Connection, id: &str) -> ApiResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT stored_path FROM attachments WHERE task_id = ?1")?;
    let paths: Vec<String> = stmt
        .query_map(params![id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;

    let n = conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(ApiError::not_found(format!("任务 {id} 不存在")));
    }
    Ok(paths)
}
