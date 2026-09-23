// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskDetailDrawer from "@/grid-app/components/TaskDetailDrawer.vue";
import type { Task } from "@/shared/types";

const { getTask, listAttachments } = vi.hoisted(() => ({
  getTask: vi.fn(),
  listAttachments: vi.fn(),
}));
vi.mock("@/grid-app/api/client", async (original) => {
  const actual = await original<typeof import("@/grid-app/api/client")>();
  return { ...actual, api: { ...actual.api, getTask, listAttachments } };
});

const task: Task = {
  id: "202609231038480000",
  seq: 1,
  project: "agents-pm-tool",
  type: "新增需求",
  description: "关联图节点任务详情",
  note: "",
  status: "进行中",
  priority: "中",
  submitter: "用户",
  created_at: "2026-09-23 10:38:48",
  finished_at: null,
  updated_at: "2026-09-23 10:38:48",
  position: 1,
  predecessor_task_ids: [],
  unlock_task_ids: [],
};
let app: App | undefined;
let pinia: Pinia;
let host: HTMLElement;
afterEach(() => {
  app?.unmount();
  host?.remove();
  disposePinia(pinia);
  document.body.innerHTML = "";
  getTask.mockReset();
  listAttachments.mockReset();
});

it("reuses the full task detail dialog from the graph without table pagination controls", async () => {
  getTask.mockResolvedValue(task);
  listAttachments.mockResolvedValue([]);
  pinia = createPinia();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(TaskDetailDrawer, { task, navigation: false });
  app.use(pinia);
  app.mount(host);
  await nextTick();
  expect(document.body.querySelector('[role="dialog"]')?.textContent).toContain(task.description);
  expect(document.body.querySelector('[role="dialog"]')?.textContent).toContain("子任务 ID");
  expect(document.body.querySelector('[role="dialog"]')?.textContent).toContain("父级任务 ID");
  expect(document.body.querySelector('[aria-label="上一条任务"]')).toBeNull();
  expect(document.body.querySelector('[aria-label="下一条任务"]')).toBeNull();
  expect(document.body.querySelector(".detail-outside-filter")).toBeNull();
});
