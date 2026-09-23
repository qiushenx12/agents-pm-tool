// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, h, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskGrid from "@/grid-app/components/TaskGrid.vue";
import { api } from "@/grid-app/api/client";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import type { Task } from "@/shared/types";

vi.mock("@/grid-app/api/client", () => ({
  api: { pageTasks: vi.fn(), listTasks: vi.fn(), patchTask: vi.fn() },
}));

let app: App | undefined;
let pinia: Pinia | undefined;
let host: HTMLElement | undefined;

afterEach(() => {
  app?.unmount();
  if (pinia) disposePinia(pinia);
  host?.remove();
  localStorage.clear();
  vi.clearAllMocks();
});

it.each(["predecessor_task_ids", "unlock_task_ids"] as const)(
  "opens %s from the middle or edge of the cell and keeps it open after a rapid second click",
  async (column) => {
    window.history.replaceState(null, "", "/");
    localStorage.clear();
    const task: Task = {
      id: "task-1",
      seq: 1,
      project: "测试项目",
      type: "优化",
      description: "有依赖的任务",
      note: "",
      status: "未开始",
      priority: "中",
      submitter: "用户",
      created_at: "",
      finished_at: null,
      updated_at: "",
      position: 1,
      predecessor_task_ids: [],
      unlock_task_ids: [],
    };
    const candidate: Task = {
      ...task,
      id: "task-2",
      seq: 2,
      description: "可关联任务",
      position: 2,
    };
    vi.mocked(api.pageTasks).mockResolvedValue({
      items: [task, candidate],
      total: 2,
      page: 1,
      page_size: 100,
      groups: [],
      anchor_found: null,
    });
    vi.mocked(api.listTasks).mockResolvedValue([task, candidate]);
    vi.mocked(api.patchTask).mockResolvedValue({
      ...task,
      [column]: [candidate.id],
    });
    pinia = createPinia();
    await useTaskStore(pinia).refresh();
    host = document.createElement("div");
    document.body.append(host);
    app = createApp({ render: () => h(TaskGrid) });
    app.use(pinia);
    app.mount(host);

    const cell = host.querySelector<HTMLElement>(
      `[data-task-id="task-1"][data-column="${column}"]`,
    )!;
    const trigger = cell.querySelector<HTMLButtonElement>(
      ".dependency-trigger",
    )!;
    expect(trigger).not.toBeNull();
    trigger.querySelector<HTMLElement>(".dependency-value")!.click();
    await vi.waitFor(() =>
      expect(trigger?.getAttribute("aria-expanded")).toBe("true"),
    );
    expect(cell.getAttribute("aria-selected")).toBe("true");

    trigger.click();
    await nextTick();
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    cell.click();
    await vi.waitFor(() =>
      expect(trigger.getAttribute("aria-expanded")).toBe("true"),
    );

    trigger.dispatchEvent(
      new MouseEvent("click", { bubbles: true, detail: 2 }),
    );
    cell.dispatchEvent(
      new MouseEvent("dblclick", { bubbles: true, detail: 2 }),
    );
    await nextTick();
    expect(trigger.getAttribute("aria-expanded")).toBe("true");

    const option = Array.from(
      document.body.querySelectorAll<HTMLButtonElement>('[role="option"]'),
    ).find((button) => button.textContent?.includes(candidate.id))!;
    option.click();
    await vi.waitFor(() =>
      expect(api.patchTask).toHaveBeenCalledWith(task.id, {
        [column]: [candidate.id],
      }),
    );
  },
);
