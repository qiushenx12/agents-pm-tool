// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import {
  buildAgentIdPrompt,
  buildAgentTaskPrompt,
  TASK_ROW_ACTIONS,
} from "@/grid-app/taskActions";
import type { Task } from "@/shared/types";

const task: Task = {
  id: "202609091234560001",
  seq: 1,
  project: "agents-pm-tool",
  type: "新增需求",
  status: "未开始",
  priority: "中",
  description: "新增复制 Agent Prompt 的操作按钮",
  note: "",
  submitter: "用户",
  created_at: "2026-09-09 12:34:56",
  finished_at: null,
  updated_at: "2026-09-09 12:34:56",
  position: 1,
};

describe("task row actions", () => {
  it("registers the copy prompt action through the extensible action list", () => {
    expect(TASK_ROW_ACTIONS.map((action) => action.key)).toEqual([
      "copy-agent-prompt",
    ]);
  });

  it("builds one concise prompt that does not enumerate commands or permissions", () => {
    const prompt = buildAgentTaskPrompt(task);

    expect(prompt).toBe(
      [
        "请使用 Agents PM Tool（pm-cli-skill）完成以下任务。",
        "先执行：pm-cli doctor --json",
        "获取任务内容命令：pm-cli get 202609091234560001 --json",
        "完整用法见 pm-cli --help 或 GET /api/agent/help",
      ].join("\n"),
    );
    // 工具介绍、命令清单与权限边界已移到 /api/agent/help，Prompt 里不再展开
    expect(prompt).not.toContain("2. 可用命令");
    expect(prompt).not.toContain("pm-cli attachments");
    expect(prompt).not.toContain("3. Agent 权限");
  });

  it("includes the service address when the backend provides one", () => {
    const prompt = buildAgentTaskPrompt(task, {
      server_url: "http://192.168.1.9:17890",
    });
    expect(prompt).toBe(
      [
        "请使用 Agents PM Tool（pm-cli-skill）完成以下任务。",
        "先执行：pm-cli doctor --json",
        "获取任务内容命令：pm-cli get 202609091234560001 --json",
        "服务地址：[http://192.168.1.9:17890](http://192.168.1.9:17890)",
        "完整用法见 pm-cli --help 或 [http://192.168.1.9:17890/api/agent/help](http://192.168.1.9:17890/api/agent/help)",
      ].join("\n"),
    );
  });

  it("omits the address when the access info is unavailable", () => {
    const prompt = buildAgentTaskPrompt(task, {});
    expect(prompt).not.toContain("服务地址：");
    expect(prompt).not.toContain("192.168.1.9");
    expect(prompt).toContain("GET /api/agent/help");
  });

  it("produces the same shape for host and remote — only the address differs", () => {
    const host = buildAgentTaskPrompt(task, { server_url: "http://127.0.0.1:3010" });
    const remote = buildAgentTaskPrompt(task, {
      server_url: "http://192.168.1.9:17890",
    });
    expect(host.replaceAll("http://127.0.0.1:3010", "<addr>")).toBe(
      remote.replaceAll("http://192.168.1.9:17890", "<addr>"),
    );
  });

  it("builds the concise prompt used by the ID cell", () => {
    expect(buildAgentIdPrompt(task)).toBe(
      "请使用 pm-cli 获取任务id=202609091234560001的内容并完成任务",
    );
  });

  it("copies the generated prompt through the registered action", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    await TASK_ROW_ACTIONS[0].run(task);

    expect(writeText).toHaveBeenCalledOnce();
    expect(writeText).toHaveBeenCalledWith(buildAgentTaskPrompt(task));
  });
});
