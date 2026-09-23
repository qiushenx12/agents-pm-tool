use serde::{Deserialize, Serialize};

pub const HOST_USER_ID: &str = "host";
pub const DEFAULT_HOST_USERNAME: &str = "主机";
pub const ROLES: [&str; 3] = ["super_admin", "admin", "user"];
pub const PERMISSION_FIELDS: [&str; 14] = [
    "project_access",
    "task_create",
    "project",
    "type",
    "priority",
    "description",
    "note",
    "predecessor_task_ids",
    "unlock_task_ids",
    "status",
    "assignee",
    "task_delete",
    "reorder",
    "attachment_upload",
];
pub const ATTACHMENT_DELETE_FIELD: &str = "attachment_delete";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub username: String,
    pub role: String,
    pub created_at: String,
    pub disabled: bool,
    pub is_host: bool,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == "admin" || self.role == "super_admin"
    }

    pub fn is_super_admin(&self) -> bool {
        self.role == "super_admin"
    }
}

pub fn is_valid_role(role: &str) -> bool {
    ROLES.contains(&role)
}

pub fn is_valid_permission_field(field: &str) -> bool {
    PERMISSION_FIELDS.contains(&field) || field == ATTACHMENT_DELETE_FIELD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_rules_are_explicit() {
        assert!(is_valid_role("super_admin"));
        assert!(is_valid_role("admin"));
        assert!(is_valid_role("user"));
        assert!(!is_valid_role("owner"));
    }
}
