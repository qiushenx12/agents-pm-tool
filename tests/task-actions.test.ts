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

  it("restores the complete prompt used by the actions column", () => {
    const prompt = buildAgentTaskPrompt(task);

    expect(prompt).toContain('任务 ID："202609091234560001"');
    expect(prompt).toContain('项目："agents-pm-tool"');
    expect(prompt).toContain("pm-cli get 202609091234560001 --json");
    expect(prompt).toContain("pm-cli attachments <任务ID> [--json]");
    expect(prompt).toContain("pm-cli download <附件ID>");
    expect(prompt).toContain("查看和下载已授权项目中的任务附件");
    expect(prompt).toContain("不能删除任务、上传或删除附件");
    expect(prompt).toContain("1. 工具介绍与访问方式");
    expect(prompt).toContain("2. 可用命令");
    expect(prompt).toContain("3. Agent 权限");
  });

  it("builds the concise prompt used by the ID cell", () => {
    expect(buildAgentIdPrompt(task)).toBe(
      "请使用 pm-cli 获取任务id=202609091234560001的内容并完成任务",
    );
  });

  it("uses server-provided remote access instructions", () => {
    const prompt = buildAgentTaskPrompt(task, {
      access_instructions: "请设置 PM_SERVER_URL=http://192.168.1.9:17890 与 PM_AGENT_TOKEN。",
      skill_ready: false,
    });
    expect(prompt).toContain("PM_SERVER_URL=http://192.168.1.9:17890");
    expect(prompt).toContain("GET /api/agent/help");
  });

  it("uses the short prompt when the matching skill is ready", () => {
    const prompt = buildAgentTaskPrompt(task, {
      access_instructions: "不会出现在短模式",
      skill_ready: true,
    });
    expect(prompt).toContain("请使用 pm-cli-skill");
    expect(prompt).toContain("pm-cli get 202609091234560001 --json");
    expect(prompt).not.toContain("2. 可用命令");
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
