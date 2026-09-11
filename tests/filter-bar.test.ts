// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";
import { createApp, nextTick } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import FilterBar from "@/grid-app/components/FilterBar.vue";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import { useMetaStore } from "@/grid-app/stores/metaStore";

let pinia: Pinia;
let host: HTMLElement;
let app: ReturnType<typeof createApp>;

afterEach(() => {
  app?.unmount();
  disposePinia(pinia);
  host?.remove();
  document.body.querySelectorAll(".ui-popover").forEach((node) => node.remove());
  localStorage.clear();
  window.history.replaceState(null, "", "/");
});

function mountBar() {
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(FilterBar);
  app.use(pinia);
  app.mount(host);
}

async function openFilterMenu() {
  const filterButton = Array.from(host.querySelectorAll("button")).find((button) =>
    button.textContent?.includes("筛选"),
  );
  expect(filterButton).toBeDefined();
  filterButton!.click();
  await nextTick();
}

it("switches the status filter between include and exclude", async () => {
  mountBar();
  await openFilterMenu();

  const mode = document.body.querySelector('[aria-label="状态筛选方式"]');
  expect(mode).not.toBeNull();
  const exclude = Array.from(mode!.querySelectorAll("button")).find(
    (button) => button.textContent?.trim() === "不包含",
  );
  exclude!.click();
  await nextTick();

  const canceled = Array.from(document.body.querySelectorAll("label.menu-item")).find(
    (label) => label.textContent?.trim() === "取消",
  );
  expect(canceled).toBeDefined();
  canceled!.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
  await nextTick();

  const store = useTaskStore(pinia);
  expect(store.filters.status_mode).toBe("exclude");
  expect(store.filters.status).toEqual(["取消"]);
  expect(host.textContent).toContain("当前状态 · 不包含");
});

it("keeps category options and adds concrete submitters for the submitter filter", async () => {
  mountBar();
  const meta = useMetaStore(pinia);
  meta.submitterNames = ["Agent（主机）", "alice", "主机"];
  await nextTick();
  await openFilterMenu();

  const labels = Array.from(
    document.body.querySelectorAll("label.menu-item"),
  ).map((label) => label.textContent?.trim());
  // 大类保留
  expect(labels).toContain("用户");
  expect(labels).toContain("Agent");
  // 具体提交人出现在「提交人」小节，与任务上的 submitter_name 形态一致
  expect(document.body.textContent).toContain("提交人");
  expect(labels).toContain("alice");
  expect(labels).toContain("主机");
  expect(labels).toContain("Agent（主机）");

  // 勾选 Agent（主机） → 写入 submitter 筛选
  const agentHost = Array.from(document.body.querySelectorAll("label.menu-item")).find(
    (label) => label.textContent?.trim() === "Agent（主机）",
  );
  agentHost!.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
  await nextTick();
  const store = useTaskStore(pinia);
  expect(store.filters.submitter).toEqual(["Agent（主机）"]);

  // 已选提交人即使不在候选名单里也继续显示（候选名单随后端收窄）
  meta.submitterNames = ["主机"];
  await nextTick();
  const labelsAfter = Array.from(
    document.body.querySelectorAll("label.menu-item"),
  ).map((label) => label.textContent?.trim());
  expect(labelsAfter).toContain("Agent（主机）");
});

it("restores concrete submitters from the share link into the submitter filter", () => {
  window.history.replaceState(null, "", "/?submitter=用户&submitter=Agent（主机）");
  pinia = createPinia();
  const store = useTaskStore(pinia);
  expect(store.filters.submitter).toEqual(["用户", "Agent（主机）"]);
});
