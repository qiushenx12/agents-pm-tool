use tokio::sync::broadcast;

/// 任务/项目变更事件总线（SSE 广播源）
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<()>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self { tx }
    }

    pub fn notify(&self) {
        // 无订阅者时忽略错误
        let _ = self.tx.send(());
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
