use rusqlite::{params, Connection, OptionalExtension};

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
    /// None = 全部项目（管理员）；Some = 仅这些项目（普通用户，可为空）。
    pub visible_projects: Option<Vec<String>>,
}

pub(super) fn row_to_task(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    let submitter: String = r.get("submitter")?;
    let owner_username: Option<String> = r.get("owner_username")?;
    Ok(Task {
        id: r.get("id")?,
        seq: r.get("seq")?,
        project: r.get("project")?,
        task_type: r.get("type")?,
        description: r.get("description")?,
        note: r.get("note")?,
        status: r.get("status")?,
        submitter_name: task::submitter_name(&submitter, owner_username.as_deref()),
        submitter,
        created_at: r.get("created_at")?,
        finished_at: r.get("finished_at")?,
        updated_at: r.get("updated_at")?,
        position: r.get("position")?,
        attachment_count: r.get("attachment_count")?,
        owner_user_id: r.get("owner_user_id")?,
    })
}

pub(super) const SELECT_TASKS: &str = r#"
SELECT t.*, u.username AS owner_username,
       (SELECT COUNT(*) FROM attachments a WHERE a.task_id = t.id) AS attachment_count
FROM tasks t
LEFT JOIN users u ON u.id = t.owner_user_id
"#;

pub(super) fn filter_sql(f: &TaskFilter) -> (String, Vec<String>) {
    let mut conds = Vec::new();
    let mut values = Vec::new();
    if let Some(projects) = &f.visible_projects {
        if projects.is_empty() {
            conds.push("1 = 0".into());
        } else {
            conds.push(format!(
                "t.project IN ({})",
                vec!["?"; projects.len()].join(",")
            ));
            values.extend(projects.iter().cloned());
        }
    }
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
        conds.push("(t.id LIKE ? OR t.description LIKE ? OR t.note LIKE ?)".into());
        values.push(format!("%{keyword}%"));
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
    // 手动排序就是用户摆好的顺序，无升/降序之分（降序会让显示位次与位置倒挂）
    if f.sort_by.as_deref() == Some("manual") {
        return "t.position ASC, t.seq ASC".into();
    }
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
    pub note: &'a str,
    pub submitter: &'a str,
    pub owner_user_id: Option<&'a str>,
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
    // 新任务排在手动排序末尾：position 取递增的 seq 即可
    tx.execute(
        "INSERT INTO tasks (id, seq, project, type, description, note, status, submitter, created_at, updated_at, position, owner_user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, '未开始', ?7, ?8, ?8, ?2, ?9)",
        params![
            id,
            seq,
            n.project,
            n.task_type,
            n.description,
            n.note,
            n.submitter,
            now,
            n.owner_user_id,
        ],
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
    pub note: Option<String>,
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
    let note = p.note.as_deref().unwrap_or(&current.note);
    let status = p.status.as_deref().unwrap_or(&current.status);
    let finished_at = task::transition(status, current.finished_at.clone());
    let now = task::now_str();

    conn.execute(
        "UPDATE tasks SET project = ?2, type = ?3, description = ?4, note = ?5, status = ?6,
            finished_at = ?7, updated_at = ?8 WHERE id = ?1",
        params![
            id,
            project,
            task_type,
            description,
            note,
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

// ── 手动排序（实数中点插入） ──────────────────────────────

fn position_of(conn: &Connection, id: &str) -> ApiResult<(f64, i64)> {
    conn.query_row(
        "SELECT position, seq FROM tasks WHERE id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => ApiError::not_found(format!("任务 {id} 不存在")),
        other => ApiError::from(other),
    })
}

/// 取 (pos, seq) 位次之前（before=true）或之后紧邻的任务位置，排除被拖动的任务本身。
/// 位次口径与 sort_sql(manual) 一致：(position, seq)。
fn neighbor_position(
    conn: &Connection,
    exclude: &str,
    pos: f64,
    seq: i64,
    before: bool,
) -> ApiResult<Option<f64>> {
    let (cmp, dir) = if before { ("<", "DESC") } else { (">", "ASC") };
    conn.query_row(
        &format!(
            "SELECT position FROM tasks
             WHERE id <> ?1 AND (position {cmp} ?2 OR (position = ?2 AND seq {cmp} ?3))
             ORDER BY position {dir}, seq {dir} LIMIT 1"
        ),
        params![exclude, pos, seq],
        |r| r.get(0),
    )
    .optional()
    .map_err(ApiError::from)
}

/// 全表按当前顺序重排位置（1024 间隔），中点插入无位可用时兜底
fn renumber_positions(conn: &Connection) -> ApiResult<()> {
    conn.execute_batch(
        "UPDATE tasks SET position = (
           SELECT 1024.0 * r FROM (
             SELECT id AS rid, ROW_NUMBER() OVER (ORDER BY position, seq) AS r FROM tasks
           ) WHERE rid = tasks.id
         );",
    )?;
    Ok(())
}

/// 以指定排序重铺全部任务的手动位置：切入手动排序时以当前视图为基线。
/// 只改 position，不改变当前视图看到的内容（同一排序键，位次一致）。
pub fn rebase_positions(
    conn: &Connection,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
) -> ApiResult<()> {
    // sort_sql 只产出白名单列与固定方向，可安全内联
    let order = sort_sql(&TaskFilter {
        sort_by: sort_by.map(str::to_string),
        sort_order: sort_order.map(str::to_string),
        ..Default::default()
    });
    conn.execute_batch(&format!(
        "UPDATE tasks SET position = (
           SELECT 1024.0 * r FROM (
             SELECT t.id AS rid, ROW_NUMBER() OVER (ORDER BY {order}) AS r FROM tasks t
           ) WHERE rid = tasks.id
         );"
    ))?;
    Ok(())
}

/// 仅重铺指定项目，避免普通用户的排序操作影响不可见项目。
pub fn rebase_positions_for_projects(
    conn: &Connection,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    projects: &[String],
) -> ApiResult<()> {
    if projects.is_empty() {
        return Ok(());
    }
    let order = sort_sql(&TaskFilter {
        sort_by: sort_by.map(str::to_string),
        sort_order: sort_order.map(str::to_string),
        ..Default::default()
    });
    let marks = vec!["?"; projects.len()].join(",");
    conn.execute(
        &format!(
            "WITH ranked AS (
               SELECT t.id AS rid, ROW_NUMBER() OVER (ORDER BY {order}) AS r
               FROM tasks t WHERE t.project IN ({marks})
             )
             UPDATE tasks SET position = (SELECT 1024.0 * r FROM ranked WHERE rid=tasks.id)
             WHERE id IN (SELECT rid FROM ranked)"
        ),
        rusqlite::params_from_iter(projects.iter()),
    )?;
    Ok(())
}

/// 把任务移到 prev_id/next_id 之间；只给一侧则贴到该侧邻居之外。
/// 都为空或落点即自身 → 原样返回。邻居不存在 → 404。
pub fn reorder(
    conn: &mut Connection,
    id: &str,
    prev_id: Option<&str>,
    next_id: Option<&str>,
) -> ApiResult<Task> {
    get(conn, id)?;
    if prev_id == Some(id) || next_id == Some(id) || (prev_id.is_none() && next_id.is_none()) {
        return get(conn, id);
    }
    if let Some(p) = prev_id {
        position_of(conn, p)?;
    }
    if let Some(n) = next_id {
        position_of(conn, n)?;
    }

    let tx = conn.transaction()?;
    let mut renumbered = false;
    let position = loop {
        let lo = match prev_id {
            Some(p) => Some(position_of(&tx, p)?.0),
            None => match next_id {
                Some(n) => {
                    let (pos, seq) = position_of(&tx, n)?;
                    neighbor_position(&tx, id, pos, seq, true)?
                }
                None => None,
            },
        };
        let hi = match next_id {
            Some(n) => Some(position_of(&tx, n)?.0),
            None => match prev_id {
                Some(p) => {
                    let (pos, seq) = position_of(&tx, p)?;
                    neighbor_position(&tx, id, pos, seq, false)?
                }
                None => None,
            },
        };
        let candidate = match (lo, hi) {
            (Some(a), Some(b)) => {
                // 两侧邻居的顺序不影响结果：中点始终落在两者之间
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                let mid = lo + (hi - lo) / 2.0;
                // 位置并列或 f64 精度耗尽（中点等于端点）→ 重排后重试
                if mid > lo && mid < hi {
                    Some(mid)
                } else {
                    None
                }
            }
            (Some(lo), None) => Some(lo + 1.0),
            (None, Some(hi)) => Some(hi - 1.0),
            (None, None) => Some(position_of(&tx, id)?.0), // 全表仅此一行
        };
        match candidate {
            Some(p) => break p,
            None if !renumbered => {
                renumber_positions(&tx)?;
                renumbered = true;
            }
            None => return Err(ApiError::internal("重排位置后仍无法计算插入位置")),
        }
    };
    tx.execute(
        "UPDATE tasks SET position = ?2 WHERE id = ?1",
        params![id, position],
    )?;
    tx.commit()?;
    get(conn, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn add(conn: &mut Connection, desc: &str) -> Task {
        create(
            conn,
            &NewTask {
                project: "default-project",
                task_type: "优化",
                description: desc,
                note: "",
                submitter: "用户",
                owner_user_id: None,
            },
        )
        .unwrap()
    }

    fn manual_order(conn: &Connection) -> Vec<String> {
        list(
            conn,
            &TaskFilter {
                sort_by: Some("manual".into()),
                sort_order: Some("asc".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .into_iter()
        .map(|t| t.description)
        .collect()
    }

    #[test]
    fn new_tasks_append_at_manual_end() {
        let mut conn = db::open_memory().unwrap();
        let a = add(&mut conn, "A");
        let b = add(&mut conn, "B");
        assert!(a.position < b.position);
        assert_eq!(manual_order(&conn), ["A", "B"]);
    }

    #[test]
    fn reorder_between_neighbors_uses_midpoint() {
        let mut conn = db::open_memory().unwrap();
        let a = add(&mut conn, "A");
        let b = add(&mut conn, "B");
        let c = add(&mut conn, "C");

        let moved = reorder(&mut conn, &c.id, Some(&a.id), Some(&b.id)).unwrap();
        assert_eq!(moved.position, (a.position + b.position) / 2.0);
        assert_eq!(manual_order(&conn), ["A", "C", "B"]);

        // 拖到末尾：只给 prev
        let moved = reorder(&mut conn, &a.id, Some(&b.id), None).unwrap();
        assert!(moved.position > b.position);
        assert_eq!(manual_order(&conn), ["C", "B", "A"]);

        // 拖到最前：只给 next
        reorder(&mut conn, &a.id, None, Some(&c.id)).unwrap();
        assert_eq!(manual_order(&conn), ["A", "C", "B"]);
    }

    #[test]
    fn manual_sort_ignores_direction() {
        let mut conn = db::open_memory().unwrap();
        add(&mut conn, "A");
        add(&mut conn, "B");
        let ids = |order: &str| {
            list(
                &conn,
                &TaskFilter {
                    sort_by: Some("manual".into()),
                    sort_order: Some(order.into()),
                    ..Default::default()
                },
            )
            .unwrap()
            .into_iter()
            .map(|t| t.description)
            .collect::<Vec<_>>()
        };
        // 手动排序无升/降序之分：desc 与 asc 结果一致，避免显示位次与位置倒挂
        assert_eq!(ids("asc"), ["A", "B"]);
        assert_eq!(ids("desc"), ["A", "B"]);
    }

    #[test]
    fn rebase_resets_manual_order_to_current_sort() {
        let mut conn = db::open_memory().unwrap();
        let a = add(&mut conn, "A");
        let b = add(&mut conn, "B");
        let c = add(&mut conn, "C");
        // 先打乱手动顺序
        reorder(&mut conn, &c.id, Some(&a.id), Some(&b.id)).unwrap();
        assert_eq!(manual_order(&conn), ["A", "C", "B"]);
        // 以创建时间升序重铺 → 恢复创建顺序
        rebase_positions(&conn, Some("created_at"), Some("asc")).unwrap();
        assert_eq!(manual_order(&conn), ["A", "B", "C"]);
        // 以创建时间降序重铺 → 整体倒序
        rebase_positions(&conn, Some("created_at"), Some("desc")).unwrap();
        assert_eq!(manual_order(&conn), ["C", "B", "A"]);
        // 以手动排序自身重铺 → 不变
        rebase_positions(&conn, Some("manual"), None).unwrap();
        assert_eq!(manual_order(&conn), ["C", "B", "A"]);
    }

    #[test]
    fn reorder_noop_and_missing_neighbor() {
        let mut conn = db::open_memory().unwrap();
        let a = add(&mut conn, "A");
        let b = add(&mut conn, "B");
        let before = get(&conn, &a.id).unwrap().position;

        let same = reorder(&mut conn, &a.id, None, None).unwrap();
        assert_eq!(same.position, before);
        let same = reorder(&mut conn, &a.id, Some(&a.id), None).unwrap();
        assert_eq!(same.position, before);
        assert!(reorder(&mut conn, &a.id, Some("missing"), None).is_err());
        assert!(reorder(&mut conn, "missing", Some(&b.id), None).is_err());
        assert_eq!(manual_order(&conn), ["A", "B"]);
    }
}
