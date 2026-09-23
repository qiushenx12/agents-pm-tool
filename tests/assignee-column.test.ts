// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, h, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskGrid from "@/grid-app/components/TaskGrid.vue";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import { DEFAULT_COLUMNS, useViewStore } from "@/grid-app/stores/viewStore";
import { api } from "@/grid-app/api/client";
import type { Task } from "@/shared/types";

vi.mock("@/grid-app/api/client", () => ({
  api: { pageTasks: vi.fn(), patchTask: vi.fn() },
}));

let app: App | undefined, pinia: Pinia, host: HTMLElement | undefined;

afterEach(() => {
  app?.unmount();
  if (pinia) disposePinia(pinia);
  host?.remove();
  vi.restoreAllMocks();
});

function taskFixture(overrides: Partial<Task> = {}): Task {
  return {
    id: "task-1",
    seq: 1,
    project: "测试项目",
    type: "优化",
    status: "进行中",
    priority: "中",
    description: "负责人列测试",
    note: "",
    submitter: "用户",
    submitter_name: "主机",
    created_at: "",
    finished_at: null,
    updated_at: "",
    position: 1,
    ...overrides,
  };
}

async function mountGrid(tasks: Task[]) {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: tasks,
    total: tasks.length,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  });
  const store = useTaskStore(pinia);
  await store.refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);
  await nextTick();
}

it("负责人列默认放在提交人右侧", () => {
  localStorage.clear();
  pinia = createPinia();
  const view = useViewStore(pinia);
  const keys = view.columns.map((column) => column.key);
  expect(keys.indexOf("assignee")).toBe(keys.indexOf("submitter") + 1);
  const defaults = DEFAULT_COLUMNS.map((column) => column.key);
  expect(defaults.indexOf("assignee")).toBe(defaults.indexOf("submitter") + 1);
});

it("渲染负责人名称，未认领时显示占位符", async () => {
  await mountGrid([
    taskFixture({ id: "task-1", assignee_name: "Agent（主机）" }),
    taskFixture({ id: "task-2", seq: 2, position: 2 }),
  ]);
  const rows = [...host!.querySelectorAll(".task-row")];
  expect(rows).toHaveLength(2);
  const assigneeCell = (row: Element) =>
    row.querySelector(".assignee-tag")?.textContent?.trim();
  expect(assigneeCell(rows[0])).toBe("Agent（主机）");
  expect(assigneeCell(rows[1])).toBe("—");
  // 表头顺序：提交人右侧即负责人
  const headers = [...host!.querySelectorAll("thead th[data-column]")].map(
    (header) => header.getAttribute("data-column"),
  );
  expect(headers.indexOf("assignee")).toBe(headers.indexOf("submitter") + 1);
});
