// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, h, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskCreateModal from "@/grid-app/components/TaskCreateModal.vue";
import { useMetaStore } from "@/grid-app/stores/metaStore";
import { api } from "@/grid-app/api/client";
import type { Project, Task } from "@/shared/types";
vi.mock("@/grid-app/api/client", () => ({
  api: { createTask: vi.fn(), uploadAttachment: vi.fn(), pageTasks: vi.fn() },
}));
let app: App | undefined,
  pinia: Pinia | undefined,
  host: HTMLElement | undefined;
const flush = async () => {
  for (let i = 0; i < 30; i++) await Promise.resolve();
  await nextTick();
};
function filePasteEvent(files: File[], exposeAsItems = false) {
  const event = new Event("paste", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clipboardData", {
    value: {
      files: exposeAsItems ? [] : files,
      items: exposeAsItems
        ? files.map((file) => ({ kind: "file", getAsFile: () => file }))
        : [],
    },
  });
  return event;
}
afterEach(() => {
  app?.unmount();
  if (pinia) disposePinia(pinia);
  host?.remove();
  vi.useRealTimers();
  vi.clearAllMocks();
  localStorage.clear();
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

it.each([
  ["clipboard files", false],
  ["clipboard items", true],
])("queues files pasted into the new-task attachment box from %s", async (_, asItems) => {
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  useMetaStore(pinia).projects = [
    {
      name: "测试项目",
      color: "#3370ff",
      sort_order: 0,
      local_path: "",
      git_url: "",
      created_at: "",
    },
  ];
  const task: Task = {
    id: "created-from-paste",
    seq: 2,
    project: "测试项目",
    type: "新增需求",
    description: "",
    note: "",
    status: "未开始",
    priority: "中",
    submitter: "用户",
    created_at: "",
    updated_at: "",
    finished_at: null,
    position: 2,
  };
  vi.mocked(api.createTask).mockResolvedValue(task);
  vi.mocked(api.uploadAttachment).mockResolvedValue({} as never);
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [task],
    total: 1,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  });
  const onCreated = vi.fn();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskCreateModal, { onCreated }) });
  app.use(pinia);
  app.mount(host);

  const zone = document.querySelector<HTMLElement>(".upload-zone")!;
  zone.click();
  expect(document.activeElement).toBe(zone);
  const file = new File(["png"], "剪贴板截图.png", { type: "image/png" });
  const paste = filePasteEvent([file], asItems);
  zone.dispatchEvent(paste);
  await nextTick();

  expect(paste.defaultPrevented).toBe(true);
  expect(zone.getAttribute("aria-label")).toContain("粘贴文件");
  expect(document.body.textContent).toContain("剪贴板截图.png");

  const create = Array.from(
    document.querySelectorAll<HTMLButtonElement>("button"),
  ).find((button) => button.textContent?.trim() === "创建任务")!;
  create.click();
  await flush();

  expect(api.createTask).toHaveBeenCalledTimes(1);
  expect(api.uploadAttachment).toHaveBeenCalledWith(task.id, file);
  expect(onCreated).toHaveBeenCalledWith(task);
});

it("remembers the last project selected in the new-task dialog", async () => {
  window.history.replaceState(null, "", "/?project=Alpha");
  localStorage.setItem("pm-create-task-project-v1", "Beta");
  const projects: Project[] = ["Alpha", "Beta"].map((name, sort_order) => ({
    name,
    color: "#3370ff",
    sort_order,
    local_path: "",
    git_url: "",
    created_at: "",
  }));

  const mountModal = () => {
    pinia = createPinia();
    useMetaStore(pinia).projects = projects;
    host = document.createElement("div");
    document.body.append(host);
    app = createApp(TaskCreateModal);
    app.use(pinia);
    app.mount(host);
  };

  mountModal();
  const projectTrigger = document.body.querySelector<HTMLButtonElement>(
    '[aria-label="项目：Beta"]',
  );
  expect(projectTrigger).not.toBeNull();
  projectTrigger!.click();
  await nextTick();
  const alpha = Array.from(
    document.body.querySelectorAll<HTMLButtonElement>(
      '[role="listbox"][aria-label="项目"] [role="option"]',
    ),
  ).find((option) => option.textContent?.trim() === "Alpha");
  expect(alpha).not.toBeUndefined();
  alpha!.click();
  await nextTick();
  expect(localStorage.getItem("pm-create-task-project-v1")).toBe("Alpha");

  app!.unmount();
  disposePinia(pinia!);
  host!.remove();
  app = undefined;
  pinia = undefined;
  host = undefined;
  window.history.replaceState(null, "", "/");

  mountModal();
  expect(
    document.body.querySelector('[aria-label="项目：Alpha"]'),
  ).not.toBeNull();
});
