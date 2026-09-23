// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createPinia, disposePinia, setActivePinia, type Pinia } from "pinia";
import { nextTick } from "vue";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import { useSavedViewStore } from "@/grid-app/stores/savedViewStore";
import { useViewStore } from "@/grid-app/stores/viewStore";
vi.mock("@/grid-app/api/client", () => ({
  api: {
    pageTasks: vi
      .fn()
      .mockResolvedValue({
        items: [],
        page: 1,
        page_size: 100,
        total: 0,
        groups: [],
        anchor_found: null,
      }),
  },
}));
let pinia: Pinia;
beforeEach(() => {
  vi.useFakeTimers();
  localStorage.clear();
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  setActivePinia(pinia);
});
afterEach(() => {
  disposePinia(pinia);
  vi.restoreAllMocks();
  vi.useRealTimers();
});
it("saves independent snapshots, tracks changes, updates, renames and removes local views", () => {
  const tasks = useTaskStore(),
    saved = useSavedViewStore();
  tasks.filters.status = ["待验证"];
  tasks.filters.group_by = "project";
  saved.save("待验证按项目");
  const id = saved.activeId;
  tasks.filters.status.push("进行中");
  expect(saved.dirty).toBe(true);
  expect(saved.views[0].filters.status).toEqual(["待验证"]);
  saved.activate(id);
  expect(tasks.filters.status).toEqual(["待验证"]);
  expect(tasks.filters.group_by).toBe("project");
  expect(saved.dirty).toBe(false);
  tasks.filters.keyword = "页面";
  saved.update();
  saved.rename(id, "页面验收");
  expect(
    JSON.parse(localStorage.getItem("pm-saved-views-v1")!)[0].filters.keyword,
  ).toBe("页面");
  expect(() => saved.save("页面验收")).toThrow("同名");
  saved.remove(id);
  expect(saved.views).toHaveLength(0);
  expect(saved.activeId).toBe("");
});
it("never claims a saved view after storage failure", () => {
  const saved = useSavedViewStore();
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("quota");
  });
  expect(() => saved.save("不能保存")).toThrow("无法保存");
  expect(saved.views).toHaveLength(0);
});
it("default columns follow the task field order and are all visible", () => {
  const view = useViewStore();
  // 任务 202609231148520000：新装用户的默认列顺序；操作列固定最后，不在此列表内
  expect(view.columns.map((column) => column.key)).toEqual([
    "description",
    "project",
    "type",
    "priority",
    "status",
    "submitter",
    "assignee",
    "attachments",
    "created_at",
    "finished_at",
    "note",
    "id",
    "predecessor_task_ids",
    "unlock_task_ids",
  ]);
  expect(view.columns.every((column) => column.visible)).toBe(true);
  expect(view.columns.slice(-2).map((column) => column.label)).toEqual([
    "子任务 ID",
    "父级任务 ID",
  ]);
});

it("preserves saved order while allowing description to move and migrating old preferences", async () => {
  localStorage.setItem(
    "pm-table-view-v1",
    JSON.stringify({
      columns: [
        { key: "status", visible: true, width: 130 },
        null,
        { key: "project", visible: true, width: 150 },
        // 用户手动隐藏的列不受「默认全部展示」影响
        { key: "finished_at", visible: false, width: 172 },
        { key: "obsolete" },
      ],
    }),
  );
  const view = useViewStore();
  expect(view.columns[0].key).toBe("status");
  expect(view.columns[1].key).toBe("project");
  expect(view.columns[3].key).toBe("description");
  expect(view.columns.find((c) => c.key === "finished_at")!.visible).toBe(
    false,
  );
  view.moveBefore("description", "status");
  expect(view.columns[0].key).toBe("description");
  view.moveColumn("description", 1);
  expect(view.columns[0].key).toBe("status");
  view.moveBefore("project", "status");
  view.moveBefore("description", "actions");
  await nextTick();
  expect(view.columns[0].key).toBe("project");
  expect(view.columns.at(-1)?.key).toBe("description");
  const persisted = JSON.parse(localStorage.getItem("pm-table-view-v1")!);
  expect(persisted.columns[0].key).toBe("project");
  expect(persisted.columns.at(-1).key).toBe("description");
  expect(view.columns.some((c) => c.key === "obsolete")).toBe(false);
  expect(view.columns).toHaveLength(14);
  expect(view.columns.find((c) => c.key === "note")).toMatchObject({
    key: "note",
    label: "备注",
    visible: true,
  });
  expect(view.visibleColumns.map((column) => column.key)).toContain("note");
  view.moveBefore("note", "actions");
  expect(view.columns[view.columns.length - 1]?.key).toBe("note");
});
