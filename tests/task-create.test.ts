// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, h, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskCreateModal from "@/grid-app/components/TaskCreateModal.vue";
import { useMetaStore } from "@/grid-app/stores/metaStore";
import { api } from "@/grid-app/api/client";
import type { Task } from "@/shared/types";
vi.mock("@/grid-app/api/client", () => ({
  api: { createTask: vi.fn(), uploadAttachment: vi.fn(), pageTasks: vi.fn() },
}));
let app: App, pinia: Pinia, host: HTMLElement;
const flush = async () => {
  for (let i = 0; i < 30; i++) await Promise.resolve();
  await nextTick();
};
afterEach(() => {
  app?.unmount();
  disposePinia(pinia);
  host?.remove();
  vi.useRealTimers();
});
it("creates the task only once when retrying an attachment failure in the new-task dialog", async () => {
  vi.useFakeTimers();
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  const meta = useMetaStore(pinia);
  meta.projects = [
    { name: "测试项目", color: "#3370ff", sort_order: 0, local_path: "", git_url: "", created_at: "" },
  ];
  const task: Task = {
    id: "created-once",
    seq: 1,
    project: "测试项目",
    type: "新增需求",
    description: "创建流程测试",
    note: "创建备注",
    status: "未开始",
    priority: "中",
    submitter: "用户",
    created_at: "",
    updated_at: "",
    finished_at: null,
    position: 1,
  };
  vi.mocked(api.createTask).mockResolvedValue(task);
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [task],
    total: 1,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  });
  vi.mocked(api.uploadAttachment)
    .mockResolvedValueOnce({} as never)
    .mockRejectedValueOnce(new Error("网络中断"))
    .mockResolvedValueOnce({} as never);
  const onCreated = vi.fn();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskCreateModal, { onCreated }) });
  app.use(pinia);
  app.mount(host);
  const input = document.querySelector<HTMLInputElement>('input[type="file"]')!;
  Object.defineProperty(input, "files", {
    configurable: true,
    value: [new File(["a"], "a.txt"), new File(["b"], "b.txt")],
  });
  input.dispatchEvent(new Event("change", { bubbles: true }));
  const note = document.querySelector<HTMLTextAreaElement>("#create-note")!;
  note.value = "创建备注";
  note.dispatchEvent(new Event("input", { bubbles: true }));
  await nextTick();
  const button = (label: string) =>
    Array.from(document.querySelectorAll<HTMLButtonElement>("button")).find(
      (el) => el.textContent?.trim() === label,
    )!;
  button("创建任务").click();
  await flush();
  expect(api.createTask).toHaveBeenCalledTimes(1);
  expect(api.createTask).toHaveBeenCalledWith({
    project: "测试项目",
    type: "新增需求",
    description: "",
    note: "创建备注",
    priority: "中",
  });
  expect(onCreated).not.toHaveBeenCalled();
  expect(document.body.textContent).toContain("任务已创建，部分附件上传失败");
  button("重试失败附件").click();
  await flush();
  expect(api.createTask).toHaveBeenCalledTimes(1);
  expect(api.uploadAttachment).toHaveBeenCalledTimes(3);
  expect(onCreated).toHaveBeenCalledWith(task);
});
