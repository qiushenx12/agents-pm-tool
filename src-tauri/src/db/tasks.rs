use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;

use crate::domain::idgen;
use crate::domain::task::{self, Task};
use crate::error::{ApiError, ApiResult};

fn split_dependency_ids(value: String) -> Vec<String> {
    value
        .split(',')
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Default)]
pub struct TaskFilter {
    pub project: Vec<String>,
    pub task_type: Vec<String>,
    pub status: Vec<String>,
    pub exclude_status: bool,
    pub submitter: Vec<String>,
    pub priority: Vec<String>,
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
        priority: r.get("priority")?,
        submitter_name: task::submitter_name(&submitter, owner_username.as_deref()),
        submitter,
        created_at: r.get("created_at")?,
        finished_at: r.get("finished_at")?,
        updated_at: r.get("updated_at")?,
        position: r.get("position")?,
        attachment_count: r.get("attachment_count")?,
        owner_user_id: r.get("owner_user_id")?,
        predecessor_task_ids: split_dependency_ids(r.get("predecessor_task_ids")?),
        unlock_task_ids: split_dependency_ids(r.get("unlock_task_ids")?),
    })
}

pub(super) const SELECT_TASKS: &str = r#"
SELECT t.*, u.username AS owner_username,
       (SELECT COUNT(*) FROM attachments a WHERE a.task_id = t.id) AS attachment_count,
       COALESCE((
         SELECT group_concat(predecessor_task_id, ',') FROM (
           SELECT d.predecessor_task_id
           FROM task_dependencies d
           JOIN tasks predecessor ON predecessor.id = d.predecessor_task_id
           WHERE d.task_id = t.id
           ORDER BY predecessor.seq
         )
       ), '') AS predecessor_task_ids,
       COALESCE((
         SELECT group_concat(task_id, ',') FROM (
           SELECT d.task_id
           FROM task_dependencies d
           JOIN tasks unlocked ON unlocked.id = d.task_id
           WHERE d.predecessor_task_id = t.id
           ORDER BY unlocked.seq
         )
       ), '') AS unlock_task_ids
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
        ("priority", &f.priority),
    ] {
        if !items.is_empty() {
            let marks = vec!["?"; items.len()].join(",");
            conds.push(format!("t.{column} IN ({marks})"));
            values.extend(items.iter().cloned());
        }
    }
    // 提交人筛选接受三类值（与任务上的 submitter_name 显示形态对齐）：
    // - 大类 `用户` / `Agent`：按提交来源匹配（t.submitter），语义不变；
    // - 裸用户名（如 `主机`）：该账号作为「用户」提交的任务；
    // - `Agent（用户名）`：该账号作为 Agent 提交的任务；
    // - `未知用户`：owner 账号已删除、owner_user_id 置空的历史任务。
    // 名字匹配按 (owner, submitter) 双列定位，避免裸用户名把 Agent 提交的任务也捞进来。
    if !f.submitter.is_empty() {
        let mut or = Vec::new();
        let kinds: Vec<&String> = f
            .submitter
            .iter()
            .filter(|v| task::is_valid_submitter(v))
            .collect();
        let names: Vec<&String> = f
            .submitter
            .iter()
            .filter(|v| !task::is_valid_submitter(v))
            .collect();
        if !kinds.is_empty() {
            or.push(format!("t.submitter IN ({})", vec!["?"; kinds.len()].join(",")));
            values.extend(kinds.iter().map(|v| (*v).clone()));
        }
        let mut user_names: Vec<&str> = Vec::new();
        let mut agent_names: Vec<&str> = Vec::new();
        let mut include_unknown = false;
        for name in &names {
            if let Some(u) = name
                .strip_prefix("Agent（")
                .and_then(|s| s.strip_suffix("）"))
            {
                agent_names.push(u);
            } else if name.as_str() == "未知用户" {
                include_unknown = true;
            } else {
                user_names.push(name.as_str());
            }
        }
        for (submitter, group) in [("用户", user_names), ("Agent", agent_names)] {
            if group.is_empty() {
                continue;
            }
            let marks = vec!["?"; group.len()].join(",");
            or.push(format!(
                "(t.submitter = ? AND t.owner_user_id IN (SELECT id FROM users WHERE username IN ({marks})))"
            ));
            values.push(submitter.to_string());
            values.extend(group.iter().map(|s| s.to_string()));
        }
        if include_unknown {
            or.push("t.owner_user_id IS NULL".into());
        }
        conds.push(format!("({})", or.join(" OR ")));
    }
    if !f.status.is_empty() {
        let marks = vec!["?"; f.status.len()].join(",");
        let operator = if f.exclude_status { "NOT IN" } else { "IN" };
        conds.push(format!("t.status {operator} ({marks})"));
        values.extend(f.status.iter().cloned());
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
        // 优先级按业务语义排序（高→低），与方向组合：asc=高在前，desc=低在前
        Some("priority") => {
            "CASE t.priority WHEN '高' THEN 0 WHEN '中' THEN 1 WHEN '低' THEN 2 ELSE 3 END"
        }
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

/// 从 API 响应中移除用户不可见项目的关联任务 ID，避免关系字段绕过项目可见性。
pub fn retain_visible_dependencies(
    conn: &Connection,
    records: &mut [Task],
    visible_projects: Option<&[String]>,
) -> ApiResult<()> {
    let Some(projects) = visible_projects else {
        return Ok(());
    };
    if projects.is_empty() {
        for task in records {
            task.predecessor_task_ids.clear();
            task.unlock_task_ids.clear();
        }
        return Ok(());
    }
    let marks = vec!["?"; projects.len()].join(",");
    let mut statement = conn.prepare(&format!(
        "SELECT id FROM tasks WHERE project IN ({marks})"
    ))?;
    let rows = statement.query_map(rusqlite::params_from_iter(projects.iter()), |row| {
        row.get::<_, String>(0)
    })?;
    let visible_ids = rows.collect::<Result<HashSet<_>, _>>()?;
    for task in records {
        task.predecessor_task_ids
            .retain(|id| visible_ids.contains(id));
        task.unlock_task_ids.retain(|id| visible_ids.contains(id));
    }
    Ok(())
}

pub struct NewTask<'a> {
    pub project: &'a str,
    pub task_type: &'a str,
    pub description: &'a str,
    pub note: &'a str,
    pub submitter: &'a str,
    pub owner_user_id: Option<&'a str>,
    /// None 时用默认优先级「中」
    pub priority: Option<&'a str>,
    pub predecessor_task_ids: &'a [String],
    pub unlock_task_ids: &'a [String],
}

fn normalize_dependency_ids(ids: &[String]) -> ApiResult<Vec<String>> {
    let mut result = Vec::new();
    for id in ids {
        let id = id.trim();
        if id.is_empty() {
            return Err(ApiError::unprocessable("任务依赖 ID 不能为空"));
        }
        if !result.iter().any(|existing| existing == id) {
            result.push(id.to_string());
        }
    }
    Ok(result)
}

fn validate_dependency_targets(conn: &Connection, ids: &[String]) -> ApiResult<()> {
    for id in ids {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(ApiError::unprocessable(format!("关联任务 {id} 不存在"))
                .with_details(serde_json::json!({ "task_id": id })));
        }
    }
    Ok(())
}

fn replace_dependencies(
    conn: &Connection,
    id: &str,
    predecessor_task_ids: Option<&[String]>,
    unlock_task_ids: Option<&[String]>,
) -> ApiResult<()> {
    if let Some(predecessors) = predecessor_task_ids {
        conn.execute("DELETE FROM task_dependencies WHERE task_id=?1", [id])?;
        for predecessor in predecessors {
            conn.execute(
                "INSERT INTO task_dependencies(predecessor_task_id, task_id) VALUES (?1, ?2)",
                params![predecessor, id],
            )?;
        }
    }
    if let Some(unlocked) = unlock_task_ids {
        conn.execute(
            "DELETE FROM task_dependencies WHERE predecessor_task_id=?1",
            [id],
        )?;
        for task_id in unlocked {
            conn.execute(
                "INSERT INTO task_dependencies(predecessor_task_id, task_id) VALUES (?1, ?2)",
                params![id, task_id],
            )?;
        }
    }
    let cyclic: bool = conn.query_row(
        "WITH RECURSIVE reach(start_id, task_id) AS (
           SELECT predecessor_task_id, task_id FROM task_dependencies
           UNION
           SELECT reach.start_id, dependency.task_id
           FROM reach
           JOIN task_dependencies dependency
             ON dependency.predecessor_task_id = reach.task_id
         )
         SELECT EXISTS(SELECT 1 FROM reach WHERE start_id = task_id)",
        [],
        |row| row.get(0),
    )?;
    if cyclic {
        return Err(ApiError::unprocessable("任务依赖不能形成循环"));
    }
    Ok(())
}

fn unfinished_predecessors(conn: &Connection, id: &str) -> ApiResult<Vec<String>> {
    let mut statement = conn.prepare(
        "SELECT predecessor.id
         FROM task_dependencies dependency
         JOIN tasks predecessor ON predecessor.id = dependency.predecessor_task_id
         WHERE dependency.task_id=?1
           AND predecessor.status NOT IN ('已完成','验收通过')
         ORDER BY predecessor.seq",
    )?;
    let rows = statement.query_map([id], |row| row.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// 校验 + 创建（ID 生成与插入在同一事务，规划 §4.2）
pub fn create(conn: &mut Connection, n: &NewTask) -> ApiResult<Task> {
    create_with_status(conn, n, None)
}

/// Agent 显式获准设置状态时使用；网页创建仍由 create() 固定为「未开始」。
pub fn create_with_status(
    conn: &mut Connection,
    n: &NewTask,
    requested_status: Option<&str>,
) -> ApiResult<Task> {
    validate_project_exists(conn, n.project)?;
    if !task::is_valid_task_type(n.task_type) {
        return Err(ApiError::unprocessable(format!(
            "任务类型不合法：{}，合法取值：{}",
            n.task_type,
            task::TASK_TYPES.join(" / ")
        )));
    }
    let priority = n.priority.unwrap_or(task::DEFAULT_PRIORITY);
    if !task::is_valid_priority(priority) {
        return Err(ApiError::unprocessable(format!(
            "优先级不合法：{priority}，合法取值：{}",
            task::PRIORITIES.join(" / ")
        )));
    }
    let status = requested_status.unwrap_or("未开始");
    if !task::is_valid_status(status) {
        return Err(ApiError::unprocessable(format!(
            "状态不合法：{status}，合法取值：{}",
            task::STATUSES.join(" / ")
        )));
    }
    let predecessor_task_ids = normalize_dependency_ids(n.predecessor_task_ids)?;
    let unlock_task_ids = normalize_dependency_ids(n.unlock_task_ids)?;
    validate_dependency_targets(conn, &predecessor_task_ids)?;
    validate_dependency_targets(conn, &unlock_task_ids)?;

    let now = task::now_str();
    let tx = conn.transaction()?;
    let (id, seq) = idgen::next_task_id(&tx)?;
    // 新任务排在手动排序末尾：position 取递增的 seq 即可
    tx.execute(
        "INSERT INTO tasks (id, seq, project, type, description, note, status, priority, submitter, created_at, finished_at, updated_at, position, owner_user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?10, ?2, ?12)",
        params![
            id,
            seq,
            n.project,
            n.task_type,
            n.description,
            n.note,
            status,
            priority,
            n.submitter,
            now,
            task::transition(status, None),
            n.owner_user_id,
        ],
    )?;
    replace_dependencies(
        &tx,
        &id,
        Some(&predecessor_task_ids),
        Some(&unlock_task_ids),
    )?;
    if status != "未开始" && status != "取消" && !unfinished_predecessors(&tx, &id)?.is_empty() {
        return Err(ApiError::unprocessable("前置任务尚未完成，暂时不能开始本任务"));
    }
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
    pub priority: Option<String>,
    pub predecessor_task_ids: Option<Vec<String>>,
    pub unlock_task_ids: Option<Vec<String>>,
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
    if let Some(priority) = &p.priority {
        if !task::is_valid_priority(priority) {
            return Err(ApiError::unprocessable(format!(
                "优先级不合法：{priority}，合法取值：{}",
                task::PRIORITIES.join(" / ")
            )));
        }
    }
    let predecessor_task_ids = p
        .predecessor_task_ids
        .as_deref()
        .map(normalize_dependency_ids)
        .transpose()?;
    let unlock_task_ids = p
        .unlock_task_ids
        .as_deref()
        .map(normalize_dependency_ids)
        .transpose()?;
    if predecessor_task_ids
        .as_deref()
        .is_some_and(|ids| ids.iter().any(|candidate| candidate == id))
        || unlock_task_ids
            .as_deref()
            .is_some_and(|ids| ids.iter().any(|candidate| candidate == id))
    {
        return Err(ApiError::unprocessable("任务不能依赖或解锁自身"));
    }
    if let Some(ids) = predecessor_task_ids.as_deref() {
        validate_dependency_targets(conn, ids)?;
    }
    if let Some(ids) = unlock_task_ids.as_deref() {
        validate_dependency_targets(conn, ids)?;
    }

    let project = p.project.as_deref().unwrap_or(&current.project);
    let task_type = p.task_type.as_deref().unwrap_or(&current.task_type);
    let description = p.description.as_deref().unwrap_or(&current.description);
    let note = p.note.as_deref().unwrap_or(&current.note);
    let status = p.status.as_deref().unwrap_or(&current.status);
    let priority = p.priority.as_deref().unwrap_or(&current.priority);
    let finished_at = task::transition(status, current.finished_at.clone());
    let now = task::now_str();

    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE tasks SET project = ?2, type = ?3, description = ?4, note = ?5, status = ?6,
            priority = ?7, finished_at = ?8, updated_at = ?9 WHERE id = ?1",
        params![
            id,
            project,
            task_type,
            description,
            note,
            status,
            priority,
            finished_at,
            now
        ],
    )?;
    replace_dependencies(
        &tx,
        id,
        predecessor_task_ids.as_deref(),
        unlock_task_ids.as_deref(),
    )?;
    let starts_task = matches!(current.status.as_str(), "未开始" | "取消")
        && p
            .status
            .as_deref()
            .is_some_and(|status| status != "未开始" && status != "取消");
    if starts_task {
        let unfinished = unfinished_predecessors(&tx, id)?;
        if !unfinished.is_empty() {
            return Err(ApiError::unprocessable(
                "前置任务尚未完成，暂时不能开始本任务",
            ));
        }
    }
    tx.commit()?;
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
                priority: None,
                predecessor_task_ids: &[],
                unlock_task_ids: &[],
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
    fn dependencies_are_bidirectional_and_block_start_until_complete() {
        let mut conn = db::open_memory().unwrap();
        let predecessor = add(&mut conn, "前置");
        let task = add(&mut conn, "当前");
        let unlocked = add(&mut conn, "后续");

        let linked = patch(
            &mut conn,
            &task.id,
            &TaskPatch {
                predecessor_task_ids: Some(vec![predecessor.id.clone()]),
                unlock_task_ids: Some(vec![unlocked.id.clone()]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(linked.predecessor_task_ids, [predecessor.id.clone()]);
        assert_eq!(linked.unlock_task_ids, [unlocked.id.clone()]);
        assert_eq!(get(&conn, &predecessor.id).unwrap().unlock_task_ids, [task.id.clone()]);
        assert_eq!(get(&conn, &unlocked.id).unwrap().predecessor_task_ids, [task.id.clone()]);

        let error = patch(
            &mut conn,
            &task.id,
            &TaskPatch {
                status: Some("进行中".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "validation_failed");
        assert!(error.message.contains("前置任务尚未完成"));
        assert_eq!(get(&conn, &task.id).unwrap().status, "未开始");
        assert!(patch(
            &mut conn,
            &task.id,
            &TaskPatch {
                status: Some("已完成".into()),
                ..Default::default()
            },
        )
        .is_err(), "不能通过直接跳到完成状态绕过前置任务");

        patch(
            &mut conn,
            &predecessor.id,
            &TaskPatch {
                status: Some("已完成".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            patch(
                &mut conn,
                &task.id,
                &TaskPatch {
                    status: Some("进行中".into()),
                    ..Default::default()
                },
            )
            .unwrap()
            .status,
            "进行中"
        );
    }

    #[test]
    fn create_with_status_rolls_back_when_predecessor_is_unfinished() {
        let mut conn = db::open_memory().unwrap();
        let predecessor = add(&mut conn, "前置未完成");
        let ids = vec![predecessor.id.clone()];
        let before: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0)).unwrap();
        let result = create_with_status(&mut conn, &NewTask {
            project: "default-project",
            task_type: "优化",
            description: "不能提前开始",
            note: "",
            submitter: "Agent",
            owner_user_id: Some("host"),
            priority: None,
            predecessor_task_ids: &ids,
            unlock_task_ids: &[],
        }, Some("进行中"));
        assert_eq!(result.unwrap_err().code, "validation_failed");
        let after: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0)).unwrap();
        assert_eq!(after, before, "失败创建不能留下任务或依赖关系");
    }

    #[test]
    fn dependency_cycles_are_rejected_and_deletion_cleans_links() {
        let mut conn = db::open_memory().unwrap();
        let a = add(&mut conn, "A");
        let b = add(&mut conn, "B");
        patch(
            &mut conn,
            &a.id,
            &TaskPatch {
                unlock_task_ids: Some(vec![b.id.clone()]),
                ..Default::default()
            },
        )
        .unwrap();

        let error = patch(
            &mut conn,
            &b.id,
            &TaskPatch {
                unlock_task_ids: Some(vec![a.id.clone()]),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "validation_failed");
        assert!(get(&conn, &b.id).unwrap().unlock_task_ids.is_empty());

        remove(&conn, &b.id).unwrap();
        assert!(get(&conn, &a.id).unwrap().unlock_task_ids.is_empty());
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
