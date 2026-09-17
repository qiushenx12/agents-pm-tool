#!/usr/bin/env node
/**
 * pm-cli skill 安装脚本（跨平台、单文件、离线可用）。
 *
 * 用途：把 pm-cli skill 写到本机某个 Agent 前端目录下。
 * 之所以做成「一个脚本」而不是压缩包：浏览器既无法指定下载位置，局域网 HTTP 页面里
 * 也不能直接写本地目录；让用户拿到这个脚本跑一次，由它自己把文件放到位最省事。
 *
 * 用法：
 *   node pm-cli-install.mjs                 自动检测已安装的前端并安装
 *   node pm-cli-install.mjs --list          只列出检测结果与候选目录
 *   node pm-cli-install.mjs --frontend codex
 *   node pm-cli-install.mjs --all           检测到的前端全部安装
 *   node pm-cli-install.mjs --dir <目录>    安装到指定目录
 *
 * --dir 给的是「前端的 skills 根目录」（会在此之下创建 pm-cli）；
 * 若该目录本身就叫 pm-cli，则直接写进去，避免多一层嵌套。
 *
 * 退出码：0 成功；2 参数不对或没有可安装的目标目录。
 *
 * 依赖：Node.js 18 或更高版本（与 pm-cli 本身一致）。
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

/** 由服务端在分发时替换为真实文件清单；模板里保持空数组，方便单独做语法检查。 */
const PAYLOAD = [];

const SKILL_NAME = "pm-cli";

function usage() {
  const frontends = (PAYLOAD?.frontends ?? []).map((item) => item.id).join("、");
  return `pm-cli skill 安装脚本

用法：
  node pm-cli-install.mjs                 自动检测已安装的前端并安装
  node pm-cli-install.mjs --list          只列出检测结果与候选目录
  node pm-cli-install.mjs --frontend <id> 安装到指定前端
  node pm-cli-install.mjs --all           检测到的前端全部安装
  node pm-cli-install.mjs --dir <目录>    安装到指定目录

--dir 请给「前端的 skills 根目录」，脚本会在其下创建 ${SKILL_NAME}；
若该目录本身就叫 ${SKILL_NAME}，则直接写进去，不再多套一层。

可用前端：${frontends || "（载荷缺失）"}
`;
}

// ---------------------------------------------------------------- 参数与路径

function parseArgs(argv) {
  const options = { list: false, all: false, frontend: null, dir: null, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (token === "--list") options.list = true;
    else if (token === "--all") options.all = true;
    else if (token === "-h" || token === "--help") options.help = true;
    else if (token === "--frontend") options.frontend = argv[++index] ?? null;
    else if (token === "--dir") options.dir = argv[++index] ?? null;
    else throw new Error(`未知参数：${token}`);
  }
  return options;
}

function expandHome(value) {
  if (value === "~") return os.homedir();
  if (value.startsWith("~/") || value.startsWith("~\\")) {
    return path.join(os.homedir(), value.slice(2));
  }
  return value;
}

/** 某个前端在本机的候选 skills 根目录（不做存在性过滤）。 */
function frontendRoots(frontend) {
  const override = frontend.env_home ? process.env[frontend.env_home] : undefined;
  const useOverride = Boolean(override && override.trim() && frontend.env_home_subpath);
  return frontend.roots.map((root) => ({
    label: root.label,
    directory: useOverride
      ? path.join(override.trim(), frontend.env_home_subpath)
      : path.join(os.homedir(), ...root.relative.split("/")),
  }));
}

/**
 * 前端是否装在本机：只看它的配置目录在不在（skill 目录安装时按需创建），
 * 否则刚装好、还没放过任何 skill 的前端会被误判为未安装。
 */
function isInstalled(directory) {
  return fs.existsSync(path.dirname(directory));
}

/** 落点：给 skills 根目录则在其下建 pm-cli；本身就叫 pm-cli 则直接用。 */
function resolveTarget(skillsRoot) {
  const normalized = path.resolve(expandHome(skillsRoot));
  return path.basename(normalized) === SKILL_NAME ? normalized : path.join(normalized, SKILL_NAME);
}

// ---------------------------------------------------------------------- 写入

function installTo(target) {
  for (const file of PAYLOAD.files) {
    const destination = path.join(target, ...file.path.split("/"));
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.writeFileSync(destination, file.content, "utf8");
    if (file.executable && process.platform !== "win32") {
      fs.chmodSync(destination, 0o755);
    }
  }
  cleanLegacy(target);
  return target;
}

/**
 * 清掉旧版安装留下的痕迹：旧的可执行文件，以及为绕过「找不到运行信息」而人为建立的
 * bin/data 目录（现在运行信息放在用户级位置，不再需要）。
 * 只删空目录或目录联接；里面有真实内容的目录不碰。
 */
function cleanLegacy(target) {
  try {
    fs.unlinkSync(path.join(target, "bin", "pm-cli.exe"));
  } catch {
    /* 不存在就跳过 */
  }
  const legacyData = path.join(target, "bin", "data");
  if (fs.existsSync(legacyData)) {
    try {
      fs.rmdirSync(legacyData);
    } catch {
      /* 里面有真实内容：留给用户自己处理 */
    }
  }
}

// -------------------------------------------------------------------- 主流程

function collectTargets() {
  const collected = [];
  for (const frontend of PAYLOAD.frontends) {
    for (const root of frontendRoots(frontend)) {
      collected.push({
        frontendId: frontend.id,
        frontendLabel: frontend.label,
        label: root.label,
        directory: root.directory,
        installed: isInstalled(root.directory),
      });
    }
  }
  return collected;
}

function candidateLines(items) {
  return `${items
    .map((item) => `  ${item.frontendId.padEnd(18, " ")} ${item.directory}`)
    .join("\n")}\n`;
}

function report(targets, headline) {
  const executable = process.platform === "win32" ? "pm-cli.cmd" : "pm-cli";
  const lines = [
    `${headline}：`,
    ...targets.map((target) => `  ${target}`),
    "",
    "下一步：",
    "  1. 重新打开终端或 Agent 前端，让它重新加载 skill。",
    `  2. 验证：node "${path.join(targets[0], "bin", "pm-cli.mjs")}" doctor`,
    `     （也可以直接运行同一目录下的 ${executable} doctor）`,
    "  本机装有 Agents PM Tool 时不需要任何连接配置；远程使用按 SKILL.md 配置一次即可。",
  ];
  process.stdout.write(`${lines.join("\n")}\n`);
}

function main(argv) {
  if (!PAYLOAD || !Array.isArray(PAYLOAD?.files) || !PAYLOAD.files.length) {
    process.stderr.write("错误：安装脚本缺少 skill 文件清单，请重新从网页端获取。\n");
    return 2;
  }

  let options;
  try {
    options = parseArgs(argv);
  } catch (error) {
    process.stderr.write(`错误：${error.message}\n\n${usage()}`);
    return 2;
  }
  if (options.help) {
    process.stdout.write(`${usage()}\n`);
    return 0;
  }

  try {
    if (options.dir) {
      const target = installTo(resolveTarget(options.dir));
      report([target], "已按指定目录安装");
      return 0;
    }

    const detected = collectTargets();
    const available = detected.filter((item) => item.installed);

    if (options.list) {
      process.stdout.write(`pm-cli skill 位置检测（脚本版本 ${PAYLOAD.version}）\n\n`);
      if (!available.length) {
        process.stdout.write("未检测到任何 Agent 前端，请用 --dir <目录> 指定其 skills 根目录。\n");
      } else {
        for (const item of available) {
          const installed = fs.existsSync(path.join(item.directory, SKILL_NAME));
          process.stdout.write(`  ${item.label}\n    ${item.directory}${installed ? "  （已安装）" : ""}\n`);
        }
      }
      return 0;
    }

    if (options.frontend) {
      const matched = available.filter((item) => item.frontendId === options.frontend);
      if (!matched.length) {
        const known = PAYLOAD.frontends.find((item) => item.id === options.frontend);
        if (!known) {
          process.stderr.write(
            `错误：未知前端 ${options.frontend}。可用：${PAYLOAD.frontends.map((item) => item.id).join("、")}\n`,
          );
        } else {
          process.stderr.write(
            `错误：未检测到 ${known.label}（其配置目录不存在）。请先安装该前端，或用 --dir <目录> 指定目录。\n`,
          );
        }
        return 2;
      }
      const targets = matched.map((item) => installTo(resolveTarget(item.directory)));
      report(
        targets,
        `已安装到 ${matched.map((item) => item.label).join("、")}`,
      );
      return 0;
    }

    if (options.all) {
      if (!available.length) {
        process.stderr.write("错误：未检测到任何 Agent 前端，请用 --dir <目录> 指定目标目录。\n");
        return 2;
      }
      const targets = available.map((item) => installTo(resolveTarget(item.directory)));
      report(targets, "已安装到检测到的全部前端");
      return 0;
    }

    if (available.length === 0) {
      process.stderr.write(
        "错误：未检测到任何 Agent 前端。请先安装要用的前端，或用 --dir <目录> 指定其 skills 根目录。\n\n" +
          candidateLines(detected),
      );
      return 2;
    }
    if (available.length > 1) {
      process.stderr.write(
        "检测到多个 Agent 前端。为避免装错位置，请明确指定目标：\n\n" +
          candidateLines(available) +
          "\n  node pm-cli-install.mjs --frontend <id>   只装其中一个\n" +
          "  node pm-cli-install.mjs --all             全部安装\n",
      );
      return 2;
    }

    const only = available[0];
    report([installTo(resolveTarget(only.directory))], `已安装到 ${only.label}`);
    return 0;
  } catch (error) {
    process.stderr.write(`错误：写入失败：${error?.message ?? String(error)}\n`);
    return 2;
  }
}

process.exitCode = main(process.argv.slice(2));
