# AGENTS.md

本文件适用于整个仓库，供后续接手本项目的 Agent 快速建立上下文并安全地开展工作。

## 1. 项目速览

Agents PM Tool 是一个 Windows 本地项目管理工具，服务于“用户维护任务、AI Agent 受限推进任务”的协作场景。

运行时由三部分组成：

1. **Tauri 桌面应用**：显示设置窗口，管理内嵌 HTTP 服务、系统托盘和应用生命周期。
2. **浏览器任务工作台**：Vue 应用，由 Rust/Axum 服务托管，用户在这里管理项目、任务和附件。
3. **`pm-cli`**：供 Agent 使用的受限 HTTP 客户端，只调用 `/api/agent/*`，不直接访问数据库。

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
- 打包相关：先执行 `npm run build:all`，只有用户明确要求正式打包时才运行 `python build.py`。

`python build.py` 是 Windows 正式发布脚本，会同步多个版本文件、构建 NSIS 安装包，并询问是否将版本记录为已发布。它会改变发布状态，不是普通的只读构建命令，不要为了“验证一下”随意运行。

### 其他脚本

```powershell
npm run dev                       # 仅启动 Vite；主要用于 Tauri 设置页开发
npm run build                     # vue-tsc + Vite，多入口构建到 dist/
npm run build:cli                 # release 编译 pm-cli，并复制 target-triple sidecar
npm run build:all                 # 前端 + CLI
npm run preview                   # 预览 Vite 构建产物
```

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
│  ├─ src/paths.rs               # 数据目录解析
│  ├─ src/settings.rs            # 端口、访问范围、关闭行为
│  ├─ src/server/
│  │  ├─ mod.rs                  # Axum 路由、端口选择、runtime.json
│  │  ├─ api_web.rs              # 网页端完整 CRUD
│  │  ├─ api_agent.rs            # Agent 受限 API
│  │  ├─ api_batch.rs            # 批量更新/删除
│  │  ├─ auth.rs                 # Bearer token 校验
│  │  ├─ events.rs               # SSE 事件总线
│  │  └─ static_site.rs          # rust-embed 静态资源
│  ├─ src/db/
│  │  ├─ mod.rs                  # SQLite 打开、PRAGMA、迁移版本
│  │  ├─ schema.rs               # 初始 Schema 和种子项目
│  │  ├─ tasks.rs                # 任务 CRUD、筛选、排序、状态规则
│  │  ├─ task_page.rs            # 分页、分组计数和锚点定位
│  │  └─ projects.rs             # 项目 CRUD、重命名级联
│  ├─ src/domain/                # Task、ID、附件白名单等领域规则
│  ├─ src/cli/main.rs            # pm-cli
│  ├─ examples/serve.rs          # 无头服务
│  └─ tests/                     # Rust API/权限/分页集成测试
├─ tests/                         # Vitest 测试
├─ scripts/                       # 前端和 CLI 增量构建脚本
├─ docs/                          # 规划与验收资料
├─ dev.py                         # 交互式开发启动器
├─ build.py                       # 正式发布脚本
└─ version.json                   # 发布版本状态
```

## 5. 核心业务约束

修改业务逻辑时必须保留以下不变量，并同步更新 Rust 测试、前端类型和 README（如用户可见行为发生变化）。

### 任务字段与枚举

- 类型只有：`新增需求`、`优化`、`BUG`。
- 状态只有：`未开始`、`进行中`、`待验证`、`已完成`、`验收未通过`、`验收通过`。
- 提交人只有：`用户`、`Agent`。
- Web 创建任务时提交人固定为“用户”；Agent API 创建时固定为“Agent”。
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

### Agent 权限

`/api/agent/*` 是真正的权限边界，CLI 只是一层薄封装。不能只在 CLI 中做校验。

Agent 可以：

- 查看、筛选任务；
- 只读查看项目；
- 创建任务，但项目、类型和非空描述三项必填；
- 将任务切换到 `进行中`、`待验证`、`已完成`；
- 修改提交人为 `Agent` 的任务描述，且不能清空描述。

Agent 不可以：

- 切换到验收状态；
- 修改项目、类型或用户创建任务的描述；
- 删除任务；
- 管理项目或附件；
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

- Web API 前缀：`/api/web`，提供完整用户能力。
- Agent API 前缀：`/api/agent`，全部经过 Bearer token 中间件。
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
- 筛选、排序、分组状态会写入 URL query，相关逻辑集中在 `stores/filters.ts`。
- 筛选方案存储在浏览器 `localStorage`，不是 SQLite 中的共享视图。
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
- `data/runtime.json`：运行端口、PID、Agent token

`data/` 已被 Git 忽略。不要提交数据库、附件、token 或真实用户路径。测试应使用临时目录或 `PM_DATA_DIR` 隔离。

默认仅监听 `127.0.0.1`。`listen_scope=lan` 会绑定 `0.0.0.0`，而 Web API 本身没有账号鉴权；任何局域网相关改动都必须保留清晰的风险提示。Agent token 只保护 `/api/agent/*`，不要误写成对整个 Web 工作台的认证。

服务默认端口为 `17890`，占用时最多向后尝试 20 个端口。CLI 必须读取 `runtime.json` 获取实际端口，不能假定始终是 17890。

## 9. 构建与生成文件

以下目录是生成物，不应手工编辑或提交：

- `node_modules/`
- `dist/`
- `src-tauri/target/`
- `src-tauri/binaries/`
- `src-tauri/release-bundle/`
- `data/`

`scripts/build-frontend.mjs` 和 `scripts/build-cli.mjs` 使用输入指纹跳过无变化构建。若确实需要强制重建 CLI，可设置 `FORCE_CLI_BUILD=1`，但不要把该环境变量持久化到项目文件。

`npm run build:cli` 会把 `pm-cli` 复制成 Tauri target-triple 命名的 sidecar 文件。涉及安装包时，不要只看到 `src-tauri/binaries/` 中存在文件就断言已随安装器发布；还要核对 `src-tauri/tauri.conf.json` 的 bundle 配置和最终安装目录。

## 10. 测试定位

前端测试：

- `tests/filter-url.test.ts`：URL 筛选状态
- `tests/task-store.test.ts`：任务 Store、刷新与批量逻辑
- `tests/grid-pagination.test.ts`：分页 UI
- `tests/saved-views.test.ts`：筛选方案
- `tests/editor.test.ts`：描述编辑
- `tests/task-create.test.ts`：任务创建
- `tests/upload-queue.test.ts` / `tests/api-upload.test.ts`：附件队列与上传

Rust 测试：

- `src-tauri/tests/api.rs`：Web CRUD、Agent 权限、项目级联、附件清理、局域网监听和静态站点等集成路径
- `src-tauri/tests/p2.rs`：分页、分组、锚点和批量操作
- 各 Rust 模块中的 `#[cfg(test)]`：领域规则、设置、排序与 ID 生成

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
