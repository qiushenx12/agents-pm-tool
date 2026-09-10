import type { Task } from "@/shared/types";
import { copyText } from "@/shared/feedback";

export interface TaskRowAction {
  key: string;
  label: string;
  title: string;
  icon: string;
  run: (task: Task) => void | Promise<void>;
}

function promptValue(value: string) {
  return JSON.stringify(value);
}

export interface AgentPromptAccess {
  access_instructions: string;
  skill_ready: boolean;
}

const DEFAULT_ACCESS: AgentPromptAccess = {
  access_instructions:
    "Agents PM Tool 是本地任务管理工具，Agent 通过受限客户端 pm-cli 读取和推进任务。请先确保桌面应用正在运行；安装版会注册 pm-cli 到用户 PATH，并自动读取实际端口和临时 token，无需手动配置。安装或升级后需重新打开终端/Agent 前端。",
  skill_ready: false,
};

let activeAccess = DEFAULT_ACCESS;

export function setAgentPromptAccess(access?: AgentPromptAccess) {
  activeAccess = access ?? DEFAULT_ACCESS;
}

/** 生成可直接交给任意具备本机终端能力的 Agent 的完整任务说明。 */
export function buildAgentTaskPrompt(
  task: Task,
  access: AgentPromptAccess = DEFAULT_ACCESS,
) {
  if (access.skill_ready) {
    return [
      "请使用 pm-cli-skill 了解 Agents PM Tool 与 pm-cli 的完整用法。",
      "",
      `任务 ID：${promptValue(task.id)}`,
      `项目：${promptValue(task.project)}`,
      `请先运行 pm-cli get ${task.id} --json 读取任务，然后完成并推进状态。`,
    ].join("\n");
  }
  return [
    "请使用 Agents PM Tool 完成以下任务：",
    `- 任务 ID：${promptValue(task.id)}`,
    `- 项目：${promptValue(task.project)}`,
    "",
    "1. 工具介绍与访问方式",
    access.access_instructions,
    `使用 pm-cli get ${task.id} --json 读取本任务。`,
    "",
    "2. 可用命令",
    "```text",
    "pm-cli list [筛选参数] [--json]                 查看/筛选任务",
    "pm-cli get <任务ID> [--json]                   查看任务详情",
    "pm-cli attachments <任务ID> [--json]           查看任务附件",
    "pm-cli download <附件ID> [--output <文件路径>] 下载附件",
    "pm-cli projects [--json]                       查看项目及 local_path/git_url",
    "pm-cli create --project <项目> --type <类型> --description <描述> [--json]",
    "pm-cli status <任务ID> --to <进行中|待验证|已完成> [--json]",
    "pm-cli describe <任务ID> --description <描述> [--json]",
    "pm-cli --help                                   查看完整帮助",
    "```",
    "完整用法见 pm-cli --help 或 GET /api/agent/help（需带 token）。",
    "",
    "3. Agent 权限",
    "- 可以查看/筛选任务、只读查看项目、创建任务，并查看和下载已授权项目中的任务附件。",
    "- 只能把任务状态改为进行中、待验证或已完成。",
    "- 只能修改由 Agent 创建的任务描述，且描述不能为空。",
    "- 不能设置验收状态，不能修改项目、类型或用户创建的任务描述，也不能删除任务、上传或删除附件、直接读写 SQLite。",
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
