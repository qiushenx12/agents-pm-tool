// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, h, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskGrid from "@/grid-app/components/TaskGrid.vue";
import { api } from "@/grid-app/api/client";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import type { Task } from "@/shared/types";

vi.mock("@/grid-app/api/client", () => ({
  api: { pageTasks: vi.fn() },
}));

function task(id: string, linked = false): Task {
  return {
    id,
    seq: Number(id),
    project: "测试项目",
    type: "优化",
    description: `任务 ${id}`,
    note: "",
    status: "未开始",
    priority: "中",
    submitter: "用户",
    created_at: "",
    finished_at: null,
    updated_at: "",
    position: Number(id),
    predecessor_task_ids: linked ? ["2"] : [],
    unlock_task_ids: [],
  };
}

let app: App | undefined;
let pinia: Pinia;
let host: HTMLElement;
afterEach(() => {
  app?.unmount();
  host?.remove();
  disposePinia(pinia);
  localStorage.clear();
  vi.clearAllMocks();
});

async function mountGrid(onShowRelationGraph = vi.fn()) {
  window.history.replaceState(null, "", "/");
  const items = [task("1", true), { ...task("2"), unlock_task_ids: ["1"] }, task("3")];
  vi.mocked(api.pageTasks).mockResolvedValue({
    items,
    total: items.length,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  });
  pinia = createPinia();
  await useTaskStore(pinia).refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid, { onShowRelationGraph }) });
  app.use(pinia);
  app.mount(host);
  return onShowRelationGraph;
}

function menu() {
  return document.body.querySelector<HTMLElement>('.ui-popover[aria-label="任务操作"]');
}

it("offers the relation graph only on rows with visible dependencies", async () => {
  const showGraph = await mountGrid();
  host.querySelector<HTMLButtonElement>('[aria-label="任务操作：1"]')!.click();
  await vi.waitFor(() => expect(menu()).not.toBeNull());
  const action = Array.from(menu()!.querySelectorAll<HTMLButtonElement>("button"))
    .find((button) => button.textContent?.includes("展示关联图"))!;
  expect(action).not.toBeNull();
  action.click();
  expect(showGraph).toHaveBeenCalledWith(expect.objectContaining({ id: "1" }));

  host.querySelector<HTMLButtonElement>('[aria-label="任务操作：2"]')!.click();
  await vi.waitFor(() => expect(menu()).not.toBeNull());
  expect(menu()!.textContent).toContain("展示关联图");

  host.querySelector<HTMLButtonElement>('[aria-label="任务操作：3"]')!.click();
  await vi.waitFor(() => expect(menu()).not.toBeNull());
  expect(menu()!.textContent).not.toContain("展示关联图");
});

it("opens the same row actions at the pointer when right-clicking any task cell", async () => {
  await mountGrid();
  const status = host.querySelector<HTMLElement>('[data-task-id="1"][data-column="status"]')!;
  const event = new MouseEvent("contextmenu", {
    bubbles: true,
    cancelable: true,
    clientX: 220,
    clientY: 190,
  });
  status.dispatchEvent(event);
  await vi.waitFor(() => expect(menu()).not.toBeNull());
  expect(event.defaultPrevented).toBe(true);
  expect(menu()!.textContent).toContain("展示关联图");
  expect(menu()!.style.left).toBe("220px");

  const note = host.querySelector<HTMLElement>('[data-task-id="3"][data-column="note"]')!;
  note.dispatchEvent(new MouseEvent("contextmenu", {
    bubbles: true, cancelable: true,
    clientX: 300, clientY: 230,
  }));
  await vi.waitFor(() => expect(menu()?.textContent).not.toContain("展示关联图"));
  expect(document.body.querySelectorAll('.ui-popover[aria-label="任务操作"]')).toHaveLength(1);
});
