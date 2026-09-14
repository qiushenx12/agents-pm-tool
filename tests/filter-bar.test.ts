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

function mountBar(path = "/") {
  window.history.replaceState(null, "", path);
  pinia = createPinia();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(FilterBar);
  app.use(pinia);
  app.mount(host);
}

function popover(label: string) {
  return document.body.querySelector<HTMLElement>(
    `.ui-popover[aria-label="${label}"]`,
  );
}

function buttonWithText(root: ParentNode, text: string) {
  return Array.from(root.querySelectorAll("button")).find(
    (button) => button.textContent?.trim() === text,
  );
}

async function openFilterMenu() {
  const filterButton = Array.from(host.querySelectorAll("button")).find((button) =>
    button.textContent?.includes("筛选"),
  );
  expect(filterButton).toBeDefined();
  filterButton!.click();
  await nextTick();
}

async function addCondition(label: string) {
  const main = popover("筛选任务");
  expect(main).not.toBeNull();
  main!.querySelector<HTMLButtonElement>(".add-filter-condition")!.click();
  await nextTick();
  const choices = popover("添加筛选条件");
  expect(choices).not.toBeNull();
  const choice = buttonWithText(choices!, label);
  expect(choice).toBeDefined();
  choice!.click();
  await nextTick();
}

async function openValues(key: string, label: string) {
  const row = popover("筛选任务")!.querySelector<HTMLElement>(
    `[data-filter-key="${key}"]`,
  );
  expect(row).not.toBeNull();
  row!.querySelector<HTMLButtonElement>(".filter-value-control")!.click();
  await nextTick();
  const values = popover(`选择${label}`);
  expect(values).not.toBeNull();
  return values!;
}

it("opens an empty filter with the first field as its default condition", async () => {
  mountBar();
  await openFilterMenu();

  const main = popover("筛选任务")!;
  expect(main.textContent).toContain("设置筛选条件");
  expect(main.querySelectorAll(".filter-condition")).toHaveLength(1);
  const project = main.querySelector<HTMLElement>(
    '[data-filter-key="project"]',
  );
  expect(project).not.toBeNull();
  expect(project!.textContent).toContain("选择项目");
});

it("switches the status condition between include and exclude", async () => {
  mountBar();
  await openFilterMenu();
  await addCondition("当前状态");

  const row = popover("筛选任务")!.querySelector<HTMLElement>(
    '[data-filter-key="status"]',
  )!;
  row
    .querySelector<HTMLButtonElement>('[aria-label="切换当前状态筛选方式"]')!
    .click();
  await nextTick();
  const mode = popover("当前状态筛选方式");
  expect(mode).not.toBeNull();
  buttonWithText(mode!, "不包含")!.click();
  await nextTick();

  const values = await openValues("status", "当前状态");
  const canceled = Array.from(values.querySelectorAll("label.menu-item")).find(
    (label) => label.textContent?.trim() === "取消",
  );
  expect(canceled).toBeDefined();
  canceled!.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
  await nextTick();

  const store = useTaskStore(pinia);
  expect(store.filters.status_mode).toBe("exclude");
  expect(store.filters.status).toEqual(["取消"]);
  expect(row.textContent).toContain("不包含");
  expect(host.querySelector(".filter-chips")).toBeNull();
  expect(host.querySelector(".tool-count")?.textContent).toBe("1");
  expect(popover("筛选任务")!.textContent).toContain("清空条件");
});

it("keeps the condition row when its last checked value is cleared", async () => {
  mountBar();
  await openFilterMenu();
  await addCondition("当前状态");

  const values = await openValues("status", "当前状态");
  const canceled = Array.from(values.querySelectorAll("label.menu-item")).find(
    (label) => label.textContent?.trim() === "取消",
  );
  const checkbox = canceled!.querySelector<HTMLInputElement>(
    'input[type="checkbox"]',
  )!;
  checkbox.click();
  await nextTick();
  checkbox.click();
  await nextTick();

  const store = useTaskStore(pinia);
  expect(store.filters.status).toEqual([]);
  const row = popover("筛选任务")!.querySelector<HTMLElement>(
    '[data-filter-key="status"]',
  );
  expect(row).not.toBeNull();
  expect(row!.textContent).toContain("选择当前状态");
});

it("keeps category options and adds concrete submitters in the value picker", async () => {
  mountBar();
  const meta = useMetaStore(pinia);
  meta.submitterNames = ["Agent（主机）", "alice", "主机"];
  await nextTick();
  await openFilterMenu();
  await addCondition("提交人");
  const values = await openValues("submitter", "提交人");

  const labels = Array.from(values.querySelectorAll("label.menu-item")).map(
    (label) => label.textContent?.trim(),
  );
  expect(values.textContent).toContain("提交来源");
  expect(values.textContent).toContain("具体提交人");
  expect(labels).toContain("用户");
  expect(labels).toContain("Agent");
  expect(labels).toContain("alice");
  expect(labels).toContain("主机");
  expect(labels).toContain("Agent（主机）");

  const agentHost = Array.from(values.querySelectorAll("label.menu-item")).find(
    (label) => label.textContent?.trim() === "Agent（主机）",
  );
  agentHost!.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
  await nextTick();
  const store = useTaskStore(pinia);
  expect(store.filters.submitter).toEqual(["Agent（主机）"]);

  meta.submitterNames = ["主机"];
  await nextTick();
  const labelsAfter = Array.from(values.querySelectorAll("label.menu-item")).map(
    (label) => label.textContent?.trim(),
  );
  expect(labelsAfter).toContain("Agent（主机）");
});

it("restores concrete submitters from a share link as a condition", async () => {
  mountBar("/?submitter=用户&submitter=Agent（主机）");
  const store = useTaskStore(pinia);
  expect(store.filters.submitter).toEqual(["用户", "Agent（主机）"]);

  await openFilterMenu();
  const row = popover("筛选任务")!.querySelector<HTMLElement>(
    '[data-filter-key="submitter"]',
  );
  expect(row).not.toBeNull();
  expect(row!.textContent).toContain("2 项");
  expect(popover("筛选任务")!.querySelector('[data-filter-key="project"]')).toBeNull();
});
