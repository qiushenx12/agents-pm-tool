use chrono::Local;
use serde::{Deserialize, Serialize};

pub const TASK_TYPES: [&str; 3] = ["新增需求", "优化", "BUG"];
pub const STATUSES: [&str; 6] = ["未开始", "进行中", "待验证", "已完成", "验收未通过", "验收通过"];
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
    pub status: String,
    pub submitter: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub updated_at: String,
    #[serde(default)]
    pub attachment_count: i64,
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

/// 状态迁移统一入口（规划 §7）：返回新的 finished_at。
/// 规则：每次进入「待验证」或「已完成」刷新为当前时间（latest-wins）；
/// 离开这两个状态不清空。
pub fn transition(new_status: &str, current_finished_at: Option<String>) -> Option<String> {
    if new_status == "待验证" || new_status == "已完成" {
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
    fn enums_are_stable() {
        assert_eq!(STATUSES.len(), 6);
        assert!(AGENT_STATUSES.iter().all(|s| is_valid_status(s)));
        assert!(!is_agent_status("验收通过"));
    }
}
