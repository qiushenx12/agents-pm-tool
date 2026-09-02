# Agents PM Tool — 项目长期备忘

## 架构要点（勿违背）
- 权限收窄在 **Rust 服务端** 强制：CLI = /api/agent/* 薄客户端，零 DB 访问；验收类状态（验收通过/未通过）只允许网页端。
- 状态迁移只走 `domain/task.rs::transition()` 单入口；finished_at 进入 待验证/已完成 刷新、离开不清空。
- 任务 ID = yyyymmdd+hhmmss+seq%04d，事务内 id_seq 自增（db/tasks.rs::create）。
- 数据目录优先级：PM_DATA_DIR 环境变量 > dev 项目根 data/ > release exe 同目录 data/。
- 枚举对齐三处：shared/types.ts、domain/task.rs、db/schema.rs CHECK 约束——改动要同步。

## 开发命令
- 前端：`npm run dev` / `npm run build`（含 vue-tsc）/ `npx vitest run`
- 后端：`cd src-tauri && cargo test --all-targets`
- 无头联调：`PM_DATA_DIR=<项目根>/data cargo run --example serve`，然后 pm-cli（同样设 PM_DATA_DIR）即可联调，无需开 Tauri 窗口。

## 技术栈基线
- 对齐 cc-launcher（D:\project\cc-launcher）：tauri.conf（frameless/NSIS/currentUser/SimpChinese）、theme.css/components.css、Cargo 依赖子集；新增 clap、rust-embed、async-stream、url、mime_guess、tower-http。
