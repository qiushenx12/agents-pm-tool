#!/usr/bin/env node
/**
 * pm-cli —— Agents PM Tool 的 Agent 命令行（跨平台单文件实现）。
 *
 * 只是 HTTP 薄客户端：本机读应用写出的运行信息，远程读环境变量或用户配置，
 * 再调用服务端 /api/agent/*。没有任何本地数据库访问能力。
 *
 * 零第三方依赖：只用 Node 内置能力，Windows / macOS / Linux 通用。
 * 运行前提：Node.js 18 或更高版本。
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const EXIT_OK = 0;
const EXIT_FAILURE = 1;
const EXIT_VALIDATION = 2;
const EXIT_SERVICE_DOWN = 3;
const MIN_NODE_MAJOR = 18;
const UNKNOWN_VERSION = "0.0.0-dev";
const API_PREFIX = "/api/agent";

// ---------------------------------------------------------------- 位置与版本

/** 脚本自身的绝对路径；从管道执行（node --input-type=module）时可能不可得。 */
function scriptPath() {
  try {
    return fileURLToPath(import.meta.url);
  } catch {
    return null;
  }
}

/** skill 的 bin 目录；脚本不在 bin/ 下时返回 null。 */
function scriptBinDir() {
  const file = scriptPath();
  if (!file) return null;
  const directory = path.dirname(file);
  return path.basename(directory) === "bin" ? directory : null;
}

function skillDir() {
  const bin = scriptBinDir();
  return bin ? path.dirname(bin) : null;
}

/** skill 版本：安装时由应用写入 <skill>/VERSION。 */
function readSkillVersion() {
  const directory = skillDir();
  if (!directory) return null;
  try {
    const content = fs.readFileSync(path.join(directory, "VERSION"), "utf8").trim();
    return content || null;
  } catch {
    return null;
  }
}

function cliVersion() {
  return readSkillVersion() ?? UNKNOWN_VERSION;
}

// ------------------------------------------------------------ 配置与连接发现

/** 用户级配置目录，与桌面应用使用同一套跨平台语义。 */
function userConfigDir() {
  if (process.platform === "win32") {
    const base =
      process.env.APPDATA ||
      process.env.LOCALAPPDATA ||
      path.join(os.homedir(), "AppData", "Roaming");
    return path.join(base, "agents-pm-tool");
  }
  if (process.platform === "darwin") {
    return path.join(os.homedir(), "Library", "Application Support", "agents-pm-tool");
  }
  const base =
    process.env.XDG_CONFIG_HOME && process.env.XDG_CONFIG_HOME.trim()
      ? process.env.XDG_CONFIG_HOME
      : path.join(os.homedir(), ".config");
  return path.join(base, "agents-pm-tool");
}

function cliConfigPath() {
  return path.join(userConfigDir(), "cli.json");
}

/**
 * 本机应用写出的运行信息候选位置，按优先级排列。
 * 首个是应用固定写入的用户级位置；第二个照顾早期「skill 目录旁放 data/」的旧安装。
 *
 * 故意不包含「当前工作目录」：那样在别的项目里恰好存在 data/runtime.json 时
 * 会把命令连到无关实例上，代价远大于便利。
 */
function runtimeCandidates() {
  const candidates = [];
  const dataDir = process.env.PM_DATA_DIR;
  if (dataDir && dataDir.trim()) {
    candidates.push(path.join(dataDir, "runtime.json"));
  }
  candidates.push(path.join(userConfigDir(), "runtime.json"));
  const directory = skillDir();
  if (directory) {
    candidates.push(path.join(directory, "data", "runtime.json"));
  }
  return [...new Set(candidates)];
}

function loadCliConfig() {
  const file = cliConfigPath();
  let content;
  try {
    content = fs.readFileSync(file, "utf8");
  } catch (error) {
    if (error.code === "ENOENT") return { server_url: "", token: "" };
    throw new Error(`读取 ${file} 失败：${error.message}`);
  }
  try {
    const parsed = JSON.parse(content);
    return {
      server_url: typeof parsed?.server_url === "string" ? parsed.server_url : "",
      token: typeof parsed?.token === "string" ? parsed.token : "",
    };
  } catch {
    throw new Error(`${file} 内容损坏`);
  }
}

function saveCliConfig(config) {
  const file = cliConfigPath();
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(config, null, 2)}\n`, "utf8");
  return file;
}

function validateServerUrl(raw) {
  const trimmed = String(raw).trim().replace(/\/+$/, "");
  let parsed;
  try {
    parsed = new URL(trimmed);
  } catch {
    throw new Error("server-url 不是有效 URL");
  }
  if (!["http:", "https:"].includes(parsed.protocol) || !parsed.hostname) {
    throw new Error("server-url 必须是包含主机名的 http/https URL");
  }
  return trimmed;
}

/** 连接来源：doctor 用它说明「当前到底用的是哪一份配置」。 */
const SOURCE_LABELS = {
  environment: "环境变量 PM_SERVER_URL / PM_AGENT_TOKEN",
  config: "用户配置文件 cli.json",
  runtime: "本机应用写出的运行信息",
};

/** 清除单个环境变量的写法，两种平台各给一个，避免只说 unset 让 Windows 用户卡住。 */
function clearEnvHint(name) {
  return `清除 ${name}（Windows：Remove-Item Env:${name}；macOS / Linux：unset ${name}）`;
}

function envPairMessage(setName, setValue, missingName) {
  if (!String(setValue).trim()) {
    return (
      `环境变量 ${setName} 已设置但为空，且 ${missingName} 未设置。请为两者都填入有效值；` +
      `若想改用已保存的配置或本机应用自动发现，请先${clearEnvHint(setName)}。`
    );
  }
  return (
    `环境变量必须成对设置：${setName} 已设置，但 ${missingName} 未设置。请补齐 ${missingName}；` +
    `若想改用已保存的配置或本机应用自动发现，请先${clearEnvHint(setName)}。`
  );
}

function blankEnvironmentMessage(serverUrl, token) {
  const urlBlank = !String(serverUrl).trim();
  const tokenBlank = !String(token).trim();
  if (urlBlank && tokenBlank) {
    return (
      "环境变量 PM_SERVER_URL 与 PM_AGENT_TOKEN 均已设置但为空。请填入有效值；" +
      `若想改用已保存的配置或本机应用自动发现，请先${clearEnvHint("两者")}。`
    );
  }
  if (urlBlank) {
    return (
      "环境变量 PM_SERVER_URL 已设置但为空。请填入有效的服务地址；" +
      `若想改用已保存的配置或本机应用自动发现，请先${clearEnvHint("PM_SERVER_URL")}。`
    );
  }
  return (
    "环境变量 PM_AGENT_TOKEN 已设置但为空。请填入有效 token；" +
    `若想改用已保存的配置或本机应用自动发现，请先${clearEnvHint("PM_AGENT_TOKEN")}。`
  );
}

/** 未配置连接时的指引：给可直接执行的两条命令，不把内部文件名抛给用户。 */
function notConfiguredMessage() {
  return (
    "尚未配置连接。请执行 pm-cli config set server-url <服务地址> 与 " +
    "pm-cli config set token <token>（token 在网页「我的 Agent 访问」面板签发）；" +
    "也可改用环境变量 PM_SERVER_URL 与 PM_AGENT_TOKEN（两者必须同时设置）"
  );
}

function unreachableMessage(baseUrl) {
  return (
    `无法连接 ${baseUrl}。请检查服务是否运行、监听范围是否为局域网、` +
    "端口是否正确，以及防火墙是否放行"
  );
}

/**
 * 连接发现：环境变量 > 用户配置 > 本机应用写出的运行信息。
 * 返回 {serverUrl, token, source, runtimePath?}；失败抛 Error（消息即可执行的下一步）。
 */
function resolveConnection() {
  const envUrl = process.env.PM_SERVER_URL;
  const envToken = process.env.PM_AGENT_TOKEN;
  const hasUrl = envUrl !== undefined;
  const hasToken = envToken !== undefined;

  if (hasUrl && hasToken) {
    if (String(envUrl).trim() && String(envToken).trim()) {
      return {
        serverUrl: validateServerUrl(envUrl),
        token: String(envToken).trim(),
        source: "environment",
      };
    }
    throw new Error(blankEnvironmentMessage(envUrl, envToken));
  }
  if (hasUrl) throw new Error(envPairMessage("PM_SERVER_URL", envUrl, "PM_AGENT_TOKEN"));
  if (hasToken) throw new Error(envPairMessage("PM_AGENT_TOKEN", envToken, "PM_SERVER_URL"));

  const config = loadCliConfig();
  if (config.server_url.trim() || config.token.trim()) {
    if (!config.server_url.trim() || !config.token.trim()) {
      throw new Error(
        `远程配置不完整，请使用 pm-cli config set 补齐 server-url 与 token（${cliConfigPath()}）`,
      );
    }
    return {
      serverUrl: validateServerUrl(config.server_url),
      token: config.token.trim(),
      source: "config",
    };
  }

  for (const candidate of runtimeCandidates()) {
    let content;
    try {
      content = fs.readFileSync(candidate, "utf8");
    } catch {
      continue;
    }
    let info;
    try {
      info = JSON.parse(content);
    } catch {
      throw new Error(`${candidate} 内容损坏，请重启 Agents PM Tool 后重试。`);
    }
    const port = Number(info?.port);
    const token = typeof info?.token === "string" ? info.token : "";
    if (!Number.isInteger(port) || port <= 0 || !token) {
      throw new Error(`${candidate} 内容损坏，请重启 Agents PM Tool 后重试。`);
    }
    return {
      serverUrl: `http://127.0.0.1:${port}`,
      token,
      source: "runtime",
      runtimePath: candidate,
    };
  }

  throw new Error(notConfiguredMessage());
}

// ------------------------------------------------------------------ HTTP 客户端

class ServiceDownError extends Error {}
class ServerError extends Error {
  constructor(exitCode) {
    super("服务端返回失败");
    this.exitCode = exitCode;
  }
}

function createClient(connection) {
  const base = `${connection.serverUrl}${API_PREFIX}`;
  const authHeaders = { authorization: `Bearer ${connection.token}` };

  async function send(method, route, body) {
    const init = { method, headers: { ...authHeaders } };
    if (body !== undefined) {
      init.headers["content-type"] = "application/json";
      init.body = JSON.stringify(body);
    }
    let response;
    try {
      response = await fetch(`${base}${route}`, init);
    } catch {
      throw new ServiceDownError(unreachableMessage(base));
    }
    const text = await response.text();
    let value;
    try {
      value = JSON.parse(text);
    } catch {
      value = { raw: text };
    }
    return { status: response.status, value };
  }

  return { base, send, authHeaders };
}

/** 服务端返回失败：按状态码打印提示，并给出对应退出码。 */
function failFromServer(status, value) {
  process.stderr.write(`错误：${value?.error?.message ?? "未知错误"}\n`);
  const projects = value?.error?.details?.projects;
  if (Array.isArray(projects)) {
    process.stderr.write(`当前项目选项：${projects.join("、")}\n`);
  }
  if (status === 401) {
    process.stderr.write(
      "请在网页「我的 Agent 访问」面板重新生成 token，然后执行 pm-cli config set token <新 token>\n",
    );
  } else if (status === 403) {
    process.stderr.write(
      "请检查该账号在网页端的项目与字段授权；若账号被停用，需联系管理员恢复。\n",
    );
  }
  return new ServerError(status >= 400 && status < 500 ? EXIT_VALIDATION : EXIT_FAILURE);
}

// -------------------------------------------------------------------- 输出格式

const text = (input) => (input === null || input === undefined ? "" : String(input));

function printTask(task) {
  const row = [
    text(task?.id),
    `[${text(task?.project)}]`,
    text(task?.type),
    text(task?.status),
    text(task?.priority ?? "中"),
    text(task?.submitter_name ?? task?.submitter),
    text(task?.description).replace(/\n/g, " "),
  ].join("\t");
  process.stdout.write(`${row}\n`);
}

function printTaskDetail(task) {
  const lines = [
    `ID：       ${text(task?.id)}`,
    `项目：     ${text(task?.project)}`,
    `类型：     ${text(task?.type)}`,
    `状态：     ${text(task?.status)}`,
    `优先级：   ${text(task?.priority ?? "中")}`,
    `提交人：   ${text(task?.submitter_name ?? task?.submitter)}`,
    `创建时间： ${text(task?.created_at)}`,
    `完成时间： ${task?.finished_at ? text(task.finished_at) : "—"}`,
    `描述：     ${text(task?.description)}`,
    `备注：     ${text(task?.note)}`,
    `附件：     ${Number(task?.attachment_count ?? 0)} 个`,
  ];
  process.stdout.write(`${lines.join("\n")}\n`);
}

function printAttachment(attachment) {
  const row = [
    text(attachment?.id),
    text(attachment?.filename),
    `${Number(attachment?.size ?? 0)} bytes`,
    text(attachment?.created_at),
  ].join("\t");
  process.stdout.write(`${row}\n`);
}

function printJson(payload) {
  process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
}

// ---------------------------------------------------------------- 附件下载细节

/** 解析 Content-Disposition 的 filename*=UTF-8'' 形式。 */
function filenameFromContentDisposition(header) {
  if (!header) return null;
  const part = String(header)
    .split(";")
    .map((item) => item.trim())
    .find((item) => item.startsWith("filename*=UTF-8''"));
  if (!part) return null;
  const encoded = part.slice("filename*=UTF-8''".length);
  try {
    return decodeURIComponent(encoded.replace(/\+/g, "%20"));
  } catch {
    return null;
  }
}

function safeDownloadFilename(filename, fallback) {
  const leaf = String(filename).split(/[/\\]/).pop() ?? "";
  const sanitized = leaf
    .replace(/[\u0000-\u001f<>:"/\\|?*]/g, "_") // eslint-disable-line no-control-regex
    .replace(/^[ .]+|[ .]+$/g, "");
  if (!sanitized || sanitized === "." || sanitized === "..") return fallback;
  return sanitized;
}

function isDirectory(target) {
  try {
    return fs.statSync(target).isDirectory();
  } catch {
    return false;
  }
}

function downloadTarget(output, filename) {
  if (!output) return filename;
  return isDirectory(output) ? path.join(output, filename) : output;
}

function writeDownload(target, bytes, force) {
  const parent = path.dirname(target);
  if (parent && parent !== "." && !isDirectory(parent)) {
    throw new Error(`输出目录不存在：${parent}`);
  }
  try {
    fs.writeFileSync(target, bytes, { flag: force ? "w" : "wx" });
  } catch (error) {
    if (error.code === "EEXIST") {
      throw new Error(`文件已存在：${target}（如需覆盖请添加 --force）`);
    }
    throw new Error(`无法写入 ${target}：${error.message}`);
  }
}

// ------------------------------------------------------------------------ 帮助

const LOCATE_NOTE = `【先找到 pm-cli 的位置】
pm-cli 随 skill 一起安装，脚本在 skill 目录的 bin/ 下。安装过程不会改动系统 PATH，
所以直接敲 pm-cli 未必能找到命令：先确认下面两行，再执行后面的命令示例。

  当前脚本：{SCRIPT}
  skill 目录：{SKILL}

用法：切到该 bin 目录后执行（macOS / Linux 可直接 ./pm-cli，Windows 用 pm-cli.cmd），
或统一写成完整形式：node "<skill>/bin/pm-cli.mjs" <命令> [选项]

运行前提：本机有 Node.js 18 或更高版本。`;

function locateBlock() {
  const file = scriptPath() ?? "（无法确定，请用 node 显式指定脚本路径）";
  const directory = skillDir() ?? "（脚本不在 skill 的 bin 目录下）";
  return LOCATE_NOTE.replace("{SCRIPT}", file).replace("{SKILL}", directory);
}

const COMMANDS = [
  ["list", "列出任务（支持筛选）"],
  ["get <任务ID>", "查看任务详情"],
  ["attachments <任务ID>", "列出任务附件（只读）"],
  ["download <附件ID>", "下载附件（只读）"],
  ["projects", "列出可见项目（只读）"],
  ["permissions", "查看当前 token 的有效项目与字段权限"],
  ["create", "创建任务（项目/类型/描述三必填）"],
  ["update <任务ID>", "按授权修改任务的可编辑字段"],
  ["status <任务ID>", "修改任务状态（默认可设进行中/待验证/已完成）"],
  ["priority <任务ID>", "调整任务优先级（高/中/低）"],
  ["describe <任务ID>", "修改描述（仅限 Agent 自己创建的任务）"],
  ["doctor", "诊断连接配置与连通性（排障第一步）"],
  ["config set|show", "保存或查看远程连接配置"],
];

/**
 * 按终端显示宽度补齐：中日韩字符占 2 列，
 * 直接用 padEnd 会按码元数计算，导致命令清单里带中文的用法列参差不齐。
 */
function padByDisplayWidth(content, width) {
  let used = 0;
  for (const character of content) {
    used += isFullWidth(character) ? 2 : 1;
  }
  return content + " ".repeat(Math.max(width - used, 1));
}

function isFullWidth(character) {
  const code = character.codePointAt(0);
  return (
    (code >= 0x1100 && code <= 0x115f) ||
    (code >= 0x2e80 && code <= 0xa4cf) ||
    (code >= 0xac00 && code <= 0xd7a3) ||
    (code >= 0xf900 && code <= 0xfaff) ||
    (code >= 0xfe30 && code <= 0xfe6f) ||
    (code >= 0xff00 && code <= 0xff60) ||
    (code >= 0xffe0 && code <= 0xffe6)
  );
}

/** 每个命令的详细帮助：--help 与 `pm-cli help <命令>` 都用它。 */
const COMMAND_HELP = {
  list: {
    summary: "列出任务。默认只返回当前 token 已授权项目中的任务。",
    usage:
      "pm-cli list [--project <项目>] [--type <新增需求|优化|BUG>] [--status <状态>]\n" +
      "             [--submitter <提交人>] [--priority <高|中|低>] [--keyword <关键词>] [--json]",
    details: [
      "--submitter 同时接受大类（用户/Agent）与具体提交人：裸用户名命中该账号作为「用户」提交的任务；",
      "「Agent（用户名）」命中该账号作为 Agent 提交的任务，与任务上的提交人显示形态一致。",
    ],
  },
  get: {
    summary: "查看单个任务详情。",
    usage: "pm-cli get <任务ID> [--json]",
    details: [],
  },
  attachments: {
    summary: "列出某个任务的附件。Agent 只能查看和下载，不能上传或删除。",
    usage: "pm-cli attachments <任务ID> [--json]",
    details: [],
  },
  download: {
    summary: "下载单个附件。默认使用服务端给出的原文件名，且不覆盖已有文件。",
    usage: "pm-cli download <附件ID> [-o <文件路径>] [--force] [--json]",
    details: [
      "-o/--output 可以给目录（按原文件名另存）或完整文件路径；",
      "--force 允许覆盖已存在的文件。",
    ],
  },
  projects: {
    summary: "列出当前 token 可见的项目，含本地路径与 Git 地址。只读。",
    usage: "pm-cli projects [--json]",
    details: [],
  },
  permissions: {
    summary: "查看当前 token 在各可见项目中真正能创建、修改哪些字段及枚举值。",
    usage: "pm-cli permissions [--json]",
    details: ["结果是 Agent 权限与账号项目/字段授权的交集；修改描述还受 description_scope 约束。"],
  },
  create: {
    summary: "创建任务。项目、类型、描述三必填；其它字段按当前权限设置，优先级默认「中」。",
    usage:
      'pm-cli create --project <项目> --type <类型> --description <描述> [--note <备注>] [--status <状态>] [--priority <优先级>] [--predecessor-task-ids <ID,ID>] [--unlock-task-ids <ID,ID>] [--json]',
    details: ["创建者固定为该 token 所属账号的 Agent 身份；依赖任务 ID 用英文逗号分隔。", "先运行 pm-cli permissions 查看哪些可选字段已获授权。"],
  },
  update: {
    summary: "一次修改一个或多个任务字段；仅授权、合法的参数会生效。",
    usage: "pm-cli update <任务ID> [--project <项目>] [--type <类型>] [--description <描述>] [--note <备注>] [--status <状态>] [--priority <优先级>] [--predecessor-task-ids <ID,ID>] [--unlock-task-ids <ID,ID>] [--json]",
    details: ["至少指定一个待修改字段；给依赖 ID 选项传空字符串可清空关联。", "ID、提交人、创建/完成时间等不可变字段不支持修改。"],
  },
  status: {
    summary: "修改任务状态；默认只可切到进行中、待验证、已完成，管理员可调整。",
    usage: "pm-cli status <任务ID> --to <状态> [--json]",
    details: ["实际可设状态请运行 pm-cli permissions 查看。"],
  },
  priority: {
    summary: "调整任务优先级。",
    usage: "pm-cli priority <任务ID> --to <高|中|低> [--json]",
    details: [],
  },
  describe: {
    summary: "修改任务描述。默认仅限自己 Agent 创建的任务，管理员可调整；描述不能为空。",
    usage: "pm-cli describe <任务ID> --description <描述> [--json]",
    details: [],
  },
  doctor: {
    summary: "诊断连接配置与连通性。连不上、401、403 时先跑它。",
    usage: "pm-cli doctor [--json]",
    details: [
      "报告实际生效的连接来源、脱敏后的 token、连通性与可见项目数，并给出下一步命令。",
    ],
  },
  config: {
    summary: "保存或查看远程连接配置。本机装有 Agents PM Tool 时通常不需要配置。",
    usage: "pm-cli config set <server-url|token> <值>\npm-cli config show",
    details: ["配置写在用户级配置文件里，长期有效；环境变量优先级更高。"],
  },
};

function helpText() {
  const commandLines = COMMANDS.map(
    ([usage, summary]) => `  ${padByDisplayWidth(usage, 26)}${summary}`,
  ).join("\n");
  return `pm-cli ${cliVersion()} —— Agents PM Tool 的 Agent 命令行（权限受限，跨平台单文件实现）

${locateBlock()}

命令：
${commandLines}

通用选项：
  --json                         成功时输出结构化 JSON；失败仍向标准错误输出文本
  -h, --help                     显示帮助；写成 "pm-cli <命令> --help" 查看单个命令
  -V, --version                  显示版本

连接方式：本机装有 Agents PM Tool 时无需任何配置（应用会把端口与 token 写到固定的
用户级位置，pm-cli 自动读取）；远程使用时配置一次即可。发现顺序为
环境变量 PM_SERVER_URL/PM_AGENT_TOKEN（必须成对）> 用户配置 cli.json > 本机运行信息。

退出码：0 成功；1 服务端故障等其它失败；2 参数、鉴权或业务校验失败；3 未配置或连接失败。

权限边界：读取限于已授权项目；任务创建和字段修改由管理员设置的 Agent 权限与网页
项目/字段权限共同决定。运行 pm-cli permissions 查看当前有效权限。任务删除、附件上传或删除
仍仅限网页端；ID、提交人、时间戳等不可变字段不能由 Agent 修改，也不能直接读写数据库。

典型用法：
  pm-cli get 202609021050340001 --json
  pm-cli attachments 202609021050340001 --json
  pm-cli status 202609021050340001 --to 待验证
  pm-cli create --project default-project --type BUG --description "登录页白屏" --priority 高
  pm-cli permissions --json
  pm-cli update 202609021050340001 --note "已复查" --predecessor-task-ids 202609021050340000
  pm-cli doctor`;
}

function commandHelpText(name) {
  const entry = COMMAND_HELP[name];
  const lines = [`pm-cli ${name} —— ${entry.summary}`, "", "用法：", `  ${entry.usage}`];
  if (entry.details.length) {
    lines.push("", "说明：", ...entry.details.map((item) => `  ${item}`));
  }
  lines.push("", `依赖连接信息；连不上时先运行 pm-cli doctor 排查。`);
  return lines.join("\n");
}

function versionText() {
  return `pm-cli ${cliVersion()}`;
}

// -------------------------------------------------------------------- 参数解析

class UsageError extends Error {}

/** 每个命令允许的选项：true 表示布尔开关，其余表示需要取值。 */
const COMMAND_OPTIONS = {
  list: {
    project: "value",
    type: "value",
    status: "value",
    submitter: "value",
    priority: "value",
    keyword: "value",
    json: true,
  },
  get: { json: true },
  attachments: { json: true },
  download: { output: "value", o: "value", force: true, json: true },
  projects: { json: true },
  permissions: { json: true },
  create: { project: "value", type: "value", description: "value", note: "value", status: "value", priority: "value", "predecessor-task-ids": "value", "unlock-task-ids": "value", json: true },
  update: { project: "value", type: "value", description: "value", note: "value", status: "value", priority: "value", "predecessor-task-ids": "value", "unlock-task-ids": "value", json: true },
  status: { to: "value", json: true },
  priority: { to: "value", json: true },
  describe: { description: "value", json: true },
  doctor: { json: true },
  config: {},
};

const POSITIONAL_RANGE = {
  list: [0, 0],
  get: [1, 1],
  attachments: [1, 1],
  download: [1, 1],
  projects: [0, 0],
  permissions: [0, 0],
  create: [0, 0],
  update: [1, 1],
  status: [1, 1],
  priority: [1, 1],
  describe: [1, 1],
  doctor: [0, 0],
  config: [1, 3],
};

function splitOption(raw) {
  const index = raw.indexOf("=");
  return index === -1 ? [raw, undefined] : [raw.slice(0, index), raw.slice(index + 1)];
}

/** 手写解析：只认声明过的选项，未知选项直接报错，避免静默忽略打错的参数。 */
function parseCommand(command, args) {
  const spec = COMMAND_OPTIONS[command];
  const options = {};
  const positionals = [];

  for (let index = 0; index < args.length; index += 1) {
    const token = args[index];
    if (token === "--") {
      positionals.push(...args.slice(index + 1));
      break;
    }
    if (token.startsWith("--")) {
      const [name, inline] = splitOption(token.slice(2));
      if (!(name in spec)) {
        throw new UsageError(`未知选项：--${name}（运行 pm-cli ${command} --help 查看可用选项）`);
      }
      if (spec[name] === true) {
        if (inline !== undefined) throw new UsageError(`选项 --${name} 不接受取值`);
        options[name] = true;
      } else {
        const next = inline !== undefined ? inline : args[++index];
        if (next === undefined) throw new UsageError(`选项 --${name} 缺少取值`);
        options[name] = next;
      }
      continue;
    }
    if (token.startsWith("-") && token.length > 1) {
      const short = token.slice(1, 2);
      if (!(short in spec)) throw new UsageError(`未知选项：-${short}`);
      if (spec[short] === true) {
        options[short] = true;
      } else {
        const next = token.length > 2 ? token.slice(2) : args[++index];
        if (next === undefined) throw new UsageError(`选项 -${short} 缺少取值`);
        options[short] = next;
      }
      continue;
    }
    positionals.push(token);
  }

  const [min, max] = POSITIONAL_RANGE[command];
  if (positionals.length < min || positionals.length > max) {
    const expected = min === max ? `${min}` : `${min}–${max}`;
    throw new UsageError(
      `pm-cli ${command} 需要 ${expected} 个参数，实际收到 ${positionals.length} 个（运行 pm-cli ${command} --help 查看用法）`,
    );
  }
  return { options, positionals };
}

function requireOption(options, name, hint) {
  if (options[name] === undefined) {
    throw new UsageError(`--${name} 为必填项（${hint}）`);
  }
  return options[name];
}

/**
 * 先校验必填选项，再连服务。
 * 用法错误和「连不上」是两件事：前者不该因为本机没配连接就报成连接失败。
 */
function validateRequiredOptions(name, options) {
  if (name === "create") {
    const hint = "Agent 创建任务时项目/类型/描述三必填";
    requireOption(options, "project", hint);
    requireOption(options, "type", hint);
    requireOption(options, "description", hint);
  }
  if (name === "update" && !Object.keys(options).some((key) => key !== "json")) {
    throw new UsageError("pm-cli update 至少需要一个待修改字段");
  }
  if (name === "status") {
    requireOption(options, "to", "可设状态见 pm-cli permissions；默认进行中/待验证/已完成");
  }
  if (name === "priority") {
    requireOption(options, "to", "可设为：高/中/低");
  }
  if (name === "describe") {
    requireOption(options, "description", "要写入的新描述");
  }
}

function taskBodyFromOptions(options) {
  const body = {};
  for (const field of ["project", "type", "description", "note", "status", "priority"]) {
    if (options[field] !== undefined) body[field] = options[field];
  }
  for (const field of ["predecessor_task_ids", "unlock_task_ids"]) {
    const option = field.replaceAll("_", "-");
    if (options[option] !== undefined) {
      body[field] = options[option] === "" ? [] : options[option].split(",").map((id) => id.trim());
    }
  }
  return body;
}

// -------------------------------------------------------------------- 命令实现

async function runTaskCommand(name, options, positionals, client) {
  const json = options.json === true;

  if (name === "list") {
    const query = new URLSearchParams();
    for (const key of ["project", "type", "status", "submitter", "priority", "keyword"]) {
      if (options[key] !== undefined) query.append(key, options[key]);
    }
    const queryString = query.toString();
    const { status, value: payload } = await client.send(
      "GET",
      queryString ? `/tasks?${queryString}` : "/tasks",
    );
    if (status >= 400) throw failFromServer(status, payload);
    if (json) {
      printJson(payload);
    } else if (Array.isArray(payload)) {
      if (!payload.length) process.stdout.write("（无匹配任务）\n");
      for (const task of payload) printTask(task);
    }
    return;
  }

  if (name === "get") {
    const { status, value: payload } = await client.send(
      "GET",
      `/tasks/${encodeURIComponent(positionals[0])}`,
    );
    if (status >= 400) throw failFromServer(status, payload);
    if (json) printJson(payload);
    else printTaskDetail(payload);
    return;
  }

  if (name === "attachments") {
    const { status, value: payload } = await client.send(
      "GET",
      `/tasks/${encodeURIComponent(positionals[0])}/attachments`,
    );
    if (status >= 400) throw failFromServer(status, payload);
    if (json) {
      printJson(payload);
    } else if (Array.isArray(payload)) {
      if (!payload.length) process.stdout.write("（该任务暂无附件）\n");
      for (const attachment of payload) printAttachment(attachment);
    }
    return;
  }

  if (name === "download") {
    const id = positionals[0];
    let response;
    try {
      response = await fetch(`${client.base}/attachments/${encodeURIComponent(id)}`, {
        headers: client.authHeaders,
      });
    } catch {
      throw new ServiceDownError(unreachableMessage(client.base));
    }
    if (response.status >= 400) {
      const raw = await response.text();
      let payload;
      try {
        payload = JSON.parse(raw);
      } catch {
        payload = { raw };
      }
      throw failFromServer(response.status, payload);
    }
    const serverFilename =
      filenameFromContentDisposition(response.headers.get("content-disposition")) ?? id;
    const filename = safeDownloadFilename(serverFilename, id);
    let bytes;
    try {
      bytes = Buffer.from(await response.arrayBuffer());
    } catch {
      throw new ServiceDownError("附件下载中断");
    }
    const target = downloadTarget(options.output ?? options.o, filename);
    try {
      writeDownload(target, bytes, options.force === true);
    } catch (error) {
      process.stderr.write(`错误：${error.message}\n`);
      throw new ServerError(EXIT_VALIDATION);
    }
    let displayed;
    try {
      displayed = fs.realpathSync(target);
    } catch {
      displayed = path.resolve(target);
    }
    if (json) {
      printJson({ id, filename, path: displayed, size: bytes.length });
    } else {
      process.stdout.write(`附件已下载：${displayed}\n`);
    }
    return;
  }

  if (name === "create") {
    const hint = "Agent 创建任务时项目/类型/描述三必填";
    requireOption(options, "project", hint);
    requireOption(options, "type", hint);
    requireOption(options, "description", hint);
    const body = taskBodyFromOptions(options);
    const { status, value: payload } = await client.send("POST", "/tasks", body);
    if (status >= 400) throw failFromServer(status, payload);
    if (json) {
      printJson(payload);
    } else {
      process.stdout.write("已创建任务：\n");
      printTaskDetail(payload);
    }
    return;
  }

  if (name === "update") {
    const id = positionals[0];
    const { status, value: payload } = await client.send(
      "PATCH", `/tasks/${encodeURIComponent(id)}`, taskBodyFromOptions(options),
    );
    if (status >= 400) throw failFromServer(status, payload);
    if (json) printJson(payload);
    else process.stdout.write(`任务 ${payload?.id ?? id} 已更新\n`);
    return;
  }

  if (name === "status" || name === "priority") {
    const id = positionals[0];
    const to = requireOption(
      options,
      "to",
      name === "status" ? "可设状态见 pm-cli permissions；默认进行中/待验证/已完成" : "可设为：高/中/低",
    );
    const body = name === "status" ? { status: to } : { priority: to };
    const { status, value: payload } = await client.send(
      "PATCH",
      `/tasks/${encodeURIComponent(id)}/${name}`,
      body,
    );
    if (status >= 400) throw failFromServer(status, payload);
    if (json) {
      printJson(payload);
      return;
    }
    const updatedId = payload?.id ?? id;
    process.stdout.write(
      name === "status"
        ? `任务 ${updatedId} 状态已更新为「${text(payload?.status)}」\n`
        : `任务 ${updatedId} 优先级已更新为「${text(payload?.priority)}」\n`,
    );
    return;
  }

  if (name === "describe") {
    const id = positionals[0];
    const description = requireOption(options, "description", "要写入的新描述");
    const { status, value: payload } = await client.send(
      "PATCH",
      `/tasks/${encodeURIComponent(id)}/description`,
      { description },
    );
    if (status >= 400) throw failFromServer(status, payload);
    if (json) printJson(payload);
    else process.stdout.write(`任务 ${payload?.id ?? id} 描述已更新\n`);
    return;
  }

  if (name === "projects") {
    const { status, value: payload } = await client.send("GET", "/projects");
    if (status >= 400) throw failFromServer(status, payload);
    if (json) {
      printJson(payload);
    } else if (Array.isArray(payload)) {
      for (const project of payload) {
        process.stdout.write(`${text(project?.name)}\n`);
        if (project?.local_path) process.stdout.write(`  本地路径：${project.local_path}\n`);
        if (project?.git_url) process.stdout.write(`  Git 地址：${project.git_url}\n`);
      }
    }
    return;
  }

  if (name === "permissions") {
    const { status, value: payload } = await client.send("GET", "/permissions");
    if (status >= 400) throw failFromServer(status, payload);
    if (json) {
      printJson(payload);
    } else {
      for (const project of payload?.projects ?? []) {
        process.stdout.write(`${text(project.project)}：${project.can_create ? "可创建" : "不可创建"}\n`);
        process.stdout.write(`  创建字段：${(project.create_fields ?? []).join("、") || "无"}\n`);
        process.stdout.write(`  修改字段：${(project.edit_fields ?? []).join("、") || "无"}\n`);
        process.stdout.write(`  状态值：${(project.allowed_values?.status ?? []).join("、") || "无"}\n`);
        process.stdout.write(`  描述范围：${project.description_scope === "any" ? "任意可见任务" : "自己 Agent 创建的任务"}\n`);
      }
      if (!payload?.projects?.length) process.stdout.write("（无可见项目）\n");
    }
    return;
  }

  throw new UsageError(`未知命令：${name}`);
}

function handleConfig(positionals) {
  try {
    return applyConfig(positionals);
  } catch (error) {
    // config 的问题都是使用方式问题：统一按参数错误退出，与其它子命令一致。
    throw new UsageError(error?.message ?? String(error));
  }
}

function applyConfig(positionals) {
  const action = positionals[0];
  const file = cliConfigPath();

  if (action === "show") {
    const config = loadCliConfig();
    process.stdout.write(`server-url：${config.server_url || "（未设置）"}\n`);
    process.stdout.write(`token：${config.token ? "********" : "（未设置）"}\n`);
    process.stdout.write(`配置文件：${file}\n`);
    return EXIT_OK;
  }
  if (action !== "set") {
    throw new UsageError("config 仅支持 set 或 show");
  }

  const key = positionals[1];
  const rawValue = positionals[2];
  if (key === undefined || rawValue === undefined) {
    throw new UsageError("config set 需要 <server-url|token> 和取值两个参数");
  }
  const config = loadCliConfig();
  if (key === "server-url") {
    config.server_url = validateServerUrl(rawValue);
  } else if (key === "token") {
    if (!String(rawValue).trim()) throw new UsageError("token 不能为空");
    config.token = String(rawValue).trim();
  } else {
    throw new UsageError("配置项仅支持 server-url 或 token");
  }
  saveCliConfig(config);
  process.stdout.write(`配置已保存：${file}\n`);
  return EXIT_OK;
}

// ---------------------------------------------------------------------- doctor

function maskToken(token) {
  const trimmed = String(token ?? "").trim();
  if (!trimmed) return "";
  const chars = [...trimmed];
  if (chars.length <= 8) return "*".repeat(chars.length);
  return `${chars.slice(0, 4).join("")}****${chars.slice(-4).join("")}`;
}

function statusHint(status) {
  if (status === 401) return "token 无效或已吊销";
  if (status === 403) return "账号被停用或无权访问";
  return "服务返回错误";
}

/**
 * 用户配置优先于本机运行信息，所以「以前手工 config set 过一次」的人可能会被旧配置盖住，
 * 而应用的实际端口早就变了 —— 这时候报错会让人完全摸不着头脑。
 * 这里只在两份地址确实不一致时给出提示，正常情况下不打扰。
 */
function staleLocalConfigHint(connection) {
  if (connection.source !== "config") return null;
  let configuredPort;
  try {
    configuredPort = new URL(connection.serverUrl).port;
  } catch {
    return null;
  }
  for (const candidate of runtimeCandidates()) {
    let info;
    try {
      info = JSON.parse(fs.readFileSync(candidate, "utf8"));
    } catch {
      continue;
    }
    const livePort = String(info?.port ?? "");
    if (!livePort || livePort === configuredPort) continue;
    return (
      `你的用户配置指向 ${connection.serverUrl}，但本机应用写出的运行信息是端口 ${livePort}；` +
      `配置优先。若这份配置已经过时，请执行 pm-cli config set server-url http://127.0.0.1:${livePort}，` +
      `或删除配置文件后改用本机自动发现。`
    );
  }
  return null;
}

/** 收集诊断信息。无论结论如何都返回完整报告，由调用方决定打印与退出码。 */
async function collectDoctor() {
  const candidates = runtimeCandidates();
  const report = {
    cli_version: cliVersion(),
    cli_path: scriptPath() ?? "（未知）",
    skill_dir: skillDir() ?? "（脚本不在 skill 的 bin 目录下）",
    node_version: process.version,
    source: null,
    server_url: null,
    token_masked: null,
    config_path: cliConfigPath(),
    runtime_candidates: candidates,
    runtime_path: null,
    reachable: false,
    status: null,
    visible_projects: null,
    problem: null,
    hints: [],
  };

  let connection;
  try {
    connection = resolveConnection();
  } catch (error) {
    report.problem = error.message;
    const existing = candidates.find((candidate) => fs.existsSync(candidate));
    if (existing) report.runtime_path = existing;
    report.hints = [
      ...(existing
        ? ["本机运行信息已存在但读取失败：重启 Agents PM Tool 桌面应用即可重新写出"]
        : []),
      `pm-cli config set server-url <服务地址>（写入 ${report.config_path}）`,
      "pm-cli config set token <token>，token 在网页「我的 Agent 访问」面板签发",
      "或改用环境变量 PM_SERVER_URL 与 PM_AGENT_TOKEN（必须成对设置）",
    ];
    return { report, exitCode: EXIT_SERVICE_DOWN };
  }

  report.source = connection.source;
  report.server_url = connection.serverUrl;
  report.token_masked = maskToken(connection.token);
  report.runtime_path = connection.runtimePath ?? null;
  const staleConfigHint = staleLocalConfigHint(connection);

  // 用需要鉴权的 /projects 探测：既验证服务可达，也验证 token 是否有效。
  // （不能用免 token 的 /api/agent/help —— token 错误时它同样返回 200。）
  let response;
  try {
    response = await createClient(connection).send("GET", "/projects");
  } catch (error) {
    report.problem = error.message;
    report.hints = [
      ...(staleConfigHint ? [staleConfigHint] : []),
      "确认 Agents PM Tool 桌面应用正在运行",
      "确认服务的监听范围包含当前访问来源",
      "确认端口未被占用且防火墙已放行",
    ];
    return { report, exitCode: EXIT_SERVICE_DOWN };
  }

  report.status = response.status;
  if (response.status >= 400) {
    const message = response.value?.error?.message;
    report.problem = message
      ? `服务返回 HTTP ${response.status}：${message}`
      : `服务返回 HTTP ${response.status}（${statusHint(response.status)}）`;
    if (response.status === 401) {
      report.hints = [
        ...(staleConfigHint ? [staleConfigHint] : []),
        "在网页「我的 Agent 访问」重新生成 token",
        "然后执行 pm-cli config set token <新 token>",
      ];
    } else if (response.status === 403) {
      report.hints = ["确认 token 所属账号未被停用", "确认当前项目已授权给该账号"];
    } else {
      report.hints = [
        ...(staleConfigHint ? [staleConfigHint] : []),
        "查看应用日志了解服务端错误详情",
      ];
    }
    const exitCode =
      response.status === 401 || response.status === 403 ? EXIT_VALIDATION : EXIT_SERVICE_DOWN;
    return { report, exitCode };
  }

  report.reachable = true;
  report.visible_projects = Array.isArray(response.value) ? response.value.length : null;
  if (report.visible_projects === 0) {
    report.hints.push("当前 token 看不到任何项目，请检查网页端的 Agent 项目授权");
  }
  return { report, exitCode: EXIT_OK };
}

function printDoctor(report) {
  const lines = [
    "pm-cli 诊断",
    `  版本：${report.cli_version}`,
    `  脚本路径：${report.cli_path}`,
    `  Node：${report.node_version}`,
    `  连接来源：${report.source ? SOURCE_LABELS[report.source] : "（尚未配置）"}`,
    `  服务地址：${report.server_url ?? "（尚未配置）"}`,
    `  Token：${report.token_masked || "（尚未配置）"}`,
    `  配置文件：${report.config_path}`,
  ];
  if (report.runtime_path) lines.push(`  运行信息：${report.runtime_path}`);
  let connectivity = "未连接";
  if (report.reachable && report.status) {
    connectivity =
      report.visible_projects === null
        ? `正常（HTTP ${report.status}）`
        : `正常（HTTP ${report.status}，可见 ${report.visible_projects} 个项目）`;
  } else if (report.status) {
    connectivity = `HTTP ${report.status}`;
  }
  lines.push(`  连通性：${connectivity}`);
  lines.push(`  结论：${report.problem ?? "连接正常。"}`);
  for (const hint of report.hints) lines.push(`  - ${hint}`);
  process.stdout.write(`${lines.join("\n")}\n`);
}

// ----------------------------------------------------------------------- 入口

async function main(argv) {
  const [first, ...rest] = argv;

  if (first === undefined || first === "help" || first === "--help" || first === "-h") {
    // `pm-cli help <命令>` 与 `pm-cli <命令> --help` 等价
    if (first === "help" && rest[0] && rest[0] in COMMAND_HELP) {
      process.stdout.write(`${commandHelpText(rest[0])}\n`);
    } else {
      process.stdout.write(`${helpText()}\n`);
    }
    return EXIT_OK;
  }
  if (first === "--version" || first === "-V") {
    process.stdout.write(`${versionText()}\n`);
    return EXIT_OK;
  }
  if (first.startsWith("-")) {
    process.stderr.write(`错误：未知选项：${first}\n用 pm-cli --help 查看用法\n`);
    return EXIT_VALIDATION;
  }
  if (!(first in COMMAND_OPTIONS)) {
    process.stderr.write(`错误：未知命令：${first}\n用 pm-cli --help 查看可用命令\n`);
    return EXIT_VALIDATION;
  }
  if (rest.includes("--help") || rest.includes("-h")) {
    process.stdout.write(`${commandHelpText(first)}\n`);
    return EXIT_OK;
  }

  const { options, positionals } = parseCommand(first, rest);
  if (first === "config") return handleConfig(positionals);
  validateRequiredOptions(first, options);

  let connection;
  try {
    connection = resolveConnection();
  } catch (error) {
    process.stderr.write(
      `错误：${error.message}\n` +
        `运行 pm-cli doctor 可查看当前生效的连接来源与修复步骤。脚本路径：${scriptPath() ?? "（未知）"}\n`,
    );
    return EXIT_SERVICE_DOWN;
  }

  if (first === "doctor") {
    const { report, exitCode } = await collectDoctor();
    if (options.json === true) printJson(report);
    else printDoctor(report);
    return exitCode;
  }

  await runTaskCommand(first, options, positionals, createClient(connection));
  return EXIT_OK;
}

/** 没有 Node 时用户根本跑不到这里；版本过低给出明确指引。 */
function checkNodeVersion() {
  const major = Number(process.versions.node.split(".")[0]);
  if (Number.isFinite(major) && major < MIN_NODE_MAJOR) {
    process.stderr.write(
      `错误：pm-cli 需要 Node.js ${MIN_NODE_MAJOR} 或更高版本，当前为 ${process.version}。\n` +
        "请升级 Node.js 后重试；没有 Node.js 的环境可改为直接调用 /api/agent/* 接口。\n",
    );
    return false;
  }
  return true;
}

if (checkNodeVersion()) {
  try {
    process.exitCode = await main(process.argv.slice(2));
  } catch (error) {
    if (error instanceof ServerError) {
      process.exitCode = error.exitCode;
    } else if (error instanceof ServiceDownError) {
      process.stderr.write(`错误：${error.message}\n`);
      process.exitCode = EXIT_SERVICE_DOWN;
    } else if (error instanceof UsageError) {
      process.stderr.write(`错误：${error.message}\n`);
      process.exitCode = EXIT_VALIDATION;
    } else {
      process.stderr.write(`错误：${error?.message ?? String(error)}\n`);
      process.exitCode = EXIT_FAILURE;
    }
  }
} else {
  process.exitCode = EXIT_SERVICE_DOWN;
}
