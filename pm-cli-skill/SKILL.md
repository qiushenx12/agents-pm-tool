---
name: pm-cli
description: 使用 Agents PM Tool 的受限 pm-cli 查看、创建和推进项目任务；当用户提供 Agents PM Tool 任务 ID、要求通过 pm-cli 工作或需要远程连接该工具时使用。
---

# Agents PM Tool pm-cli

使用同目录 `bin/pm-cli.exe` 访问 Agents PM Tool。不要直接读取或修改 SQLite；权限边界由服务端 `/api/agent/*` 强制执行。

<!-- pm-cli-skill-version: 1.0.0 -->

## 连接

本机安装版会自动从应用的 `data/runtime.json` 读取实际端口与主机临时 token。开始前确保 Agents PM Tool 桌面应用正在运行；安装或升级后重新打开终端或 Agent 前端。

远程连接优先使用环境变量：

```powershell
$env:PM_SERVER_URL = "http://192.168.1.10:17890"
$env:PM_AGENT_TOKEN = "网页中签发的 token"
```

也可保存到用户配置：

```powershell
pm-cli config set server-url http://192.168.1.10:17890
pm-cli config set token "网页中签发的 token"
pm-cli config show
```

不要把 token 写入仓库、任务描述、日志或本技能文件。

手动安装时，将整个 `pm-cli-skill` 目录解压到以下位置之一：

- Codex：`$HOME/.agents/skills/pm-cli`（当前官方用户级目录）；旧版 Codex 可使用 `$HOME/.codex/skills/pm-cli`。
- Claude Code：`$HOME/.claude/skills/pm-cli`。

安装后如未被识别，重新启动对应 Agent 前端。

## 标准流程

1. 运行 `pm-cli get <任务ID> --json` 读取完整任务。
2. 运行 `pm-cli projects --json` 获取项目、本地路径与 Git 地址。
3. 开始实现时运行 `pm-cli status <任务ID> --to 进行中 --json`。
4. 在任务对应仓库内实现并验证；保留用户已有改动。
5. 完成且自测通过后运行 `pm-cli status <任务ID> --to 待验证 --json`。

## 命令

```text
pm-cli list [--project <项目>] [--type <类型>] [--status <状态>] [--submitter <提交人>] [--keyword <关键词>] [--json]
pm-cli get <任务ID> [--json]
pm-cli projects [--json]
pm-cli create --project <项目> --type <新增需求|优化|BUG> --description <描述> [--json]
pm-cli status <任务ID> --to <进行中|待验证|已完成> [--json]
pm-cli describe <任务ID> --description <描述> [--json]
pm-cli config set <server-url|token> <值>
pm-cli config show
pm-cli --help
```

Agent 只能把任务切换到进行中、待验证或已完成；只能修改由 Agent 创建的任务描述；不能设置验收状态、删除任务、管理附件，或修改用户创建任务的描述。用户的项目与字段授权会进一步收窄这些能力。

如 `pm-cli` 不可执行，可使用 Windows 10+ 自带的 `curl.exe` 调用相同接口：

```powershell
curl.exe -H "Authorization: Bearer $env:PM_AGENT_TOKEN" "$env:PM_SERVER_URL/api/agent/help"
```

遇到 401 时检查 token 是否已吊销；遇到 403 时检查网页端项目/字段授权；连接失败时确认服务监听范围为局域网、地址和端口正确、防火墙允许访问。完整接口说明可请求 `GET /api/agent/help`。
