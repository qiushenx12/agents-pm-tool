// 与 Rust 端 DTO 严格对齐（src-tauri/src/domain/task.rs）

export const TASK_TYPES = ["新增需求", "优化", "BUG"] as const;
export type TaskType = (typeof TASK_TYPES)[number];

export const TASK_STATUSES = [
  "未开始",
  "进行中",
  "待验证",
  "已完成",
  "验收未通过",
  "验收通过",
] as const;
export type TaskStatus = (typeof TASK_STATUSES)[number];

/** Agent（CLI）允许切换到的状态子集 */
export const AGENT_STATUSES = ["进行中", "待验证", "已完成"] as const;

export const SUBMITTERS = ["用户", "Agent"] as const;
export type Submitter = (typeof SUBMITTERS)[number];

export interface Task {
  id: string;
  seq: number;
  project: string;
  type: TaskType;
  description: string;
  status: TaskStatus;
  submitter: Submitter;
  created_at: string; // 'YYYY-MM-DD HH:MM:SS' 本地时间
  finished_at: string | null;
  updated_at: string;
  attachment_count?: number;
}

export interface Project {
  name: string;
  color: string;
  sort_order: number;
  created_at: string;
}

export interface Attachment {
  id: string;
  task_id: string;
  filename: string;
  stored_path: string;
  mime: string | null;
  size: number;
  created_at: string;
}

export interface ApiError {
  error: {
    code: string;
    message: string;
    details?: Record<string, unknown>;
  };
}

export interface TaskListQuery {
  project?: string[];
  type?: TaskType[];
  status?: TaskStatus[];
  submitter?: Submitter[];
  keyword?: string;
  sort_by?: "created_at" | "seq" | "updated_at" | "finished_at";
  sort_order?: "asc" | "desc";
}

/** 展示用：'YYYY-MM-DD HH:MM:SS' → 'yyyy/mm/dd hh:mm:ss' */
export function formatDateTime(s: string | null): string {
  if (!s) return "";
  return s.replace(/-/g, "/");
}
