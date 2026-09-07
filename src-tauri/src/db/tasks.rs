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

fn row_to_task(r: &rusqlite::Row) -> rusqlite::Result<Task> {
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

const SELECT_TASKS: &str = r#"
SELECT t.*, (SELECT COUNT(*) FROM attachments a WHERE a.task_id = t.id) AS attachment_count
FROM tasks t
"#;

pub fn list(conn: &Connection, f: &TaskFilter) -> ApiResult<Vec<Task>> {
    let mut sql = String::from(SELECT_TASKS);
    let mut conds: Vec<String> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    let push_in = |col: &str, items: &[String], conds: &mut Vec<String>, values: &mut Vec<Box<dyn rusqlite::ToSql>>| {
        if !items.is_empty() {
            let marks = items.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            conds.push(format!("t.{col} IN ({marks})"));
            for it in items {
                values.push(Box::new(it.clone()));
            }
        }
    };

    push_in("project", &f.project, &mut conds, &mut values);
    push_in("type", &f.task_type, &mut conds, &mut values);
    push_in("status", &f.status, &mut conds, &mut values);
    push_in("submitter", &f.submitter, &mut conds, &mut values);

    if let Some(kw) = &f.keyword {
        let kw = kw.trim();
        if !kw.is_empty() {
            conds.push("(t.id LIKE ? OR t.description LIKE ?)".into());
            let like = format!("%{kw}%");
            values.push(Box::new(like.clone()));
            values.push(Box::new(like));
        }
    }

    if !conds.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conds.join(" AND "));
    }

    let sort_col = match f.sort_by.as_deref() {
        Some("seq") => "t.seq",
        Some("updated_at") => "t.updated_at",
        Some("finished_at") => "t.finished_at",
        _ => "t.created_at",
    };
    let order = if f.sort_order.as_deref() == Some("asc") { "ASC" } else { "DESC" };
    sql.push_str(&format!(" ORDER BY {sort_col} {order}, t.seq {order}"));

    let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), row_to_task)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
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
        params![id, project, task_type, description, status, finished_at, now],
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
