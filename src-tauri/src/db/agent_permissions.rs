use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    domain::task,
    error::{ApiError, ApiResult},
};

pub const CREATE_FIELDS: [&str; 5] = [
    "note",
    "status",
    "priority",
    "predecessor_task_ids",
    "unlock_task_ids",
];
pub const EDIT_FIELDS: [&str; 8] = [
    "project",
    "type",
    "description",
    "note",
    "status",
    "priority",
    "predecessor_task_ids",
    "unlock_task_ids",
];

/// 独立于网页权限；最终 Agent 能力始终还要与用户的项目/字段权限取交集。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentPermissions {
    pub task_create: bool,
    pub create_fields: Vec<String>,
    pub edit_fields: Vec<String>,
    pub status_values: Vec<String>,
    /// false = 仅可修改当前 token 用户的 Agent 创建任务描述。
    pub description_any_task: bool,
}

impl Default for AgentPermissions {
    fn default() -> Self {
        Self {
            task_create: true,
            create_fields: vec!["priority".into()],
            edit_fields: vec!["status".into(), "priority".into(), "description".into()],
            status_values: task::AGENT_STATUSES.iter().map(|s| s.to_string()).collect(),
            description_any_task: false,
        }
    }
}

impl AgentPermissions {
    pub fn allows_create_field(&self, field: &str) -> bool {
        self.create_fields.iter().any(|item| item == field)
    }

    pub fn allows_edit_field(&self, field: &str) -> bool {
        self.edit_fields.iter().any(|item| item == field)
    }

    pub fn require_create_field(&self, field: &str) -> ApiResult<()> {
        if self.allows_create_field(field) {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "Agent 无权在创建时设置字段：{field}"
            )))
        }
    }

    pub fn require_edit_field(&self, field: &str) -> ApiResult<()> {
        if self.allows_edit_field(field) {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!("Agent 无权修改字段：{field}")))
        }
    }

    pub fn require_value(&self, field: &str, value: &str) -> ApiResult<()> {
        if field != "status" || self.status_values.iter().any(|item| item == value) {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "Agent 无权将状态设置为「{value}」"
            )))
        }
    }

    pub fn validate(&self) -> ApiResult<()> {
        for (fields, choices) in [
            (&self.create_fields, CREATE_FIELDS.as_slice()),
            (&self.edit_fields, EDIT_FIELDS.as_slice()),
        ] {
            let mut seen = std::collections::HashSet::new();
            for field in fields {
                if !choices.contains(&field.as_str()) || !seen.insert(field) {
                    return Err(ApiError::unprocessable(format!(
                        "Agent 权限字段无效或重复：{field}"
                    )));
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        for status in &self.status_values {
            if !task::is_valid_status(status) || !seen.insert(status) {
                return Err(ApiError::unprocessable(format!(
                    "Agent 状态权限无效或重复：{status}"
                )));
            }
        }
        Ok(())
    }
}

pub fn get(conn: &Connection, user_id: &str) -> ApiResult<AgentPermissions> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT permissions_json FROM agent_permission_profiles WHERE user_id=?1",
            [user_id],
            |row| row.get(0),
        )
        .optional()?;
    raw.map(|value| serde_json::from_str(&value).map_err(ApiError::internal))
        .transpose()
        .map(|value| value.unwrap_or_default())
}

pub fn put(
    conn: &Connection,
    user_id: &str,
    profile: &AgentPermissions,
) -> ApiResult<AgentPermissions> {
    profile.validate()?;
    let raw = serde_json::to_string(profile).map_err(ApiError::internal)?;
    conn.execute(
        "INSERT INTO agent_permission_profiles(user_id, permissions_json) VALUES (?1, ?2)
         ON CONFLICT(user_id) DO UPDATE SET permissions_json=excluded.permissions_json",
        params![user_id, raw],
    )?;
    get(conn, user_id)
}
