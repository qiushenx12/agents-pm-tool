//! 任务进入「待验证」「已完成」时的桌面提醒通道。
//!
//! 与 SSE 的 `EventBus` 刻意分开：SSE 只携带“数据已变化”信号，
//! 而这里的通知带有任务摘要，仅在进程内传递给 Tauri 外壳用于系统通知，
//! 不经过任何网络接口，因此不受项目可见性授权约束的影响面限制在本地桌面。
use tokio::sync::broadcast;

use crate::domain::task::Task;

/// 一条“任务等待验收/已完成”提醒
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinishNotice {
    pub id: String,
    pub project: String,
    pub description: String,
    /// 进入的目标状态（「待验证」或「已完成」）
    pub status: String,
}

/// 进程内提醒总线；无订阅者（如无头服务模式）时静默丢弃
#[derive(Clone)]
pub struct FinishNoticeBus {
    tx: broadcast::Sender<FinishNotice>,
}

impl FinishNoticeBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self { tx }
    }

    pub fn notify(&self, notice: FinishNotice) {
        let _ = self.tx.send(notice);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<FinishNotice> {
        self.tx.subscribe()
    }
}

impl Default for FinishNoticeBus {
    fn default() -> Self {
        Self::new()
    }
}

/// 状态真正跨入「待验证」或「已完成」时给出提醒；重复设置同一状态、
/// 或切到其它状态（含验收类）都不提醒。
pub fn notice_on_finish(before: &Task, after: &Task) -> Option<FinishNotice> {
    let finished = matches!(after.status.as_str(), "待验证" | "已完成");
    if !finished || before.status == after.status {
        return None;
    }
    Some(FinishNotice {
        id: after.id.clone(),
        project: after.project.clone(),
        description: after.description.clone(),
        status: after.status.clone(),
    })
}

/// 系统通知正文用的描述摘要：取首行并按字符数截断（不切断 UTF-8 字符）。
pub fn summarize(text: &str, max_chars: usize) -> String {
    let first_line = text.lines().next().unwrap_or("").trim();
    let mut chars = first_line.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(status: &str) -> Task {
        Task {
            id: "202609141052110000".into(),
            seq: 1,
            project: "p".into(),
            task_type: "BUG".into(),
            description: "修复闪退".into(),
            note: String::new(),
            status: status.into(),
            priority: "中".into(),
            submitter: "用户".into(),
            submitter_name: "主机".into(),
            created_at: "2026-09-14 10:52:11".into(),
            finished_at: None,
            updated_at: "2026-09-14 10:52:11".into(),
            position: 1.0,
            attachment_count: 0,
            owner_user_id: None,
        }
    }

    #[test]
    fn notices_only_when_entering_review_or_done() {
        // 进行中 → 待验证 / 已完成：提醒
        assert!(notice_on_finish(&task("进行中"), &task("待验证")).is_some());
        assert!(notice_on_finish(&task("未开始"), &task("已完成")).is_some());
        // 同状态重复保存：不提醒
        assert!(notice_on_finish(&task("待验证"), &task("待验证")).is_none());
        // 离开或经过其它状态：不提醒
        assert!(notice_on_finish(&task("待验证"), &task("进行中")).is_none());
        assert!(notice_on_finish(&task("进行中"), &task("验收通过")).is_none());
        assert!(notice_on_finish(&task("未开始"), &task("取消")).is_none());
    }

    #[test]
    fn notice_carries_task_summary() {
        let notice = notice_on_finish(&task("进行中"), &task("待验证")).unwrap();
        assert_eq!(notice.status, "待验证");
        assert_eq!(notice.project, "p");
        assert_eq!(notice.description, "修复闪退");
    }

    #[test]
    fn summarize_takes_first_line_and_truncates_by_chars() {
        assert_eq!(summarize("第一行\n第二行", 80), "第一行");
        assert_eq!(summarize("短描述", 80), "短描述");
        let long = "一".repeat(100);
        let s = summarize(&long, 80);
        assert_eq!(s.chars().count(), 81);
        assert!(s.ends_with('…'));
        assert_eq!(summarize("", 80), "");
    }
}
