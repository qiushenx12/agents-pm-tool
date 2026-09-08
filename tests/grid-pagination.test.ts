// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, h, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskGrid from "@/grid-app/components/TaskGrid.vue";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import { api } from "@/grid-app/api/client";
import type { Task } from "@/shared/types";
vi.mock("@/grid-app/api/client", () => ({ api: { pageTasks: vi.fn() } }));
let app: App, pinia: Pinia, host: HTMLElement;
afterEach(() => {
  app?.unmount();
  disposePinia(pinia);
  host?.remove();
  vi.restoreAllMocks();
});
it("requests only the desired server page and reaches records after 2000", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  const fixtures: Task[] = Array.from({ length: 2001 }, (_, index) => ({
    id: "task-" + index,
    seq: index,
    project: "测试项目",
    type: "优化",
    status: "未开始",
    description: "性能验收记录 " + index,
    submitter: "用户",
    created_at: "",
    finished_at: null,
    updated_at: "",
  }));
  vi.mocked(api.pageTasks).mockImplementation(async (query) => ({
    items: fixtures.slice(
      ((query?.page ?? 1) - 1) * 100,
      (query?.page ?? 1) * 100,
    ),
    total: 2001,
    page: query?.page ?? 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  }));
  const tasks = useTaskStore(pinia);
  await tasks.refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);
  host.querySelector<HTMLElement>(".grid-wrap")!.scrollTo = vi.fn();
  expect(tasks.tasks).toHaveLength(100);
  expect(host.querySelectorAll(".task-row")).toHaveLength(100);
  const input = host.querySelector<HTMLInputElement>(
    '[aria-label="跳转页码"]',
  )!;
  input.value = "21";
  input.dispatchEvent(new Event("change", { bubbles: true }));
  for (let i = 0; i < 20; i++) await Promise.resolve();
  await nextTick();
  expect(tasks.tasks).toHaveLength(1);
  expect(host.querySelectorAll(".task-row")).toHaveLength(1);
  expect(host.querySelector(".description-text")?.textContent).toBe(
    "性能验收记录 2000",
  );
  expect(
    host.querySelector<HTMLButtonElement>('[aria-label="下一页"]')?.disabled,
  ).toBe(true);
  expect(api.pageTasks).toHaveBeenLastCalledWith(
    expect.objectContaining({ page: 21, page_size: 100 }),
    expect.any(AbortSignal),
  );
});
