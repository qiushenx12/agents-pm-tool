use super::tasks::{self, TaskFilter};
use crate::{
    domain::task::Task,
    error::{ApiError, ApiResult},
};
use rusqlite::{params_from_iter, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug)]
pub struct PageOptions {
    pub page: i64,
    pub page_size: i64,
    pub group_by: Option<String>,
    pub anchor_id: Option<String>,
}
impl Default for PageOptions {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 100,
            group_by: None,
            anchor_id: None,
        }
    }
}
impl PageOptions {
    pub fn parse(raw: Option<&str>) -> ApiResult<Self> {
        let mut options = Self::default();
        for (key, value) in url::form_urlencoded::parse(raw.unwrap_or("").as_bytes()) {
            match key.as_ref() {
                "page" => {
                    options.page = value
                        .parse()
                        .map_err(|_| ApiError::bad_request("页码必须为正整数"))?
                }
                "page_size" => {
                    options.page_size = value
                        .parse()
                        .map_err(|_| ApiError::bad_request("每页条数必须为整数"))?
                }
                "group_by" if !value.is_empty() => options.group_by = Some(value.into_owned()),
                "anchor_id" if !value.is_empty() => options.anchor_id = Some(value.into_owned()),
                _ => {}
            }
        }
        if options.page < 1 || !(1..=200).contains(&options.page_size) {
            return Err(ApiError::bad_request("页码从 1 开始，每页条数范围为 1–200"));
        }
        if let Some(field) = &options.group_by {
            group_column(field)?;
        }
        Ok(options)
    }
}
#[derive(Serialize)]
pub struct GroupCount {
    pub value: String,
    pub count: i64,
}
#[derive(Serialize)]
pub struct TaskPage {
    pub items: Vec<Task>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub groups: Vec<GroupCount>,
    pub anchor_found: Option<bool>,
}
fn group_column(field: &str) -> ApiResult<&'static str> {
    match field {
        "project" => Ok("t.project"),
        "status" => Ok("t.status"),
        "type" => Ok("t.type"),
        "submitter" => Ok("t.submitter"),
        _ => Err(ApiError::bad_request("不支持的分组字段")),
    }
}
fn group_order(field: &str, column: &str) -> String {
    let options: &[&str] = match field {
        "status" => &crate::domain::task::STATUSES,
        "type" => &crate::domain::task::TASK_TYPES,
        _ => &[],
    };
    if options.is_empty() {
        return format!("{column} COLLATE NOCASE ASC, {column} ASC");
    }
    let cases = options
        .iter()
        .enumerate()
        .map(|(i, value)| format!("WHEN '{value}' THEN {i}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("CASE {column} {cases} ELSE 999 END ASC, {column} ASC")
}

/// The application's DB mutex prevents application writes between counts, anchor lookup and rows.
pub fn list_page(
    conn: &Connection,
    filter: &TaskFilter,
    options: &PageOptions,
) -> ApiResult<TaskPage> {
    let (conditions, values) = tasks::filter_sql(filter);
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM tasks t{conditions}"),
        params_from_iter(values.iter()),
        |row| row.get(0),
    )?;
    let mut order = tasks::sort_sql(filter);
    let mut groups = Vec::new();
    if let Some(field) = &options.group_by {
        let column = group_column(field)?;
        let group_order = group_order(field, column);
        order = format!("{group_order}, {order}");
        let mut stmt = conn.prepare(&format!("SELECT {column}, COUNT(*) FROM tasks t{conditions} GROUP BY {column} ORDER BY {group_order}"))?;
        let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
            Ok(GroupCount {
                value: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        groups = rows.collect::<Result<Vec<_>, _>>()?;
    }
    let pages = ((total + options.page_size - 1) / options.page_size).max(1);
    let mut page = options.page.min(pages);
    let mut anchor_found = None;
    if let Some(id) = &options.anchor_id {
        let mut anchor_values = values.clone();
        anchor_values.push(id.clone());
        let position: Option<i64> = conn.query_row(
            &format!("SELECT position FROM (SELECT t.id, ROW_NUMBER() OVER (ORDER BY {order}) AS position FROM tasks t{conditions}) WHERE id = ?"),
            params_from_iter(anchor_values.iter()), |row| row.get(0),
        ).optional()?;
        anchor_found = Some(position.is_some());
        if let Some(position) = position {
            page = (position - 1) / options.page_size + 1;
        }
    }
    let offset = (page - 1) * options.page_size;
    // Page numbers are validated integers; all user text remains parameterized.
    let mut stmt = conn.prepare(&format!(
        "{}{conditions} ORDER BY {order} LIMIT {} OFFSET {offset}",
        tasks::SELECT_TASKS,
        options.page_size
    ))?;
    let rows = stmt.query_map(params_from_iter(values.iter()), tasks::row_to_task)?;
    Ok(TaskPage {
        items: rows.collect::<Result<Vec<_>, _>>()?,
        total,
        page,
        page_size: options.page_size,
        groups,
        anchor_found,
    })
}
