// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, h, nextTick, ref, type App } from "vue";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import TaskRelationGraph from "@/grid-app/components/TaskRelationGraph.vue";
import type { Task } from "@/shared/types";

const { listTasks } = vi.hoisted(() => ({ listTasks: vi.fn() }));
vi.mock("@/grid-app/api/client", () => ({ api: { listTasks } }));

const first: Task = {
  id: "202609230000000000",
  seq: 1,
  project: "项目甲",
  type: "新增需求",
  description: "第一项任务的详细说明",
  note: "",
  status: "已完成",
  priority: "中",
  submitter: "用户",
  created_at: "2026-09-23 00:00:00",
  finished_at: "2026-09-23 01:00:00",
  updated_at: "2026-09-23 01:00:00",
  position: 1,
  predecessor_task_ids: [],
  unlock_task_ids: ["202609230000000001"],
};
const second: Task = {
  ...first,
  id: "202609230000000001",
  seq: 2,
  description: "第二项任务",
  status: "未开始",
  predecessor_task_ids: [first.id],
  unlock_task_ids: [],
};
let app: App | undefined;
let host: HTMLElement;
let style: HTMLStyleElement | undefined;
afterEach(() => {
  app?.unmount();
  app = undefined;
  host?.remove();
  style?.remove();
  listTasks.mockReset();
  vi.unstubAllGlobals();
});

function pointer(type: string, x: number, y: number, button = 0): PointerEvent {
  const event = new MouseEvent(type, { bubbles: true, button, clientX: x, clientY: y });
  Object.defineProperty(event, "pointerId", { value: 1 });
  return event as PointerEvent;
}

function stubViewport(width: number, height: number) {
  vi.stubGlobal("ResizeObserver", class {
    constructor(private callback: ResizeObserverCallback) {}
    observe() {
      this.callback([{ contentRect: { width, height } } as ResizeObserverEntry], this as unknown as ResizeObserver);
    }
    disconnect() {}
  });
}

it("loads the selected projects, draws task cards, and opens the shared detail on click", async () => {
  listTasks.mockResolvedValue([first, second]);
  host = document.createElement("div");
  document.body.append(host);
  style = document.createElement("style");
  style.textContent = readFileSync(resolve("src/grid-app/grid.css"), "utf8");
  document.head.append(style);
  const onOpenDetail = vi.fn();
  app = createApp(TaskRelationGraph, {
    taskId: first.id,
    revision: 0,
    onOpenDetail,
  });
  app.mount(host);
  await nextTick();
  await nextTick();
  expect(listTasks).toHaveBeenCalledWith({}, expect.any(AbortSignal));
  expect(host.querySelectorAll(".relation-card")).toHaveLength(2);
  expect(host.querySelectorAll(".relation-line")).toHaveLength(1);
  expect(host.querySelector(".relation-line")?.hasAttribute("marker-end")).toBe(false);
  expect(host.querySelector(".relation-scroll")).toBeTruthy();
  expect(host.querySelector(".relation-canvas")).toBeTruthy();
  expect(host.textContent).toContain(first.id);
  expect(host.textContent).toContain("已完成");
  // 打开时自动选中当前任务节点
  const cards = host.querySelectorAll<HTMLButtonElement>(".relation-card");
  const firstCard = [...cards].find((card) => card.textContent?.includes(first.id))!;
  expect(firstCard.classList.contains("relation-card-selected")).toBe(true);
  expect(firstCard.getAttribute("aria-pressed")).toBe("true");
  // 单击另一节点：选中并打开详情
  const secondCard = [...cards].find((card) => card.textContent?.includes(second.id))!;
  secondCard.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await nextTick();
  expect(onOpenDetail).toHaveBeenCalledWith(second);
  expect(secondCard.classList.contains("relation-card-selected")).toBe(true);
});

it("loads all active related trees by default and keeps accepted trees available when focused", async () => {
  const acceptedRoot: Task = { ...second, id: "202609230000000002", seq: 3, status: "验收通过", predecessor_task_ids: ["202609230000000003"] };
  const acceptedChild: Task = { ...first, id: "202609230000000003", seq: 4, unlock_task_ids: [acceptedRoot.id] };
  listTasks.mockResolvedValue([first, second, acceptedRoot, acceptedChild]);
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(TaskRelationGraph, { taskId: null, revision: 0 });
  app.mount(host);
  await vi.waitFor(() => expect(host.querySelectorAll(".relation-card")).toHaveLength(2));
  expect(listTasks).toHaveBeenCalledWith({}, expect.any(AbortSignal));
  expect(host.textContent).toContain("全部未验收通过的关联树");
  expect(host.textContent).not.toContain(acceptedRoot.id);
  expect(host.querySelectorAll(".relation-line")).toHaveLength(1);
  app.unmount();
  app = createApp(TaskRelationGraph, { taskId: acceptedRoot.id, revision: 0 });
  app.mount(host);
  await vi.waitFor(() => expect(host.querySelectorAll(".relation-card")).toHaveLength(2));
  expect(host.textContent).toContain("当前任务所在的关联树");
  expect(host.textContent).toContain(acceptedRoot.id);
  expect(host.textContent).not.toContain(second.id);
});

it("drags a card and updates its connected line without opening task details", async () => {
  listTasks.mockResolvedValue([first, second]);
  host = document.createElement("div");
  document.body.append(host);
  const onOpenDetail = vi.fn();
  app = createApp(TaskRelationGraph, { taskId: first.id, revision: 0, onOpenDetail });
  app.mount(host);
  await nextTick();
  await nextTick();
  const card = [...host.querySelectorAll<HTMLButtonElement>(".relation-card")]
    .find((item) => item.textContent?.includes(second.id))!;
  const canvas = host.querySelector<HTMLElement>(".relation-scroll")!;
  const line = host.querySelector<SVGPathElement>(".relation-line")!;
  const beforeLeft = Number.parseFloat(card.style.left);
  const beforeTop = Number.parseFloat(card.style.top);
  const beforePath = line.getAttribute("d");
  const beforeTransform = host.querySelector<HTMLElement>(".relation-canvas")!.style.transform;
  card.dispatchEvent(pointer("pointerdown", 100, 100));
  canvas.dispatchEvent(pointer("pointermove", -40, -40));
  await nextTick();
  expect(Number.parseFloat(card.style.left)).toBe(beforeLeft - 140);
  expect(Number.parseFloat(card.style.top)).toBe(beforeTop - 140);
  expect(line.getAttribute("d")).not.toBe(beforePath);
  expect(host.querySelector<HTMLElement>(".relation-canvas")!.style.transform).toBe(beforeTransform);
  canvas.dispatchEvent(pointer("pointerup", -40, -40));
  card.click();
  expect(onOpenDetail).not.toHaveBeenCalled();
});

it("clears the highlighted card and detail when the canvas background is clicked", async () => {
  listTasks.mockResolvedValue([first, second]);
  host = document.createElement("div");
  document.body.append(host);
  const onOpenDetail = vi.fn();
  const onClearDetail = vi.fn();
  app = createApp(TaskRelationGraph, { taskId: first.id, revision: 0, onOpenDetail, onClearDetail });
  app.mount(host);
  await nextTick();
  await nextTick();
  const card = [...host.querySelectorAll<HTMLButtonElement>(".relation-card")]
    .find((item) => item.textContent?.includes(second.id))!;
  card.click();
  await nextTick();
  expect(card.getAttribute("aria-pressed")).toBe("true");
  expect(onClearDetail).not.toHaveBeenCalled();
  host.querySelector<HTMLElement>(".relation-canvas")!.click();
  await nextTick();
  expect(host.querySelectorAll(".relation-card-selected")).toHaveLength(0);
  expect(onClearDetail).toHaveBeenCalledOnce();
});

it("clears the highlighted card when the shared detail drawer closes", async () => {
  listTasks.mockResolvedValue([first, second]);
  host = document.createElement("div");
  document.body.append(host);
  const selected = ref<string | null>(first.id);
  app = createApp({ render: () => h(TaskRelationGraph, { taskId: first.id, revision: 0, selectedTaskId: selected.value }) });
  app.mount(host);
  await nextTick();
  await nextTick();
  expect(host.querySelectorAll(".relation-card-selected")).toHaveLength(1);
  selected.value = null;
  await nextTick();
  expect(host.querySelectorAll(".relation-card-selected")).toHaveLength(0);
});

it("centers the tree by its card bounds even when wider than the viewport", async () => {
  stubViewport(300, 240);
  listTasks.mockResolvedValue([first, second]);
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(TaskRelationGraph, { taskId: first.id, revision: 0 });
  app.mount(host);
  await nextTick();
  await nextTick();
  const cards = [...host.querySelectorAll<HTMLButtonElement>(".relation-card")];
  const left = Math.min(...cards.map((card) => Number.parseFloat(card.style.left)));
  const right = Math.max(...cards.map((card) => Number.parseFloat(card.style.left) + 272));
  const transform = host.querySelector<HTMLElement>(".relation-canvas")!.style.transform;
  const translateX = Number(transform.match(/translate\((-?\d+(?:\.\d+)?)px/)?.[1]);
  expect(translateX + (left + right) / 2).toBe(150);
});

it("fits multiple default trees into the initial viewport", async () => {
  stubViewport(500, 300);
  const child: Task = { ...first, id: "202609230000000002", seq: 3, unlock_task_ids: ["202609230000000003"] };
  const parent: Task = { ...second, id: "202609230000000003", seq: 4, predecessor_task_ids: [child.id] };
  listTasks.mockResolvedValue([first, second, child, parent]);
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(TaskRelationGraph, { taskId: null, revision: 0 });
  app.mount(host);
  await vi.waitFor(() => expect(host.querySelectorAll(".relation-card")).toHaveLength(4));
  await nextTick();
  const transform = host.querySelector<HTMLElement>(".relation-canvas")!.style.transform;
  const translateX = Number(transform.match(/translate\((-?\d+(?:\.\d+)?)px/)?.[1]);
  const translateY = Number(transform.match(/, (-?\d+(?:\.\d+)?)px/)?.[1]);
  const scale = Number(transform.match(/scale\((-?\d+(?:\.\d+)?)\)/)?.[1]);
  expect(scale).toBeGreaterThanOrEqual(0.25);
  expect(scale).toBeLessThan(1);
  for (const card of host.querySelectorAll<HTMLElement>(".relation-card")) {
    const left = translateX + Number.parseFloat(card.style.left) * scale;
    const top = translateY + Number.parseFloat(card.style.top) * scale;
    expect(left).toBeGreaterThanOrEqual(0);
    expect(top).toBeGreaterThanOrEqual(0);
    expect(left + 272 * scale).toBeLessThanOrEqual(500);
    expect(top + 106 * scale).toBeLessThanOrEqual(300);
  }
});

it("refreshes new relationships and resets dragged cards, zoom, and pan", async () => {
  stubViewport(300, 240);
  const third: Task = {
    ...second,
    id: "202609230000000002",
    seq: 3,
    predecessor_task_ids: [second.id],
  };
  listTasks.mockResolvedValueOnce([first, second]).mockResolvedValueOnce([first, second, third]);
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(TaskRelationGraph, { taskId: first.id, revision: 0 });
  app.mount(host);
  await nextTick();
  await nextTick();
  const card = [...host.querySelectorAll<HTMLButtonElement>(".relation-card")]
    .find((item) => item.textContent?.includes(second.id))!;
  const viewport = host.querySelector<HTMLElement>(".relation-scroll")!;
  const canvas = host.querySelector<HTMLElement>(".relation-canvas")!;
  card.dispatchEvent(pointer("pointerdown", 100, 100));
  viewport.dispatchEvent(pointer("pointermove", -40, -40));
  viewport.dispatchEvent(pointer("pointerup", -40, -40));
  viewport.dispatchEvent(new WheelEvent("wheel", { bubbles: true, cancelable: true, deltaY: -100, clientX: 120, clientY: 90 }));
  viewport.setPointerCapture = vi.fn();
  viewport.hasPointerCapture = vi.fn(() => false);
  viewport.dispatchEvent(pointer("pointerdown", 0, 0, 1));
  viewport.dispatchEvent(pointer("pointermove", 70, 45, 1));
  viewport.dispatchEvent(pointer("pointerup", 70, 45, 1));
  await nextTick();
  expect(Number.parseFloat(card.style.left)).toBeLessThan(0);
  expect(canvas.style.transform).toContain("scale(1.1)");
  host.querySelector<HTMLButtonElement>('[aria-label="刷新关联图"]')!.click();
  await vi.waitFor(() => expect(host.querySelectorAll(".relation-card")).toHaveLength(3));
  await nextTick();
  expect(listTasks).toHaveBeenCalledTimes(2);
  expect(host.querySelectorAll(".relation-line")).toHaveLength(2);
  expect(Number.parseFloat(card.style.left)).toBe(372);
  expect(Number.parseFloat(card.style.top)).toBe(32);
  expect(canvas.style.transform).toBe("translate(-358px, 35px) scale(1)");
});
