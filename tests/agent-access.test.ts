// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, type App } from "vue";
import AgentAccessDialog from "@/grid-app/components/users/AgentAccessDialog.vue";
import { api } from "@/grid-app/api/client";
import type { LocalSkillTarget, SkillPayload, User } from "@/shared/types";

vi.mock("@/grid-app/api/client", () => ({
  api: {
    getAgentAccess: vi.fn(),
    listLocalSkills: vi.fn(),
    installLocalSkills: vi.fn(),
    openLocalSkillDirectory: vi.fn(),
    getSkillPayload: vi.fn(),
    regenerateAgentToken: vi.fn(),
    revokeAgentToken: vi.fn(),
    patchUser: vi.fn(),
  },
}));

let root: HTMLDivElement | undefined;
let app: App | undefined;

afterEach(() => {
  app?.unmount();
  root?.remove();
  document
    .querySelectorAll(".dialog-mask")
    .forEach((element) => element.remove());
  vi.clearAllMocks();
});

it("opens targets and installs Codex and Claude Code skills independently", async () => {
  const user: User = {
    id: "host",
    username: "主机",
    role: "super_admin",
    created_at: "2026-09-10 10:00:00",
    disabled: false,
    is_host: true,
  };
  const targets: LocalSkillTarget[] = [
    {
      frontend_id: "codex",
      frontend: "Codex",
      path: "C:/Users/test/.agents/skills/pm-cli",
      installed: false,
      version: null,
    },
    {
      frontend_id: "claude_code",
      frontend: "Claude Code",
      path: "C:/Users/test/.claude/skills/pm-cli",
      installed: false,
      version: null,
    },
  ];
  vi.mocked(api.getAgentAccess).mockResolvedValue({
    server_url: "http://127.0.0.1:17890",
    token: "host-token",
    access_instructions: "本机 pm-cli 会自动读取连接信息。",
  });
  vi.mocked(api.listLocalSkills).mockResolvedValue(targets);
  vi.mocked(api.openLocalSkillDirectory).mockResolvedValue(undefined);
  vi.mocked(api.installLocalSkills).mockImplementation(async (frontend) =>
    targets.map((target) => ({
      ...target,
      installed: target.frontend_id === frontend,
      version: target.frontend_id === frontend ? "1.0.0" : null,
    })),
  );

  root = document.createElement("div");
  document.body.append(root);
  app = createApp(AgentAccessDialog, { user });
  app.mount(root);

  await vi.waitFor(() =>
    expect(
      document.querySelector<HTMLButtonElement>(
        '[data-frontend="codex"] .skill-install-button',
      )?.disabled,
    ).toBe(false),
  );
  document
    .querySelector<HTMLButtonElement>(
      '[data-frontend="codex"] .skill-open-button',
    )!
    .click();
  await vi.waitFor(() =>
    expect(api.openLocalSkillDirectory).toHaveBeenCalledWith("codex"),
  );
  await vi.waitFor(() =>
    expect(
      document.querySelector<HTMLButtonElement>(
        '[data-frontend="codex"] .skill-install-button',
      )?.disabled,
    ).toBe(false),
  );
  document
    .querySelector<HTMLButtonElement>(
      '[data-frontend="codex"] .skill-install-button',
    )!
    .click();
  await vi.waitFor(() =>
    expect(api.installLocalSkills).toHaveBeenCalledWith("codex"),
  );
  await vi.waitFor(() =>
    expect(
      document.querySelector<HTMLButtonElement>(
        '[data-frontend="claude_code"] .skill-install-button',
      )?.disabled,
    ).toBe(false),
  );
  document
    .querySelector<HTMLButtonElement>(
      '[data-frontend="claude_code"] .skill-install-button',
    )!
    .click();
  await vi.waitFor(() =>
    expect(api.installLocalSkills).toHaveBeenCalledWith("claude_code"),
  );
});

it("renders and installs the WorkBuddy skill target", async () => {
  const user: User = {
    id: "host",
    username: "主机",
    role: "super_admin",
    created_at: "2026-09-10 10:00:00",
    disabled: false,
    is_host: true,
  };
  const workbuddy: LocalSkillTarget = {
    frontend_id: "workbuddy",
    frontend: "WorkBuddy",
    path: "C:/Users/test/.workbuddy/skills/pm-cli",
    installed: false,
    version: null,
  };
  vi.mocked(api.getAgentAccess).mockResolvedValue({
    server_url: "http://127.0.0.1:17890",
    token: "host-token",
    access_instructions: "本机 pm-cli 会自动读取连接信息。",
  });
  vi.mocked(api.listLocalSkills).mockResolvedValue([workbuddy]);
  vi.mocked(api.installLocalSkills).mockResolvedValue([
    { ...workbuddy, installed: true, version: "1.0.0" },
  ]);

  root = document.createElement("div");
  document.body.append(root);
  app = createApp(AgentAccessDialog, { user });
  app.mount(root);

  await vi.waitFor(() =>
    expect(
      document.querySelector('[data-frontend="workbuddy"] .skill-path')?.textContent,
    ).toContain(".workbuddy"),
  );
  document
    .querySelector<HTMLButtonElement>(
      '[data-frontend="workbuddy"] .skill-install-button',
    )!
    .click();
  await vi.waitFor(() =>
    expect(api.installLocalSkills).toHaveBeenCalledWith("workbuddy"),
  );
  await vi.waitFor(() =>
    expect(
      document.querySelector('[data-frontend="workbuddy"] .skill-state')?.textContent,
    ).toContain("已就绪"),
  );
});

it("renders and installs the DeepSeek Harness skill target", async () => {
  const user: User = {
    id: "host",
    username: "主机",
    role: "super_admin",
    created_at: "2026-09-10 10:00:00",
    disabled: false,
    is_host: true,
  };
  const harness: LocalSkillTarget = {
    frontend_id: "deepseek_harness",
    frontend: "DeepSeek Harness",
    path: "C:/Users/test/.dsh/skills/pm-cli",
    installed: false,
    version: null,
  };
  vi.mocked(api.getAgentAccess).mockResolvedValue({
    server_url: "http://127.0.0.1:17890",
    token: "host-token",
    access_instructions: "本机 pm-cli 会自动读取连接信息。",
  });
  vi.mocked(api.listLocalSkills).mockResolvedValue([harness]);
  vi.mocked(api.installLocalSkills).mockResolvedValue([
    { ...harness, installed: true, version: "1.0.0" },
  ]);

  root = document.createElement("div");
  document.body.append(root);
  app = createApp(AgentAccessDialog, { user });
  app.mount(root);

  await vi.waitFor(() =>
    expect(
      document.querySelector('[data-frontend="deepseek_harness"] .skill-path')
        ?.textContent,
    ).toContain(".dsh"),
  );
  expect(
    document.querySelector('[data-frontend="deepseek_harness"]')?.textContent,
  ).toContain("DeepSeek Harness");
  document
    .querySelector<HTMLButtonElement>(
      '[data-frontend="deepseek_harness"] .skill-install-button',
    )!
    .click();
  await vi.waitFor(() =>
    expect(api.installLocalSkills).toHaveBeenCalledWith("deepseek_harness"),
  );
  await vi.waitFor(() =>
    expect(
      document.querySelector('[data-frontend="deepseek_harness"] .skill-state')
        ?.textContent,
    ).toContain("已就绪"),
  );
});

it("renders each Agent frontend with its official logo mark", async () => {
  const user: User = {
    id: "host",
    username: "主机",
    role: "super_admin",
    created_at: "2026-09-10 10:00:00",
    disabled: false,
    is_host: true,
  };
  vi.mocked(api.getAgentAccess).mockResolvedValue({
    server_url: "http://127.0.0.1:17890",
    token: "host-token",
    access_instructions: "本机 pm-cli 会自动读取连接信息。",
  });
  vi.mocked(api.listLocalSkills).mockResolvedValue([]);

  root = document.createElement("div");
  document.body.append(root);
  app = createApp(AgentAccessDialog, { user });
  app.mount(root);

  await vi.waitFor(() =>
    expect(document.querySelectorAll(".skill-frontend-card").length).toBe(7),
  );
  for (const frontend of [
    "codex",
    "claude_code",
    "workbuddy",
    "opencode",
    "cursor",
    "pi",
    "deepseek_harness",
  ]) {
    const mark = document.querySelector(`[data-frontend="${frontend}"] .skill-frontend-mark`)!;
    // 用真实矢量标识替换占位文字，卡片上不再出现 "CX"/"CL"/"WB" 之类的字母占位。
    expect(mark.textContent?.trim()).toBe("");
    const svg = mark.querySelector("svg")!;
    expect(svg.getAttribute("viewBox")).toBeTruthy();
    const paths = svg.querySelectorAll("path");
    expect(paths.length).toBeGreaterThan(0);
    // opencode 的方形标识是三者里最短的路径（31 个字符），仍远长于任何占位符。
    expect(
      [...paths].reduce((total, path) => total + (path.getAttribute("d")?.length ?? 0), 0),
    ).toBeGreaterThan(20);
    expect(svg.getAttribute("fill")).toBe("currentColor");
  }
});

// 非主机用户：服务端碰不到对方电脑，网页直接把 skill 写进用户选中的目录。
const remoteUser: User = {
  id: "u-1",
  username: "小明",
  role: "user",
  created_at: "2026-09-10 10:00:00",
  disabled: false,
  is_host: false,
};

const skillPayload: SkillPayload = {
  name: "pm-cli",
  version: "1.0.0",
  directory: "pm-cli",
  frontends: [
    {
      id: "claude_code",
      label: "Claude Code",
      roots: [{ relative: ".claude/skills", label: "Claude Code" }],
      windows_paths: ["%USERPROFILE%\.claude\skills"],
      macos_paths: ["~/.claude/skills"],
    },
  ],
  files: [
    { path: "SKILL.md", content: "# doc\n", executable: false },
    { path: "bin/pm-cli.mjs", content: "// cli\n", executable: false },
  ],
};

interface FakeDirectory {
  name: string;
  files: Map<string, string>;
  dirs: Map<string, FakeDirectory>;
}

function makeDirectory(name: string): FakeDirectory {
  return { name, files: new Map(), dirs: new Map() };
}

function directoryHandle(directory: FakeDirectory) {
  return {
    name: directory.name,
    getDirectoryHandle: async (child: string) => {
      if (!directory.dirs.has(child)) {
        directory.dirs.set(child, makeDirectory(child));
      }
      return directoryHandle(directory.dirs.get(child)!);
    },
    getFileHandle: async (filename: string) => ({
      createWritable: async () => ({
        write: async (data: string) => {
          directory.files.set(filename, data);
        },
        close: async () => {},
      }),
    }),
  };
}

function setPicker(picker: unknown) {
  Object.defineProperty(window, "showDirectoryPicker", {
    configurable: true,
    writable: true,
    value: picker,
  });
}

function mountRemoteDialog() {
  vi.mocked(api.getAgentAccess).mockResolvedValue({
    server_url: "http://192.168.1.9:17890",
    token: "remote-token",
    access_instructions: "远程主机：需要配置一次连接。",
  });
  vi.mocked(api.getSkillPayload).mockResolvedValue(skillPayload);
  root = document.createElement("div");
  document.body.append(root);
  app = createApp(AgentAccessDialog, { user: remoteUser });
  app.mount(root);
}

it("shows per-frontend directory hints and the install-script fallback", async () => {
  setPicker(undefined);
  mountRemoteDialog();

  await vi.waitFor(() =>
    expect(
      document.querySelector('[data-frontend="claude_code"] .skill-path')
        ?.textContent,
    ).toContain("%USERPROFILE%\.claude\skills"),
  );
  // 另一套系统的写法在切换后展示
  document
    .querySelectorAll<HTMLButtonElement>(".skill-os-switch button")[1]!
    .click();
  await vi.waitFor(() =>
    expect(
      document.querySelector('[data-frontend="claude_code"] .skill-path')
        ?.textContent,
    ).toContain("~/.claude/skills"),
  );

  const body = document.body.textContent ?? "";
  expect(body).toContain("/api/agent/skill/install.mjs");
  expect(body).toContain("选择目录并写入");
  // 浏览器不支持直接写目录时，按钮不可用并给出说明
  expect(
    document.querySelector<HTMLButtonElement>(
      '[data-frontend="claude_code"] .skill-install-button',
    )?.disabled,
  ).toBe(true);
  expect(body).toContain("安装脚本");
  delete (window as unknown as { showDirectoryPicker?: unknown })
    .showDirectoryPicker;
});

it("writes the whole skill into the directory the user picks", async () => {
  const picked = makeDirectory("skills");
  setPicker(async () => directoryHandle(picked));
  mountRemoteDialog();

  await vi.waitFor(() =>
    expect(
      document.querySelector<HTMLButtonElement>(
        '[data-frontend="claude_code"] .skill-install-button',
      )?.disabled,
    ).toBe(false),
  );
  document
    .querySelector<HTMLButtonElement>(
      '[data-frontend="claude_code"] .skill-install-button',
    )!
    .click();

  await vi.waitFor(() =>
    expect(
      document.querySelector('[data-frontend="claude_code"]')?.textContent,
    ).toContain("已写入"),
  );
  const installed = picked.dirs.get("pm-cli")!;
  expect(installed.files.get("SKILL.md")).toBe("# doc\n");
  expect(installed.dirs.get("bin")?.files.get("pm-cli.mjs")).toBe("// cli\n");
  delete (window as unknown as { showDirectoryPicker?: unknown })
    .showDirectoryPicker;
});

it("writes directly when the picked directory is already named pm-cli", async () => {
  const picked = makeDirectory("pm-cli");
  setPicker(async () => directoryHandle(picked));
  mountRemoteDialog();

  await vi.waitFor(() =>
    expect(
      document.querySelector<HTMLButtonElement>(
        '[data-frontend="claude_code"] .skill-install-button',
      )?.disabled,
    ).toBe(false),
  );
  document
    .querySelector<HTMLButtonElement>(
      '[data-frontend="claude_code"] .skill-install-button',
    )!
    .click();

  await vi.waitFor(() => expect(picked.files.get("SKILL.md")).toBe("# doc\n"));
  // 不再多套一层 pm-cli/pm-cli
  expect(picked.dirs.has("pm-cli")).toBe(false);
  delete (window as unknown as { showDirectoryPicker?: unknown })
    .showDirectoryPicker;
});
