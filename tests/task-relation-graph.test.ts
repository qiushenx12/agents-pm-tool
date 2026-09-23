// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, nextTick, type App } from "vue";
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
});

it("loads the selected projects, draws task cards, and opens the shared detail on double click", async () => {
  listTasks.mockResolvedValue([first, second]);
  host = document.createElement("div");
  document.body.append(host);
  style = document.createElement("style");
  style.textContent = readFileSync(resolve("src/grid-app/grid.css"), "utf8");
  document.head.append(style);
  const onOpenDetail = vi.fn();
  app = createApp(TaskRelationGraph, {
    projects: ["项目甲"],
    revision: 0,
    onOpenDetail,
  });
  app.mount(host);
  await nextTick();
  await nextTick();
  expect(listTasks).toHaveBeenCalledWith({ project: ["项目甲"] }, expect.any(AbortSignal));
  expect(host.querySelectorAll(".relation-card")).toHaveLength(2);
  expect(host.querySelectorAll(".relation-line")).toHaveLength(1);
  expect(host.querySelector(".relation-line")?.hasAttribute("marker-end")).toBe(false);
  expect(getComputedStyle(host.querySelector(".relation-scroll")!).display).toBe("flex");
  expect(getComputedStyle(host.querySelector(".relation-canvas")!).marginTop).toBe("auto");
  expect(host.textContent).toContain(first.id);
  expect(host.textContent).toContain("已完成");
  host.querySelector<HTMLButtonElement>(".relation-card")!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  expect(onOpenDetail).toHaveBeenCalledWith(first);
});
