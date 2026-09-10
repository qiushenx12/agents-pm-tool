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
/// Agent 仅可切到的状态（验收类状态留给用户，规划 §5.4）
pub const AGENT_STATUSES: [&str; 3] = ["进行中", "待验证", "已完成"];
pub const SUBMITTERS: [&str; 2] = ["用户", "Agent"];

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

pub fn is_agent_status(s: &str) -> bool {
    AGENT_STATUSES.contains(&s)
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

/// 状态迁移统一入口（规划 §7）：返回新的 finished_at。
/// 规则：每次进入「待验证」「已完成」或「验收通过」刷新为当前时间（latest-wins）；
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
    }
}
