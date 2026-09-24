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

it("shows grouped rows without the group summary and global collapse action", async () => {
  window.history.replaceState(null, "", "/?group_by=status");
  localStorage.clear();
  pinia = createPinia();
  const fixtures: Task[] = [
    {
      id: "task-group-1",
      seq: 1,
      project: "测试项目",
      type: "优化",
      status: "未开始",
      priority: "中",
      description: "未开始任务",
      note: "",
      submitter: "用户",
      created_at: "",
      finished_at: null,
      updated_at: "",
      position: 1,
    },
    {
      id: "task-group-2",
      seq: 2,
      project: "测试项目",
      type: "BUG",
      status: "进行中",
      priority: "高",
      description: "进行中任务",
      note: "",
      submitter: "Agent",
      created_at: "",
      finished_at: null,
      updated_at: "",
      position: 2,
    },
  ];
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: fixtures,
    total: 2,
    page: 1,
    page_size: 100,
    groups: [
      { value: "未开始", count: 1 },
      { value: "进行中", count: 1 },
    ],
    anchor_found: null,
  });
  const tasks = useTaskStore(pinia);
  await tasks.refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);

  expect(host.querySelector(".group-strip")).toBeNull();
  expect(host.textContent).not.toContain("按当前状态分组");
  expect(host.textContent).not.toContain("全部收起");
  expect(host.querySelectorAll(".group-row")).toHaveLength(2);
  expect(host.querySelectorAll(".task-row")).toHaveLength(2);

  host
    .querySelector<HTMLButtonElement>('[aria-label="分组：未开始"]')!
    .click();
  await nextTick();
  expect(host.querySelectorAll(".task-row")).toHaveLength(1);
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
  const lastDataCell = table.querySelector<HTMLElement>(
    'tbody .task-row td[data-column="unlock_task_ids"]',
  );
  const actionCell = table.querySelector<HTMLElement>(
    "tbody .task-row td:last-child",
  );
  // 备注列之后还有 ID 与依赖列：按 visibleColumns 的位置取 colgroup 中对应的 col
  const cols = [...table.querySelectorAll<HTMLElement>("colgroup col")];
  const noteColumn =
    cols[1 + view.visibleColumns.findIndex((column) => column.key === "note")];
  const actionColumn = table.querySelector<HTMLElement>(
    "colgroup col:last-child",
  );
  const noteHeader = table.querySelector<HTMLElement>(
    'thead th[data-column="note"]',
  );
  expect(table.querySelector(".grid-filler")).toBeNull();
  // 默认排布下「父级任务 ID」是最后一个数据列，操作列固定在它右侧
  expect(lastDataCell?.nextElementSibling).toBe(actionCell);
  expect(noteCell?.textContent).toBe("末列备注");
  expect(actionCell?.dataset.column).toBe("actions");
  // 操作列两个按钮：一句话 Prompt 在左，完整 Prompt 在右
  expect(
    [...(actionCell?.querySelectorAll("button") ?? [])].map((button) =>
      button.textContent?.trim(),
    ),
  ).toEqual(["Prompt", "完整Prompt"]);
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

  const descriptionHeader = table.querySelector<HTMLElement>(
    'thead th[data-column="description"]',
  )!;
  expect(descriptionHeader.draggable).toBe(true);
  view.moveColumn("description", 1);
  await nextTick();
  expect(descriptionHeader.classList.contains("pinned-first-column")).toBe(false);
  expect(
    table
      .querySelector<HTMLElement>('thead th[data-column="project"]')
      ?.classList.contains("pinned-first-column"),
  ).toBe(true);
  expect(
    table
      .querySelector<HTMLElement>('tbody td[data-column="project"]')
      ?.classList.contains("pinned-first-column"),
  ).toBe(true);
  view.columns.find((column) => column.key === "project")!.visible = false;
  await nextTick();
  expect(
    table
      .querySelector<HTMLElement>('thead th[data-column="description"]')
      ?.classList.contains("pinned-first-column"),
  ).toBe(true);
});

it("freezes the requested left columns and adjusts to layout changes", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  const task: Task = {
    id: "task-freeze",
    seq: 1,
    project: "测试项目",
    type: "优化",
    status: "未开始",
    priority: "中",
    description: "冻结列测试",
    note: "",
    submitter: "用户",
    created_at: "",
    finished_at: null,
    updated_at: "",
    position: 1,
  };
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [task], total: 1, page: 1, page_size: 100, groups: [], anchor_found: null,
  });
  await useTaskStore(pinia).refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);
  const view = useViewStore(pinia);
  const wrap = host.querySelector<HTMLElement>(".grid-wrap")!;
  const header = (key: string) =>
    host.querySelector<HTMLElement>(`thead th[data-column="${key}"]`)!;
  const cell = (key: string) =>
    host.querySelector<HTMLElement>(`tbody td[data-column="${key}"]`)!;

  view.frozenColumns = 0;
  await nextTick();
  expect(header("description").classList.contains("pinned-column")).toBe(false);
  expect(host.querySelector("thead .pinned-index")).not.toBeNull();
  expect(host.querySelector("thead .task-actions-column")).not.toBeNull();

  view.frozenColumns = 3;
  await nextTick();
  expect(["description", "project", "type"].map((key) => header(key).style.left))
    .toEqual(["64px", "444px", "600px"]);
  expect(["description", "project", "type"].every((key) =>
    header(key).classList.contains("pinned-column") &&
    cell(key).classList.contains("pinned-column"),
  )).toBe(true);
  expect(header("priority").classList.contains("pinned-column")).toBe(false);

  view.moveColumn("type", -1);
  await nextTick();
  expect(header("type").style.left).toBe("444px");
  expect(header("project").style.left).toBe("564px");
  view.columns.find((column) => column.key === "project")!.visible = false;
  await nextTick();
  expect(header("priority").style.left).toBe("564px");

  // 冻结列的可用宽度 = 容器宽 - 索引列 - 固定操作列 - 最小可滚动宽度，
  // 所以这个模拟宽度必须留出操作列的 196px：64 + 380 要能放下，而 +120 的「任务类型」放不下
  Object.defineProperty(wrap, "clientWidth", { configurable: true, value: 800 });
  window.dispatchEvent(new Event("resize"));
  await nextTick();
  expect(header("description").classList.contains("pinned-column")).toBe(true);
  expect(header("type").classList.contains("pinned-column")).toBe(false);
  view.columns.find((column) => column.key === "description")!.width = 500;
  await nextTick();
  expect(header("description").classList.contains("pinned-column")).toBe(false);
  expect(view.frozenColumns).toBe(3);
  Object.defineProperty(wrap, "clientWidth", { configurable: true, value: 1200 });
  window.dispatchEvent(new Event("resize"));
  await nextTick();
  expect(header("priority").classList.contains("pinned-column")).toBe(true);
});

it("copies only the task ID from the ID cell button", async () => {
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
    ".id-copy-button",
  );
  expect(idCell?.textContent).toContain(task.id);
  expect(button?.getAttribute("aria-label")).toBe(
    `复制任务 ID：${task.id}`,
  );

  button?.click();
  await vi.waitFor(() => {
    // ID 单元格只复制 ID 本身，一句话 Prompt 已移到操作列
    expect(writeText).toHaveBeenCalledWith(task.id);
  });
});

it("shows the page size remembered from the last visit", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  // 上次选了 50 条/页，重开页面应当沿用
  localStorage.setItem("pm-table-page-size-v1", "50");
  pinia = createPinia();
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [],
    total: 0,
    page: 1,
    page_size: 50,
    groups: [],
    anchor_found: null,
  });
  const tasks = useTaskStore(pinia);
  expect(tasks.pageSize).toBe(50);
  await tasks.refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);
  expect(
    host.querySelector(".page-size-button")?.textContent,
  ).toContain("50 条/页");
});

it("opens the action button list from the header menu arrow and hides the picked button", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  const task: Task = {
    id: "202609091504000002",
    seq: 2,
    project: "agents-pm-tool",
    type: "优化",
    status: "未开始",
    priority: "中",
    description: "操作列按钮显隐",
    note: "",
    submitter: "用户",
    created_at: "",
    finished_at: null,
    updated_at: "",
    position: 2,
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

  const labels = () =>
    [...host.querySelectorAll('tbody .task-row td[data-column="actions"] button')]
      .map((button) => button.textContent?.trim());
  expect(labels()).toEqual(["Prompt", "完整Prompt"]);

  // 与其它字段同款：表头右侧的小箭头（.column-menu，悬停才显形），左键展开
  const header = host.querySelector<HTMLElement>('thead th[data-column="actions"]')!;
  expect(header.querySelector(".column-heading .column-menu")).not.toBeNull();
  expect(header.querySelector(".column-menu")?.getAttribute("aria-label")).toBe(
    "操作列按钮",
  );
  header.querySelector<HTMLButtonElement>(".column-menu")!.click();
  await nextTick();
  await nextTick();
  const boxes = [
    ...document.body.querySelectorAll<HTMLInputElement>(
      '.ui-popover input[type="checkbox"]',
    ),
  ];
  expect(boxes.map((box) => box.checked)).toEqual([true, true]);

  // 取消「Prompt」后行内只剩「完整Prompt」
  boxes[0].click();
  await nextTick();
  expect(labels()).toEqual(["完整Prompt"]);
  // 只剩一个时另一个不可取消
  expect(boxes[1].disabled).toBe(true);
  document.body.querySelector(".ui-popover")?.remove();
});

it("sizes the actions column to the buttons that are visible", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  const task: Task = {
    id: "202609091504000003",
    seq: 3,
    project: "agents-pm-tool",
    type: "优化",
    status: "未开始",
    priority: "中",
    description: "操作列宽度跟随按钮",
    note: "",
    submitter: "用户",
    created_at: "",
    finished_at: null,
    updated_at: "",
    position: 3,
  };
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [task],
    total: 1,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  });
  // jsdom 没有布局：给量尺里的按钮桩上真实宽度（Prompt 74 / 完整Prompt 98），其余元素保持原样
  const original = Element.prototype.getBoundingClientRect;
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(
    function (this: Element) {
      if (this instanceof HTMLButtonElement && this.closest(".action-measure"))
        return {
          width: this.textContent?.includes("完整") ? 98 : 74,
        } as DOMRect;
      return original.call(this);
    },
  );
  const tasks = useTaskStore(pinia);
  await tasks.refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);
  await nextTick();
  await nextTick();

  const view = useViewStore(pinia);
  const indexWidth = 64; // 与 TaskGrid 的 INDEX_COLUMN_WIDTH 一致
  const actionsWidth = () =>
    Number.parseFloat(
      host.querySelector<HTMLElement>(".task-grid")!.style.width,
    ) - view.visibleColumns.reduce((n, c) => n + c.width, indexWidth);

  // 74 + 98 + 4 间距 + 16 内边距
  expect(actionsWidth()).toBe(192);
  expect(host.querySelectorAll(".action-measure button")).toHaveLength(2);

  // 隐藏「Prompt」后列宽跟着收窄，不再留一大片空白
  view.toggleRowAction("copy-one-line-prompt");
  await nextTick();
  await nextTick();
  expect(host.querySelectorAll(".action-measure button")).toHaveLength(1);
  expect(actionsWidth()).toBe(98 + 16);

  // 恢复显示后回到两个按钮的宽度
  view.toggleRowAction("copy-one-line-prompt");
  await nextTick();
  await nextTick();
  expect(actionsWidth()).toBe(192);
});

it("shows the insertion column while dragging a table header", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [],
    total: 0,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  });
  const tasks = useTaskStore(pinia);
  await tasks.refresh();
  const view = useViewStore(pinia);
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);

  const source = host.querySelector<HTMLElement>('thead th[data-column="note"]')!;
  const target = host.querySelector<HTMLElement>('thead th[data-column="priority"]')!;
  const actions = host.querySelector<HTMLElement>('thead th[data-column="actions"]')!;
  const grid = host.querySelector<HTMLElement>(".grid-wrap")!;
  const table = host.querySelector<HTMLTableElement>(".task-grid")!;
  Object.defineProperty(table, "offsetHeight", { value: 600 });
  vi.spyOn(grid, "getBoundingClientRect").mockReturnValue({
    left: 10,
    top: 20,
  } as DOMRect);
  vi.spyOn(table, "getBoundingClientRect").mockReturnValue({
    top: 20,
  } as DOMRect);
  vi.spyOn(target, "getBoundingClientRect").mockReturnValue({
    left: 100,
  } as DOMRect);
  vi.spyOn(actions, "getBoundingClientRect").mockReturnValue({
    left: 500,
  } as DOMRect);
  source.dispatchEvent(new Event("dragstart", { bubbles: true, cancelable: true }));
  target.dispatchEvent(new Event("dragover", { bubbles: true, cancelable: true }));
  await nextTick();
  expect(host.querySelector<HTMLElement>(".column-insertion-line")?.style).toMatchObject({
    left: "90px",
    top: "0px",
    height: "600px",
  });
  expect(target.classList.contains("column-drop-before")).toBe(false);

  actions.dispatchEvent(new Event("dragover", { bubbles: true, cancelable: true }));
  await nextTick();
  expect(host.querySelector<HTMLElement>(".column-insertion-line")?.style.left).toBe(
    "490px",
  );

  actions.dispatchEvent(new Event("drop", { bubbles: true, cancelable: true }));
  await nextTick();
  expect(host.querySelector(".column-insertion-line")).toBeNull();
  expect(view.visibleColumns.at(-1)?.key).toBe("note");

  const description = host.querySelector<HTMLElement>(
    'thead th[data-column="description"]',
  )!;
  description.dispatchEvent(
    new Event("dragstart", { bubbles: true, cancelable: true }),
  );
  actions.dispatchEvent(new Event("dragover", { bubbles: true, cancelable: true }));
  actions.dispatchEvent(new Event("drop", { bubbles: true, cancelable: true }));
  await nextTick();
  expect(view.visibleColumns.at(-1)?.key).toBe("description");
});
