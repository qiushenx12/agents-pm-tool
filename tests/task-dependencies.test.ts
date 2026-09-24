// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";
import { createApp, h, nextTick, ref, type App, type Ref } from "vue";
import TaskMultiSelect from "@/grid-app/components/TaskMultiSelect.vue";
import type { Task } from "@/shared/types";

let app: App | undefined;
let host: HTMLElement | undefined;

const task = (id: string, description: string): Task => ({
  id,
  seq: Number(id),
  project: "测试项目",
  type: "优化",
  description,
  note: "",
  status: "未开始",
  priority: "中",
  submitter: "用户",
  created_at: "",
  finished_at: null,
  updated_at: "",
  position: Number(id),
});

const flush = async () => {
  for (let i = 0; i < 12; i++) await Promise.resolve();
  await nextTick();
};

function mountSelect(selected: Ref<string[]>, located: string[] = []) {
  const options = [
    task("1", "子任务一"),
    task("2", "子任务二"),
    task("3", "子任务三"),
  ];
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({
    setup: () => () =>
      h(TaskMultiSelect, {
        modelValue: selected.value,
        options,
        label: "子任务 ID",
        "onUpdate:modelValue": (value: string[]) => (selected.value = value),
        onLocate: (id: string) => located.push(id),
      }),
  });
  app.mount(host);
  return host.querySelector<HTMLButtonElement>(".dependency-trigger")!;
}

function optionButton(id: string) {
  return Array.from(
    document.body.querySelectorAll<HTMLButtonElement>('[role="option"]'),
  ).find((button) => button.querySelector("strong")?.textContent === id)!;
}

function optionIds() {
  return Array.from(
    document.body.querySelectorAll<HTMLButtonElement>('[role="option"]'),
  ).map((button) => button.querySelector("strong")?.textContent);
}

function searchInput() {
  return document.body.querySelector<HTMLInputElement>(
    ".ui-popover .menu-search input",
  )!;
}

async function typeSearch(value: string) {
  const input = searchInput();
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await flush();
}

function emptyText() {
  return (
    document.body.querySelector(".ui-popover .menu-empty")?.textContent?.trim() ||
    ""
  );
}

function clearButton() {
  return (
    document.body.querySelector<HTMLButtonElement>(
      ".ui-popover .menu-search-clear",
    ) ?? undefined
  );
}

function locateMenuItem() {
  return Array.from(
    document.body.querySelectorAll<HTMLButtonElement>(".ui-popover .menu-item"),
  ).find((button) => button.textContent?.includes("定位任务"));
}

afterEach(() => {
  app?.unmount();
  host?.remove();
  document.body.innerHTML = "";
});

it("displays selected task IDs with commas and supports multiple selections", async () => {
  const selected = ref(["1", "2"]);
  const trigger = mountSelect(selected);
  expect(trigger.textContent?.trim()).toBe("1,2");
  trigger.click();
  await nextTick();

  // 默认只列出已勾选的任务，其余任务需要输入关键词才能匹配出来。
  expect(optionIds()).toEqual(["1", "2"]);
  await typeSearch("3");
  expect(optionIds()).toEqual(["3"]);
  optionButton("3").click();
  await nextTick();

  expect(selected.value).toEqual(["1", "2", "3"]);
  expect(trigger.textContent?.trim()).toBe("1,2,3");
});

it("lists only checked tasks by default and hints how to search when none is checked", async () => {
  const selected = ref<string[]>([]);
  const trigger = mountSelect(selected);
  trigger.click();
  await nextTick();

  expect(optionIds()).toEqual([]);
  expect(emptyText()).toContain("输入任务 ID");

  await typeSearch("1");
  optionButton("1").click();
  await nextTick();
  expect(selected.value).toEqual(["1"]);

  // 输入被清空后回到“只显示已勾选任务”的状态。
  await typeSearch("");
  expect(optionIds()).toEqual(["1"]);
  expect(emptyText()).toBe("");
});

it("shows only matching tasks while searching and never pins checked tasks", async () => {
  const selected = ref(["3"]);
  const trigger = mountSelect(selected);
  trigger.click();
  await nextTick();
  expect(optionIds()).toEqual(["3"]);

  // 已勾选但未命中关键词的任务不再显示。
  await typeSearch("2");
  expect(optionIds()).toEqual(["2"]);

  // 命中的已勾选任务仍按候选顺序出现，不被置顶。
  await typeSearch("子");
  expect(optionIds()).toEqual(["1", "2", "3"]);

  // 描述也参与匹配。
  await typeSearch("三");
  expect(optionIds()).toEqual(["3"]);

  // 清空输入后恢复“只显示已勾选任务”。
  await typeSearch("");
  expect(optionIds()).toEqual(["3"]);
});

it("reports an empty result while searching when nothing matches", async () => {
  const selected = ref(["1"]);
  const trigger = mountSelect(selected);
  trigger.click();
  await nextTick();

  await typeSearch("9999");
  expect(optionIds()).toEqual([]);
  expect(emptyText()).toBe("没有匹配的任务");

  await typeSearch("");
  expect(optionIds()).toEqual(["1"]);
  expect(emptyText()).toBe("");
});

it("keeps the keyword after checking a task from the search result", async () => {
  const selected = ref<string[]>([]);
  const trigger = mountSelect(selected);
  trigger.click();
  await nextTick();

  await typeSearch("2");
  optionButton("2").click();
  await flush();

  expect(selected.value).toEqual(["2"]);
  expect(searchInput().value).toBe("2"); // 勾选后不主动清空输入框
  expect(optionIds()).toEqual(["2"]); // 列表停留在当前匹配结果
  expect(trigger.getAttribute("aria-expanded")).toBe("true");

  // 取消勾选同样保留输入内容。
  optionButton("2").click();
  await flush();
  expect(selected.value).toEqual([]);
  expect(searchInput().value).toBe("2");
});

it("shows a clear button only while the search box has content and restores checked tasks", async () => {
  const selected = ref(["1"]);
  const trigger = mountSelect(selected);
  trigger.click();
  await nextTick();
  // 输入框为空时没有清空按钮。
  expect(clearButton()).toBeUndefined();

  await typeSearch("3");
  const button = clearButton();
  expect(button).toBeTruthy();
  expect(optionIds()).toEqual(["3"]);

  button!.click();
  await flush();

  expect(searchInput().value).toBe("");
  expect(clearButton()).toBeUndefined();
  expect(optionIds()).toEqual(["1"]); // 恢复已勾选任务
  expect(document.activeElement).toBe(searchInput()); // 焦点留在搜索框
});

it("clears back to the hint when nothing is checked", async () => {
  const selected = ref<string[]>([]);
  const trigger = mountSelect(selected);
  trigger.click();
  await nextTick();
  await typeSearch("2");
  expect(optionIds()).toEqual(["2"]);

  clearButton()!.click();
  await flush();

  expect(optionIds()).toEqual([]);
  expect(emptyText()).toContain("输入任务 ID");
});

it("focuses the search box on open so an ID can be typed right away", async () => {
  // 已有勾选时列表默认显示已勾选任务，打开下拉仍应能直接输入。
  const selected = ref(["1"]);
  const trigger = mountSelect(selected);
  trigger.click();
  await flush();

  expect(document.activeElement).toBe(searchInput());
  expect(optionIds()).toEqual(["1"]); // 列表里仍然能看到已勾选任务

  await typeSearch("2");
  expect(optionIds()).toEqual(["2"]);
});

it("shows 定位任务 on right-click when the list has checked items", async () => {
  const selected = ref(["1"]);
  const located: string[] = [];
  const trigger = mountSelect(selected, located);
  trigger.click();
  await nextTick();
  await typeSearch("2");

  // 右键未勾选的条目同样可用：定位的是被右键的那条任务。
  const event = new MouseEvent("contextmenu", {
    bubbles: true,
    cancelable: true,
    clientX: 120,
    clientY: 160,
  });
  const dispatched = optionButton("2").dispatchEvent(event);
  await flush();

  expect(dispatched).toBe(false); // 已阻止浏览器默认右键菜单
  expect(selected.value).toEqual(["1"]); // 右键不改变勾选状态
  const item = locateMenuItem();
  expect(item).toBeTruthy();
  item!.click();
  await flush();

  expect(located).toEqual(["2"]);
  // 选择后下拉列表与右键菜单都已关闭
  expect(document.body.querySelectorAll('[role="option"]')).toHaveLength(0);
  expect(locateMenuItem()).toBeUndefined();
});

it("keeps the native context menu when nothing is checked", async () => {
  const selected = ref<string[]>([]);
  const trigger = mountSelect(selected);
  trigger.click();
  await nextTick();
  await typeSearch("2");

  const event = new MouseEvent("contextmenu", {
    bubbles: true,
    cancelable: true,
    clientX: 120,
    clientY: 160,
  });
  const dispatched = optionButton("2").dispatchEvent(event);
  await flush();

  expect(dispatched).toBe(true); // 未阻止默认行为
  expect(locateMenuItem()).toBeUndefined();
});
