# 「完整Prompt」模板

本文档列出 `buildAgentTaskPrompt()` 实际生成的 Prompt 文本。内容由代码导出，与运行时行为逐字一致；
**修改 `taskActions.ts` 后需重新导出并同步本文档。**

- 生成入口：`src/grid-app/taskActions.ts::buildAgentTaskPrompt()`
- 示例任务固定为 `id = 202609101412350000`
- 触发入口：操作列的「完整Prompt」按钮（一句话版 [`buildAgentOneLinePrompt()`](../src/grid-app/taskActions.ts)
  走操作列左侧的「Prompt」按钮，不在本文档范围内）

> 模板只有 **一个**，变量只有服务地址。工具介绍、命令清单、权限边界都不在 Prompt 里展开——
> 已装 pm-cli-skill 的 Agent 会读 `SKILL.md`，未装的按给出的地址请求免 token 的 `/api/agent/help`。
> pm-cli 不在系统 PATH 上，所以模板固定带一行「先定位到它」的提示。

## 1. 输入

`buildAgentTaskPrompt(task, access)` 只用到一个可选字段：

| 字段 | 类型 | 来源 |
|---|---|---|
| `server_url` | `string \| undefined` | `GET /api/web/me/agent-access` 的 `server_url` |

`GridApp.vue:133` 把整个接入信息透传进来；取不到时回落到 `DEFAULT_ACCESS`（空对象）。

### 1.1 `server_url` 的来源（非硬编码）

`suggested_server_url()`（`api_agent_access.rs`）按以下顺序动态计算，**结果每次请求都可能不同**：

| 条件 | 取值 |
|---|---|
| 主机账号 | `http://127.0.0.1:<实际端口>`——Agent 与桌面应用同机，loopback 永远可达 |
| 非主机 + 监听范围不是 `lan` | `http://127.0.0.1:<实际端口>` |
| 非主机 + `lan` + 已配置 `agent_server_url` | 该配置值（去掉结尾 `/`），**显式覆盖优先于一切自动判断** |
| 非主机 + `lan` + 未配置 | 调用方**这次实际走通的那个地址**：局域网进来给局域网地址、Tailscale 进来给 Tailscale 地址、经域名/隧道进来给那个域名 |
| 请求里没有可用地址（畸形 Host、`0.0.0.0` 等） | 自动探测的局域网 IPv4 + 实际端口 |
| 探测失败 | 回落到 `http://127.0.0.1:<端口>` |

端口来自 `core.actual_port`，每次启动可能变化。

第四行是默认生效的那条。取地址的规则：

- 优先 `X-Forwarded-Host`（反代/隧道常把 Host 改写成 `localhost:端口`，原始域名在这个头里），
  其次 `Host`；两者都取首个逗号分隔值。
- 协议按 `X-Forwarded-Proto` 还原：隧道对外是 https、转发到本机是 http，
  不看这个头就会把 `https://域名` 报成 `http://域名`。
- **地址按调用方给的写法原样保留**：带端口就带端口，不带端口就不带
  （隧道对外的 443 本来就不写端口，拿本地端口去补只会得到一个连不上的地址）。
- 只接受能当地址用的 Host：IP 不能是 `0.0.0.0` / `::`，域名只允许字母数字与 `-` `_` `.`
  （挡掉 `/ ? # @ 空格`，它们会让拼出来的地址跑偏）。

> `agent_server_url` 填了就以它为准，留给「Agent 不在浏览页面那台机器上」这类自动识别
> 猜不到的情况；想让面板按访问方式给地址，把这一栏留空即可。

同一个 `server_url` 也用在 `/api/agent/help`（该接口免 token，Agent 拿到的就是它自己请求用的那个地址）
和「重新生成 token」的提示语里。

## 2. 场景覆盖

原来的 8 种组合全部收敛到同一个模板：

| 原场景 | 现在 | 说明 |
|---|---|---|
| 主机 · 装了 skill | 模板 A | 地址是 loopback；Agent 读 `SKILL.md` 得知 pm-cli 在 skill 的 `bin/` 下 |
| 主机 · 什么都没有 | 模板 A | 同上；无 pm-cli 时按地址请求 help |
| 远程 · 装了 skill | 模板 A | 地址是可达的 LAN 地址；需 `config set` |
| 远程 · 什么都没有 | 模板 A | 同上；无 pm-cli 时按地址请求 help |
| 未取到接入信息（未登录、请求失败、退出登录） | 模板 B | 无地址，Agent 会向用户索取 |

**主机与远程的模板逐字相同，只有地址不同。**

## 3. 模板

### 模板 A：取到接入信息（主机与远程共用，仅地址不同）

````text
请使用 Agents PM Tool（pm-cli-skill）完成以下任务。
pm-cli 在 pm-cli-skill 的 bin 目录下，先定位到它再执行（需要 Node.js 18 或更高版本）。
先执行：pm-cli doctor --json
获取任务内容命令：pm-cli get 202609101412350000 --json
服务地址：[http://<服务地址>](http://<服务地址>)
完整用法见 skill 目录里的 SKILL.md、pm-cli --help 或 [http://<服务地址>/api/agent/help](http://<服务地址>/api/agent/help)
````

`<服务地址>` 为占位符，运行时由 `server_url` 逐字替换，取值规则见 §1.1。

### 模板 B：未取到接入信息（`DEFAULT_ACCESS` 兜底）

````text
请使用 Agents PM Tool（pm-cli-skill）完成以下任务。
pm-cli 在 pm-cli-skill 的 bin 目录下，先定位到它再执行（需要 Node.js 18 或更高版本）。
先执行：pm-cli doctor --json
获取任务内容命令：pm-cli get 202609101412350000 --json
完整用法见 skill 目录里的 SKILL.md、pm-cli --help 或 GET /api/agent/help
````

触发时机（`GridApp.vue`）：首屏 `getAgentAccess()` 抛异常（未登录、未授权、请求失败）、
退出登录（`:179`）、以及接入信息返回前就点「完整Prompt」。

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
| `docs/pm-cli-service-discovery-plan.md` | 历史记录：早期服务发现与降级的决策过程 |
| `docs/pm-cli-cross-platform-plan.md` | pm-cli 跨平台化与 skill 安装方式改造的方案 |
