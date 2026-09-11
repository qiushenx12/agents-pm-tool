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
  "取消",
] as const;
export type TaskStatus = (typeof TASK_STATUSES)[number];

/** Agent（CLI）允许切换到的状态子集 */
export const AGENT_STATUSES = ["进行中", "待验证", "已完成"] as const;

export const SUBMITTERS = ["用户", "Agent"] as const;
export type Submitter = (typeof SUBMITTERS)[number];

export const PRIORITIES = ["高", "中", "低"] as const;
export type Priority = (typeof PRIORITIES)[number];
/** 新建任务默认优先级 */
export const DEFAULT_PRIORITY: Priority = "中";

export const USER_ROLES = ["super_admin", "admin", "user"] as const;
export type UserRole = (typeof USER_ROLES)[number];

export interface User {
  id: string;
  username: string;
  role: UserRole;
  created_at: string;
  disabled: boolean;
  is_host: boolean;
  /** 用户管理列表中返回；普通用户是否至少拥有一个项目访问授权。 */
  has_permissions?: boolean;
}

/** 本机工作区设置；与 Rust settings::Settings 的可编辑字段对齐。 */
export interface WorkspaceSettings {
  port: number;
  autostart: boolean;
  close_behavior: "keep_service" | "stop_all";
  listen_scope: "local" | "lan";
  agent_server_url: string;
  theme?: "light" | "dark" | null;
}

export interface WorkspaceServerStatus {
  running: boolean;
  port: number;
  url: string;
  lan_url: string;
  data_dir: string;
}

export interface HostSettingsResponse {
  settings: WorkspaceSettings;
  status: WorkspaceServerStatus;
}

export interface SaveHostSettingsResponse extends HostSettingsResponse {
  restarted: boolean;
  port: number;
}

export interface UserPermission {
  project: string;
  field: string;
  allowed_values: string[] | null;
}

export interface UserPermissionsResponse {
  user: User;
  permissions: UserPermission[];
}

export interface AgentAccess {
  server_url: string;
  token: string | null;
  access_instructions: string;
}

export type LocalSkillFrontendId =
  | "codex"
  | "claude_code"
  | "workbuddy"
  | "opencode"
  | "cursor"
  | "pi";

export interface LocalSkillTarget {
  frontend_id: LocalSkillFrontendId;
  frontend: string;
  path: string;
  installed: boolean;
  version: string | null;
}

export interface Task {
  id: string;
  seq: number;
  project: string;
  type: TaskType;
  description: string;
  note: string;
  status: TaskStatus;
  priority: Priority;
  submitter: Submitter;
  /** 用于界面展示：用户名，或 Agent（用户名）。旧服务响应缺失时回退 submitter。 */
  submitter_name?: string;
  created_at: string; // 'YYYY-MM-DD HH:MM:SS' 本地时间
  finished_at: string | null;
  updated_at: string;
  /** 手动排序位置（sort_by=manual 时生效） */
  position: number;
  attachment_count?: number;
  owner_user_id?: string | null;
}

export interface Project {
  name: string;
  color: string;
  sort_order: number;
  local_path: string;
  git_url: string;
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
  status_mode?: "include" | "exclude";
  /** 提交人筛选：大类（用户/Agent）与具体用户名共存 */
  submitter?: string[];
  priority?: Priority[];
  keyword?: string;
  sort_by?: "created_at" | "seq" | "updated_at" | "finished_at" | "manual" | "priority";
  sort_order?: "asc" | "desc";
}

export const GROUP_FIELDS = [
  "project",
  "status",
  "type",
  "submitter",
  "priority",
] as const;
export type GroupField = (typeof GROUP_FIELDS)[number];
export interface TaskPageQuery extends TaskListQuery {
  page?: number;
  page_size?: number;
  group_by?: GroupField;
  anchor_id?: string;
}
export interface TaskGroupCount {
  value: string;
  count: number;
}
export interface TaskPage {
  items: Task[];
  total: number;
  page: number;
  page_size: number;
  groups: TaskGroupCount[];
  anchor_found: boolean | null;
}
export type TaskBatchRequest =
  | {
      action: "update";
      ids: string[];
      patch: Partial<Pick<Task, "project" | "type" | "status" | "priority">>;
    }
  | { action: "delete"; ids: string[] };
export interface TaskBatchResult {
  results: {
    id: string;
    task?: Task;
    error?: { code: string; message: string };
  }[];
  succeeded: number;
  failed: number;
}

/** 展示用：'YYYY-MM-DD HH:MM:SS' → 'yyyy/mm/dd hh:mm:ss' */
export function formatDateTime(s: string | null): string {
  if (!s) return "";
  return s.replace(/-/g, "/");
}
