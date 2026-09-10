import type {
  TaskPage,
  TaskPageQuery,
  TaskBatchRequest,
  TaskBatchResult,
  ApiError,
  Attachment,
  Project,
  Task,
  TaskListQuery,
  TaskStatus,
  TaskType,
  User,
  UserRole,
  UserPermission,
  UserPermissionsResponse,
  AgentAccess,
  LocalSkillTarget,
} from "@/shared/types";

export class ApiRequestError extends Error {
  code: string;
  details?: Record<string, unknown>;
  constructor(
    e: ApiError["error"],
    public status: number,
  ) {
    super(e.message);
    this.code = e.code;
    this.details = e.details;
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  if (init?.body && !(init.body instanceof FormData)) {
    headers.set("Content-Type", "application/json");
  }
  if (init?.method && !["GET", "HEAD", "OPTIONS"].includes(init.method)) {
    headers.set("X-PM-Client", "web");
  }
  const res = await fetch(path, {
    ...init,
    headers,
    credentials: "same-origin",
  });
  if (!res.ok) {
    let err: ApiError["error"] = {
      code: "unknown",
      message: `请求失败（${res.status}）`,
    };
    try {
      const body = (await res.json()) as ApiError;
      if (body?.error) err = body.error;
    } catch {
      /* ignore */
    }
    throw new ApiRequestError(err, res.status);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

function buildQuery(q: TaskPageQuery): string {
  const p = new URLSearchParams();
  q.project?.forEach((v) => p.append("project", v));
  q.type?.forEach((v) => p.append("type", v));
  q.status?.forEach((v) => p.append("status", v));
  q.submitter?.forEach((v) => p.append("submitter", v));
  if (q.keyword) p.set("keyword", q.keyword);
  if (q.sort_by) p.set("sort_by", q.sort_by);
  if (q.sort_order) p.set("sort_order", q.sort_order);
  if (q.page) p.set("page", String(q.page));
  if (q.page_size) p.set("page_size", String(q.page_size));
  if (q.group_by) p.set("group_by", q.group_by);
  if (q.anchor_id) p.set("anchor_id", q.anchor_id);
  const s = p.toString();
  return s ? `?${s}` : "";
}

export const api = {
  me: () => request<User>("/api/web/auth/me"),
  register: (body: { username: string; password: string }) =>
    request<{ user: User }>("/api/web/auth/register", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  login: (body: { username: string; password: string }) =>
    request<{ user: User }>("/api/web/auth/login", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  hostLogin: () =>
    request<{ user: User }>("/api/web/auth/host-login", { method: "POST" }),
  logout: () => request<void>("/api/web/auth/logout", { method: "POST" }),

  pageTasks: (q: TaskPageQuery = {}, signal?: AbortSignal) =>
    request<TaskPage>(`/api/web/tasks/page${buildQuery(q)}`, { signal }),
  getTask: (id: string) =>
    request<Task>(`/api/web/tasks/${encodeURIComponent(id)}`),
  batchTasks: (body: TaskBatchRequest) =>
    request<TaskBatchResult>("/api/web/tasks/batch", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  listTasks: (q: TaskListQuery = {}, signal?: AbortSignal) =>
    request<Task[]>(`/api/web/tasks${buildQuery(q)}`, { signal }),

  createTask: (body: {
    project: string;
    type: TaskType;
    description?: string;
    note?: string;
  }) =>
    request<Task>("/api/web/tasks", {
      method: "POST",
      body: JSON.stringify(body),
    }),

  patchTask: (
    id: string,
    body: Partial<{
      project: string;
      type: TaskType;
      description: string;
      note: string;
      status: TaskStatus;
    }>,
  ) =>
    request<Task>(`/api/web/tasks/${id}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),

  deleteTask: (id: string) =>
    request<void>(`/api/web/tasks/${id}`, { method: "DELETE" }),

  /** 手动排序：把任务移到 prev/next 之间（只给一侧即贴到该侧之外） */
  reorderTask: (id: string, body: { prev_id?: string; next_id?: string }) =>
    request<Task>(`/api/web/tasks/${encodeURIComponent(id)}/reorder`, {
      method: "POST",
      body: JSON.stringify(body),
    }),

  /** 以指定排序重铺手动位置：切入手动排序时以当前视图为基线 */
  rebaseOrder: (body: { sort_by?: string; sort_order?: string }) =>
    request<void>("/api/web/tasks/rebase-order", {
      method: "POST",
      body: JSON.stringify(body),
    }),

  listProjects: () => request<Project[]>("/api/web/projects"),

  createProject: (body: {
    name: string;
    color?: string;
    local_path?: string;
    git_url?: string;
  }) =>
    request<Project>("/api/web/projects", {
      method: "POST",
      body: JSON.stringify(body),
    }),

  patchProject: (
    name: string,
    body: Partial<{
      new_name: string;
      color: string;
      sort_order: number;
      local_path: string;
      git_url: string;
    }>,
  ) =>
    request<Project>(`/api/web/projects/${encodeURIComponent(name)}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),

  /** 弹系统文件夹选择框；用户取消时返回 undefined */
  pickFolder: () =>
    request<{ path: string } | undefined>("/api/web/pick-folder", {
      method: "POST",
    }),

  deleteProject: (name: string) =>
    request<void>(`/api/web/projects/${encodeURIComponent(name)}`, {
      method: "DELETE",
    }),

  listAttachments: (taskId: string) =>
    request<Attachment[]>(`/api/web/tasks/${taskId}/attachments`),

  attachmentUrl: (id: string) => `/api/web/attachments/${id}`,

  uploadAttachment: (taskId: string, file: File) => {
    const fd = new FormData();
    fd.append("file", file);
    return request<Attachment>(`/api/web/tasks/${taskId}/attachments`, {
      method: "POST",
      body: fd,
    });
  },

  deleteAttachment: (id: string) =>
    request<void>(`/api/web/attachments/${id}`, { method: "DELETE" }),

  listUsers: () => request<User[]>("/api/web/users"),
  patchUser: (
    id: string,
    body: Partial<{ username: string; role: UserRole; disabled: boolean }>,
  ) =>
    request<User>(`/api/web/users/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),
  deleteUser: (id: string) =>
    request<void>(`/api/web/users/${encodeURIComponent(id)}`, {
      method: "DELETE",
    }),
  getUserPermissions: (id: string) =>
    request<UserPermissionsResponse>(
      `/api/web/users/${encodeURIComponent(id)}/permissions`,
    ),
  putUserPermissions: (id: string, permissions: UserPermission[]) =>
    request<UserPermissionsResponse>(
      `/api/web/users/${encodeURIComponent(id)}/permissions`,
      { method: "PUT", body: JSON.stringify({ permissions }) },
    ),
  getAgentAccess: () => request<AgentAccess>("/api/web/me/agent-access"),
  regenerateAgentToken: () =>
    request<AgentAccess>("/api/web/me/agent-token", { method: "POST" }),
  revokeAgentToken: () =>
    request<void>("/api/web/me/agent-token", { method: "DELETE" }),
  listLocalSkills: () =>
    request<LocalSkillTarget[]>("/api/web/local-skills"),
  installLocalSkills: (frontend: LocalSkillTarget["frontend_id"]) =>
    request<LocalSkillTarget[]>("/api/web/local-skills/install", {
      method: "POST",
      body: JSON.stringify({ frontend }),
    }),
};

/** SSE 订阅：任务变更时触发 onChange；断线自动降级为 10s 轮询 */
export function subscribeTaskEvents(
  onChange: () => void,
  onStatus?: (state: "connecting" | "live" | "reconnecting") => void,
): () => void {
  let stopped = false;
  let pollTimer: number | undefined;
  let es: EventSource | undefined;

  const startPolling = () => {
    if (pollTimer !== undefined) return;
    pollTimer = window.setInterval(() => onChange(), 10_000);
  };
  const stopPolling = () => {
    if (pollTimer !== undefined) {
      clearInterval(pollTimer);
      pollTimer = undefined;
    }
  };

  const connect = () => {
    if (stopped) return;
    onStatus?.("connecting");
    es = new EventSource("/api/web/events");
    es.addEventListener("tasks_changed", () => onChange());
    es.onopen = () => {
      stopPolling();
      onStatus?.("live");
      onChange();
    };
    es.onerror = () => {
      onStatus?.("reconnecting");
      // EventSource 会自动重连；重连期间用轮询兜底
      startPolling();
    };
  };

  connect();

  return () => {
    stopped = true;
    stopPolling();
    es?.close();
  };
}
