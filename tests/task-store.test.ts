// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, disposePinia, setActivePinia, type Pinia } from "pinia";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import { api } from "@/grid-app/api/client";
import type { Task, TaskPage } from "@/shared/types";
vi.mock("@/grid-app/api/client", () => ({
  api: {
    pageTasks: vi.fn(),
    patchTask: vi.fn(),
    deleteTask: vi.fn(),
    batchTasks: vi.fn(),
    reorderTask: vi.fn(),
    rebaseOrder: vi.fn(),
  },
}));
const task = (id: string, description = id): Task => ({
  id,
  seq: 1,
  description,
  note: "",
  project: "项目",
  type: "优化",
  status: "未开始",
  submitter: "用户",
  created_at: "2026-09-07 10:00:00",
  finished_at: null,
  updated_at: "2026-09-07 10:00:00",
  position: 1,
});
const result = (items: Task[], page = 1, total = items.length): TaskPage => ({
  items,
  page,
  total,
  page_size: 100,
  groups: [],
  anchor_found: null,
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
let pinia: Pinia;
beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  setActivePinia(pinia);
  vi.mocked(api.pageTasks).mockResolvedValue(result([]));
});
afterEach(() => {
  disposePinia(pinia);
  vi.useRealTimers();
});
describe("task request coordination", () => {
  it("rejects older responses while a new keyword is debouncing", async () => {
    const old = deferred<TaskPage>(),
      latest = deferred<TaskPage>();
    vi.mocked(api.pageTasks)
      .mockReturnValueOnce(old.promise)
      .mockReturnValueOnce(latest.promise);
    const store = useTaskStore();
    const first = store.refresh();
    store.filters.keyword = "新";
    old.resolve(result([task("old")]));
    await first;
    expect(store.tasks).toEqual([]);
    await vi.advanceTimersByTimeAsync(300);
    latest.resolve(result([task("new")]));
    await Promise.resolve();
    await Promise.resolve();
    expect(store.tasks.map((t) => t.id)).toEqual(["new"]);
    expect(window.location.search).toContain("keyword=");
  });
  it("does not let an old page response overwrite a saved task", async () => {
    const old = deferred<TaskPage>();
    vi.mocked(api.pageTasks).mockReturnValueOnce(old.promise);
    vi.mocked(api.patchTask).mockResolvedValue(task("1", "新内容"));
    const store = useTaskStore();
    store.tasks = [task("1", "旧内容")];
    store.acceptTask(store.tasks[0]);
    const first = store.refresh();
    await store.updateTask("1", { description: "新内容" });
    old.resolve(result([task("1", "旧内容")]));
    await first;
    expect(store.tasks[0].description).toBe("新内容");
  });
  it("serializes writes and continues after failure", async () => {
    const first = deferred<Task>(),
      second = deferred<Task>();
    vi.mocked(api.patchTask)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const store = useTaskStore();
    const a = store.updateTask("1", { description: "第一次" }).catch((e) => e),
      b = store.updateTask("1", { description: "第二次" });
    await Promise.resolve();
    await Promise.resolve();
    expect(api.patchTask).toHaveBeenCalledTimes(1);
    first.reject(new Error("连接失败"));
    await a;
    await Promise.resolve();
    await Promise.resolve();
    expect(api.patchTask).toHaveBeenCalledTimes(2);
    second.resolve(task("1", "第二次"));
    await b;
    expect(store.records["1"].description).toBe("第二次");
    expect(store.saving).toBe(false);
  });
  it("discards invalid URL enums including group fields", () => {
    window.history.replaceState(
      null,
      "",
      "/?status=unknown&status=进行中&type=bad&sort_by=bad&sort_order=oops&group_by=bad",
    );
    const store = useTaskStore();
    expect(store.filters.status).toEqual(["进行中"]);
    expect(store.filters.type).toEqual([]);
    expect(store.filters.sort_by).toBe("created_at");
    expect(store.filters.sort_order).toBe("desc");
    expect(store.filters.group_by).toBe("");
  });
  it("keeps selections across pages but clears on filter change", async () => {
    vi.mocked(api.pageTasks)
      .mockResolvedValueOnce(result([task("1")], 1, 101))
      .mockResolvedValueOnce(result([task("101")], 2, 101));
    const store = useTaskStore();
    await store.refresh();
    store.toggleSelection(store.tasks[0]);
    await store.setPage(2);
    store.toggleSelection(store.tasks[0]);
    expect(store.selectedIds).toEqual(["1", "101"]);
    expect(store.total).toBe(101);
    expect(store.tasks).toHaveLength(1);
    store.filters.status = ["进行中"];
    expect(store.page).toBe(1);
    expect(store.selectedIds).toEqual([]);
    expect(store.isCurrentPage).toBe(false);
  });
  it("keeps failed batch items selected and applies successful server values", async () => {
    vi.mocked(api.pageTasks).mockResolvedValue(result([task("1"), task("2")]));
    const store = useTaskStore();
    await store.refresh();
    store.tasks.forEach((t) => store.toggleSelection(t));
    vi.mocked(api.batchTasks).mockResolvedValue({
      succeeded: 1,
      failed: 1,
      results: [
        { id: "1", task: task("1", "更新") },
        { id: "2", error: { code: "not_found", message: "任务不存在" } },
      ],
    });
    vi.mocked(api.pageTasks).mockResolvedValue(result([task("1", "更新")]));
    const response = await store.applyBatch({
      action: "update",
      ids: ["1", "2"],
      patch: { status: "进行中" },
    });
    expect(response.failed).toBe(1);
    expect(store.selectedIds).toEqual(["2"]);
    expect(store.records["1"].description).toBe("更新");
    expect(store.saving).toBe(false);
  });
  it("moves a task optimistically and adopts the server position", async () => {
    vi.mocked(api.pageTasks).mockResolvedValue(
      result([task("1"), task("2"), task("3")]),
    );
    const store = useTaskStore();
    await store.refresh();
    vi.mocked(api.reorderTask).mockResolvedValue({
      ...task("3"),
      position: 1.5,
    });
    await store.moveTask("3", "1", "2");
    expect(store.tasks.map((t) => t.id)).toEqual(["1", "3", "2"]);
    expect(api.reorderTask).toHaveBeenCalledWith("3", {
      prev_id: "1",
      next_id: "2",
    });
    expect(store.records["3"].position).toBe(1.5);
    expect(store.saving).toBe(false);
  });
  it("restores the server order when a move fails", async () => {
    vi.mocked(api.pageTasks).mockResolvedValue(
      result([task("1"), task("2"), task("3")]),
    );
    const store = useTaskStore();
    await store.refresh();
    vi.mocked(api.reorderTask).mockRejectedValue(new Error("网络异常"));
    await store.moveTask("3", null, "1");
    // 乐观顺序先生效；失败后的刷新经 scheduleRefresh 延迟触发
    expect(store.tasks.map((t) => t.id)).toEqual(["3", "1", "2"]);
    await vi.advanceTimersByTimeAsync(150);
    expect(store.tasks.map((t) => t.id)).toEqual(["1", "2", "3"]);
    expect(api.reorderTask).toHaveBeenCalledWith("3", {
      prev_id: undefined,
      next_id: "1",
    });
  });
  it("rebases manual order with the current sort before switching", async () => {
    const store = useTaskStore();
    store.filters.sort_by = "updated_at";
    store.filters.sort_order = "asc";
    vi.mocked(api.rebaseOrder).mockResolvedValue(undefined);
    await store.rebaseManualOrder();
    expect(api.rebaseOrder).toHaveBeenCalledWith({
      sort_by: "updated_at",
      sort_order: "asc",
    });
    expect(store.batchBusy).toBe(false);
  });
  it("rethrows and clears the busy flag when rebase fails", async () => {
    const store = useTaskStore();
    vi.mocked(api.rebaseOrder).mockRejectedValue(new Error("网络异常"));
    await expect(store.rebaseManualOrder()).rejects.toThrow("网络异常");
    expect(store.batchBusy).toBe(false);
  });
});
