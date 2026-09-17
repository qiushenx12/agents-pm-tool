// @vitest-environment node
/**
 * pm-cli 行为测试。
 *
 * pm-cli 是 skill 里的一个 Node 脚本（不再有可执行二进制），所以直接以子进程方式跑它，
 * 与用户和 Agent 的实际用法一致。除了解析与本地文件行为，这里还起桩服务验证 HTTP 交互、
 * 退出码与 --json 结构没有漂移。
 *
 * 注意：子进程一律用异步 spawn。桩服务就跑在本进程里，spawnSync 会把事件循环堵死，
 * 服务端永远回不了响应，测试会直接卡住。
 */
import { spawn } from "node:child_process";
import { createServer, type IncomingMessage, type Server } from "node:http";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, afterEach, describe, expect, it } from "vitest";

const projectRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const cli = path.join(projectRoot, "pm-cli-skill", "bin", "pm-cli.mjs");
const installerTemplate = path.join(projectRoot, "pm-cli-skill", "installer.mjs");

interface RunResult {
  status: number;
  stdout: string;
  stderr: string;
}

/** 每个用例都用一个独立的用户级配置目录，避免碰到开发机真实的 cli.json 与运行信息。 */
const sandboxHome = mkdtempSync(path.join(tmpdir(), "pm-cli-home-"));

function runCli(
  script: string,
  args: string[],
  env: NodeJS.ProcessEnv = {},
): Promise<RunResult> {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [script, ...args], {
      // 必须带上 process.env：Windows 上少了 SystemRoot 之类的变量，子进程会起不来。
      env: {
        ...process.env,
        APPDATA: sandboxHome,
        HOME: sandboxHome,
        USERPROFILE: sandboxHome,
        ...env,
      },
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += String(chunk);
    });
    child.stderr.on("data", (chunk) => {
      stderr += String(chunk);
    });
    child.on("close", (code) => resolve({ status: code ?? -1, stdout, stderr }));
  });
}

const run = (args: string[], env: NodeJS.ProcessEnv = {}) =>
  runCli(cli, args, env);

interface StubOutcome {
  status?: number;
  json?: unknown;
  binary?: Uint8Array;
  filename?: string;
}

type StubHandler = (request: IncomingMessage) => StubOutcome;

interface StubServer {
  base: string;
  requests: { method: string; url: string; authorization?: string }[];
}

const servers: Server[] = [];

async function startStub(
  handler?: StubHandler,
  fallback: StubOutcome = {
    json: [{ name: "default-project", local_path: "D:/x", git_url: "" }],
  },
): Promise<StubServer> {
  const requests: StubServer["requests"] = [];
  const server = createServer((request, response) => {
    requests.push({
      method: request.method ?? "",
      url: request.url ?? "",
      authorization: request.headers.authorization,
    });
    const outcome = handler?.(request) ?? fallback;
    if (outcome.binary) {
      response.writeHead(outcome.status ?? 200, {
        "content-type": "application/octet-stream",
        "content-disposition": `attachment; filename*=UTF-8''${encodeURIComponent(
          outcome.filename ?? "file.bin",
        )}`,
      });
      response.end(outcome.binary);
      return;
    }
    response.writeHead(outcome.status ?? 200, {
      "content-type": "application/json",
    });
    response.end(JSON.stringify(outcome.json ?? {}));
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  servers.push(server);
  const { port } = server.address() as AddressInfo;
  return { base: `http://127.0.0.1:${port}`, requests };
}

/** 连到桩服务所需的调用环境。 */
const remote = (base: string): NodeJS.ProcessEnv => ({
  PM_SERVER_URL: base,
  PM_AGENT_TOKEN: "token",
});

const emptyDir = (prefix: string) => mkdtempSync(path.join(tmpdir(), prefix));

afterEach(() => {
  // 清掉可能写入的配置，避免用例之间互相影响连接来源判定。
  const configFile = path.join(sandboxHome, "agents-pm-tool", "cli.json");
  if (existsSync(configFile)) {
    writeFileSync(configFile, "{}\n", "utf8");
  }
});

afterAll(async () => {
  // 桩服务必须收干净：pm-cli 用的是 keep-alive 连接，只 close() 会一直等下去。
  await Promise.all(
    servers.splice(0).map(
      (server) =>
        new Promise<void>((resolve) => {
          server.closeAllConnections?.();
          server.close(() => resolve());
        }),
    ),
  );
});

describe("帮助与参数", () => {
  it("默认帮助先说明如何找到 pm-cli", async () => {
    const result = await run(["--help"]);
    expect(result.status).toBe(0);
    expect(result.stdout).toContain("先找到 pm-cli");
    expect(result.stdout).toContain("脚本在 skill 目录的 bin/ 下");
    expect(result.stdout).toContain(path.join("bin", "pm-cli.mjs"));
    expect(result.stdout).toContain("Node.js 18");
    // 统一口径：不再出现「随包安装/写进 PATH/解压压缩包」这类旧说法。
    expect(result.stdout).not.toContain("解压");
    expect(result.stdout).not.toContain("加入用户 PATH");
  });

  it("子命令帮助可读，且与总帮助口径一致", async () => {
    const result = await run(["list", "--help"]);
    expect(result.status).toBe(0);
    expect(result.stdout).toContain("pm-cli list");
    expect(result.stdout).toContain("--submitter");
    expect((await run(["help", "doctor"])).stdout).toContain("pm-cli doctor");
  });

  it("版本号来自 skill 的 VERSION", async () => {
    const bundled = readFileSync(
      path.join(projectRoot, "pm-cli-skill", "VERSION"),
      "utf8",
    ).trim();
    expect((await run(["--version"])).stdout.trim()).toBe(`pm-cli ${bundled}`);
  });

  it("未知命令与未知选项都按参数错误退出（2）", async () => {
    const unknownCommand = await run(["frobnicate"]);
    expect(unknownCommand.status).toBe(2);
    expect(unknownCommand.stderr).toContain("未知命令");

    const unknownOption = await run(["get", "--bogus", "1"]);
    expect(unknownOption.status).toBe(2);
    expect(unknownOption.stderr).toContain("未知选项");

    const missingRequired = await run(["status", "202601010101010001"]);
    expect(missingRequired.status).toBe(2);
    expect(missingRequired.stderr).toContain("--to 为必填项");
  });
});

describe("连接发现", () => {
  it("环境变量必须成对，缺一个要点名缺哪个并给出出路", async () => {
    const result = await run(["projects"], {
      PM_SERVER_URL: "http://127.0.0.1:1",
    });
    expect(result.status).toBe(3);
    expect(result.stderr).toContain("PM_AGENT_TOKEN");
    expect(result.stderr).toContain("PM_SERVER_URL");
  });

  it("未配置时给出可执行的配置命令，不抛内部文件名", async () => {
    const result = await run(["projects"]);
    expect(result.status).toBe(3);
    expect(result.stderr).toContain("config set server-url");
    expect(result.stderr).toContain("config set token");
    expect(result.stderr).not.toContain("runtime.json");
  });

  it("用户配置可保存并读回，token 打码", async () => {
    expect(
      (await run(["config", "set", "server-url", "http://127.0.0.1:17890/"]))
        .status,
    ).toBe(0);
    expect((await run(["config", "set", "token", "token-value"])).status).toBe(0);
    const shown = await run(["config", "show"]);
    expect(shown.stdout).toContain("http://127.0.0.1:17890");
    expect(shown.stdout).toContain("********");
    expect(shown.stdout).not.toContain("token-value");
  });

  it("非法 server-url 直接拒绝", async () => {
    const result = await run(["config", "set", "server-url", "not-a-url"]);
    expect(result.status).toBe(2);
    expect(result.stderr).toContain("server-url");
  });

  it("本机运行信息兜底：PM_DATA_DIR 里的 runtime.json 零配置可用", async () => {
    const stub = await startStub();
    const dataDir = emptyDir("pm-cli-data-");
    writeFileSync(
      path.join(dataDir, "runtime.json"),
      JSON.stringify({ port: Number(new URL(stub.base).port), token: "runtime-token" }),
      "utf8",
    );
    const result = await run(["doctor", "--json"], { PM_DATA_DIR: dataDir });
    expect(result.status).toBe(0);
    const report = JSON.parse(result.stdout);
    expect(report.source).toBe("runtime");
    expect(report.server_url).toBe(stub.base);
    expect(report.reachable).toBe(true);
    expect(report.visible_projects).toBe(1);
    // token 只以脱敏形式出现
    expect(result.stdout).not.toContain("runtime-token");
  });

  it("用户级运行信息：应用写出的那一份优先于旧版 skill 目录旁的 data/", async () => {
    const stub = await startStub();
    const port = Number(new URL(stub.base).port);
    const configDir = path.join(sandboxHome, "agents-pm-tool");
    mkdirSync(configDir, { recursive: true });
    writeFileSync(
      path.join(configDir, "runtime.json"),
      JSON.stringify({ port, token: "pointer-token" }),
      "utf8",
    );
    const result = await run(["doctor", "--json"]);
    expect(result.status).toBe(0);
    const report = JSON.parse(result.stdout);
    expect(report.source).toBe("runtime");
    expect(report.runtime_path).toBe(path.join(configDir, "runtime.json"));
    expect(report.server_url).toBe(stub.base);
    writeFileSync(path.join(configDir, "runtime.json"), "{}\n", "utf8");
  });

  it("用户配置盖住了本机运行信息时，doctor 要点明端口不一致", async () => {
    const configDir = path.join(sandboxHome, "agents-pm-tool");
    mkdirSync(configDir, { recursive: true });
    // 配置指向一个没人监听的端口，运行信息里是另一个端口
    writeFileSync(
      path.join(configDir, "cli.json"),
      JSON.stringify({ server_url: "http://127.0.0.1:1", token: "t" }),
      "utf8",
    );
    writeFileSync(
      path.join(configDir, "runtime.json"),
      JSON.stringify({ port: 17890, token: "pointer-token" }),
      "utf8",
    );
    const result = await run(["doctor", "--json"]);
    expect(result.status).toBe(3);
    const report = JSON.parse(result.stdout);
    expect(report.source).toBe("config");
    expect(report.hints.join("\n")).toContain("17890");
    writeFileSync(path.join(configDir, "runtime.json"), "{}\n", "utf8");
  });

  it("doctor 在连不上时仍返回完整报告并退出 3", async () => {
    const result = await run(["doctor", "--json"], {
      PM_SERVER_URL: "http://127.0.0.1:1",
      PM_AGENT_TOKEN: "t",
      PM_DATA_DIR: emptyDir("pm-cli-empty-"),
    });
    expect(result.status).toBe(3);
    const report = JSON.parse(result.stdout);
    expect(report.reachable).toBe(false);
    expect(report.problem).toContain("无法连接");
    expect(report.hints.length).toBeGreaterThan(0);
    expect(report.cli_path).toContain("pm-cli.mjs");
  });
});

describe("HTTP 交互", () => {
  it("list 的人类可读输出与 JSON 输出", async () => {
    const stub = await startStub(() => ({
      json: [
        {
          id: "202609162008000000",
          project: "agents-launcher",
          type: "BUG",
          status: "进行中",
          priority: "高",
          submitter: "Agent",
          submitter_name: "Agent（主机）",
          description: "多行\n描述",
        },
      ],
    }));
    const env = remote(stub.base);

    const human = await run(["list"], env);
    expect(human.status).toBe(0);
    expect(human.stdout).toContain("202609162008000000");
    expect(human.stdout).toContain("[agents-launcher]");
    expect(human.stdout).toContain("Agent（主机）");
    // 描述里的换行会压成空格，保持一行一条
    expect(human.stdout.trim().split("\n")).toHaveLength(1);

    const json = await run(["list", "--json"], env);
    expect(JSON.parse(json.stdout)[0].id).toBe("202609162008000000");

    await run(["list", "--project", "gqsj", "--status", "进行中"], env);
    const lastUrl = decodeURIComponent(stub.requests.at(-1)?.url ?? "");
    expect(lastUrl).toContain("project=gqsj");
    expect(lastUrl).toContain("status=进行中");
    expect(stub.requests.at(-1)?.authorization).toBe("Bearer token");
  });

  it("空列表给出明确提示而不是空白", async () => {
    const stub = await startStub(undefined, { json: [] });
    expect((await run(["list"], remote(stub.base))).stdout).toContain(
      "（无匹配任务）",
    );
  });

  it("create 三必填，缺一个直接报错且不发请求", async () => {
    const stub = await startStub(undefined, { json: { id: "x" } });
    const env = remote(stub.base);

    const missing = await run(["create", "--project", "p"], env);
    expect(missing.status).toBe(2);
    expect(missing.stderr).toContain("--type");
    expect(stub.requests).toHaveLength(0);

    const created = await run(
      ["create", "--project", "p", "--type", "BUG", "--description", "d"],
      env,
    );
    expect(created.status).toBe(0);
    expect(stub.requests.at(-1)?.method).toBe("POST");
  });

  it("401 按鉴权失败退出（2），并给出重新生成 token 的提示", async () => {
    const stub = await startStub(undefined, {
      status: 401,
      json: { error: { code: "unauthorized", message: "token 无效" } },
    });
    const result = await run(["projects"], remote(stub.base));
    expect(result.status).toBe(2);
    expect(result.stderr).toContain("token 无效");
    expect(result.stderr).toContain("重新生成 token");
  });

  it("服务端 5xx 按一般失败退出（1）", async () => {
    const stub = await startStub(undefined, {
      status: 500,
      json: { error: { code: "internal", message: "内部错误" } },
    });
    expect((await run(["projects"], remote(stub.base))).status).toBe(1);
  });

  it("projects 展示本地路径与 Git 地址", async () => {
    const stub = await startStub();
    const result = await run(["projects"], remote(stub.base));
    expect(result.stdout).toContain("default-project");
    expect(result.stdout).toContain("本地路径：D:/x");
  });
});

describe("附件下载", () => {
  it("按服务端文件名落盘、默认不覆盖、--force 可覆盖", async () => {
    const fixture = Buffer.from("attachment fixture");
    const stub = await startStub((request) =>
      request.url?.startsWith("/api/agent/attachments/")
        ? { binary: fixture, filename: "需求说明.txt" }
        : { json: [] },
    );
    const env = remote(stub.base);
    const outputDir = emptyDir("pm-cli-out-");

    const first = await run(
      ["download", "att-1", "--output", outputDir, "--json"],
      env,
    );
    expect(first.status).toBe(0);
    const meta = JSON.parse(first.stdout);
    expect(meta.filename).toBe("需求说明.txt");
    expect(meta.size).toBe(fixture.length);
    expect(readFileSync(path.join(outputDir, "需求说明.txt"))).toEqual(fixture);

    const second = await run(["download", "att-1", "--output", outputDir], env);
    expect(second.status).toBe(2);
    expect(second.stderr).toContain("--force");

    expect(
      (await run(["download", "att-1", "--output", outputDir, "--force"], env))
        .status,
    ).toBe(0);

    const missingDir = await run(
      ["download", "att-1", "--output", path.join(outputDir, "nope", "x.txt")],
      env,
    );
    expect(missingDir.status).toBe(2);
    expect(missingDir.stderr).toContain("输出目录不存在");
  });
});

describe("skill 安装脚本", () => {
  /** 用真实模板 + 一份小载荷拼出可执行的安装脚本。 */
  function writeInstaller(): string {
    const payload = {
      name: "pm-cli",
      version: "1.0.0",
      directory: "pm-cli",
      frontends: [
        {
          id: "claude_code",
          label: "Claude Code",
          roots: [{ relative: ".claude/skills", label: "Claude Code" }],
          windows_paths: ["%USERPROFILE%\\.claude\\skills"],
          macos_paths: ["~/.claude/skills"],
        },
        {
          id: "codex",
          label: "Codex",
          roots: [
            { relative: ".agents/skills", label: "Codex" },
            { relative: ".codex/skills", label: "Codex（旧版目录）" },
          ],
          windows_paths: ["%USERPROFILE%\\.agents\\skills"],
          macos_paths: ["~/.agents/skills"],
        },
      ],
      files: [
        { path: "SKILL.md", content: "# doc\n", executable: false },
        { path: "bin/pm-cli.mjs", content: "// cli\n", executable: false },
      ],
    };
    const source = readFileSync(installerTemplate, "utf8").replace(
      "const PAYLOAD = [];",
      `const PAYLOAD = ${JSON.stringify(payload)};`,
    );
    const script = path.join(emptyDir("pm-cli-inst-"), "install.mjs");
    writeFileSync(script, source, "utf8");
    return script;
  }

  const runInstaller = (args: string[], env: NodeJS.ProcessEnv = {}) =>
    runCli(writeInstaller(), args, env);

  it("--dir 把 skill 写进目标目录下的 pm-cli", async () => {
    const home = emptyDir("pm-cli-target-");
    const result = await runInstaller(["--dir", home]);
    expect(result.status).toBe(0);
    expect(existsSync(path.join(home, "pm-cli", "SKILL.md"))).toBe(true);
    expect(existsSync(path.join(home, "pm-cli", "bin", "pm-cli.mjs"))).toBe(true);
  });

  it("选中的目录本身就叫 pm-cli 时不再多套一层", async () => {
    const home = emptyDir("pm-cli-target-");
    const target = path.join(home, "pm-cli");
    expect((await runInstaller(["--dir", target])).status).toBe(0);
    expect(existsSync(path.join(target, "SKILL.md"))).toBe(true);
    expect(existsSync(path.join(target, "pm-cli", "SKILL.md"))).toBe(false);
  });

  it("检不到任何前端时提示用 --dir，并列出候选目录", async () => {
    const home = emptyDir("pm-cli-scan-");
    const result = await runInstaller([], { HOME: home, USERPROFILE: home });
    expect(result.status).toBe(2);
    expect(result.stderr).toContain("--dir");
    expect(result.stderr).toContain(".claude");
  });

  it("检到单个前端时默认安装到它，--list 可先看候选", async () => {
    const home = emptyDir("pm-cli-scan-");
    mkdirSync(path.join(home, ".claude"), { recursive: true });
    const env = { HOME: home, USERPROFILE: home };

    const listed = await runInstaller(["--list"], env);
    expect(listed.status).toBe(0);
    expect(listed.stdout).toContain("Claude Code");

    const installed = await runInstaller([], env);
    expect(installed.status).toBe(0);
    expect(
      existsSync(
        path.join(home, ".claude", "skills", "pm-cli", "bin", "pm-cli.mjs"),
      ),
    ).toBe(true);
  });

  it("检到多个前端时要求明确选择，--all 才全部安装", async () => {
    const home = emptyDir("pm-cli-scan-");
    mkdirSync(path.join(home, ".claude"), { recursive: true });
    mkdirSync(path.join(home, ".agents"), { recursive: true });
    const env = { HOME: home, USERPROFILE: home };

    const ambiguous = await runInstaller([], env);
    expect(ambiguous.status).toBe(2);
    expect(ambiguous.stderr).toContain("--frontend");
    expect(ambiguous.stderr).toContain("--all");

    expect((await runInstaller(["--all"], env)).status).toBe(0);
    expect(
      existsSync(
        path.join(home, ".claude", "skills", "pm-cli", "bin", "pm-cli.mjs"),
      ),
    ).toBe(true);
    expect(
      existsSync(
        path.join(home, ".agents", "skills", "pm-cli", "bin", "pm-cli.mjs"),
      ),
    ).toBe(true);
  });

  it("未知 --frontend 报错并列出可用值", async () => {
    const home = emptyDir("pm-cli-scan-");
    const result = await runInstaller(["--frontend", "amp"], {
      HOME: home,
      USERPROFILE: home,
    });
    expect(result.status).toBe(2);
    expect(result.stderr).toContain("claude_code");
  });

  /** 连配置一起装的场景：给一个干净的 HOME，让 userConfigDir 落在里面。 */
  function connectionEnv(home: string): NodeJS.ProcessEnv {
    return {
      HOME: home,
      USERPROFILE: home,
      APPDATA: home,
      XDG_CONFIG_HOME: home,
    };
  }

  /** 从脚本输出里取配置路径（跨平台，不重复实现那边的路径规则）。 */
  function configFrom(result: { stdout: string }): string {
    const matched = /连接配置已写入：(.+)/.exec(result.stdout);
    expect(matched, `输出里没有配置路径：${result.stdout}`).toBeTruthy();
    return matched![1]!.trim();
  }

  it("--server-url / --token 顺带写好连接配置，一条命令装完即可用", async () => {
    const home = emptyDir("pm-cli-conn-");
    mkdirSync(path.join(home, ".claude"), { recursive: true });
    const result = await runInstaller(
      ["--all", "--server-url", "http://127.0.0.1:17890/", "--token", "tok-123"],
      connectionEnv(home),
    );
    expect(result.status).toBe(0);
    expect(result.stdout).toContain("连接配置已写入");
    const config = JSON.parse(readFileSync(configFrom(result), "utf8"));
    // 结尾斜杠与 config set 一样会被去掉
    expect(config.server_url).toBe("http://127.0.0.1:17890");
    expect(config.token).toBe("tok-123");
  });

  it("只给 --server-url 时合并已有 token，不整份覆盖", async () => {
    const home = emptyDir("pm-cli-conn-");
    mkdirSync(path.join(home, ".claude"), { recursive: true });
    const env = connectionEnv(home);
    const first = await runInstaller(
      ["--all", "--server-url", "http://127.0.0.1:1", "--token", "old-token"],
      env,
    );
    const second = await runInstaller(
      ["--all", "--server-url", "http://127.0.0.1:2"],
      env,
    );
    const config = JSON.parse(
      readFileSync(configFrom(second), "utf8"),
    );
    expect(configFrom(second)).toBe(configFrom(first));
    expect(config.server_url).toBe("http://127.0.0.1:2");
    expect(config.token).toBe("old-token");
  });

  it("只给了地址、没有 token 时，提示去哪生成", async () => {
    const home = emptyDir("pm-cli-conn-");
    mkdirSync(path.join(home, ".claude"), { recursive: true });
    const result = await runInstaller(
      ["--all", "--server-url", "http://127.0.0.1:17890"],
      connectionEnv(home),
    );
    expect(result.status).toBe(0);
    expect(result.stdout).toContain("还缺 Agent token");
    expect(result.stdout).toContain("我的 Agent 访问");
  });

  it("--list 只做检测，不写连接配置", async () => {
    const home = emptyDir("pm-cli-conn-");
    mkdirSync(path.join(home, ".claude"), { recursive: true });
    const result = await runInstaller(
      ["--list", "--server-url", "http://127.0.0.1:17890", "--token", "t"],
      connectionEnv(home),
    );
    expect(result.status).toBe(0);
    expect(result.stdout).not.toContain("连接配置已写入");
  });

  it("非法 --server-url 直接拒绝，不落盘", async () => {
    const home = emptyDir("pm-cli-conn-");
    mkdirSync(path.join(home, ".claude"), { recursive: true });
    const result = await runInstaller(
      ["--all", "--server-url", "127.0.0.1:17890"],
      connectionEnv(home),
    );
    expect(result.status).toBe(2);
    expect(result.stderr).toContain("server-url");
  });
});
