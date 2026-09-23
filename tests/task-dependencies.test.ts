// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";
import { createApp, h, nextTick, ref, type App } from "vue";
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

afterEach(() => {
  app?.unmount();
  host?.remove();
  document.body.innerHTML = "";
});

it("displays selected task IDs with commas and supports multiple selections", async () => {
  const selected = ref(["1", "2"]);
  const options = [task("1", "前置一"), task("2", "前置二"), task("3", "前置三")];
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({
    setup: () => () =>
      h(TaskMultiSelect, {
        modelValue: selected.value,
        options,
        label: "前置任务 ID",
        "onUpdate:modelValue": (value: string[]) => (selected.value = value),
      }),
  });
  app.mount(host);

  const trigger = host.querySelector<HTMLButtonElement>(".dependency-trigger")!;
  expect(trigger.textContent?.trim()).toBe("1,2");
  trigger.click();
  await nextTick();

  const third = Array.from(
    document.body.querySelectorAll<HTMLButtonElement>('[role="option"]'),
  ).find((button) => button.textContent?.includes("3"))!;
  third.click();
  await nextTick();

  expect(selected.value).toEqual(["1", "2", "3"]);
  expect(trigger.textContent?.trim()).toBe("1,2,3");
});
