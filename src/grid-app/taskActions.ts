import type { Task } from "@/shared/types";
import { copyText } from "@/shared/feedback";

export interface TaskRowAction {
  key: string;
  label: string;
  title: string;
  icon: string;
  run: (task: Task) => void | Promise<void>;
}

/** 「完整Prompt」所需的接入信息。只有服务地址会进入 Prompt。 */
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
 * pm-cli 不注册到 PATH，所以先提醒它按 skill 目录定位，再给命令示例。
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
    "pm-cli 在 pm-cli-skill 的 bin 目录下，先定位到它再执行（需要 Node.js 18 或更高版本）。",
    "先执行：pm-cli doctor --json",
    `获取任务内容命令：pm-cli get ${task.id} --json`,
    ...(serverUrl ? [`服务地址：[${serverUrl}](${serverUrl})`] : []),
    helpUrl
      ? `完整用法见 skill 目录里的 SKILL.md、pm-cli --help 或 [${helpUrl}](${helpUrl})`
      : "完整用法见 skill 目录里的 SKILL.md、pm-cli --help 或 GET /api/agent/help",
  ].join("\n");
}

/** 一句话任务指引：只给任务 ID，让 Agent 自己去取内容。供操作列的「Prompt」按钮使用。 */
export function buildAgentOneLinePrompt(task: Task) {
  return `请使用 pm-cli 获取任务id=${task.id}的内容并完成任务`;
}

/**
 * 表格操作列的扩展入口。新增行级操作时在此注册，表格本身无需增加分支。
 * 数组顺序即按钮从左到右的顺序。
 */
export const TASK_ROW_ACTIONS: readonly TaskRowAction[] = [
  {
    key: "copy-one-line-prompt",
    label: "Prompt",
    title: "复制一句话任务指引（与任务 ID 旁的复制按钮一致）",
    icon: "copy",
    run: (task: Task) => copyText(buildAgentOneLinePrompt(task)),
  },
  {
    key: "copy-agent-prompt",
    label: "完整Prompt",
    title: "复制完整的 Agent 任务 Prompt",
    icon: "copy",
    run: (task: Task) => copyText(buildAgentTaskPrompt(task, activeAccess)),
  },
];
