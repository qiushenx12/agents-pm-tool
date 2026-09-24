// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createApp, h, nextTick, ref, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskGrid from "@/grid-app/components/TaskGrid.vue";
import TaskDetailDrawer from "@/grid-app/components/TaskDetailDrawer.vue";
import { api } from "@/grid-app/api/client";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import type { Task } from "@/shared/types";

vi.mock("@/grid-app/api/client", () => ({
  api: {
    pageTasks: vi.fn(),
    listTasks: vi.fn(),
    patchTask: vi.fn(),
    getTask: vi.fn(),
    listAttachments: vi.fn(),
  },
}));

const makeTask = (id: string, extra: Partial<Task> = {}): Task => ({
  id,
  seq: 1,
  project: "测试项目",
  type: "优化",
  description: "任务 " + id,
  note: "",
  status: "未开始",
  priority: "中",
  submitter: "用户",
  created_at: "",
  finished_at: null,
  updated_at: "",
  position: 1,
  ...extra,
});

const taskA = makeTask("task-a", { predecessor_task_ids: ["task-b"] });
const taskB = makeTask("task-b", { seq: 2, position: 2 });

let app: App | undefined;
let pinia: Pinia | undefined;
let host: HTMLElement | undefined;

afterEach(() => {
  app?.unmount();
  if (pinia) disposePinia(pinia);
  host?.remove();
  document.body.innerHTML = "";
  localStorage.clear();
  vi.clearAllMocks();
});

async function openDropdownAndLocate(trigger: HTMLButtonElement) {
  trigger.click();
  await vi.waitFor(() =>
    expect(
      document.body.querySelectorAll('[role="option"]').length,
    ).toBeGreaterThan(0),
  );
  const option = Array.from(
    document.body.querySelectorAll<HTMLButtonElement>('[role="option"]'),
  ).find((button) => button.textContent?.includes("task-b"))!;
  option.dispatchEvent(
    new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
      clientX: 120,
      clientY: 160,
    }),
  );
  await vi.waitFor(() => {
    expect(
      Array.from(
        document.body.querySelectorAll<HTMLButtonElement>(".menu-item"),
      ).some((button) => button.textContent?.includes("定位任务")),
    ).toBe(true);
  });
  Array.from(
    document.body.querySelectorAll<HTMLButtonElement>(".menu-item"),
  )
    .find((button) => button.textContent?.includes("定位任务"))!
    .click();
}

function expectDescriptionCellSelected() {
  return vi.waitFor(() => {
    const cellB = host!.querySelector<HTMLElement>(
      '[data-task-id="task-b"][data-column="description"]',
    );
    expect(cellB?.getAttribute("aria-selected")).toBe("true");
  });
}

beforeEach(() => {
  window.history.replaceState(null, "", "/");
  // jsdom 未实现 scrollIntoView
  HTMLElement.prototype.scrollIntoView = vi.fn();
  vi.mocked(api.pageTasks).mockImplementation(async (q) => ({
    items: [taskA, taskB],
    total: 2,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: q?.anchor_id ? true : null,
  }));
  vi.mocked(api.listTasks).mockResolvedValue([taskA, taskB]);
  vi.mocked(api.getTask).mockResolvedValue(taskA);
  vi.mocked(api.listAttachments).mockResolvedValue([]);
});

it("locates a dependency task from the grid cell dropdown and selects its description cell", async () => {
  pinia = createPinia();
  await useTaskStore(pinia).refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);

  // 表格里 task-a 行的子任务 ID 单元格内联下拉（用户实际操作的入口）
  const cell = host.querySelector<HTMLElement>(
    '[data-task-id="task-a"][data-column="predecessor_task_ids"]',
  )!;
  const trigger = cell.querySelector<HTMLButtonElement>(".dependency-trigger")!;
  await openDropdownAndLocate(trigger);

  await vi.waitFor(() =>
    expect(api.pageTasks).toHaveBeenCalledWith(
      expect.objectContaining({ anchor_id: "task-b" }),
      expect.anything(),
    ),
  );
  await expectDescriptionCellSelected();
});

it("forwards locate from the detail drawer and selects the description cell", async () => {
  pinia = createPinia();
  await useTaskStore(pinia).refresh();

  // 复刻 GridApp 的接线：抽屉 locate → reveal
  const grid = ref<InstanceType<typeof TaskGrid>>();
  async function locateTask(id: string) {
    await nextTick();
    await grid.value?.reveal(id);
  }
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({
    setup: () => () => [
      h(TaskGrid, { ref: grid }),
      h(TaskDetailDrawer, { task: taskA, onLocate: locateTask }),
    ],
  });
  app.use(pinia);
  app.mount(host);
  await vi.waitFor(() =>
    expect(
      document.body.querySelector(".drawer-panel .dependency-trigger"),
    ).not.toBeNull(),
  );

  const trigger = document.body.querySelector<HTMLButtonElement>(
    ".drawer-panel .dependency-trigger",
  )!;
  await openDropdownAndLocate(trigger);

  await vi.waitFor(() =>
    expect(api.pageTasks).toHaveBeenCalledWith(
      expect.objectContaining({ anchor_id: "task-b" }),
      expect.anything(),
    ),
  );
  await expectDescriptionCellSelected();
});
