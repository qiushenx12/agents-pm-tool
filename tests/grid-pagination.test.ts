// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, h, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskGrid from "@/grid-app/components/TaskGrid.vue";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import { useViewStore } from "@/grid-app/stores/viewStore";
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
    note: "",
    submitter: "用户",
    created_at: "",
    finished_at: null,
    updated_at: "",
    position: index,
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

it("clears the active cell when clicking outside the task cells", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  const task: Task = {
    id: "task-1",
    seq: 1,
    project: "测试项目",
    type: "优化",
    status: "未开始",
    description: "单元格选择测试",
    note: "",
    submitter: "用户",
    created_at: "",
    finished_at: null,
    updated_at: "",
    position: 1,
  };
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [task],
    total: 1,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  });
  const tasks = useTaskStore(pinia);
  await tasks.refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);

  const descriptionCell = host.querySelector<HTMLElement>(
    '[data-task-id="task-1"][data-column="description"]',
  )!;
  descriptionCell.click();
  await nextTick();
  expect(descriptionCell.classList.contains("cell-selected")).toBe(true);
  expect(descriptionCell.getAttribute("aria-selected")).toBe("true");

  host.querySelector<HTMLElement>(".grid-footer")!.click();
  await nextTick();
  expect(descriptionCell.classList.contains("cell-selected")).toBe(false);
  expect(descriptionCell.getAttribute("aria-selected")).toBe("false");
  expect(document.activeElement).not.toBe(descriptionCell);
});

it("fills the table with the last column and resizes only its left neighbor", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  const task: Task = {
    id: "task-resize",
    seq: 1,
    project: "测试项目",
    type: "优化",
    status: "未开始",
    description: "列宽拖动测试",
    note: "末列备注",
    submitter: "用户",
    created_at: "",
    finished_at: null,
    updated_at: "",
    position: 1,
  };
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [task],
    total: 1,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  });
  const tasks = useTaskStore(pinia);
  await tasks.refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);

  const view = useViewStore(pinia);
  const penultimate = view.visibleColumns[view.visibleColumns.length - 2];
  const originalWidth = penultimate.width;
  const table = host.querySelector<HTMLTableElement>(".task-grid")!;
  const originalTableWidth = Number.parseFloat(table.style.width);
  const lastCell = table.querySelector<HTMLElement>("tbody .task-row td:last-child");
  const lastColumn = table.querySelector<HTMLElement>("colgroup col:last-child");
  const lastHeader = table.querySelector<HTMLElement>('thead th[data-column="note"]');
  const penultimateHeader = table.querySelector<HTMLElement>(
    `thead th[data-column="${penultimate.key}"]`,
  )!;
  expect(table.querySelector(".grid-filler")).toBeNull();
  expect(lastCell?.dataset.column).toBe("note");
  expect(lastColumn?.style.width).toBe("");
  expect(lastHeader?.querySelector(".col-resize")).toBeNull();
  expect(penultimateHeader.querySelector(".col-resize")).not.toBeNull();
  expect(
    host.querySelector('[data-column="note"] .note-cell')?.textContent,
  ).toBe("末列备注");

  penultimateHeader
    .querySelector<HTMLElement>(".col-resize")!
    .dispatchEvent(
      new MouseEvent("pointerdown", { bubbles: true, clientX: 200 }),
    );
  window.dispatchEvent(new MouseEvent("pointermove", { clientX: 240 }));
  window.dispatchEvent(new MouseEvent("pointerup"));
  await nextTick();

  expect(penultimate.width).toBe(originalWidth + 40);
  expect(Number.parseFloat(table.style.width)).toBe(originalTableWidth + 40);
});
