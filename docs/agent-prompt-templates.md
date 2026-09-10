# 「复制 Prompt」模板

本文档列出 `buildAgentTaskPrompt()` 实际生成的 Prompt 文本。内容由代码导出，与运行时行为逐字一致；
**修改 `taskActions.ts` 后需重新导出并同步本文档。**

- 生成入口：`src/grid-app/taskActions.ts::buildAgentTaskPrompt()`
- 示例任务固定为 `id = 202609101412350000`、`project = agents-pm-tool`

> 模板只有 **一个**，变量只有服务地址。工具介绍、命令清单、权限边界都不在 Prompt 里展开——
> 已装 pm-cli-skill 的 Agent 会读 `SKILL.md`，未装的按给出的地址请求免 token 的 `/api/agent/help`。

## 1. 输入

`buildAgentTaskPrompt(task, access)` 只用到一个可选字段：

| 字段 | 类型 | 来源 |
|---|---|---|
| `server_url` | `string \| undefined` | `GET /api/web/me/agent-access` 的 `server_url` |

`GridApp.vue:133` 把整个接入信息透传进来；取不到时回落到 `DEFAULT_ACCESS`（空对象）。

### 1.1 `server_url` 的来源（非硬编码）

`server_url()`（`api_agent_access.rs:47`）按以下顺序动态计算，**结果每次请求都可能不同**：

| 条件 | 取值 |
|---|---|
| 主机账号 | `http://127.0.0.1:<实际端口>`——Agent 与桌面应用同机，loopback 永远可达 |
| 非主机 + 监听范围不是 `lan` | `http://127.0.0.1:<实际端口>` |
| 非主机 + `lan` + 已配置 `agent_server_url` | 该配置值（去掉结尾 `/`） |
| 非主机 + `lan` + 未配置 | 自动探测的局域网 IPv4 + 实际端口 |
| 探测失败 | 回落到 `http://127.0.0.1:<端口>` |

端口来自 `core.actual_port`，每次启动可能变化。

> **若局域网自动探测取到错误的网卡**（虚拟机、VPN、多网卡），远程用户会拿到不可达地址。
> 解决办法是在设置里显式填 `agent_server_url`，它会优先于探测结果。

## 2. 场景覆盖

原来的 8 种组合全部收敛到同一个模板：

| 原场景 | 现在 | 说明 |
|---|---|---|
| 主机 · 应用侧 exe 可用 | 模板 A | 地址是 loopback |
| 主机 · 仅有 skill | 模板 A | 同上；Agent 读 `SKILL.md` 得知用 `bin/pm-cli.exe` 完整路径 |
| 主机 · 什么都没有 | 模板 A | 同上；无 pm-cli 时按地址请求 help |
| 远程 · 装了 skill | 模板 A | 地址是可达的 LAN 地址；需 `config set` |
| 远程 · 什么都没有 | 模板 A | 同上；无 pm-cli 时按地址请求 help |
| 未取到接入信息（未登录、请求失败、退出登录） | 模板 B | 无地址，Agent 会向用户索取 |

**主机与远程的模板逐字相同，只有地址不同。**

## 3. 模板

### 模板 A：取到接入信息（主机与远程共用，仅地址不同）

````text
请使用 Agents PM Tool（pm-cli-skill）完成以下任务：
- 任务 ID："202609101412350000"
- 项目："agents-pm-tool"
命令：pm-cli get 202609101412350000 --json
服务地址：http://<服务地址>
完整用法见 pm-cli --help 或 http://<服务地址>/api/agent/help
````

`<服务地址>` 为占位符，运行时由 `server_url` 逐字替换，取值规则见 §1.1。

### 模板 B：未取到接入信息（`DEFAULT_ACCESS` 兜底）

````text
请使用 Agents PM Tool（pm-cli-skill）完成以下任务：
- 任务 ID："202609101412350000"
- 项目："agents-pm-tool"
命令：pm-cli get 202609101412350000 --json
完整用法见 pm-cli --help 或 GET /api/agent/help
````

触发时机（`GridApp.vue`）：首屏 `getAgentAccess()` 抛异常（未登录、未授权、请求失败）、
退出登录（`:179`）、以及接入信息返回前就点「复制 Prompt」。

## 4. 被移出 Prompt 的内容去哪了

| 内容 | 承接方 |
|---|---|
| 工具介绍 | `/api/agent/help` 的 `introduction` |
| 访问与配置方式 | `/api/agent/help` 的 `bootstrap`（`ask_the_user` / `configure` / `if_pm_cli_missing`）与 `configuration` |
| 接口清单 | `/api/agent/help` 的 `commands`（REST 路径） |
| pm-cli 子命令清单 | `SKILL.md` §命令 |
| Agent 权限边界 | `/api/agent/help` 的 `permissions`，以及 `SKILL.md` §命令 末尾 |
| 排障 | `SKILL.md` §排障、`pm-cli doctor` |

`GET /api/agent/help` **无需 token**，且响应里带 `server_url`，供 Agent 直接 `config set`。

## 5. 相关文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/server/api_agent_access.rs` | 计算 `server_url` 与 `access_instructions`（后者仅供面板展示） |
| `src-tauri/src/server/api_agent.rs` | 免 token 的 `/api/agent/help`，承载工具介绍与权限边界 |
| `src/grid-app/taskActions.ts` | 生成 Prompt |
| `src/grid-app/GridApp.vue` | 拉取接入信息；失败时回落到 `DEFAULT_ACCESS` |
| `pm-cli-skill/SKILL.md` | 已装 skill 时 Agent 的完整用法来源 |
| `docs/pm-cli-service-discovery-plan.md` | §10 记录三档降级的决策过程（已被本文档取代） |
