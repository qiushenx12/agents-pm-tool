// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createApp, h, nextTick, reactive, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskHistory from "@/grid-app/components/TaskHistory.vue";
import { api } from "@/grid-app/api/client";
import type { TaskHistoryPage } from "@/shared/types";
vi.mock("@/grid-app/api/client", () => ({ api: { taskHistory: vi.fn() } }));
let app: App, pinia: Pinia, host: HTMLElement;
const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); await nextTick(); };
const page: TaskHistoryPage = { items: [{ operation_id: 12, actor_name: "小明", source: "agent", action: "update", created_at: "2026-09-24 12:00:00", changes: [
  { field: "description", before: "旧描述", after: "新描述<script>bad</script>" },
  { field: "status", before: "未开始", after: "进行中" },
] }], next_before: 12 };
function mount() {
  const state = reactive({ taskId: "one", revision: "1" });
  host = document.createElement("div"); document.body.append(host);
  pinia = createPinia(); app = createApp({ render: () => h(TaskHistory, state) }); app.use(pinia).mount(host);
  return state;
}
beforeEach(() => { vi.resetAllMocks(); window.history.replaceState(null, "", "/"); });
afterEach(() => { app?.unmount(); if (pinia) disposePinia(pinia); host?.remove(); });
it("shows actor and field differences as plain text, and paginates", async () => {
  vi.mocked(api.taskHistory).mockResolvedValueOnce(page).mockResolvedValueOnce({ items: [], next_before: null });
  mount(); await flush();
  expect(host.textContent).toContain("Agent（小明）"); expect(host.textContent).toContain("旧描述");
  expect(host.textContent).toContain("新描述<script>bad</script>"); expect(host.querySelector("script")).toBeNull();
  expect(host.querySelectorAll("tbody tr")).toHaveLength(2);
  const arrows = host.querySelectorAll("tbody td.history-arrow");
  expect(arrows).toHaveLength(2);
  expect(arrows[0].textContent).toBe("→");
  const firstRowCells = host.querySelectorAll("tbody tr")[0].querySelectorAll("td");
  expect(firstRowCells[3].textContent).toContain("旧描述");
  expect(firstRowCells[4].classList.contains("history-arrow")).toBe(true);
  expect(firstRowCells[5].textContent).toContain("新描述");
  (host.querySelector(".history-more") as HTMLButtonElement).click(); await flush();
  expect(api.taskHistory).toHaveBeenLastCalledWith("one", 12);
  expect(host.querySelector(".history-more")).toBeNull();
});
it("ignores a stale response when switching tasks, and exposes retry and empty state", async () => {
  let resolveOld!: (page: TaskHistoryPage) => void;
  vi.mocked(api.taskHistory).mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve; }))
    .mockRejectedValueOnce(new Error("无法加载"))
    .mockResolvedValueOnce({ items: [], next_before: null });
  const state = mount(); state.taskId = "two"; await flush(); resolveOld(page); await flush();
  expect(host.textContent).toContain("无法加载"); expect(host.textContent).not.toContain("小明");
  (host.querySelector('[role="alert"] button') as HTMLButtonElement).click(); await flush();
  expect(api.taskHistory).toHaveBeenLastCalledWith("two", undefined);
  expect(host.textContent).toContain("暂无可见的操作历史");
});
