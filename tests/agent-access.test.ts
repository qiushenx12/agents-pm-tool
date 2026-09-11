// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, type App } from "vue";
import AgentAccessDialog from "@/grid-app/components/users/AgentAccessDialog.vue";
import { api } from "@/grid-app/api/client";
import type { LocalSkillTarget, User } from "@/shared/types";

vi.mock("@/grid-app/api/client", () => ({
  api: {
    getAgentAccess: vi.fn(),
    listLocalSkills: vi.fn(),
    installLocalSkills: vi.fn(),
    openLocalSkillDirectory: vi.fn(),
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
    expect(document.querySelectorAll(".skill-frontend-card").length).toBe(6),
  );
  for (const frontend of [
    "codex",
    "claude_code",
    "workbuddy",
    "opencode",
    "cursor",
    "pi",
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
