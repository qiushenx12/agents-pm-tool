// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createPinia, disposePinia, setActivePinia, type Pinia } from "pinia";
import { createApp, nextTick, type App } from "vue";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import SavedViews from "@/grid-app/components/SavedViews.vue";
import { useMetaStore } from "@/grid-app/stores/metaStore";
import { useTaskStore } from "@/grid-app/stores/taskStore";

const { pageTasks } = vi.hoisted(() => ({
  pageTasks: vi.fn().mockResolvedValue({
    items: [],
    page: 1,
    page_size: 100,
    total: 0,
    groups: [],
    anchor_found: null,
  }),
}));
vi.mock("@/grid-app/api/client", () => ({ api: { pageTasks } }));

let app: App | undefined;
let host: HTMLElement;
let pinia: Pinia;
let style: HTMLStyleElement;

beforeEach(() => {
  localStorage.clear();
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  setActivePinia(pinia);
  host = document.createElement("div");
  document.body.append(host);
  style = document.createElement("style");
  // jsdom does not ship the browser's native button face; model it so the regression is observable.
  style.textContent = "button { background: rgb(190, 190, 190); }\n" +
    readFileSync(resolve("src/grid-app/grid.css"), "utf8");
  document.head.append(style);
});

afterEach(() => {
  app?.unmount();
  app = undefined;
  host.remove();
  style.remove();
  disposePinia(pinia);
  vi.restoreAllMocks();
});

it("keeps the graph tab transparent and leaves only the active tab underlined", () => {
  app = createApp(SavedViews, { mode: "graph" });
  app.mount(host);
  const graph = host.querySelector<HTMLButtonElement>(".graph-view-tab")!;
  const table = host.querySelector<HTMLButtonElement>(".view-selector")!;
  expect(getComputedStyle(graph).backgroundColor).toBe("rgba(0, 0, 0, 0)");
  expect(getComputedStyle(graph).borderBottomColor).not.toBe("transparent");
  expect(getComputedStyle(table).borderBottomColor).toBe("rgba(0, 0, 0, 0)");
});

it("switches back from the graph without popping the saved-views list", async () => {
  const onMode = vi.fn();
  app = createApp(SavedViews, { mode: "graph", "onUpdate:mode": onMode });
  app.mount(host);

  host.querySelector<HTMLButtonElement>("[aria-label=切换筛选方案]")!.click();
  await nextTick();
  await nextTick();

  expect(onMode).toHaveBeenCalledWith("table");
  expect(document.body.querySelector(".ui-popover")).toBeNull();
  expect(document.body.textContent).not.toContain("保存当前条件为新方案");
});

it("switches to the graph without popping the project list", async () => {
  const onMode = vi.fn();
  app = createApp(SavedViews, { mode: "table", "onUpdate:mode": onMode });
  app.mount(host);

  host.querySelector<HTMLButtonElement>("[aria-label=关联图]")!.click();
  await nextTick();
  await nextTick();

  expect(onMode).toHaveBeenCalledWith("graph");
  expect(document.body.querySelector(".ui-popover")).toBeNull();
  expect(document.body.textContent).not.toContain("按项目筛选");
});

it("opens the project filter from the graph tab, sharing the table filter state", async () => {
  useMetaStore().projects = [
    { name: "项目甲", color: "#111111", sort_order: 0, local_path: "", git_url: "", created_at: "" },
    { name: "项目乙", color: "#222222", sort_order: 1, local_path: "", git_url: "", created_at: "" },
  ];
  const onMode = vi.fn();
  app = createApp(SavedViews, { mode: "graph", "onUpdate:mode": onMode });
  app.mount(host);
  const tab = host.querySelector<HTMLButtonElement>("[aria-label=关联图]")!;
  expect(tab.textContent).toContain("关联图");

  // 已处于关联图：单击页签开合项目下拉，不再发出切换
  tab.click();
  await nextTick();
  await nextTick();
  expect(onMode).not.toHaveBeenCalled();
  const items = [...document.body.querySelectorAll<HTMLButtonElement>(".ui-popover .menu-item")];
  expect(items.map((item) => item.textContent)).toEqual(
    expect.arrayContaining([
      expect.stringContaining("全部项目"),
      expect.stringContaining("项目甲"),
      expect.stringContaining("项目乙"),
    ]),
  );

  items.find((item) => item.textContent?.includes("项目乙"))!.click();
  await nextTick();
  const tasks = useTaskStore();
  // 写的就是任务表的项目筛选：侧栏高亮、URL、本地保存都跟着它走
  expect(tasks.filters.project).toEqual(["项目乙"]);
  expect(window.location.search).toContain("project");
  expect(JSON.parse(localStorage.getItem("pm-grid-filters-v1")!).project).toEqual(["项目乙"]);
  // 页签像任务表显示方案名一样显示当前项目
  expect(tab.textContent).toContain("项目乙");
  expect(tab.textContent).not.toContain("关联图");
  expect(document.body.querySelector(".ui-popover")).toBeNull();

  // 再点页签重新打开，选「全部项目」清除筛选
  tab.click();
  await nextTick();
  await nextTick();
  [...document.body.querySelectorAll<HTMLButtonElement>(".ui-popover .menu-item")]
    .find((item) => item.textContent?.includes("全部项目"))!
    .click();
  await nextTick();
  expect(tasks.filters.project).toEqual([]);
  expect(tab.textContent).toContain("关联图");
});

it("keeps a deleted project selectable in the graph tab list", async () => {
  useTaskStore().setProject("已删项目");
  useMetaStore().projects = [
    { name: "项目甲", color: "#111111", sort_order: 0, local_path: "", git_url: "", created_at: "" },
  ];
  app = createApp(SavedViews, { mode: "graph" });
  app.mount(host);
  const tab = host.querySelector<HTMLButtonElement>("[aria-label=关联图]")!;
  expect(tab.textContent).toContain("已删项目");

  tab.click();
  await nextTick();
  await nextTick();
  const labels = [...document.body.querySelectorAll(".ui-popover .menu-item")].map(
    (item) => item.textContent,
  );
  expect(labels).toEqual(
    expect.arrayContaining([
      expect.stringContaining("全部项目"),
      expect.stringContaining("项目甲"),
      expect.stringContaining("已删项目"),
    ]),
  );
});

it("shows a graph tab in place of the external save button while keeping save inside the list", async () => {
  const onMode = vi.fn();
  app = createApp(SavedViews, { mode: "table", "onUpdate:mode": onMode });
  app.mount(host);

  expect(host.querySelector(".save-view-button")).toBeNull();
  const graph = host.querySelector<HTMLButtonElement>("[role=tab][aria-label=关联图]");
  expect(graph).not.toBeNull();
  graph!.click();
  expect(onMode).toHaveBeenCalledWith("graph");

  host.querySelector<HTMLButtonElement>("[aria-label=切换筛选方案]")!.click();
  await nextTick();
  expect(document.body.textContent).toContain("保存当前条件为新方案");
});
