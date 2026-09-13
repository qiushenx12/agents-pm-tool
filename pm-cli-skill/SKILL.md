---
name: pm-cli
description: 使用 Agents PM Tool 的受限 pm-cli 查看、创建和推进项目任务；当用户提供 Agents PM Tool 任务 ID、要求通过 pm-cli 工作或需要远程连接该工具时使用。
---

# Agents PM Tool pm-cli

使用同目录 `bin/pm-cli.exe` 访问 Agents PM Tool。不要直接读取或修改 SQLite；权限边界由服务端 `/api/agent/*` 强制执行。

<!-- pm-cli-skill-version: 1.0.0 -->

## 连接

连接信息有两项：**服务地址**和 **Agent token**。两者都由网页「我的 Agent 访问」面板签发，token 与该登录账号绑定。

### 方式一：手动部署或远程使用（下载 ZIP / 未安装桌面应用）

必须先配置，任选一种：

环境变量（两者必须同时设置，适合 CI 或临时使用；只设其中一个会直接报错，不会回落到用户配置）：

```powershell
$env:PM_SERVER_URL = "http://192.168.1.10:17890"
$env:PM_AGENT_TOKEN = "网页中签发的 token"
```

若已只设了其中一个导致报错：补齐另一个，或 `Remove-Item Env:PM_SERVER_URL` 清掉它、改用下面的用户配置。

或保存到用户配置（推荐，长期有效）：

```powershell
pm-cli config set server-url http://192.168.1.10:17890
pm-cli config set token "网页中签发的 token"
pm-cli config show
```

桌面应用未安装时不需要、也不会生成 `data/runtime.json`；遇到「尚未配置连接」按上面的命令配置即可。

### 方式二：应用一键安装到本机前端

由桌面应用安装到本机 Agent 前端时，exe 同级会存在 `data/runtime.json`，pm-cli 自动读取实际端口与 token，无需手动配置。确保桌面应用正在运行；安装或升级后重新打开终端或 Agent 前端。

## token 安全

- token 等同身份：pm-cli 完全以 token 所属账号的身份操作，权限也按该账号的授权。
- 不要把 token 写入仓库、任务描述、日志或本技能文件，也不要转给其他人使用。
- 重新生成 token 会让旧 token 立即失效，需同步更新配置。

## 安装位置

手动安装时，将整个 `pm-cli-skill` 目录解压到以下位置之一：

- Codex：`$HOME/.agents/skills/pm-cli`（当前官方用户级目录）；旧版 Codex 可使用 `$HOME/.codex/skills/pm-cli`。
- Claude Code：`$HOME/.claude/skills/pm-cli`。
- WorkBuddy：`$HOME/.workbuddy/skills/pm-cli`。
- OpenCode：`$HOME/.config/opencode/skills/pm-cli`。
- Cursor：`$HOME/.cursor/skills/pm-cli`。
- Pi：`$HOME/.pi/agent/skills/pm-cli`。
- DeepSeek Harness：`$HOME/.dsh/skills/pm-cli`（设置了 `DSH_HOME` 时用 `$DSH_HOME/skills/pm-cli`）。

安装后如未被识别，重新启动对应 Agent 前端。

## 标准流程

1. 运行 `pm-cli get <任务ID> --json` 读取完整任务。
2. 若 `attachment_count` 大于 0，运行 `pm-cli attachments <任务ID> --json` 查看附件，再使用 `pm-cli download <附件ID>` 按需下载。
3. 运行 `pm-cli projects --json` 获取项目、本地路径与 Git 地址。
4. 开始实现时运行 `pm-cli status <任务ID> --to 进行中 --json`。
5. 在任务对应仓库内实现并验证；保留用户已有改动。
6. 完成且自测通过后运行 `pm-cli status <任务ID> --to 待验证 --json`。

## 命令

```text
pm-cli list [--project <项目>] [--type <类型>] [--status <状态>] [--submitter <提交人>] [--keyword <关键词>] [--json]
# --submitter 同时接受大类（用户/Agent）与具体提交人：裸用户名命中该账号作为「用户」提交的任务；
# 「Agent（用户名）」命中该账号作为 Agent 提交的任务，与任务上的 submitter_name 显示形态一致。
pm-cli get <任务ID> [--json]
pm-cli attachments <任务ID> [--json]
pm-cli download <附件ID> [--output <文件路径>] [--force] [--json]
pm-cli projects [--json]
pm-cli create --project <项目> --type <新增需求|优化|BUG> --description <描述> [--json]
pm-cli status <任务ID> --to <进行中|待验证|已完成> [--json]
pm-cli describe <任务ID> --description <描述> [--json]
pm-cli config set <server-url|token> <值>
pm-cli config show
pm-cli --help
```

Agent 可以列出和下载已授权项目中任务的附件，但不能上传或删除附件。Agent 只能把任务切换到进行中、待验证或已完成；只能修改由 Agent 创建的任务描述；不能设置验收状态、删除任务，或修改用户创建任务的描述。用户的项目与字段授权会进一步收窄这些能力。

## 排障

连接异常时先运行 `pm-cli doctor`（加 `--json` 便于解析）。它会报告：

- 实际生效的**连接来源**（环境变量 / 用户配置 / 本机 `data/runtime.json`）与脱敏后的 token；
- 连通性与 HTTP 状态，以及当前 token 可见的项目数；
- skill 目录版本与 exe 版本是否一致；
- 针对结论的下一步命令。

退出码与其它命令一致：`0` 正常，`2` 参数或鉴权失败，`3` 未配置或连不上。Agent 可据此分支处理。

如 `pm-cli` 不可执行，可使用 Windows 10+ 自带的 `curl.exe` 调用相同接口。注意 `GET /api/agent/help` **无需 token**——没有 pm-cli、没有 skill 时应先访问它，按返回的 `bootstrap` 步骤提示用户签发 token：

```powershell
curl.exe "$env:PM_SERVER_URL/api/agent/help"
curl.exe -H "Authorization: Bearer $env:PM_AGENT_TOKEN" "$env:PM_SERVER_URL/api/agent/tasks"
```

`bootstrap` 里给出三类信息：`ask_the_user`（要让用户做什么）、`configure`（拿到 token 后执行的命令）、`if_pm_cli_missing`（没有 pm-cli 时怎么办）。除此之外所有 `/api/agent/*` 接口仍需 token。

遇到 401 时检查 token 是否已吊销；遇到 403 时检查网页端项目/字段授权；连接失败时确认服务监听范围为局域网、地址和端口正确、防火墙允许访问。报「环境变量必须成对设置」说明 `PM_SERVER_URL` / `PM_AGENT_TOKEN` 只设了其一或其一为空，按报错提示补齐，或清掉已设的那个改走用户配置。完整接口说明可请求 `GET /api/agent/help`。
