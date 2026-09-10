# pm-cli 服务发现修复方案（v2）

## 0. 修订说明

v1 把「已安装应用的本机用户」当成了主线，因此引入了两类改动：**安装侧写入指针文件**、`%LOCALAPPDATA%` **平台硬编码**。

这是方向性错误。目标使用者是从「我的 Agent 访问」面板**下载 skill ZIP** 的用户——他们不安装 Agents PM Tool，只使用 skill 目录下的 exe。让这类用户受益于「安装时的写入动作」在逻辑上不成立，也让 exe 变得不自包含。

v2 删除全部安装侧耦合，回到唯一约束：**skill 目录下的 exe 自身完成所有事情。**

### 已定决策：data 目录走「路 1」

关于「ZIP 不附带 `data/`，远程用户如何配置」，三条候选已定选 **路 1**：

| | 结论 |
|---|---|
| 路 1：skill 目录里**永不出现** `data/`，远程用户走 `config set` / 环境变量 | **采纳** |
| 路 2：ZIP 附带 `data/` 或 `runtime.json` 模板，用户手填 | 否决 |
| 路 3：服务端生成已填好的 `runtime.json` 下载或打进 ZIP | 否决 |

否决路 2/3 的理由：

- `data/runtime.json` 由 `write_runtime_json` 在**服务启动时**写入（`mod.rs:215`），内容是 `{port, token, pid}`，语义是"应用此刻在哪个端口、以哪个身份运行"，属**运行时状态**而非用户配置；让它被手工填写会稀释语义。
- 路 2 要求用户手写 JSON，且空目录在各解压工具下行为不一致，"模板"与"真实"两个 runtime.json 也会语义打架。
- 路 3 会让 token 明文进压缩包，并使 ZIP 从"通用包"变成"绑定某个用户的包"，且与 `config set` 能力重复。

**由此确定 P0-2 的方向是"让 `data/` 在 skill 目录永不出现"，而不是"打包时创建它"。**

配套增强（低成本，抹掉路 1 唯一的"要两条命令"缺点）：网页端在「我的 Agent 访问」面板给出**一条可复制的完整命令**。技术上不需要新接口——`web_download` 的签名里已有 `Extension(_user)`，只是当前被丢弃。归入 P1-2 一并处理。

## 1. 实测结论：exe 自身已具备完整能力

在纯净临时目录（只有 `bin/pm-cli.exe`，无任何 `data/`、无应用痕迹）中实测：

| 配置方式 | 命令 | 结果 |
|---|---|---|
| 无配置 | `pm-cli get <ID> --json` | 退出码 3，报「未找到 data/runtime.json。请检查 Agents PM Tool 是否运行…」 |
| 环境变量 | `PM_SERVER_URL` + `PM_AGENT_TOKEN` | 退出码 0，正常返回任务 |
| `config set` | `pm-cli config set server-url/token` | 退出码 0，正常返回任务 |

**结论**：远程/无应用场景所需的全部能力，exe 已经具备，且**不需要安装任何东西、不需要创建任何目录、不需要安装器配合**。缺口不在"发现不了服务"，而在"配错了不知道、没配不知道该干什么"。

## 2. 真正需要修的问题

| # | 问题 | 性质 | 影响 |
|---|---|---|---|
| 1 | 未配置时报错为「请检查 Agents PM Tool 是否运行」 | 文案误导 | 用户根本没装应用，被引导去排查一个不存在的东西 |
| 2 | 读取连接信息时会 `create_dir_all` 并写 `.write_probe` | 副作用缺陷 | 用户 skill 目录里凭空多出空的 `bin/data/` |
| 3 | `pm-cli-skill/SKILL.md` 第 14 行称"本机安装版会自动从应用的 data/runtime.json 读取" | 文档错误 | 对 skill 目录下的 exe 不成立 |
| 4 | 配完之后没有自检手段，只能等业务命令失败 | 体验缺失 | 排障成本高，Agent 尤其容易卡住 |

问题 1、2 直接损害"下载 ZIP 即用"的体验，是本次主线。

## 3. 修复项

### P0-1 报错分场景，并给出可执行的下一步（已实施）

**位置**：`src-tauri/src/cli/main.rs:212` 起、`service_down()`（`main.rs:219`）

现状是所有失败共用一个文案。改为按实际状态区分：

| 判定条件 | 文案要点 |
|---|---|
| 环境变量/配置**均未设置**，且本机未发现应用痕迹 | 未配置连接 → 直接给出 `pm-cli config set server-url <URL>` 与 `pm-cli config set token <token>` 两条命令；并说明 token 在网页「我的 Agent 访问」面板签发 |
| 已配置但连不上 | 无法连接 `<server_url>` → 检查服务是否运行、监听范围是否为局域网、防火墙 |
| 已配置但返回 401 | token 无效或已吊销 → 重新签发 |
| 已配置但返回 403 | 无该项目/字段授权 → 检查网页端授权 |

要求：

- 任何情况下**不打印 token 本身**。
- 退出码保持 `3`（`EXIT_SERVICE_DOWN`）不变。
- 文案面向中文用户、简洁可操作（AGENTS.md §11）。

### P0-2 只读探测，消除建目录副作用（已实施）

**位置**：`src-tauri/src/paths.rs:46` `data_dir_lossy()`

它原本调用 `data_dir()`，后者执行 `create_dir_all` 并写 `.write_probe`。pm-cli 只做读取，不应产生任何写入。

改法（已落地）：`data_dir_lossy()` 改为**纯路径解析**，保留原有规则（`PM_DATA_DIR` 覆盖、debug 上溯 4 级、release 取 exe 同级），但不再创建目录、不再写探测文件。

```rust
pub fn data_dir_lossy() -> PathBuf {
    if let Ok(custom) = std::env::var("PM_DATA_DIR") {
        return PathBuf::from(custom);
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    if cfg!(debug_assertions) {
        // dev：exe 位于 src-tauri/target/debug/，上溯 4 级 = 项目根
        exe.parent().and_then(|p| p.parent())
            .and_then(|p| p.parent()).and_then(|p| p.parent())
            .map(|p| p.join("data"))
            .unwrap_or_else(|| PathBuf::from("data"))
    } else {
        exe.parent().map(|p| p.join("data"))
            .unwrap_or_else(|| PathBuf::from("data"))
    }
}
```

验收（已实测）：纯净目录下执行任意命令，目录树不产生任何新条目。

### P1-1 新增 `pm-cli doctor`（已实施）

让用户和 Agent 能一次性确认配置是否正确。实测输出（本机，已装应用）：

```
pm-cli doctor
pm-cli 诊断
  版本：1.0.0
  可执行文件：C:\Users\30919\.workbuddy\skills\pm-cli\bin\pm-cli.exe
  连接来源：本机应用 data/runtime.json
  服务地址：http://127.0.0.1:3010
  Token：c88d****c494
  连通性：正常（HTTP 200，可见 1 个项目）
  skill 版本：1.0.0（与 pm-cli 一致）
  结论：连接正常。
```

实现要点：

- 新增 `resolve_connection_detailed()`，在原 `resolve_connection()` 之外返回**来源**（`Environment` / `Config` / `Runtime`）；原函数降级为薄封装，既有测试零改动。
- `Client::call()` 拆出 `send()`：只返回结果、不 `eprintln`，避免 doctor 打印两份错误。
- `--json` 输出 `DoctorReport` 结构体，供 Agent 解析；人读走 `print_doctor()`。
- 探测用 `GET /api/agent/help`（轻量且验权），成功后补一次 `GET /api/agent/projects` 统计可见项目数；为 0 时提示检查授权。
- 版本号比对：skill 布局 `<skill>/bin/pm-cli.exe` + `<skill>/VERSION`，版本不一致时提示重新安装。
- 退出码与其它命令一致：`0` 正常、`2` 鉴权失败（401/403）、`3` 未配置或连不上。诊断报告无论如何都会完整打印。

实测四个场景：纯净目录（退出码 3，无副作用）、exe 同级 `data/runtime.json`（退出码 0）、skill 版本不一致（附加提示但退出码 0）、错误 token（HTTP 401，退出码 2）。

### P1-2 修正 SKILL.md

**位置**：`pm-cli-skill/SKILL.md:14`

现行表述「本机安装版会自动从应用的 `data/runtime.json` 读取实际端口与主机临时 token」对下载 ZIP 的用户不成立。改为按来源分述：

- **从面板下载 ZIP / 手动部署**：必须配置 `PM_SERVER_URL` + `PM_AGENT_TOKEN`，或执行 `pm-cli config set`；这是主要使用方式。
- **应用一键安装到本机前端**：exe 同级存在 `data/runtime.json` 时自动读取。

并补充 WorkBuddy 安装目录 `$HOME/.workbuddy/skills/pm-cli`。

### P1-3 README 同步（已实施）

`README.md` 第 120 行附近补充"不安装应用如何使用 pm-cli"一节。

## 4. 明确不做

以下均因**与"只使用 skill 下的 exe"这一前提冲突**而移出方案：

| 移除项 | 原因 |
|---|---|
| 安装时写入 `pm-cli-home.json` 指针文件 | 依赖安装侧动作；下载 ZIP 的用户永远拿不到该文件 |
| `%LOCALAPPDATA%\Agents PM Tool\data` 硬编码候选 | 只服务已装应用的本机用户；与安装方式和平台耦合 |
| 对 `install_into_roots` 的任何改动 | 同上，本次不动安装侧 |
| `PM_PM_DATA_DIR` 环境变量 | 为排障本机路径而设，主线场景用不到，且与既有 `PM_DATA_DIR` 易混淆 |

## 5. 待确认：本机已装应用时，skill 内 exe 是否自动发现

> **状态：待定，不阻塞 P0/P1 的落地。** P0-1、P0-2 已于 2026-09-10 实施完成并通过实测。

这是一个**独立于主线**的问题，v1 错误地把它当成了主线。它影响的是「本机装了应用、同时又用前端 Agent 调用 skill 内 exe」的用户——例如本机 WorkBuddy 中的 Agent。

难点：自动发现要求 exe 知道应用在哪个目录，而应用位置不是 exe 自带的；且 `runtime.json` 中的端口与 token 会变，必须实时读取，不能固化。

| 选项 | 做法 | 优点 | 缺点 |
|---|---|---|---|
| A | **不处理** | 方案最干净，完全符合"exe 自包含" | 本机的前端 Agent 需手动配置，且 token 重新生成后配置失效 |
| B | 本机用户改用 PATH 中的 `pm-cli`（应用安装时已注册 PATH） | 零改动，那份 exe 同级就有 `data/` | 前端调用的是 skill 内 exe，不走 PATH，需另行约定 |
| C | exe 自省：运行时在 PATH 中查找 `pm-cli`，命中则复用其同级的 `data/runtime.json` | exe 自身完成，不碰安装侧、不要指针文件、不硬编码平台路径 | 仍依赖"应用已注册 PATH"这一约定；远程机器上无效（但远程本就该用配置） |

**需用户选择**：若选 C，它是纯 exe 侧改动，与本次主线（P0-1/P0-2）无耦合，可作为独立任务排期。

## 6. 验证计划

单元测试（`src-tauri/src/cli/main.rs` 的 `#[cfg(test)]`）：

1. 未配置时，`resolve_connection` 返回"未配置"而非"未找到 runtime.json"。
2. `config set` 只写其中一项时，报错指向补齐命令。
3. `cli_runtime_path()` 解析后不产生任何目录或文件。
4. 401 / 403 分别映射到对应文案。

集成实测（纯净临时目录，仅有 exe）：

| 场景 | 验证动作 | 期望 |
|---|---|---|
| 未配置 | `get` | 退出码 3，文案指向 `config set`，且**未创建** `data/` |
| 环境变量 | `get` | 退出码 0 |
| `config set` | `get` | 退出码 0 |
| 错误 token | `get` | 401 文案 |
| `doctor` | — | 输出配置来源与连通性，token 仅前 4 位 |

Rust 侧执行 `cargo test --manifest-path src-tauri/Cargo.toml`；涉及前后端 DTO 时同步执行 `npm run build` + `npm test`。

## 7. 落地拆分

| 步骤 | 内容 | 涉及文件 |
|---|---|---|
| 1 | P0-2 只读探测（含副作用单测） | `src-tauri/src/paths.rs`、`src-tauri/src/cli/main.rs` |
| 2 | P0-1 分级报错（含四类文案与单测） | `src-tauri/src/cli/main.rs` |
| 3 | P1-1 `doctor` | `src-tauri/src/cli/main.rs` |
| 4 | P1-2 / P1-3 文档 | `pm-cli-skill/SKILL.md`、`README.md` |

第 1、2 步为必需；第 3、4 步可独立排期。第 5 节的选项 C 若采纳，另立任务。

## 8. 兼容性要求

- 不改既有 JSON 字段名、CLI 输出格式、退出码（AGENTS.md §11）。
- 环境变量与用户配置的优先级保持不变。
- 不在任何日志、报错、`doctor` 输出中完整打印 token。

## 9. 遗留清理

此前为绕过该问题，在本机手工创建了 junction
`~/.workbuddy/skills/pm-cli/bin/data` → 应用 `data` 目录。

第 10 节落地后，该联接已不在关键路径上：Prompt 现在优先指示 Agent 使用应用侧 `pm-cli`（档 1），
它本身就与 `runtime.json` 同级，不需要任何联接。联接仍可保留，仅用于「Agent 自行找到 skill 目录
并直接调用其中 exe」这种非典型路径；若要彻底收敛，可直接删除，不影响任何推荐用法。

## 10. 「复制 Prompt」的三档降级（已废弃）

> **本节记录的过程已被推翻，实现见 `docs/agent-prompt-templates.md`。**
>
> 三档降级按「用哪个 exe」分支，产出 8 个模板，Prompt 里重复了一遍 SKILL.md 已有的命令清单，
> 过重。最终改为**单一模板**：Prompt 只给任务 ID、项目、命令、服务地址与 help 地址，
> 工具介绍／访问方式／接口清单／权限边界全部交给 `/api/agent/help`（免 token）与 `SKILL.md`。
>
> 随之删除：`AgentCli`、`resolve_cli()`、`resolve_skill_state()`、`app_cli_path()`、
> `installed_skill_cli_path()`、`local_skill_state()`、`skill_state_from()`。
> 保留 `find_sidecar()`——打包 skill ZIP 时仍要用到它。

以下是被废弃的设计，留作记录。

### 已定决策

1. **档 1 优先于档 2**。对主机账号，档 1 几乎恒真（应用目录里就有 sidecar）。理由：应用侧 pm-cli
   自动读 `runtime.json`，端口漂移与重新生成 token 都能跟上；而 skill 目录按路 1 永不附带 `data/`，
   必须 `config set`，token 重生成后会静默失效。
2. **档 1 推荐不带路径的 `pm-cli`，同时给出绝对路径兜底**（`fallback`）。PATH 由 NSIS 注册，
   装完要新进程才生效，而 Agent 进程可能早于安装启动，所以必须能用绝对路径救场。
   服务端用 `find_sidecar()` 拿绝对路径，不扫 PATH。
3. **`skill_ready` 拆成两个正交字段**：`cli`（用哪个 exe）决定长/短提示，
   `skill_state`（装没装好）只用来附加提示，不再决定分支。

### 数据结构

```ts
interface AgentCli {
  kind: "app" | "skill" | "none";  // 应用侧 / 仅 skill 内有 exe / 都不可用
  command: string | null;          // app: "pm-cli"；skill: 绝对路径；none: null
  fallback: string | null;         // app: 应用侧 exe 绝对路径
  needs_config: boolean;
}
interface AgentAccess {
  server_url: string;
  token: string | null;
  access_instructions: string;
  cli: AgentCli;
  skill_state: "ready" | "outdated" | "missing" | "unknown";
}
```

### 判定与分支

| 场景 | `cli.kind` | `skill_state` | 给的提示 |
|---|---|---|---|
| 主机 + 应用侧 exe | `app` | `ready` | 短，首行引导读 SKILL.md |
| 主机 + 应用侧 exe | `app` | `missing` | 短（**修复了原先误给长提示的 bug**） |
| 主机 + 应用侧 exe | `app` | `outdated` | 短 + 建议更新 skill |
| 主机但无应用侧 exe | `skill` | — | 短，先交代完整路径 + 提示需 `config set` |
| 远程账号 | `none` | `unknown` | 长提示（沿用） |

**档 2 只覆盖主机且应用侧 exe 缺失的情况。** 远程用户机器上有没有 skill，服务端无法感知
（`local_skill_targets()` 读的是服务器本机的 home 目录），因此远程账号一律 `none`。
远程用户「装了 skill」的场景由长提示中的 `access_instructions`（`config set` 指引）覆盖，
「什么都没有」的场景由 `/api/agent/help` 免 token 引导覆盖。

### 改动文件

- `src-tauri/src/server/api_skill.rs`：新增 `app_cli_path()`、`installed_skill_cli_path()`、
  `local_skill_state()`（纯函数 `skill_state_from()` 可测）；删除已被取代的 `local_skill_ready()`。
- `src-tauri/src/server/api_agent_access.rs`：新增 `AgentCli`、`resolve_cli()`、
  `resolve_skill_state()`；`AgentAccess` 用 `cli` + `skill_state` 取代 `skill_ready`。
- `src/shared/types.ts`、`src/grid-app/taskActions.ts`：三分支替换二分；
  路径不经 `JSON.stringify`（否则 Windows 反斜杠会变成双反斜杠），改用 `quotePath()`。
- 测试：新增 4 条前端用例（主机无 skill 仍走短提示、skill 过期、档 2 绝对路径、档 3 长提示）
  与 1 条 Rust 用例（`skill_state_from` 四态）；8 处旧 `skill_ready` fixture 同步替换。

`GridApp.vue` 无需改动——它把整个 `AgentAccess` 透传给 `setAgentPromptAccess()`，
新字段类型兼容。

**全部场景的完整 Prompt 模板见 `docs/agent-prompt-templates.md`**（8 个组合，由代码导出，
逐字一致）。修改 `taskActions.ts` 后需同步重新导出该文档。
