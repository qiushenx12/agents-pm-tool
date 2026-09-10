# Agents PM Tool — 开发规划

> 面向 Agents 的项目管理工具：单个桌面 app 启动内嵌 Web 服务，多维表格在**浏览器**中展示与操作；
> 数据落盘在 **app 同目录的 SQLite**；同时提供一个**权限收窄的 CLI** 供 Agent 创建/查看/推进任务。
> 技术栈与工程规范全面对齐 `D:\project\cc-launcher`。

---

## 1. 项目定位

### 1.1 产品形态

```
┌─────────────────────────────────────────────────────────────┐
│  Agents PM Tool（Tauri 2 桌面 app，单 exe 安装）              │
│  ├─ 配置窗口（自绘标题栏）：端口/自启动/打开网页/服务状态        │
│  ├─ 内嵌 axum HTTP 服务（127.0.0.1:<port>）                  │
│  │   ├─ 静态托管 Vue 网页（多维表格）                          │
│  │   ├─ /api/web/*   网页端全量 API                           │
│  │   └─ /api/agent/* Agent 受限 API（Bearer token）           │
│  └─ 数据落盘：<exe 同目录>/data/pm.db + attachments/          │
├─────────────────────────────────────────────────────────────┤
│  浏览器：用户打开 http://127.0.0.1:<port> 使用多维表格          │
├─────────────────────────────────────────────────────────────┤
│  pm-cli.exe（随 app 同目录发布）：Agent 的命令行入口            │
│      ↓ 读取 <exe 同目录>/data/runtime.json（port + token）     │
│      ↓ 调用 /api/agent/*（权限服务端强制收窄）                  │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 关键决策

- **表格是固定结构的任务表**，不是飞书式自由建模：9 个字段全部内置，无字段类型系统、无多表/多视图（见 §4）。
- **所有鉴权与校验在服务端（Rust）执行**，CLI 只是 HTTP 薄客户端 —— 这是"避免给 Agent 过多权限"的根本保障：即使 Agent 绕过 CLI 直接发 HTTP，也只能摸到 `/api/agent/*`。
- **数据目录 = exe 同目录下的 `data/`**（便携式）。安装器沿用 cc-launcher 的 `installMode: currentUser`（装到 `%LOCALAPPDATA%`，保证可写）；开发模式用项目根 `data/`。

### 1.3 非目标

- 多用户/账号/协同/云同步；多张表、自定义字段、看板/日历等视图；公式/自动化。

---

## 2. 技术栈（对齐 cc-launcher）

| 层 | 选型 | 说明 |
|---|---|---|
| 桌面壳 | **Tauri 2.5** | frameless 窗口、NSIS 打包，沿用 cc-launcher 配置 |
| 前端 | **Vue 3.5 + TS(strict) + Vite 6 + Pinia(setup 风格)** | 自研组件 + CSS 变量双主题（沿用 `theme.css/components.css` 体系） |
| 网页服务 | **axum 0.8 + tokio** | cc-launcher Cargo 依赖中已有，直接沿用 |
| 静态资源嵌入 | **rust-embed**（新增） | 把 `dist/` 打进 exe，单文件分发 |
| 数据库 | **rusqlite 0.32（bundled）** | 同 cc-launcher |
| CLI | **clap 4 + reqwest(rustls)**（新增 clap） | 独立 `[[bin]] pm-cli`，只依赖 HTTP |
| 实时刷新 | **SSE**（axum `Sse` + tokio broadcast） | Agent 通过 CLI 改数据后网页秒级刷新；断线降级为 10s 轮询 |
| 其余 | serde/serde_json、chrono、uuid、dirs | 沿用 cc-launcher 子集 |

---

## 3. 项目结构

```
agents-pm-tool/
├── docs/development-plan.md
├── package.json / tsconfig*.json / vite.config.ts   # vite 多入口：index.html(网页) + config.html(配置窗)
├── index.html                    # 网页端入口（axum 托管）
├── config.html                   # Tauri 配置窗入口
├── src/
│   ├── main.ts  / config-main.ts # 两个入口各自的 bootstrap
│   ├── grid-app/                 # 网页端（多维表格）
│   │   ├── GridApp.vue
│   │   ├── api/client.ts         # fetch 封装：统一错误、SSE 订阅
│   │   ├── stores/taskStore.ts   # 任务列表、筛选条件、SSE 失效刷新
│   │   ├── stores/metaStore.ts   # 项目选项、枚举常量
│   │   └── components/
│   │       ├── TaskGrid.vue / GridHeader.vue / GridRow.vue / GridCell.vue
│   │       ├── ProjectOptionPopover.vue  # 表头下拉：项目选项增删改（§5.3）
│   │       ├── FilterBar.vue             # 筛选 + 关键字搜索
│   │       ├── TaskCreateModal.vue / TaskDetailDrawer.vue
│   │       ├── AttachmentCell.vue / AttachmentUploader.vue
│   │       └── StatusSelect.vue / SubmitterTag.vue
│   ├── config-app/               # Tauri 配置窗（小页面）
│   │   └── ConfigApp.vue         # 端口、开机自启服务、打开网页、服务状态、数据目录
│   ├── shared/
│   │   ├── theme.css / components.css   # 拷贝自 cc-launcher
│   │   └── types.ts              # Task/Project/枚举 与 Rust DTO 对齐
│   └── ...
├── src-tauri/
│   ├── Cargo.toml                # [lib] + [[bin]] name="pm-cli"
│   ├── tauri.conf.json           # 沿用 cc-launcher（frameless、NSIS、currentUser、SimpChinese）
│   └── src/
│       ├── main.rs / lib.rs      # Tauri 启动 → 拉起 ServerHandle；托盘可选
│       ├── paths.rs              # exe 同目录 data/ 解析（dev 模式回退项目根）
│       ├── settings.rs           # app 配置（port/autostart）持久化 data/settings.json
│       ├── server/
│       │   ├── mod.rs            # axum Router 组装、启动/停止、端口占用自动顺延
│       │   ├── static_site.rs    # rust-embed 托管 dist
│       │   ├── api_web.rs        # /api/web/*（全量）
│       │   ├── api_agent.rs      # /api/agent/*（token + 收窄）
│       │   ├── auth.rs           # Agent token 校验中间层
│       │   └── events.rs         # SSE broadcast
│       ├── db/
│       │   ├── mod.rs            # 连接（Mutex）+ PRAGMA + 迁移（user_version）
│       │   ├── schema.rs
│       │   └── tasks.rs          # 查询构造（筛选/排序/关键字）
│       ├── domain/
│       │   ├── task.rs           # Task 模型、状态机、校验规则（§4/§6）
│       │   ├── idgen.rs          -- ID 生成（§4.2）
│       │   └── attachment.rs     # 扩展名白名单、落盘
│       └── cli/main.rs           # pm-cli：clap 子命令 → HTTP 客户端
└── tests/                        # Vitest + src-tauri/tests（cargo test）
```

---

## 4. 数据模型与核心规则

### 4.1 SQLite Schema（固定任务表，无泛化建模）

```sql
CREATE TABLE tasks (
  id           TEXT PRIMARY KEY,              -- §4.2 规则生成
  seq          INTEGER NOT NULL UNIQUE,       -- 全局创建序号（即 id 尾号）
  project      TEXT NOT NULL,                 -- 项目选项名（外键约束到 projects.name 由应用层保证）
  type         TEXT NOT NULL CHECK (type IN ('新增需求','优化','BUG')),
  description  TEXT NOT NULL DEFAULT '',
  status       TEXT NOT NULL DEFAULT '未开始'
               CHECK (status IN ('未开始','进行中','待验证','已完成','验收未通过','验收通过')),
  submitter    TEXT NOT NULL CHECK (submitter IN ('用户','Agent')),
  created_at   TEXT NOT NULL,                 -- 'YYYY-MM-DD HH:MM:SS' 本地时间（展示渲染为 yyyy/mm/dd hh:mm:ss）
  finished_at  TEXT,                          -- §4.3 规则
  updated_at   TEXT NOT NULL
);
CREATE INDEX idx_tasks_filter ON tasks(project, type, status, submitter);
CREATE INDEX idx_tasks_created ON tasks(created_at DESC);

CREATE TABLE projects (                       -- 「项目」单选选项集，可扩展
  name       TEXT PRIMARY KEY,
  color      TEXT NOT NULL DEFAULT '#007AFF',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE TABLE attachments (
  id          TEXT PRIMARY KEY,               -- uuid
  task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  filename    TEXT NOT NULL,                  -- 原始文件名（展示用）
  stored_path TEXT NOT NULL,                  -- 相对 data/ 的路径
  mime        TEXT,
  size        INTEGER NOT NULL,
  created_at  TEXT NOT NULL
);
CREATE INDEX idx_attach_task ON attachments(task_id);

CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);  -- id_seq 等
PRAGMA user_version = 1;
```

### 4.2 任务 ID 生成规则

- 格式：`yyyymmdd` + `hhmmss` + `xxxx`（18 位），例：`202609021050340001`。
- `xxxx` = **全局创建序号** `%04d` 零填充（来自 `meta.id_seq`，超过 9999 自然变长，不截断）。
- 生成在**同一 SQLite 事务**内完成（`id_seq+1 → UPDATE meta → INSERT task`），并发下不重复、不回退。

### 4.3 时间与状态规则

| 规则 | 说明 |
|---|---|
| 创建时间 | 插入时服务端写本地时间，**任何接口不可修改**；网页显示格式严格为 `yyyy/mm/dd hh:mm:ss` |
| 完成时间 | 状态**每次进入** `待验证`、`已完成` 或 `验收通过` 时刷新为当前时间（latest-wins，如验收未通过后重做再回到待验证，取最新一次）；离开这几个状态**不清空**；任何接口不可直接写入 |
| 状态默认 | 创建时固定 `未开始`，创建接口不接受 status 入参 |
| 提交人 | 创建时按来源强制：网页 → `用户`（可选改 Agent 无意义，直接锁定）；CLI → `Agent`。均不可事后修改 |

---

## 5. 功能规格

### 5.1 字段规格总表

| # | 字段 | 类型 | 创建时规则 | 创建后修改 |
|---|---|---|---|---|
| 1 | ID | 系统生成 | §4.2 | 不可改 |
| 2 | 项目 | 单选（选项可扩展） | **必填**（Agent 不填 → 创建失败）；只能选已存在选项 | 仅网页端可改 |
| 3 | 任务类型 | 单选：新增需求/优化/BUG | **必填**（Agent 不填 → 创建失败） | 仅网页端可改 |
| 4 | 任务描述 | 字符串 | 网页端可留空；**Agent 必须写入，否则创建失败** | 网页端任意改；CLI 只能改 `submitter=Agent` 的任务 |
| 5 | 当前状态 | 单选 6 态 | 默认 `未开始` | 见 §5.4 权限矩阵 |
| 6 | 提交人 | 来源为用户/Agent；展示实际用户名或 `Agent（用户名）` | 按来源与当前账号自动写入 | 不可改 |
| 7 | 附件 | 文件列表 | 创建后可随时上传 | 网页端管理；Agent 只读列出和下载 |
| 8 | 创建时间 | 系统时间 | 自动 | 不可改 |
| 9 | 完成时间 | 系统时间 | — | 不可改（§4.3 自动记录） |
| 10 | 备注 | 字符串 | 网页端可留空；Agent 创建时为空 | 仅网页端可改；CLI 只读 |

### 5.2 网页端（多维表格）

- **单表网格**：10 列固定；行内编辑（项目/类型/状态下拉、描述与备注点击进编辑）；备注固定在末列并自动填满剩余宽度，末列右边界不可拖动；列宽拖拽、按列排序（单字段升降序）；行高自适应描述换行。
- **筛选栏**：项目/类型/状态/提交人多选筛选 + 关键字（匹配 ID、描述与备注）；筛选条件进 URL query，刷新不丢。
- **新建/详情**：顶部「新建任务」弹窗（项目*、类型*、描述、备注、附件）；点击行展开右侧抽屉，含完整描述、备注、附件管理和时间信息。
- **删除任务**：仅网页端，行菜单内二次确认。
- **空态**：内置示例项目选项 `default-project`（初始化种子数据）。
- **实时刷新**：SSE 收到 `tasks_changed` 事件后增量刷新当前筛选结果；SSE 断开降级 10s 轮询。

### 5.3 项目选项管理（对齐多维表格表头交互）

- 点击「项目」列表头的下拉图标 → 弹出选项列表面板（参考飞书多维表格）：
  选项 = 彩色标签；支持**新建选项**（输入即建）、**重命名**、**改颜色**、拖拽排序、删除。
- 删除/重命名保护：重命名级联更新存量任务（事务）；**被任务引用的选项不可删除**（提示引用数量）。
- Agent/CLI 侧**只能消费**选项列表（`GET /api/agent/projects` 只读），不可增删改。

### 5.4 权限矩阵（核心：CLI 能力精确收窄）

| 能力 | 网页端（用户） | CLI（Agent） |
|---|---|---|
| 查看/筛选任务 | ✅ | ✅ |
| 创建任务 | ✅（描述可空） | ✅（**项目/类型/描述三必填**，submit 强制 Agent，status 固定未开始） |
| 修改任务状态 | ✅ 六态任意切换 | ✅ **仅可切到 `进行中` / `待验证` / `已完成`**（验收类状态留给用户，Agent 不能自己验收自己） |
| 修改任务描述 | ✅ | ✅ **仅限 `submitter='Agent'` 的任务**（用户创建的任务描述 CLI 不可碰） |
| 修改备注 | ✅ | ❌（只读） |
| 修改项目/任务类型 | ✅ | ❌ |
| 上传/删除附件 | ✅ | ❌ |
| 删除任务 | ✅ | ❌ |
| 管理项目选项 | ✅ | ❌（只读列表） |
| 修改 ID/提交人/创建时间/完成时间 | ❌（系统字段） | ❌ |

> 以上全部在 **Rust 服务端**强制：CLI 能力 = `/api/agent/*` 的全部能力，不存在"隐藏接口"。

### 5.5 Agent CLI（`pm-cli`）

```
pm-cli list   [--project <名>] [--type <新增需求|优化|BUG>] [--status <状态>]
              [--submitter <用户|Agent>] [--keyword <词>] [--json]
pm-cli get    <id> [--json]
pm-cli create --project <名> --type <类型> --description <文本> [--json]
pm-cli status <id> --to <进行中|待验证|已完成> [--json]
pm-cli describe <id> --description <文本> [--json]     # 仅 Agent 创建的任务
```

- **服务发现**：读 `<exe 同目录>/data/runtime.json`（app 启动时写入 `{port, token, pid}`）；文件缺失或连接失败 → 退出码 3，报「服务未启动，请先打开 Agents PM Tool」。
- **校验失败**：退出码 2，stderr 输出中文原因 + 合法取值列表（如项目不存在时列出全部项目选项）。
- **`--json`**：所有命令支持，输出结构化 JSON，方便 Agent 解析；默认输出为人类可读表格。
- **token**：`Authorization: Bearer <runtime.json.token>`，服务端中间层校验。
- CLI 无任何本地 DB 访问能力，不读 sqlite。

### 5.6 App 配置窗（Tauri 窗口，基础配置）

- HTTP 端口（默认 17890，占用时启动自动顺延并提示）、保存即重启服务；
- 「启动 app 后自动开启服务」开关（默认开）、「打开网页」按钮、服务运行状态与访问地址展示；
- 数据目录展示（固定 `<exe>/data/`，不支持迁移，v1 简化）；
- 重新生成 Agent token（按钮，立即生效并写 runtime.json）；
- 设置持久化：`data/settings.json`，前端走 Tauri invoke（cc-launcher `appSettings` hydrate/load 模式照搬）。

### 5.7 HTTP API 分层

| 路由 | 鉴权 | 能力 |
|---|---|---|
| `GET /api/web/tasks`（筛选/排序/关键字）、`POST`、`PATCH /:id`、`DELETE /:id` | 仅监听 127.0.0.1 | 全量 |
| `GET/POST/PATCH/DELETE /api/web/projects`（含重命名级联） | 同上 | 全量 |
| `POST /api/web/tasks/:id/attachments`（multipart）、`GET /api/web/attachments/:id`、`DELETE` | 同上 | 全量 |
| `GET /api/web/events` | 同上 | SSE |
| `GET /api/agent/tasks`、`GET /:id`、`POST`、<br>`GET /:id/attachments`、`GET /api/agent/attachments/:id`、<br>`PATCH /:id/status`、`PATCH /:id/description`、`GET /api/agent/projects` | 127.0.0.1 **+ Bearer token** | §5.4 收窄集；附件仅可读 |

统一错误格式：`{"error": {"code": "...", "message": "中文描述", "details": {...}}}`。

---

## 6. 开发阶段规划

### Phase 0 — 工程脚手架（0.5~1 天）

- [x] 按 §3 初始化：Vite 多入口（index/config）、TS strict、`@` 别名、Pinia；拷贝 cc-launcher 的 `vite.config.ts/tsconfig/tauri.conf/theme.css/components.css` 并裁剪
- [x] Rust：`paths.rs`（exe 同目录 `data/`，dev 回退项目根）、rusqlite 建库 + `user_version` 迁移框架
- [x] axum 服务最小启动 + rust-embed 托管 + 端口顺延；Tauri 配置窗显示「服务已启动 http://127.0.0.1:17890」
- [x] `runtime.json` 写入 + token 生成

**验收**：`tauri dev` 起窗，浏览器打开网页看到占位页；data/ 生成 pm.db 与 runtime.json。

### Phase 1 — 数据层 + Agent 通路（2 天）

- [x] schema 全量 + 种子项目 `default-project`；idgen（事务内序号）+ 状态机/校验规则单测先行
- [x] `/api/web/tasks` CRUD + 筛选排序关键字；`/api/web/projects` 全量（含级联重命名、引用保护）
- [x] `/api/agent/*` 五个接口 + token 中间层 + §5.4 全部收窄规则
- [x] `pm-cli`：五个子命令 + `--json` + 退出码/中文报错
- [x] SSE broadcast（任务变更即广播）

**验收**：curl/CLI 全链路通；CLI 越权用例（改项目、改用户任务描述、设验收通过）全部 403/422。

### Phase 2 — 网页表格（3 天）

- [x] TaskGrid：9 列渲染、行内编辑（下拉/文本）、列排序、列宽拖拽
- [x] FilterBar（多选筛选 + 关键字 + URL query 同步）
- [x] 新建弹窗 + 详情抽屉 + 删除二次确认
- [x] ProjectOptionPopover 表头选项管理（增删改色排序、引用保护提示）
- [x] SSE 实时刷新 + 降级轮询；空态/加载态/错误态

**验收**：网页端完成 §5.2/§5.3 全部交互；CLI 建任务网页 1s 内出现。

### Phase 3 — 附件 + 配置窗完善（2 天）

- [x] 附件上传（multipart，扩展名白名单：png/jpg/jpeg/gif/webp/mp4/mov/doc/docx/ppt/pptx/md/txt/pdf/xlsx；单文件 ≤200MB）、图片/视频内联预览、其余下载、删除
- [x] 配置窗：端口修改重启服务、自启动开关、打开网页、重生成 token、数据目录展示
- [x] 窗口关闭行为（关窗保服务 or 退出一并停服务，配置项）

**验收**：附件全链路；配置项全部生效并持久化。

### Phase 4 — 测试与发布（1~2 天）

- [x] cargo test：idgen 并发、状态机、CLI 权限收窄、级联重命名、完成时间规则
- [x] Vitest：筛选条件序列化、时间格式渲染、类型对齐
- [x] 10 万行任务冒烟（网格目前数据量小，分页/滚动按需即可，**不做虚拟滚动过度设计**；若实测卡顿再加行虚拟化）
- [x] NSIS 打包：`Agents PM Tool.exe` + `pm-cli.exe` 同目录；安装后新建→CLI→网页闭环验证（2026-09-02 静默安装 `setup.exe /S` 实测通过，见 `docs/review-2026-09-02.md`）

---

## 7. 技术难点与对策

| 难点 | 对策 |
|---|---|
| exe 同目录可写性 | 安装器 `installMode: currentUser`（装用户目录天然可写）；启动时探测不可写则明确报错引导重装，**不静默回退** |
| CLI 找到服务 | runtime.json（port/token/pid）；pid 失效 → 报「请先启动 app」 |
| 权限收窄被绕过 | 鉴权全部在 `/api/agent/*` 服务端路由内；token 中间层；CLI 零 DB 访问 |
| ID 并发唯一 | 事务内 `id_seq` 自增 + `seq UNIQUE` 双保险 |
| 完成时间语义 | 状态迁移统一走 `domain/task.rs::transition()` 单入口，进入待验证/已完成即刷新，单测覆盖往返路径 |
| 网页实时性 | SSE + broadcast channel；失败降级轮询，不阻塞主流程 |
| 端口被占 | 启动时顺延 + 配置窗显式提示当前实际端口 |

---

## 8. 测试策略

- **Rust（cargo test，核心）**：idgen（并发 100 线程无重复）、状态机/完成时间、agent API 权限矩阵全用例、项目选项级联与引用保护、筛选 SQL 构造。
- **前端（Vitest）**：时间 `yyyy/mm/dd hh:mm:ss` 渲染、筛选 query 编解码、枚举常量与后端对齐快照。
- **端到端冒烟（Phase 4 手工清单）**：安装 → 开 app → 网页建任务 → CLI 筛选/改状态/改描述 → 网页刷新可见 → 验收状态切换 → 附件上传预览 → 重启数据完整。

---

## 9. 与 cc-launcher 的复用清单

| 资产 | 复用方式 |
|---|---|
| `vite.config.ts` / `tsconfig*.json` | 拷贝裁剪；改为多入口（index + config） |
| `tauri.conf.json` + capabilities | 改 productName/identifier；保留 frameless、NSIS、currentUser、SimpChinese |
| `theme.css` / `components.css` | 沿用设计变量与双主题，增补标签色/表格样式 |
| Pinia setup 风格 + hydrate/load 两段式 | 配置窗与网页端 store 统一照写 |
| Cargo 依赖子集 | axum/tokio/rusqlite/serde/chrono/uuid/dirs 原样沿用；新增 clap、rust-embed |
| 设置持久化模式（Rust 命令 + settings store） | 配置窗照搬 |
| NSIS 打包配置 | 沿用，增加 pm-cli.exe 随包 |
