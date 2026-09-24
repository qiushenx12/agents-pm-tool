// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createApp, nextTick, type App } from "vue";
import { createPinia, disposePinia, setActivePinia, type Pinia } from "pinia";
import TaskField from "@/grid-app/components/TaskField.vue";
import type { Task } from "@/shared/types";

const { patchTask, pageTasks } = vi.hoisted(() => ({
  patchTask: vi.fn(),
  pageTasks: vi.fn().mockResolvedValue({
    items: [],
    page: 1,
    page_size: 100,
    total: 0,
    groups: [],
    anchor_found: null,
  }),
}));
vi.mock("@/grid-app/api/client", () => ({ api: { patchTask, pageTasks } }));

const task: Task = {
  id: "202609240000000000",
  seq: 1,
  project: "项目甲",
  type: "新增需求",
  description: "测试任务",
  note: "",
  status: "未开始",
  priority: "中",
  submitter: "用户",
  created_at: "2026-09-24 00:00:00",
  finished_at: null,
  updated_at: "2026-09-24 00:00:00",
  position: 1,
  predecessor_task_ids: [],
  unlock_task_ids: [],
};

let app: App | undefined;
let host: HTMLElement;
let pinia: Pinia;

beforeEach(() => {
  vi.useFakeTimers();
  localStorage.clear();
  pinia = createPinia();
  setActivePinia(pinia);
  host = document.createElement("div");
  document.body.append(host);
  patchTask.mockRejectedValue(new Error("子任务须全部处于待验证、已完成或验收通过"));
});

afterEach(() => {
  app?.unmount();
  app = undefined;
  host.remove();
  document.body.querySelectorAll(".ui-popover").forEach((node) => node.remove());
  disposePinia(pinia);
  patchTask.mockReset();
  vi.useRealTimers();
});

/** 微任务链较长（点击 → store 队列 → 拒绝 → 字段错误），多刷几轮保证落定 */
async function flush() {
  for (let i = 0; i < 10; i++) await nextTick();
}

async function choose(value: string) {
  host.querySelector<HTMLButtonElement>(".select-trigger")!.click();
  await flush();
  [
    ...document.body.querySelectorAll<HTMLButtonElement>(
      '.ui-popover [role="option"]',
    ),
  ]
    .find((option) => option.textContent?.includes(value))!
    .click();
  await flush();
}

it("gives the form select a full-width anchor while the cell variant does not", async () => {
  // 表单里的下拉要撑满，单元格（field）形态由所在格子的规则管，两者靠锚点类名区分，
  // 不用 :has()（Safari 15.4+），见 shared/components.css 的 .select-anchor
  app = createApp(TaskField, { task, field: "status" });
  app.mount(host);
  await nextTick();
  const cellAnchor = host.querySelector<HTMLElement>(".popover-anchor")!;
  expect(cellAnchor).not.toBeNull();
  expect(cellAnchor.classList.contains("select-anchor")).toBe(false);
  app.unmount();

  app = createApp(TaskField, { task, field: "status", form: true });
  app.mount(host);
  await nextTick();
  const formAnchor = host.querySelector<HTMLElement>(".popover-anchor")!;
  expect(formAnchor).not.toBeNull();
  expect(formAnchor.classList.contains("select-anchor")).toBe(true);
});

it("auto-dismisses the field error three seconds after it appears", async () => {
  app = createApp(TaskField, { task, field: "status" });
  app.mount(host);
  expect(host.querySelector(".field-error")).toBeNull();

  await choose("待验证");
  const error = host.querySelector<HTMLElement>(".field-error")!;
  expect(error.textContent).toContain("子任务须全部处于待验证");
  expect(error.textContent).toContain("点击字段重试");

  await vi.advanceTimersByTimeAsync(2999);
  expect(host.querySelector(".field-error")).not.toBeNull();
  await vi.advanceTimersByTimeAsync(1);
  expect(host.querySelector(".field-error")).toBeNull();
});

it("restarts the countdown on a new failure and clears immediately on success", async () => {
  app = createApp(TaskField, { task, field: "status" });
  app.mount(host);

  await choose("待验证");
  expect(host.querySelector(".field-error")).not.toBeNull();
  await vi.advanceTimersByTimeAsync(2000);

  // 再次失败：重新计时 3 秒，而不是沿用第一次的剩余时间
  await choose("进行中");
  expect(host.querySelector(".field-error")).not.toBeNull();
  await vi.advanceTimersByTimeAsync(2999);
  expect(host.querySelector(".field-error")).not.toBeNull();
  await vi.advanceTimersByTimeAsync(1);
  expect(host.querySelector(".field-error")).toBeNull();

  // 成功后错误立即消失，不再等计时器
  await choose("待验证");
  expect(host.querySelector(".field-error")).not.toBeNull();
  patchTask.mockResolvedValueOnce({ ...task, status: "进行中" });
  await choose("进行中");
  expect(host.querySelector(".field-error")).toBeNull();
});
