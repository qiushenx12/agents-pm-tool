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

  optionButton("3").click();
  await nextTick();

  expect(selected.value).toEqual(["1", "2", "3"]);
  expect(trigger.textContent?.trim()).toBe("1,2,3");
});

it("shows 定位任务 on right-click when the list has checked items", async () => {
  const selected = ref(["1"]);
  const located: string[] = [];
  const trigger = mountSelect(selected, located);
  trigger.click();
  await nextTick();

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
