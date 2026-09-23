use chrono::Local;
use serde::{Deserialize, Serialize};

pub const TASK_TYPES: [&str; 3] = ["新增需求", "优化", "BUG"];
pub const STATUSES: [&str; 7] = [
    "未开始",
    "进行中",
    "待验证",
    "已完成",
    "验收未通过",
    "验收通过",
    "取消",
];
/// 未单独配置权限时 Agent 默认可切到的状态。
pub const AGENT_STATUSES: [&str; 3] = ["进行中", "待验证", "已完成"];
pub const SUBMITTERS: [&str; 2] = ["用户", "Agent"];
/// 优先级：默认「中」；展示颜色高=红、中=黄、低=绿（对齐状态色）
pub const PRIORITIES: [&str; 3] = ["高", "中", "低"];
pub const DEFAULT_PRIORITY: &str = "中";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub seq: i64,
    pub project: String,
    #[serde(rename = "type")]
    pub task_type: String,
    pub description: String,
    pub note: String,
    pub status: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    pub submitter: String,
    pub submitter_name: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub updated_at: String,
    /// 手动排序位置（实数中点插入，sort_by=manual 时生效）
    #[serde(default)]
    pub position: f64,
    #[serde(default)]
    pub attachment_count: i64,
    pub owner_user_id: Option<String>,
    /// 负责人：Agent 把任务从「未开始」推进到其它任意状态时自动认领（记录 token 所属用户）。
    /// 认领后其它 Agent 不能再修改该任务；网页端用户仍按自身权限修改，也可改派或清空。
    #[serde(default)]
    pub assignee_user_id: Option<String>,
    /// 展示用：`Agent（用户名）`；无负责人时为 None。
    #[serde(default)]
    pub assignee_name: Option<String>,
    /// 本任务的子任务 ID；进入进行中或完成流程的门槛不同，按任务序号排序。
    #[serde(default)]
    pub predecessor_task_ids: Vec<String>,
    /// 本任务的父级任务 ID；子任务回退或新增时父级可能回到进行中，按任务序号排序。
    #[serde(default)]
    pub unlock_task_ids: Vec<String>,
}

/// 当前本地时间，统一 'YYYY-MM-DD HH:MM:SS'
pub fn now_str() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn is_valid_task_type(s: &str) -> bool {
    TASK_TYPES.contains(&s)
}

pub fn is_valid_status(s: &str) -> bool {
    STATUSES.contains(&s)
}

pub fn is_valid_priority(s: &str) -> bool {
    PRIORITIES.contains(&s)
}

pub fn is_valid_submitter(s: &str) -> bool {
    SUBMITTERS.contains(&s)
}

fn default_priority() -> String {
    DEFAULT_PRIORITY.to_string()
}

pub fn is_agent_status(s: &str) -> bool {
    AGENT_STATUSES.contains(&s)
}

/// 只有明确完成或验收通过才满足父级任务的开始条件；待验证尚未完成验收流程。
pub fn satisfies_predecessor(status: &str) -> bool {
    status == "已完成" || status == "验收通过"
}

/// 子任务进入这些状态后，父级任务才可以进入待验证、已完成或验收通过。
pub fn ready_for_parent_completion(status: &str) -> bool {
    matches!(status, "待验证" | "已完成" | "验收通过")
}

pub fn submitter_name(submitter: &str, owner_username: Option<&str>) -> String {
    let username = owner_username
        .map(str::trim)
        .filter(|username| !username.is_empty())
        .unwrap_or("未知用户");
    if submitter == "Agent" {
        format!("Agent（{username}）")
    } else {
        username.to_string()
    }
}

/// 负责人展示名：与 `submitter_name` 的 Agent 形态一致（`Agent（用户名）`）。
/// assignee_user_id 为 None 时返回 None；账号已删除（外键置空外的异常情况）回退「未知用户」。
pub fn assignee_name(assignee_user_id: Option<&str>, username: Option<&str>) -> Option<String> {
    assignee_user_id?;
    let username = username
        .map(str::trim)
        .filter(|username| !username.is_empty())
        .unwrap_or("未知用户");
    Some(format!("Agent（{username}）"))
}

/// 状态迁移统一入口（规划 §7）：返回新的 finished_at。/// 规则：每次进入「待验证」「已完成」或「验收通过」刷新为当前时间（latest-wins）；
/// 离开这几个状态不清空。「验收通过」是终态，必须记完成时间。
/// 「取消」不刷新也不清空 finished_at。
pub fn transition(new_status: &str, current_finished_at: Option<String>) -> Option<String> {
    if new_status == "待验证" || new_status == "已完成" || new_status == "验收通过" {
        Some(now_str())
    } else {
        current_finished_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignee_display_mirrors_agent_submitter_form() {
        assert_eq!(assignee_name(None, Some("主机")), None);
        assert_eq!(
            assignee_name(Some("u1"), Some("alice")).as_deref(),
            Some("Agent（alice）")
        );
        assert_eq!(
            assignee_name(Some("gone"), None).as_deref(),
            Some("Agent（未知用户）")
        );
    }

    #[test]
    fn transition_refreshes_on_review_and_done() {
        let f = transition("待验证", None);
        assert!(f.is_some());
        let f2 = transition("已完成", f.clone());
        assert!(f2.is_some());
        assert!(f2 >= f); // latest-wins（同秒则相等）
    }

    #[test]
    fn submitter_display_includes_the_owning_username() {
        assert_eq!(submitter_name("用户", Some("主机")), "主机");
        assert_eq!(submitter_name("Agent", Some("alice")), "Agent（alice）");
        assert_eq!(submitter_name("用户", None), "未知用户");
        assert_eq!(submitter_name("Agent", None), "Agent（未知用户）");
    }

    #[test]
    fn transition_refreshes_on_acceptance() {
        // 未开始 → 验收通过（跳过待验证/已完成）：必须记完成时间
        let f = transition("验收通过", None);
        assert!(f.is_some(), "验收通过是终态，必须有完成时间");
        // 进行中（无 finished）→ 验收通过：同样要记
        let f2 = transition("验收通过", None);
        assert!(f2.is_some());
        // 已有完成时间 → 再验收通过：刷新为最新
        let f3 = transition("验收通过", f2.clone());
        assert!(f3.is_some());
        assert!(f3 >= f2);
    }

    #[test]
    fn transition_keeps_when_leaving() {
        let f = transition("待验证", None);
        // 验收未通过 → 回到进行中 → 不清空
        let kept = transition("进行中", f.clone());
        assert_eq!(kept, f);
        let kept2 = transition("验收未通过", f.clone());
        assert_eq!(kept2, f);
        // 重做后再进待验证 → 刷新
        let refreshed = transition("待验证", kept2.clone());
        assert!(refreshed.is_some());
    }

    #[test]
    fn transition_keeps_finished_at_on_cancel() {
        // 取消：不刷新完成时间
        assert_eq!(transition("取消", None), None);
        // 已有完成时间的任务被取消：保留原时间
        let f = transition("已完成", None);
        assert_eq!(transition("取消", f.clone()), f);
    }

    #[test]
    fn enums_are_stable() {
        assert_eq!(STATUSES.len(), 7);
        assert!(is_valid_status("取消"));
        assert!(AGENT_STATUSES.iter().all(|s| is_valid_status(s)));
        assert!(!is_agent_status("验收通过"));
        assert!(!is_agent_status("取消"), "取消只能由网页端设置");
        assert!(satisfies_predecessor("已完成"));
        assert!(satisfies_predecessor("验收通过"));
        assert!(!satisfies_predecessor("待验证"));
        assert_eq!(PRIORITIES, ["高", "中", "低"]);
        assert!(is_valid_priority(DEFAULT_PRIORITY));
        assert!(!is_valid_priority("紧急"));
    }
}
