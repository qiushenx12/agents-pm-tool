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
    priority: "中",
    description: "性能验收记录 " + index,
    note: "",
    submitter: index === 2000 ? "Agent" : "用户",
    submitter_name: index === 2000 ? "Agent（主机）" : "主机",
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
  const quickCreate = vi.fn();
  app = createApp({
    render: () => h(TaskGrid, { onQuickCreate: quickCreate }),
  });
  app.use(pinia);
  app.mount(host);
  host.querySelector<HTMLElement>(".grid-wrap")!.scrollTo = vi.fn();
  expect(tasks.tasks).toHaveLength(100);
  expect(host.querySelectorAll(".task-row")).toHaveLength(100);
  host.querySelector<HTMLButtonElement>(".add-record-row")!.click();
  expect(quickCreate).toHaveBeenCalledOnce();
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
  expect(host.querySelector(".submitter-label")?.textContent).toBe(
    "Agent（主机）",
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
    priority: "中",
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

it("keeps actions fixed last while notes remain a resizable data column", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  const task: Task = {
    id: "task-resize",
    seq: 1,
    project: "测试项目",
    type: "优化",
    status: "未开始",
    priority: "中",
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
  const note = view.columns.find((column) => column.key === "note")!;
  const originalWidth = note.width;
  const leftWidths = view.visibleColumns
    .filter((column) => column.key !== "note")
    .map((column) => [column.key, column.width]);
  const table = host.querySelector<HTMLTableElement>(".task-grid")!;
  const originalTableWidth = Number.parseFloat(table.style.width);
  const noteCell = table.querySelector<HTMLElement>(
    'tbody .task-row td[data-column="note"]',
  );
  const actionCell = table.querySelector<HTMLElement>(
    "tbody .task-row td:last-child",
  );
  const noteColumn = table.querySelector<HTMLElement>(
    "colgroup col:nth-last-child(2)",
  );
  const actionColumn = table.querySelector<HTMLElement>(
    "colgroup col:last-child",
  );
  const noteHeader = table.querySelector<HTMLElement>(
    'thead th[data-column="note"]',
  );
  expect(table.querySelector(".grid-filler")).toBeNull();
  expect(noteCell?.nextElementSibling).toBe(actionCell);
  expect(noteCell?.textContent).toBe("末列备注");
  expect(actionCell?.dataset.column).toBe("actions");
  expect(actionCell?.querySelector("button")?.textContent).toContain(
    "复制 Prompt",
  );
  expect(noteColumn?.style.width).toBe("240px");
  expect(actionColumn?.style.width).toBe("");
  expect(noteHeader?.draggable).toBe(true);
  expect(noteHeader?.querySelector(".col-resize")).not.toBeNull();
  expect(
    table.querySelector('[data-column="actions"] .col-resize'),
  ).toBeNull();

  noteHeader!
    .querySelector<HTMLElement>(".col-resize")!
    .dispatchEvent(
      new MouseEvent("pointerdown", { bubbles: true, clientX: 200 }),
    );
  window.dispatchEvent(new MouseEvent("pointermove", { clientX: 240 }));
  window.dispatchEvent(new MouseEvent("pointerup"));
  await nextTick();

  expect(note.width).toBe(originalWidth + 40);
  expect(
    view.visibleColumns
      .filter((column) => column.key !== "note")
      .map((column) => [column.key, column.width]),
  ).toEqual(leftWidths);
  expect(Number.parseFloat(table.style.width)).toBe(originalTableWidth + 40);
});

it("copies the concise task prompt from the ID cell button", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  const task: Task = {
    id: "202609091504000001",
    seq: 1,
    project: "agents-pm-tool",
    type: "新增需求",
    status: "未开始",
    priority: "中",
    description: "ID 字段复制按钮",
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
  const view = useViewStore(pinia);
  view.columns.find((column) => column.key === "id")!.visible = true;
  await tasks.refresh();
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);

  const idCell = host.querySelector<HTMLElement>(
    `[data-task-id="${task.id}"][data-column="id"]`,
  );
  const button = idCell?.querySelector<HTMLButtonElement>(
    ".id-prompt-button",
  );
  expect(idCell?.textContent).toContain(task.id);
  expect(button?.getAttribute("aria-label")).toBe(
    `复制任务 Prompt：${task.id}`,
  );

  button?.click();
  await vi.waitFor(() => {
    expect(writeText).toHaveBeenCalledWith(
      `请使用 pm-cli 获取任务id=${task.id}的内容并完成任务`,
    );
  });
});
