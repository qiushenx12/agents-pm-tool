use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{task, user},
    error::{ApiError, ApiResult},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Permission {
    pub project: String,
    pub field: String,
    pub allowed_values: Option<Vec<String>>,
}

fn decode_values(raw: Option<String>) -> ApiResult<Option<Vec<String>>> {
    raw.map(|value| serde_json::from_str(&value).map_err(ApiError::internal))
        .transpose()
}

pub fn list(conn: &Connection, user_id: &str) -> ApiResult<Vec<Permission>> {
    let mut statement = conn.prepare(
        "SELECT project, field, allowed_values FROM user_permissions
         WHERE user_id=?1 ORDER BY project, field",
    )?;
    let rows = statement.query_map([user_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (project, field, allowed_values) = row?;
        result.push(Permission {
            project,
            field,
            allowed_values: decode_values(allowed_values)?,
        });
    }
    Ok(result)
}

pub fn visible_projects(conn: &Connection, user: &user::User) -> ApiResult<Option<Vec<String>>> {
    if user.is_admin() {
        return Ok(None);
    }
    let mut statement = conn.prepare(
        "SELECT project FROM user_permissions
         WHERE user_id=?1 AND field='project_access'
         ORDER BY project",
    )?;
    let rows = statement.query_map([&user.id], |row| row.get(0))?;
    Ok(Some(rows.collect::<Result<Vec<_>, _>>()?))
}

pub fn require_project(conn: &Connection, user: &user::User, project: &str) -> ApiResult<()> {
    if user.is_admin() {
        return Ok(());
    }
    let allowed: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM user_permissions
           WHERE user_id=?1 AND project=?2 AND field='project_access'
         )",
        params![user.id, project],
        |row| row.get(0),
    )?;
    if allowed {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!("无权访问项目「{project}」")))
    }
}

pub fn require_field(
    conn: &Connection,
    user: &user::User,
    project: &str,
    field: &str,
    target_value: Option<&str>,
) -> ApiResult<()> {
    require_project(conn, user, project)?;
    if user.is_admin() {
        return Ok(());
    }
    let raw: Option<Option<String>> = conn
        .query_row(
            "SELECT allowed_values FROM user_permissions
             WHERE user_id=?1 AND project=?2 AND field=?3",
            params![user.id, project, field],
            |row| row.get(0),
        )
        .optional()?;
    let Some(raw) = raw else {
        return Err(
            ApiError::forbidden(format!("无权操作项目「{project}」的字段：{field}"))
                .with_details(serde_json::json!({ "fields": [field] })),
        );
    };
    if let (Some(value), Some(allowed_values)) = (target_value, decode_values(raw)?) {
        if !allowed_values.iter().any(|allowed| allowed == value) {
            return Err(ApiError::forbidden(format!(
                "无权将项目「{project}」的 {field} 设置为「{value}」"
            ))
            .with_details(serde_json::json!({
                "field": field,
                "value": value,
                "allowed_values": allowed_values,
            })));
        }
    }
    Ok(())
}

pub fn require_fields(
    conn: &Connection,
    user: &user::User,
    project: &str,
    fields: &[(&str, Option<&str>)],
) -> ApiResult<()> {
    let mut denied = Vec::new();
    for (field, target) in fields {
        if require_field(conn, user, project, field, *target).is_err() {
            denied.push(*field);
        }
    }
    if denied.is_empty() {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!(
            "无权操作项目「{project}」的字段：{}",
            denied.join("、")
        ))
        .with_details(serde_json::json!({ "fields": denied })))
    }
}

fn validate_permission(permission: &Permission) -> ApiResult<()> {
    if !user::is_valid_permission_field(&permission.field) {
        return Err(ApiError::unprocessable(format!(
            "不支持的权限字段：{}",
            permission.field
        )));
    }
    let values = permission.allowed_values.as_deref();
    match permission.field.as_str() {
        "status" => {
            if values
                .unwrap_or_default()
                .iter()
                .any(|value| !task::is_valid_status(value))
            {
                return Err(ApiError::unprocessable("状态权限中包含非法选项"));
            }
        }
        "type" => {
            if values
                .unwrap_or_default()
                .iter()
                .any(|value| !task::is_valid_task_type(value))
            {
                return Err(ApiError::unprocessable("类型权限中包含非法选项"));
            }
        }
        "priority" => {
            if values
                .unwrap_or_default()
                .iter()
                .any(|value| !task::is_valid_priority(value))
            {
                return Err(ApiError::unprocessable("优先级权限中包含非法选项"));
            }
        }
        _ if values.is_some() => {
            return Err(ApiError::unprocessable(format!(
                "权限字段 {} 不支持 allowed_values",
                permission.field
            )))
        }
        _ => {}
    }
    Ok(())
}

pub fn replace(
    conn: &mut Connection,
    user_id: &str,
    permissions: &[Permission],
) -> ApiResult<Vec<Permission>> {
    for permission in permissions {
        validate_permission(permission)?;
        super::projects::get(conn, &permission.project)?;
    }
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM user_permissions WHERE user_id=?1", [user_id])?;
    for permission in permissions {
        let values = permission
            .allowed_values
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(ApiError::internal)?;
        tx.execute(
            "INSERT INTO user_permissions (user_id, project, field, allowed_values)
             VALUES (?1, ?2, ?3, ?4)",
            params![user_id, permission.project, permission.field, values],
        )
        .map_err(|error| match error {
            rusqlite::Error::SqliteFailure(ref code, _)
                if code.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                ApiError::unprocessable("权限列表包含重复项")
            }
            other => ApiError::from(other),
        })?;
    }
    tx.commit()?;
    list(conn, user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, db::users};

    #[test]
    fn ordinary_user_needs_project_field_and_allowed_value() {
        let mut conn = db::open_memory().unwrap();
        let user = users::create(&conn, "alice", "password-123").unwrap();
        replace(
            &mut conn,
            &user.id,
            &[
                Permission {
                    project: "default-project".into(),
                    field: "project_access".into(),
                    allowed_values: None,
                },
                Permission {
                    project: "default-project".into(),
                    field: "status".into(),
                    allowed_values: Some(vec!["进行中".into()]),
                },
            ],
        )
        .unwrap();
        assert!(require_project(&conn, &user, "default-project").is_ok());
        assert!(require_field(&conn, &user, "default-project", "status", Some("进行中")).is_ok());
        assert!(
            require_field(&conn, &user, "default-project", "status", Some("验收通过")).is_err()
        );
        assert!(require_field(&conn, &user, "default-project", "description", None).is_err());
    }
}
