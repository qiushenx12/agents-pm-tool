import type { Task } from "@/shared/types";
import { copyText } from "@/shared/feedback";

export interface TaskRowAction {
  key: string;
  label: string;
  title: string;
  icon: string;
  run: (task: Task) => void | Promise<void>;
}

/** 「复制 Prompt」所需的接入信息。只有服务地址会进入 Prompt。 */
export interface AgentPromptAccess {
  /** 服务端动态计算：主机账号为 loopback，局域网账号为可达地址（见 api_agent_access.rs）。 */
  server_url?: string;
}

/** 尚未取到接入信息时的兜底：Prompt 里不出现服务地址，Agent 会向用户索取。 */
const DEFAULT_ACCESS: AgentPromptAccess = {};

let activeAccess = DEFAULT_ACCESS;

export function setAgentPromptAccess(access?: AgentPromptAccess) {
  activeAccess = access ?? DEFAULT_ACCESS;
}

/**
 * 生成可直接交给任意 Agent 的任务说明。
 *
 * 模板对所有场景一致：工具介绍、命令清单与权限边界不在这里展开——装了 pm-cli-skill 的
 * Agent 会读 SKILL.md，没装的则按给出的地址请求免 token 的 /api/agent/help。
 * 模板全集见 docs/agent-prompt-templates.md。
 */
export function buildAgentTaskPrompt(
  task: Task,
  access: AgentPromptAccess = DEFAULT_ACCESS,
) {
  const serverUrl = access.server_url;
  const helpUrl = serverUrl ? `${serverUrl}/api/agent/help` : undefined;
  return [
    "请使用 Agents PM Tool（pm-cli-skill）完成以下任务。",
    "先执行：pm-cli doctor --json",
    `获取任务内容命令：pm-cli get ${task.id} --json`,
    ...(serverUrl ? [`服务地址：[${serverUrl}](${serverUrl})`] : []),
    helpUrl
      ? `完整用法见 pm-cli --help 或 [${helpUrl}](${helpUrl})`
      : "完整用法见 pm-cli --help 或 GET /api/agent/help",
  ].join("\n");
}

/** 生成 ID 单元格复制按钮使用的一句话任务指引。 */
export function buildAgentIdPrompt(task: Task) {
  return `请使用 pm-cli 获取任务id=${task.id}的内容并完成任务`;
}

/**
 * 表格操作列的扩展入口。新增行级操作时在此注册，表格本身无需增加分支。
 */
export const TASK_ROW_ACTIONS: readonly TaskRowAction[] = [
  {
    key: "copy-agent-prompt",
    label: "复制 Prompt",
    title: "复制 Agent 任务 Prompt",
    icon: "copy",
    run: (task: Task) => copyText(buildAgentTaskPrompt(task, activeAccess)),
  },
];
