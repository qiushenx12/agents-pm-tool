use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::domain::task::now_str;
use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub color: String,
    pub sort_order: i64,
    pub created_at: String,
}

fn row_to_project(r: &rusqlite::Row) -> rusqlite::Result<Project> {
    Ok(Project {
        name: r.get("name")?,
        color: r.get("color")?,
        sort_order: r.get("sort_order")?,
        created_at: r.get("created_at")?,
    })
}

pub fn list(conn: &Connection) -> ApiResult<Vec<Project>> {
    let mut stmt =
        conn.prepare("SELECT * FROM projects ORDER BY sort_order ASC, created_at ASC")?;
    let rows = stmt.query_map([], row_to_project)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn create(conn: &Connection, name: &str, color: Option<&str>) -> ApiResult<Project> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ApiError::unprocessable("项目选项名不能为空"));
    }
    let max_order: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) FROM projects",
        [],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO projects (name, color, sort_order, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![name, color.unwrap_or("#007AFF"), max_order + 1, now_str()],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            ApiError::conflict(format!("项目选项「{name}」已存在"))
        }
        other => ApiError::from(other),
    })?;
    get(conn, name)
}

pub fn get(conn: &Connection, name: &str) -> ApiResult<Project> {
    conn.query_row(
        "SELECT * FROM projects WHERE name = ?1",
        params![name],
        row_to_project,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            ApiError::not_found(format!("项目选项「{name}」不存在"))
        }
        other => ApiError::from(other),
    })
}

#[derive(Debug, Default)]
pub struct ProjectPatch {
    pub new_name: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i64>,
}

/// 重命名级联更新存量任务（事务，规划 §5.3）
pub fn patch(conn: &mut Connection, name: &str, p: &ProjectPatch) -> ApiResult<Project> {
    let current = get(conn, name)?;

    let new_name = p
        .new_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&current.name)
        .to_string();

    if new_name != current.name {
        if get(conn, &new_name).is_ok() {
            return Err(ApiError::conflict(format!("项目选项「{new_name}」已存在")));
        }
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE projects SET name = ?2, color = ?3, sort_order = ?4 WHERE name = ?1",
            params![
                name,
                new_name,
                p.color.as_deref().unwrap_or(&current.color),
                p.sort_order.unwrap_or(current.sort_order),
            ],
        )?;
        tx.execute(
            "UPDATE tasks SET project = ?2, updated_at = ?3 WHERE project = ?1",
            params![name, new_name, now_str()],
        )?;
        tx.commit()?;
    } else {
        conn.execute(
            "UPDATE projects SET color = ?2, sort_order = ?3 WHERE name = ?1",
            params![
                name,
                p.color.as_deref().unwrap_or(&current.color),
                p.sort_order.unwrap_or(current.sort_order),
            ],
        )?;
    }
    get(conn, &new_name)
}

/// 被任务引用的选项不可删除（规划 §5.3）
pub fn remove(conn: &Connection, name: &str) -> ApiResult<()> {
    get(conn, name)?;
    let refs: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE project = ?1",
        params![name],
        |r| r.get(0),
    )?;
    if refs > 0 {
        return Err(ApiError::conflict(format!(
            "项目选项「{name}」正被 {refs} 个任务引用，不可删除"
        ))
        .with_details(serde_json::json!({ "ref_count": refs })));
    }
    conn.execute("DELETE FROM projects WHERE name = ?1", params![name])?;
    Ok(())
}
