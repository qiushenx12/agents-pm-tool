# AGENTS.md

本文件适用于整个仓库，供后续接手本项目的 Agent 快速建立上下文并安全地开展工作。

## 1. 项目速览

Agents PM Tool 是一个 Windows 本地项目管理工具，服务于“用户维护任务、AI Agent 受限推进任务”的协作场景。

运行时由三部分组成：

1. **Tauri 桌面应用**：显示设置窗口，管理内嵌 HTTP 服务、系统托盘和应用生命周期。
2. **浏览器任务工作台**：Vue 应用，由 Rust/Axum 服务托管，用户在这里管理项目、任务和附件。
3. **`pm-cli`**：供 Agent 使用的受限 HTTP 客户端，是 skill 里的一个零依赖 Node 脚本（`pm-cli-skill/bin/pm-cli.mjs`，需要 Node.js 18+，不随安装包分发、不写系统 PATH），只调用 `/api/agent/*`，不直接访问数据库。

主要技术栈：

- Vue 3 + TypeScript + Vite + Pinia
- Tauri 2 + Rust + Axum + Tokio
- SQLite（rusqlite）
- Vitest + Cargo Test

产品介绍和用户使用方法见 `README.md`。`docs/` 中包含开发规划、评审和验收记录，但部分描述可能早于当前实现；发生冲突时，以**当前代码、测试和配置文件**为准。

## 2. 开始工作前

先执行以下只读检查：

```powershell
git status --short
git diff --stat
rg --files -g "AGENTS.md" -g "!node_modules" -g "!src-tauri/target"
```

工作区可能已有用户的未提交修改。不要覆盖、回退或格式化无关文件；只修改任务需要的内容。尤其不要使用 `git reset --hard`、`git checkout -- <file>` 等破坏性命令。

本项目主要在 Windows 和 PowerShell 下开发。文件写入优先使用补丁式修改，搜索优先使用 `rg`。

## 3. 常用命令

### 安装依赖

```powershell
npm ci
```

### 开发运行

推荐人工交互时使用：

```powershell
python dev.py
```

`dev.py` 会检查 Node.js、npm、Rust，按需安装依赖，先构建 `dist/`，再启动 Tauri 开发模式。它是**交互式长驻脚本，没有命令行参数或 `--help`**；自动化 Agent 不要用它做环境探测，否则会直接启动应用并在退出时等待输入。

自动化调试应拆分执行：

```powershell
npm run build
npm run tauri dev
```

只启动无桌面窗口的 Axum 服务：

```powershell
npm run build
cargo run --manifest-path src-tauri/Cargo.toml --example serve
```

可通过 `PM_DATA_DIR` 为无头服务或测试指定独立数据目录。不要让实验数据写入用户正在使用的 `data/`。

### 验证

```powershell
npm run build
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

按变更范围选择验证：

- 仅文档：至少执行 `git diff --check` 并核对命令、路径和功能描述。
- 前端逻辑或组件：执行 `npm run build` 和 `npm test`。
- Rust、数据库、API、CLI：执行 `cargo test --manifest-path src-tauri/Cargo.toml`；涉及前后端 DTO 时同时执行前端构建和测试。
- 打包相关：先执行 `npm run build`，只有用户明确要求正式打包时才运行 `python build.py`。

`python build.py` 是 Windows 正式发布脚本，会同步多个版本文件、构建 NSIS 安装包，并询问是否将版本记录为已发布。它会改变发布状态，不是普通的只读构建命令，不要为了“验证一下”随意运行。

### 其他脚本

```powershell
npm run dev                       # 仅启动 Vite；主要用于 Tauri 设置页开发
npm run build                     # vue-tsc + Vite，多入口构建到 dist/
npm run preview                   # 预览 Vite 构建产物
```

`npm test` 里包含 pm-cli 与 skill 安装脚本的行为测试（以子进程方式跑真实的 `pm-cli.mjs`）。

前端有两个入口：`index.html` 是浏览器任务工作台，`config.html` 是 Tauri 设置窗口。Tauri 开发窗口使用 Vite 的 `config.html`，而任务工作台由 Axum 从编译时嵌入的 `dist/` 提供。因此修改 `grid-app` 后，仅运行 `npm run dev` 不足以更新内嵌页面；需要重新执行 `npm run build`，并让 Rust 服务重新编译/启动。

## 4. 目录与模块职责

```text
agents-pm-tool/
├─ src/
│  ├─ main.ts                    # 浏览器任务工作台入口
│  ├─ config-main.ts             # Tauri 设置页入口
│  ├─ grid-app/
│  │  ├─ GridApp.vue             # 工作台外壳与项目导航
│  │  ├─ api/client.ts           # Web API 客户端、查询参数、SSE
│  │  ├─ stores/
│  │  │  ├─ taskStore.ts         # 分页任务、筛选、批量、排序、刷新
│  │  │  ├─ metaStore.ts         # 项目选项
│  │  │  ├─ viewStore.ts         # 表格列设置
│  │  │  ├─ savedViewStore.ts    # 浏览器本地筛选方案
│  │  │  └─ filters.ts           # URL 筛选状态序列化/校验
│  │  └─ components/             # 表格、详情、新建、筛选、附件等 UI
│  ├─ config-app/ConfigApp.vue   # 服务与桌面行为设置
│  └─ shared/                    # DTO、枚举、主题、反馈和基础组件
├─ src-tauri/
│  ├─ src/lib.rs                 # Tauri 生命周期、托盘、设置命令、服务启停
│  ├─ src/netinfo.rs             # 本机地址探测（局域网 IPv4、Tailscale）
│  ├─ src/paths.rs               # 数据目录解析
│  ├─ src/settings.rs            # 端口、访问范围、关闭行为
│  ├─ src/window_state.rs        # 应用内窗口的位置/大小记忆
│  ├─ src/server/
│  │  ├─ mod.rs                  # Axum 路由、端口选择、runtime.json
│  │  ├─ api_auth.rs             # 注册、登录、主机登录与会话
│  │  ├─ api_web.rs              # 网页端完整 CRUD
│  │  ├─ api_agent.rs            # Agent 受限 API
│  │  ├─ api_users.rs            # 用户、角色与权限管理
│  │  ├─ api_agent_access.rs     # 当前用户 Agent token 与接入文案
│  │  ├─ api_skill.rs            # skill 载荷、安装脚本与本机一键安装
│  │  ├─ api_batch.rs            # 批量更新/删除
│  │  ├─ auth.rs                 # Web 会话、CSRF 头与 Agent token 校验
│  │  ├─ events.rs               # SSE 事件总线
│  │  └─ static_site.rs          # rust-embed 静态资源
│  ├─ src/db/
│  │  ├─ mod.rs                  # SQLite 打开、PRAGMA、迁移版本
│  │  ├─ schema.rs               # 初始 Schema 和种子项目
│  │  ├─ tasks.rs                # 任务 CRUD、筛选、排序、状态规则
│  │  ├─ task_page.rs            # 分页、分组计数和锚点定位
│  │  ├─ projects.rs             # 项目 CRUD、重命名级联
│  │  ├─ users.rs                # 用户、会话和 Agent token
│  │  ├─ view_state.rs           # 按用户保存的分组/排序/筛选
│  │  └─ permissions.rs          # 项目/字段/枚举选项授权
│  ├─ src/domain/                # Task、ID、附件白名单等领域规则
│  ├─ examples/serve.rs          # 无头服务
│  └─ tests/                     # Rust API/权限/分页集成测试
├─ tests/                         # Vitest 测试（含 pm-cli 行为测试）
├─ pm-cli-skill/                  # skill 载荷源：SKILL.md、VERSION、bin/ 下的 pm-cli 脚本、安装脚本
├─ scripts/                       # 前端增量构建脚本
├─ docs/                          # 规划与验收资料
├─ dev.py                         # 交互式开发启动器
├─ build.py                       # 正式发布脚本
└─ version.json                   # 发布版本状态
```

## 5. 核心业务约束

修改业务逻辑时必须保留以下不变量，并同步更新 Rust 测试、前端类型和 README（如用户可见行为发生变化）。

### 任务字段与枚举

- 类型只有：`新增需求`、`优化`、`BUG`。
- 状态只有：`未开始`、`进行中`、`待验证`、`已完成`、`验收未通过`、`验收通过`、`取消`。
- 提交人只有：`用户`、`Agent`。
- Web 创建任务时提交人固定为“用户”；Agent API 创建时固定为“Agent”。
- `tasks.submitter` 只表示不可变的提交来源枚举；`owner_user_id` 记录实际创建账号，任务 DTO 的 `submitter_name` 显示用户名或 `Agent（用户名）`。无法追溯账号的历史记录显示“未知用户”。
- 创建状态固定为“未开始”。
- ID、提交人、创建时间和完成时间均不可由客户端直接修改。

枚举在以下位置保持一致：

- `src/shared/types.ts`
- `src-tauri/src/domain/task.rs`
- `src-tauri/src/db/schema.rs` 中的 SQLite `CHECK`
- 相关前端选项和测试

### ID 与时间

- 任务 ID 在 SQLite 事务中生成，格式为本地时间前缀（`yyyymmddhhmmss`）加同一秒内递增的 4 位后缀（每秒从 0000 开始）；全局递增序号仅存于 `tasks.seq`，不要把 ID 生成移到前端或 CLI。
- 任务每次进入 `待验证`、`已完成`、`验收通过` 时刷新 `finished_at`。
- 离开上述状态时不清空已有 `finished_at`。

### 用户、会话与 Web 权限

- 固定 `users.id='host'` 的主机账号在每次打开数据库时确保存在并纠偏为 `super_admin`；只能从 loopback 免密登录，不能密码登录、停用、删除或降级。
- 命名用户密码仅保存 Argon2 哈希；登录会话保存在 `sessions`，通过 HttpOnly、`SameSite=Lax` Cookie 传递，默认 7 天滑动过期。
- 除公开注册/登录接口外，`/api/web/*` 均需有效会话；所有 Web 写请求还必须携带 `X-PM-Client: web`。前端统一由 `src/grid-app/api/client.ts` 附加，不要在组件中散落裸 `fetch`。
- 角色为 `super_admin`、`admin`、`user`。管理员可管理普通用户与项目；超级管理员可调整其他非主机账号角色；普通用户只能访问 `user_permissions` 授权的项目、字段和枚举值。
- 任务的 `owner_user_id` 记录网页或 Agent 创建任务的所属用户。命名用户的 Agent 只能修改自己通过 Agent 创建的任务描述，不能修改同一用户在网页创建的任务；v8 迁移会把旧版全局 Agent 创建的存量任务归属到主机账号。
- v9 迁移会把仍无 `owner_user_id` 的历史任务归属到主机账号；不得覆盖已经记录的用户归属。账号删除后外键会将归属置空，展示为“未知用户”。
- 项目可见性必须覆盖列表、分页、详情、分组/锚点和附件入口。SSE 只发送“数据变化”信号，不得夹带未授权任务内容。

### Agent 权限

`/api/agent/*` 是真正的权限边界，CLI 只是一层薄封装。不能只在 CLI 中做校验。

Agent 可以：

- 查看、筛选已授权项目中的任务；
- 只读查看已授权项目；
- 创建任务，但项目、类型和非空描述三项必填；
- 列出和下载已授权项目中任务的附件；
- 将任务切换到 `进行中`、`待验证`、`已完成`；
- 修改属于当前 token 用户、且提交人为 `Agent` 的任务描述，且不能清空描述。

上述能力还必须与 token 所属用户的 `user_permissions` 取交集。主机/管理员并不绕过 Agent API 自身的固有限制。

Agent 不可以：

- 切换到验收状态；
- 修改项目、类型或用户创建任务的描述；
- 删除任务；
- 管理项目，或上传、删除附件；
- 直接读取/写入 SQLite。

涉及 Agent 权限的修改至少覆盖 `src-tauri/tests/api.rs` 中的权限矩阵测试。不要为了复用 Web handler 而意外扩大 Agent 能力。

### 项目、附件与批量操作

- 项目名称唯一；重命名必须在事务中级联已有任务。
- 被任务引用的项目不可删除。
- 项目还包含 `color`、`local_path`、`git_url`；变更 DTO 时同步前端 `Project` 类型和 CLI 项目输出。
- 附件扩展名白名单与 200 MB 上限定义在 `src-tauri/src/domain/attachment.rs`。
- 删除任务时同时清理附件记录和磁盘文件。
- Web 批量操作每次限定 1–500 个任务，并返回逐项成功/失败结果；不要把部分失败伪装成全量成功。

## 6. API 与前端状态约定

- Web API 前缀：`/api/web`。`/auth/register`、`/auth/login`、`/auth/host-login` 为公开入口，其余接口全部经过会话中间件；用户管理、权限、当前用户 Agent 接入与本机 skill 安装也在此前缀下。
- Agent API 前缀：`/api/agent`，默认全部按用户 Bearer token 鉴权并注入用户上下文；`/help` 提供自描述帮助，`/skill/payload`（文件清单）与 `/skill/install.mjs`（自包含安装脚本）提供 skill —— 这三个**免 token**，因为「还没有 skill 的 Agent」必须先拿到它们，且内容不含任何机密。
- 错误响应统一为：

  ```json
  {
    "error": {
      "code": "error_code",
      "message": "中文错误信息",
      "details": {}
    }
  }
  ```

- Rust DTO 和 `src/shared/types.ts` 必须保持字段名、可空性和枚举一致。
- 任务查询支持项目、类型、状态、提交人、关键词、排序；分页接口还支持分组和锚点定位。
- 提交人筛选的 `submitter` 查询参数接受三类值：大类（`用户`/`Agent`）按 `tasks.submitter` 匹配；裸用户名（如 `主机`）命中该账号作为「用户」提交的任务；`Agent（用户名）` 命中该账号作为 Agent 提交的任务——与任务 DTO 的 `submitter_name` 显示形态一一对应。混选取并集；`未知用户` 命中 `owner_user_id IS NULL` 的历史任务。候选清单来自 `GET /api/web/users/submitter-directory`（登录即可），返回可见项目内未停用账号出现过的所有 (owner, submitter) 组合。
- 筛选、排序、分组状态会写入 URL query，相关逻辑集中在 `stores/filters.ts`。
- **分组、排序、筛选**按登录账号存在服务端 `user_view_state`（`/api/web/me/view-state`）：网页端与「进入应用」的内嵌窗口是两套 WebView 存储，localStorage 互不可见，只有服务端那份能两端共用。前端在登录后对账（服务端有则以服务端为准，没有就把本机推上去），本地一改就防抖回推；带筛选参数的 URL（分享链接）不参与对账。「是否分享链接」只看**首次加载**时的 query —— 本地筛选随后会被回写进 URL，之后再判断就永远是"带参数的链接"。
- 筛选方案（命名视图）仍存浏览器 `localStorage`，不随账号同步。
- 应用内网页窗口（label=`web`）关闭时把位置与大小写进 `data/window-state.json`，下次按它打开。窗口几何只在**正常态**采集：最大化（含拖到屏幕顶部触发）与最小化期间的位置大小不可信，只更新「下次要不要最大化」；贴边吸附（半屏、四分之一）是正常态，如实保存。还原时会夹进当前显示器工作区，并用 `.visible(false)` 先摆好再显示。
- SSE 只负责通知“数据已变化”，前端收到后重新请求；断线期间由 10 秒轮询兜底。
- 手动排序使用数据库中的 `position`。切换到手动排序时会按当前排序基线重铺位置；分组视图只允许组内拖动。

前端新增 API 时，优先在 `src/grid-app/api/client.ts` 中统一封装错误处理，不要在组件中散落裸 `fetch`。

## 7. SQLite 迁移规则

数据库迁移入口在 `src-tauri/src/db/mod.rs`，当前数据目录可能包含历史版本数据库。

新增数据库字段或表时：

1. 增加 `USER_VERSION`。
2. 追加 `if version < N` 迁移，迁移需要可让旧库顺序升级。
3. 考虑从空数据库开始时会依次执行所有迁移。
4. 更新 Rust 查询映射、领域 DTO 和 TypeScript 类型。
5. 添加“旧库升级”和“新库初始化”路径的测试。

不要通过删除 `data/pm.db` 来规避迁移问题，也不要直接重写既有迁移使老用户数据库无法升级。

SQLite 连接启用了 WAL、外键和 5 秒 busy timeout。多步一致性操作应在事务中完成。

## 8. 数据目录与安全

数据目录规则：

- Debug：仓库根目录 `data/`
- Release：主程序所在目录的 `data/`
- 测试/无头模式可通过 `PM_DATA_DIR` 覆盖

关键文件：

- `data/pm.db`：数据库
- `data/attachments/`：附件
- `data/settings.json`：用户设置
- `data/runtime.json`：运行端口、PID、本机主机 Agent token；桌面应用实例还会在用户级配置目录（Windows `%APPDATA%\agents-pm-tool\runtime.json`）写一份，供装在任意 skill 目录下的 pm-cli 零配置发现本机服务。写入受 `CoreState::publish_runtime_pointer` 控制，**测试与无头脚本必须保持关闭**，否则会覆盖用户真实的连接信息。

`data/` 已被 Git 忽略。不要提交数据库、附件、token 或真实用户路径。测试应使用临时目录或 `PM_DATA_DIR` 隔离。

默认仅监听 `127.0.0.1`。`listen_scope=lan` 会绑定 `0.0.0.0`；局域网 Web 用户必须注册、登录并获得授权，但系统不提供互联网级 HTTPS、防爆破或外部身份认证。任何局域网相关改动都必须保留清晰的可信网络风险提示。

`data/` 的物理安全等同于最高权限：完整复制数据库到另一台机器后，那台机器的 loopback 主机账号会自动获得超级管理员身份。不要提交或分享数据库、`runtime.json`、用户 Agent token 或 `%APPDATA%\agents-pm-tool\cli.json`。

服务默认端口为 `17890`，占用时最多向后尝试 20 个端口。pm-cli 必须按「环境变量 > 用户配置 > 本机应用写出的运行信息」的顺序解析连接，从运行信息里取实际端口，不能假定始终是 17890。

## 9. 构建与生成文件

以下目录是生成物，不应手工编辑或提交：

- `node_modules/`
- `dist/`
- `src-tauri/target/`
- `src-tauri/release-bundle/`
- `data/`

`scripts/build-frontend.mjs` 使用输入指纹跳过无变化构建。

skill 的载荷（`pm-cli-skill/SKILL.md`、`bin/pm-cli.mjs`、`bin/pm-cli.cmd`、`bin/pm-cli`、`installer.mjs`）由 `src-tauri/src/server/api_skill.rs` 用 `include_str!` 编进二进制；改了这些文件必须重编 Rust，否则分发出的仍是旧内容。版本号来自 `pm-cli-skill/VERSION`，由 `build.py` 随其它版本文件一起同步（`api_skill.rs` 里有测试校验它与 `Cargo.toml` 一致）。`installer.mjs` 里的 `const PAYLOAD = [];` 是服务端替换载荷的锚点，改名要同步 `PAYLOAD_ANCHOR`，否则会发出空载荷脚本（有测试兜底）。

## 10. 测试定位

前端测试：

- `tests/filter-url.test.ts`：URL 筛选状态
- `tests/task-store.test.ts`：任务 Store、刷新与批量逻辑
- `tests/grid-pagination.test.ts`：分页 UI
- `tests/saved-views.test.ts`：筛选方案
- `tests/editor.test.ts`：描述编辑
- `tests/task-create.test.ts`：任务创建
- `tests/upload-queue.test.ts` / `tests/api-upload.test.ts`：附件队列与上传
- `tests/pm-cli.test.ts`：以子进程方式跑真实的 `pm-cli.mjs` 与 `installer.mjs`，覆盖参数解析、连接发现分层、退出码、附件下载与安装脚本的落点规则
- `tests/agent-access.test.ts`：Agent 访问面板（本机一键安装、远程选目录写入与路径提示）

Rust 测试：

- `src-tauri/tests/api.rs`：Web CRUD、会话、用户/角色/权限矩阵、Agent token、项目级联、附件清理、局域网监听、静态站点，以及**用真实 pm-cli 脚本**打通的环境变量连接与附件下载
- `src-tauri/tests/p2.rs`：分页、分组、锚点和批量操作
- 各 Rust 模块中的 `#[cfg(test)]`：领域规则、设置、排序与 ID 生成

修改 pm-cli 或 skill 相关行为时，前端测试（`tests/pm-cli.test.ts`）和 Rust 测试（`src-tauri/tests/api.rs`）都要跟着更新：前者验证 CLI 自身，后者验证服务端接口与 CLI 的配合。

修复缺陷时先补能复现问题的最小测试，再实现修复。测试断言应覆盖行为和权限边界，不要只断言 HTTP 成功。

## 11. 代码约定

- UI 文案和 API 错误面向中文用户，保持简洁、可操作。
- TypeScript 使用 strict 模式；避免 `any`，优先复用 `src/shared/types.ts`。
- Pinia 使用 setup store 风格。
- 共享视觉变量放在 `src/shared/theme.css`，通用组件样式放在 `src/shared/components.css`。
- 组件只负责交互和展示；数据请求集中在 API client，跨组件业务状态集中在 Store。
- Rust handler 负责输入解析与权限边界，数据库模块负责持久化与事务，领域模块负责稳定枚举和纯业务规则。
- 不要无故更改对外 JSON 字段名、CLI 输出格式、退出码或中文错误；Agent 可能依赖这些接口。
- 新增用户可见能力时，同步更新 `README.md`；重大设计变更同步更新相应 `docs/` 文档。

## 12. 完成任务前检查

提交结果前确认：

1. `git diff` 中没有覆盖用户已有改动，也没有生成物或运行数据。
2. 业务约束、Agent 权限和数据库迁移没有被意外放宽或破坏。
3. TypeScript DTO、Rust DTO、SQLite Schema/迁移和 API client 保持一致。
4. 已执行与变更风险相称的构建和测试，并如实报告结果。
5. README 或其他用户文档已在用户可见行为变化时同步更新。
6. 没有遗留由调试启动的 Vite、Cargo、Tauri 或无头服务进程。
