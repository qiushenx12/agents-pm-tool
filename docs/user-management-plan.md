# 用户管理系统规划（局域网多用户 + 权限 + 远程 Agent）

来源任务：202609091818140006（用户管理系统）、202609091822120008（pm-cli-skill，已并入本规划 §6.3 与实施步骤 8）

## 0. 接手须知（给下一个 Agent）

- **当前进度**：T1–T12 已于 2026-09-10 实现并进入验证；§10 记录具体交付项和验收命令。
- **先读 `AGENTS.md`**：项目架构、命令、业务不变量以它为准。关键命令：Rust 测试 `cargo test --manifest-path src-tauri/Cargo.toml`；前端 `npm run build` + `npx vitest run`；无头联调 `PM_DATA_DIR=<临时目录> cargo run --example serve`（不要污染根目录 `data/`）。
- **本文件中的设计冲突时**：以本文件 §2/§3/§4 的规则为准（它比 AGENTS.md 新）；实现后由 T12 反向同步 AGENTS.md。
- **§9 的开放问题已全部给出默认决策**，直接按默认实现，不要停工等待确认；Paul 另有指示时以对话为准并回写本节。
- **已查证的兼容点**：Codex 当前用户级 skill 目录为 `~/.agents/skills/`，旧版 `~/.codex/skills/` 作为兼容探测；SKILL.md 使用官方要求的 `name`、`description` frontmatter。
- **不要**为了图省事把权限校验做在前端或 CLI 里——所有权限边界都在 Rust 服务端（这是本项目的核心架构原则）。

## 1. 背景与目标

实施前基线：

- `/api/web/*` **没有任何鉴权**，`listen_scope=lan` 时局域网内任何人都能完整操作所有数据；
- `/api/agent/*` 只有一个全局 Bearer token（写在 `data/runtime.json`），pm-cli 在**本机**读取该文件完成服务发现；
- 前端「复制 Prompt」（`src/grid-app/taskActions.ts::buildAgentTaskPrompt`）固定输出本机使用方式的指引。

目标：引入用户系统，让同一局域网内的其他用户先注册、经授权后访问网页工作台；支持按项目/字段/枚举选项粒度的权限控制；支持管理员体系；并让其他用户的 Agent 能通过网页获取属于自己的远程 pm-cli 使用方式。

非目标（本期不做）：

- 互联网级安全（HTTPS、防爆破、审计日志只做最简版本）；
- 多主机数据同步；
- 验收类权限仍只允许网页端用户，Agent 权限边界不因此扩大。

## 2. 角色模型

三种角色，存于 `users.role`：

| 角色 | 说明 |
| --- | --- |
| `super_admin` 超级管理员 | **不唯一**。包括一个内置「主机账号」和任意多个被提升的用户。任何超级管理员都可以提升/降级其他超级管理员（降级为管理员或普通用户），但**不能**降级/删除内置主机账号。 |
| `admin` 管理员 | 由超级管理员设置/取消。自动拥有所有项目的访问与操作权限，可管理普通用户的权限，**不能**查看/修改/删除超级管理员，也不能设置其他管理员。 |
| `user` 普通用户 | 默认无任何项目权限，由管理员或超级管理员授权后使用。 |

### 2.1 内置主机账号（核心规则）

- 数据库中存在一个**内置主机账号**（固定 id，如 `users.id = 'host'`，用户名默认「主机」可改），由服务端**每次启动时确保存在且 role 为 `super_admin`**（INSERT OR IGNORE + 强制纠偏 UPDATE）。
- 只有来自 **loopback（127.0.0.1）** 的请求可以以主机账号身份登录，该账号不设密码、不签发 agent token、不可被删除/停用/降级（服务端硬性拒绝）。
- **数据库迁移天然兼容**：把 `data/` 整个拷到另一台机器，新主机启动服务时同样以 loopback 自动获得主机账号的超级管理员身份，无需任何账号交接——这取代了原「转让超级管理员」的需求。
- 原主机上的其他命名超管账号仍然保留其角色（角色跟着数据库走），新主机上的主机账号可以对它们做降级/清理。

通用规则：

- 所有用户都可以修改**自己的名字**（保持唯一）；
- 超级管理员可以修改**其他用户**的名字、删除用户（主机账号除外）；
- 超级管理员可以提升普通用户/管理员为超管，也可以把其他超管降级为管理员或普通用户（操作需二次确认，且至少有主机账号兜底，不存在「最后一个超管被降级」的死锁问题）；
- 删除用户为软删除或级联清理其会话与 token，任务上的历史操作记录（如未来增加的操作人字段）保留用户名快照。

## 3. 注册与登录

1. 局域网用户首次打开网页 → 进入注册页，填写用户名 + 密码；
2. 注册成功后账号立即创建，但**默认没有任何项目权限**，看到「等待管理员授权」提示；
3. 管理员/超级管理员在用户管理面板中看到新用户并授权；
4. 登录采用会话机制：登录成功签发 session token（HttpOnly Cookie + `SameSite=Lax`），服务端保存会话表，支持过期（默认 7 天滑动过期）与注销；Cookie 方案在局域网下需防 CSRF——所有写接口（POST/PATCH/DELETE）要求携带自定义头（如 `X-PM-Client: web`），缺失则 403（前端 `api/client.ts` 统一附加，成本极低、能挡掉跨站表单/图片类攻击）；
5. **主机双入口**：从 loopback（`127.0.0.1`，含 Tauri 内嵌与主机浏览器）访问时，登录页提供两个入口——
   - **主机登录**：一键以「主机账号」（永远是超级管理员）进入，免密码，等价于现在的无感体验，作为默认入口；
   - **用户登录**：用用户名 + 密码登录某个命名账号（方便主机所有者以普通用户身份验证权限配置，或主机所有者同时维护一个个人账号）。
   两个入口都可随时切换（注销后重选）。局域网来源（非 loopback）只有「用户登录」，且永远拿不到主机账号身份——这是与当前「Web 无鉴权」行为的关键分界，需要在 README 和风险提示中写清楚；
6. 密码仅存哈希（argon2 或 bcrypt，依赖入 `src-tauri`），不明文、不出现在任何 API 响应中；主机账号无密码字段，任何针对主机账号的密码登录请求直接拒绝。

## 4. 权限模型

权限在**服务端每个 Web handler 入口强制校验**（与 Agent API 的设计原则一致：权限边界在服务端，前端只是隐藏入口）。

### 4.1 数据表（新增迁移，USER_VERSION +1）

```sql
CREATE TABLE users (
  id            TEXT PRIMARY KEY,          -- uuid；内置主机账号固定为 'host'
  username      TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL DEFAULT '',  -- 主机账号恒为 ''（无密码），密码登录一律拒绝
  role          TEXT NOT NULL DEFAULT 'user'
                CHECK (role IN ('super_admin','admin','user')),
  created_at    TEXT NOT NULL,
  disabled      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE sessions (
  token      TEXT PRIMARY KEY,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL
);

-- 普通用户的细粒度权限；admin/super_admin 不查此表（自动全量）
CREATE TABLE user_permissions (
  user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  project        TEXT NOT NULL,            -- 可访问项目
  field          TEXT NOT NULL,            -- 可操作字段：description/status/note/...
  allowed_values TEXT,                     -- 枚举字段允许勾选的选项，JSON 数组；NULL = 全部
  PRIMARY KEY (user_id, project, field)
);

-- agent token 改为按用户签发
CREATE TABLE agent_tokens (
  token      TEXT PRIMARY KEY,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL,
  revoked    INTEGER NOT NULL DEFAULT 0
);
```

主机账号落库：内置账号固定 `id='host'`，服务端启动时 `INSERT OR IGNORE` 并强制将其 role 纠偏为 `super_admin`（防止库里被手工改掉）。不再需要 `meta.super_admin_user_id`，也不存在「转让」动作。

### 4.2 校验规则

- **项目可见性**：`list/page/get` 任务、筛选、分组计数、附件、SSE 变更后的所有查询都按「用户可见项目」过滤；管理员/超级管理员 = 全部项目。
- **字段可操作**：`PATCH /api/web/tasks/{id}` 等写接口先加载该用户在该项目上的 `user_permissions`，请求中包含未授权字段 → 整单拒绝（403 + 中文错误，列出被拒绝字段），不做「部分应用」。
- **枚举选项**：`status`/`type` 等枚举字段额外检查目标值是否在 `allowed_values` 内。验收类状态（验收通过/验收未通过）依旧只允许网页端，但现在还要逐用户勾选「是否允许验收」。
- **项目管理、批量删除、用户管理**：默认仅管理员及以上；删除任务/附件权限也纳入字段级勾选项之外的独立开关（规划中作为 `field='task_delete'` 等伪字段表达，保持一张表）。
- 创建任务时提交人规则不变（Web = 用户，Agent = Agent）；若后续要区分「哪个用户/哪个 Agent」，新增 `owner_user_id` 可空列，不与现有 `submitter` 枚举冲突。

### 4.3 三处枚举对齐

新增 `role` 等枚举时同样保持 `src/shared/types.ts`、`domain/`（新增 `domain/user.rs`）、`db/schema.rs` CHECK 三处一致，遵循现有任务枚举的对齐规则。

## 5. 用户管理面板（前端）

在工作台增加「用户管理」入口（仅管理员及以上可见，入口显示不代表权限，服务端仍强制校验）：

- 用户列表：用户名、角色、注册时间、状态（待授权/正常/停用）；主机账号固定置顶并标注「主机」徽标，不提供删除/停用/降级按钮；
- 角色操作（仅超级管理员）：设为/取消管理员、**提升为超级管理员 / 将其他超管降级**（二次确认）、改名、删除用户；主机账号的所有危险操作在服务端同样硬性拒绝（前端隐藏只是体验层）；
- 权限编辑器（对普通用户）：
  - 项目勾选列表；
  - 每个项目下可勾选字段（描述、状态、备注、删除任务、上传/删除附件……）；
  - 枚举字段展开勾选允许的选项（如状态仅允许「进行中/待验证/已完成」，不允许「验收通过」）；
- 管理员条目展示「全部项目权限（自动）」，不可编辑；
- 我的 Agent 访问（所有用户可见，见 §6）。

前端路由/组件放在 `src/grid-app/components/` 下新增 `users/` 子目录，API 统一封装进 `src/grid-app/api/client.ts`，禁止组件内裸 `fetch`。

## 6. 其他用户的 Agent 如何通过网页使用 pm-cli

现状：pm-cli 只读本机 `data/runtime.json` 获取端口 + 全局 token，其他用户的电脑上没有这个文件，且全局 token 无法区分使用者。

关联任务：202609091822120008（pm-cli-skill，兼容 Claude Code / Codex）。

规划：

1. **按用户签发 agent token**（见 `agent_tokens` 表）。用户在网页「我的 Agent 访问」面板中可查看/重新生成/吊销自己的 token；
2. **pm-cli 支持远程模式**：
   - 新增环境变量 `PM_SERVER_URL`（如 `http://192.168.1.10:17890`）与 `PM_AGENT_TOKEN`；两者存在时跳过 `runtime.json` 发现逻辑，直接请求远程服务；
   - 也支持 `pm-cli config set server-url/token` 写入用户级配置文件（`%APPDATA%\agents-pm-tool\cli.json`），避免每次设环境变量；
   - 本机用户不传这些变量时行为完全不变（仍读 `runtime.json` 用主机 token）；
3. **pm-cli-skill 打包分发**（核心交付物，不再是裸 exe）：
   - Skill 包结构（zip）：`pm-cli-skill/SKILL.md`（用法说明、远程配置步骤、错误排查）+ `pm-cli-skill/bin/pm-cli.exe`；SKILL.md 中不写死 token，只指引设置环境变量；
   - 兼容 Claude Code（`~/.claude/skills/`）、Codex（`~/.agents/skills/`）与旧版 Codex（`~/.codex/skills/`）的 skill 目录约定，SKILL.md frontmatter 按通用字段编写；
   - **分发三条路径**：
     a. **网页手动下载**：「我的 Agent 访问」面板提供「下载 pm-cli-skill」按钮（`GET /api/web/skill/download`，zip 由服务端在构建/启动时打好），用户解压到自己前端的 skill 目录——这是用户在网页上手动操作的路径；
     b. **Agent 自助下载安装**：`GET /api/agent/skill/download`（Bearer token 鉴权），Agent 拿到任务 Prompt 后可自行下载 zip 并解压安装到当前前端的 skill 目录，全程无需人介入；`/api/agent/help` 的返回中给出该接口地址与安装说明；
     c. **主机自动检测安装**（仅主机，对应任务 202609091822120008）：服务端在本机检测 Claude Code / Codex 的 skill 目录是否存在，网页设置中提供「检测到 N 个前端，一键安装/更新 skill」；检测到本机前端且 skill 已就绪时，复制 Prompt 切换为短模式（见第 5 点）。远程用户的浏览器无法访问其本地文件系统，检测+自动安装只适用于主机；
   - Skill 包版本跟随服务端，下载天然版本配套；`/api/agent/skill/download` 响应头带版本号，Agent 可比对本地 skill 版本决定是否更新；
4. **服务端归属与权限**：`/api/agent/*` 中间件由「校验全局 token」改为「按 token 查用户」，请求上下文中携带 `user_id`。Agent 的可操作范围 = 该用户在 Web 端权限 ∩ Agent 固有权限（仍只能改状态到进行中/待验证/已完成、只能改自己创建任务的描述等，权限矩阵测试同步扩展）。零安装场景下 Agent 也可用 `curl`（Win10+ 自带）直连同一组接口，鉴权与权限校验走同一链路；
5. **复制 Prompt：维持现有单套模板微改，不做双模板**——`buildAgentTaskPrompt` 保持当前结构，仅两处变化：
   - 「工具介绍与访问方式」段落由服务端注入（`GET /api/web/me/agent-access` 返回当前用户的接入文案与 server_url，主机用户 = 现有文案，远程用户 = 远程配置文案），函数改为接收该参数，保持纯函数可测。**server_url 的生成**：服务端枚举本机非 loopback IPv4 网卡地址候选（`local-ip` 类 crate 或 std 实现），取第一个作为建议值，面板中允许用户手工修正后保存到 `settings.json`（`agent_server_url` 可空，空=自动探测），避免多网卡场景给错地址；
   - 命令清单下方固定追加一行自助发现入口：`完整用法见 pm-cli --help 或 GET /api/agent/help（需带 token）`；新增 `GET /api/agent/help` 返回 JSON 格式接口清单与示例；
   - **短模式**（任务 202609091822120008）：检测到目标机器 skill 已就绪时，Prompt 精简为两段：① 让 Agent 使用 pm-cli-skill 了解 pm-cli 用法；② 任务 ID + 需要执行的命令。短/长模式的切换由「当前用户的 skill 就绪状态」决定，不引入额外模板分支之外的文案维护负担。

## 7. 实施步骤（建议按序，每步独立可验收）

1. **数据层**：`domain/user.rs`、`users/sessions/agent_tokens/user_permissions` 表迁移（USER_VERSION +1），旧库升级与新库初始化测试；
2. **Web 鉴权中间件**：会话签发/校验/注销；loopback 双入口（主机登录一键进入 / 用户登录）；`require_web_auth` 挂载到 `/api/web`；现有集成测试补齐「未登录 401」与「非 loopback 请求主机登录被拒」；
3. **内置主机账号**：启动时确保 `id='host'` 账号存在且为超管；对主机账号的降级/删除/停用/密码登录请求一律拒绝（服务端测试覆盖）；
4. **权限校验层**：`db/permissions.rs` + handler 入口校验（项目可见性、字段、枚举选项），权限矩阵测试（对齐现有 `tests/api.rs` 的 Agent 权限矩阵写法）；
5. **用户管理 API + 面板**：`/api/web/users/*`（仅管理员以上），前端用户管理页；
6. **按用户 agent token**：`agent_tokens` 签发/吊销 API、`/api/agent` 中间件改造、Agent 权限 ∩ 用户权限；
7. **pm-cli 远程模式**：`PM_SERVER_URL`/`PM_AGENT_TOKEN`/`config set`，README 更新；
8. **pm-cli-skill 打包与分发**（任务 202609091822120008）：SKILL.md 编写（兼容 Claude Code / Codex）、zip 构建脚本、`GET /api/web/skill/download` + `GET /api/agent/skill/download`（带版本号响应头）、主机前端检测与一键安装/更新；
9. **Prompt 微改与自助发现**：`GET /api/web/me/agent-access` + `buildAgentTaskPrompt` 参数化、追加 `pm-cli --help` / `GET /api/agent/help` 自助入口、skill 就绪时的短模式切换 + 前端测试；
10. **收尾**：README「局域网访问」风险段落重写（从「无鉴权警告」改为「需注册授权」）、`AGENTS.md` §5/§8 同步、版本迁移说明。

## 8. 兼容与安全注意事项

- 现有 `data/` 中的单用户数据库升级后：自动创建内置主机账号（`id='host'`，用户名「主机」），主机使用习惯完全无感；原全局 agent token 作废并改为主机账号名下签发，pm-cli 本机路径无感；
- **数据库整体搬迁**：`data/` 拷到别的机器后，新主机通过 loopback 主机登录直接获得超管；原库里的命名超管账号角色保留，新主机可降级/清理；这意味着**拿到 `data/` 拷贝的人在其主机上即为超管**，`data/` 的物理安全即最高权限，需在 README 明示；
- `listen_scope=lan` 的警告文案保留，但内容从「Web API 没有鉴权」更新为「需注册并经管理员授权，密码请勿复用重要口令」；
- 会话与 token 均为随机 UUID/加密随机串，不落 Git；`data/` 继续在 `.gitignore`；
- 批量接口的逐项结果中，权限拒绝作为单项失败原因返回，不伪装全量成功（沿用现有约定）；
- 所有错误信息保持中文、简洁可操作。

## 9. 开放问题与默认决策（已拍板，按默认实现；Paul 可推翻并回写本节）

| # | 问题 | 默认决策 |
| --- | --- | --- |
| 1 | 注册是否需要管理员审批才能登录？ | **不需要**。注册即可登录，但默认零项目权限，界面提示「等待管理员授权」。`users.disabled` 仅用于事后停用。 |
| 2 | 主机账号的用户名允许修改吗？ | **允许**。id 固定 `'host'` 不变，改名不影响 loopback 主机登录。 |
| 3 | 同一用户允许多端同时登录吗？ | **允许**（sessions 表天然支持，暂不做单点登录限制）。 |
| 4 | 网页端是否显示「哪个用户的 Agent」？ | **v1 不显示**。服务端仅记录 `agent_tokens.user_id`，UI 后续版本再加。 |
| 5 | 命名超管数量设上限吗？ | **不设**。由主机账号兜底管理。 |
| 6 | 会话有效期？ | **7 天滑动过期**，注销即删 session 行。 |
| 7 | pm-cli 用户级配置文件位置？ | `%APPDATA%\agents-pm-tool\cli.json`（仅远程模式需要，本机不写）。 |

## 10. TODO（任务拆分）

> 顺序对齐 §7 实施步骤；每步验收标准写在子项末尾。状态标记：`[ ]` 未开始 / `[x]` 完成。

### 阶段 1：数据层与鉴权地基

- [x] **T1 用户数据表迁移**（2026-09-10）：新增 `users / sessions / agent_tokens / user_permissions` 四张表（`db/schema.rs`，USER_VERSION +1，追加式迁移）；`domain/user.rs` 定义角色枚举与校验规则；`src/shared/types.ts` 同步类型。验收：`cargo test` 覆盖旧库顺序升级与新库初始化两条路径。
- [x] **T2 内置主机账号**（2026-09-10）：服务启动时 `INSERT OR IGNORE id='host'` 并强制 role 纠偏为 `super_admin`；对主机账号的降级/删除/停用/密码登录请求一律 403。验收：集成测试覆盖四种拒绝场景 + 纠偏场景。
- [x] **T3 Web 会话鉴权**（2026-09-10）：`require_web_auth` 中间件挂载 `/api/web`；登录/注销/会话过期；密码 argon2/bcrypt 哈希；loopback 双入口（主机登录免密 / 用户登录），非 loopback 请求主机登录拒绝。验收：未登录 401、非 loopback 主机登录 403、会话过期 401 的集成测试。

### 阶段 2：权限体系

- [x] **T4 权限校验层**（2026-09-10）：`db/permissions.rs`；项目可见性过滤（列表/分页/详情/分组计数/附件/SSE）；写接口字段级校验（未授权字段整单 403 并列出字段名）；枚举字段 `allowed_values` 校验（含「是否允许验收」）。验收：权限矩阵测试，对齐 `tests/api.rs` 现有 Agent 矩阵写法。
- [x] **T5 用户管理 API**（2026-09-10）：`/api/web/users/*`（列表/改名/设角色/提升降级超管/删除/停用），仅管理员以上；管理员操作普通用户、超管操作一切的矩阵在服务端强制。验收：越权操作全部 403 的矩阵测试。
- [x] **T6 用户管理面板（前端）**（2026-09-10）：`components/users/` 用户列表 + 权限编辑器（项目勾选 → 字段勾选 → 枚举选项勾选）；主机账号置顶徽标、隐藏危险按钮；「我的 Agent 访问」面板（查看/重新生成/吊销 token）。验收：`npm run build` + Vitest 组件/Store 测试。

### 阶段 3：Agent 接入与 pm-cli-skill

- [x] **T7 按用户 agent token**（2026-09-10）：`agent_tokens` 签发/吊销 API；`/api/agent` 中间件改为按 token 查用户并注入 `user_id`；Agent 权限 = 用户权限 ∩ Agent 固有权限；`GET /api/agent/help` 自描述接口（含 skill 下载地址说明）。验收：token 归属、吊销即失效、越界项目/字段拒绝的集成测试。
- [x] **T8 pm-cli 远程模式**（2026-09-10）：`PM_SERVER_URL` / `PM_AGENT_TOKEN` 环境变量优先于 `runtime.json`；`pm-cli config set` 写 `%APPDATA%\agents-pm-tool\cli.json`；本机无变量时行为不变。验收：CLI 单测 + 对无头服务的远程联调用例。
- [x] **T9 pm-cli-skill 打包**（2026-09-10，任务 202609091822120008）：编写 `SKILL.md`（frontmatter 兼容 Claude Code / Codex，含远程配置与 curl 兜底说明）+ `bin/pm-cli.exe` 的 zip 构建脚本（纳入 `npm run build:cli` 链路）。验收：zip 解压到 `~/.claude/skills/`、`~/.agents/skills/`（以及旧版 `~/.codex/skills/`）均能被对应前端识别。
- [x] **T10 skill 分发接口**（2026-09-10）：`GET /api/web/skill/download`（面板按钮）与 `GET /api/agent/skill/download`（Bearer，响应头带版本号）；主机端 Claude Code / Codex skill 目录检测 + 一键安装/更新（服务端写本机文件系统）。验收：Agent 凭 token 可完成「下载 → 解压 → 安装」全流程的自动化测试。
- [x] **T11 复制 Prompt 微改**（2026-09-10）：`GET /api/web/me/agent-access` 下发当前用户接入文案；`buildAgentTaskPrompt` 参数化并保持单模板；固定追加 `pm-cli --help` / `GET /api/agent/help` 自助入口；skill 就绪时切换短模式（① pm-cli-skill 用法 ② 任务 ID + 命令）。验收：Vitest 覆盖主机长文案 / 远程长文案 / 短模式三种输出。

### 阶段 4：收尾

- [x] **T12 文档与兼容**（2026-09-10）：README 局域网章节重写（注册授权 + `data/` 物理安全警告）；`AGENTS.md` §5/§8 同步新枚举与新接口；旧库升级后主机账号与原全局 token 迁移说明。验收：`git diff --check` + 干净临时数据库集成测试走通注册→授权→Agent 接入全流程。

### 依赖关系

- T2 依赖 T1；T3 依赖 T1、T2；T4/T5 依赖 T3；T6 依赖 T5；T7 依赖 T4；T8/T9 可并行，均依赖 T7；T10 依赖 T9；T11 依赖 T7、T10；T12 最后。
- 可先行独立开发：T8（CLI 远程模式，可先对固定 token 开发）、T9（SKILL.md 与打包脚本不依赖后端接口）。

### 进度同步规则（给执行者）

- 每完成一项 TODO：勾选本节对应项（`[x]` + 日期），并在 commit message 中引用 TODO 编号（如 `T3: web session auth`）。
- PM 任务状态：阶段 1–2 完成前两个任务保持「进行中」；阶段 3 全部完成且自测通过后，将 202609091822120008 置「待验证」；T12 完成后将 202609091818140006 置「待验证」。验收状态只能由网页端用户操作，Agent 不要自行设置。
- 每个阶段结束必须全绿：`cargo test --manifest-path src-tauri/Cargo.toml` + `npm run build` + `npx vitest run`。
