---
name: pm-cli
description: 使用 Agents PM Tool 的受限 pm-cli 查看、创建和推进项目任务；当用户提供 Agents PM Tool 任务 ID、要求通过 pm-cli 工作或需要远程连接该工具时使用。
---

# Agents PM Tool pm-cli

用本 skill 里的 `pm-cli` 访问 Agents PM Tool 读取和推进任务。不要直接读取或修改它的数据库：权限边界由服务端 `/api/agent/*` 强制执行，`pm-cli` 只是一个薄客户端。

## 第一步：先找到 pm-cli

`pm-cli` 就在本 skill 目录里，**不在系统 PATH 上**，所以先确认它在哪里、再调用。目录结构固定为：

```text
<前端 skills 根目录>/
└─ pm-cli/
   ├─ SKILL.md          本文件
   ├─ VERSION           版本号
   └─ bin/
      ├─ pm-cli.mjs     实现本体
      ├─ pm-cli.cmd     Windows 便捷入口
      └─ pm-cli         macOS / Linux 便捷入口
```

各前端的 skills 根目录（Windows / macOS 两种写法）：

| 前端 | Windows | macOS |
| --- | --- | --- |
| Codex | `%USERPROFILE%\.agents\skills` | `~/.agents/skills` |
| Codex（旧版目录） | `%USERPROFILE%\.codex\skills` | `~/.codex/skills` |
| Claude Code | `%USERPROFILE%\.claude\skills` | `~/.claude/skills` |
| WorkBuddy | `%USERPROFILE%\.workbuddy\skills` | `~/.workbuddy/skills` |
| OpenCode | `%USERPROFILE%\.config\opencode\skills` | `~/.config/opencode/skills` |
| Cursor | `%USERPROFILE%\.cursor\skills` | `~/.cursor/skills` |
| Pi | `%USERPROFILE%\.pi\agent\skills` | `~/.pi/agent/skills` |
| DeepSeek Harness | `%USERPROFILE%\.dsh\skills` | `~/.dsh/skills` |

DeepSeek Harness 设置了 `DSH_HOME` 时，用 `$DSH_HOME/skills`。

确认位置后，用完整路径调用即可（下面示例里的 `pm-cli` 都指这一种调用）：

```text
Windows：  <skill>\bin\pm-cli.cmd doctor
macOS：    <skill>/bin/pm-cli doctor      或  node <skill>/bin/pm-cli.mjs doctor
```

`pm-cli` 需要 **Node.js 18 或更高版本**。装上后如果 Agent 前端没识别到 skill，重新启动它。

## 连接

连接信息只有两项：**服务地址**和 **Agent token**。token 由用户在网页「我的 Agent 访问」面板签发，与该登录账号绑定。

**Agent 运行在装了 Agents PM Tool 的那台电脑上**：不需要任何配置。应用会把端口与 token 写到你电脑上固定的一处位置，`pm-cli` 自动读取。如果报「尚未配置连接」，先确认桌面应用正在运行。

**Agent 运行在其它电脑上（远程）**：需要配置一次，任选一种。

环境变量（适合 CI 或临时使用；两者必须同时设置，只设其中一个会直接报错，不会回落到用户配置）：

```text
PM_SERVER_URL=http://192.168.1.10:17890
PM_AGENT_TOKEN=<网页中签发的 token>
```

或保存到用户配置（推荐，长期有效）：

```text
pm-cli config set server-url http://192.168.1.10:17890
pm-cli config set token <网页中签发的 token>
pm-cli config show
```

配置写在用户级配置文件里：Windows 是 `%APPDATA%\agents-pm-tool\cli.json`，macOS 是 `~/Library/Application Support/agents-pm-tool/cli.json`。

**优先级**：环境变量 > 用户配置 > 本机应用写出的运行信息。也就是说，手工配置过一次之后，这份配置会一直优先于本机自动发现；如果应用换了端口而旧配置还指向老端口，`pm-cli doctor` 会点明这个不一致，按提示更新或删掉配置文件即可。

### token 安全

- token 等同身份：`pm-cli` 完全以 token 所属账号的身份操作，权限也按该账号的授权。
- 不要把 token 写进仓库、任务描述、日志或本文件，也不要转给其他人使用。
- 重新生成 token 会让旧 token 立即失效，需同步更新配置。

## 标准流程

1. 运行 `pm-cli get <任务ID> --json` 读取完整任务。
2. 若 `attachment_count` 大于 0，运行 `pm-cli attachments <任务ID> --json` 查看附件，再用 `pm-cli download <附件ID>` 按需下载。
3. 运行 `pm-cli projects --json` 获取项目、本地路径与 Git 地址。
4. 开始实现时运行 `pm-cli status <任务ID> --to 进行中 --json`。
5. 在任务对应仓库内实现并验证；保留用户已有改动。
6. 完成且自测通过后运行 `pm-cli status <任务ID> --to 待验证 --json`。

## 命令

```text
pm-cli list [--project <项目>] [--type <类型>] [--status <状态>] [--submitter <提交人>] [--priority <高|中|低>] [--keyword <关键词>] [--json]
# --submitter 同时接受大类（用户/Agent）与具体提交人：裸用户名命中该账号作为「用户」提交的任务；
# 「Agent（用户名）」命中该账号作为 Agent 提交的任务，与任务上的提交人显示形态一致。
pm-cli get <任务ID> [--json]
pm-cli attachments <任务ID> [--json]
pm-cli download <附件ID> [--output <文件路径>] [--force] [--json]
pm-cli projects [--json]
pm-cli create --project <项目> --type <新增需求|优化|BUG> --description <描述> [--priority <高|中|低>] [--json]
pm-cli status <任务ID> --to <进行中|待验证|已完成> [--json]
pm-cli priority <任务ID> --to <高|中|低> [--json]
pm-cli describe <任务ID> --description <描述> [--json]
pm-cli doctor [--json]
pm-cli config set <server-url|token> <值>
pm-cli config show
pm-cli --help
pm-cli <命令> --help
```

## 权限边界

可以：查看与筛选任务、只读查看项目、创建任务、列出并下载已授权项目中任务的附件。

只能：把任务状态切到 `进行中`、`待验证`、`已完成`；把优先级改为高/中/低；修改**由 Agent 创建**的任务描述（且不能清空）。

不可以：设置验收状态；修改项目、类型，或用户创建任务的描述；删除任务；上传或删除附件；管理项目；直接读写数据库。

用户的项目与字段授权会进一步收窄上述能力。以上限制由服务端强制执行，不因客户端不同而放宽。

## 排障

连接异常时先运行 `pm-cli doctor`（加 `--json` 便于解析）。它会报告：

- 实际生效的**连接来源**（环境变量 / 用户配置 / 本机应用写出的运行信息）与脱敏后的 token；
- 连通性与 HTTP 状态，以及当前 token 可见的项目数；
- 针对结论的下一步命令。

退出码与其它命令一致：`0` 正常，`2` 参数或鉴权失败，`3` 未配置或连不上。据此分支处理。

遇 401 检查 token 是否已吊销；遇 403 检查网页端的项目/字段授权，以及账号是否被停用；连不上时确认服务监听的地址和端口正确、防火墙放行。「环境变量必须成对设置」说明 `PM_SERVER_URL` / `PM_AGENT_TOKEN` 只设了其一或其一为空，按提示补齐另一个，或清掉已设的那个改走用户配置。

**完全没有 `pm-cli` 时**可以直接调 HTTP。`GET /api/agent/help` **不需要 token**，先看它：

```text
curl <服务地址>/api/agent/help
curl -H "Authorization: Bearer <token>" <服务地址>/api/agent/tasks
```

也可以在 `POST /api/agent/tasks` 创建任务、`PATCH /api/agent/tasks/{id}/status` 推进状态。`help` 里给出三类信息：`ask_the_user`（要让用户做什么）、`configure`（拿到 token 后执行什么命令）、`if_pm_cli_missing`（没有 `pm-cli` 时怎么办，含获取安装脚本的命令）。除此之外所有 `/api/agent/*` 接口仍需 token。
