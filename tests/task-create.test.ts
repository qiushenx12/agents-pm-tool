// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, h, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskCreateModal from "@/grid-app/components/TaskCreateModal.vue";
import { useMetaStore } from "@/grid-app/stores/metaStore";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import { api } from "@/grid-app/api/client";
import type { Project, Task } from "@/shared/types";
vi.mock("@/grid-app/api/client", () => ({
  api: {
    createTask: vi.fn(),
    uploadAttachment: vi.fn(),
    pageTasks: vi.fn(),
    listTasks: vi.fn().mockResolvedValue([]),
  },
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
  expect(document.body.textContent).toContain("子任务 ID");
  expect(document.body.textContent).toContain("父级任务 ID");
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
    predecessor_task_ids: [],
    unlock_task_ids: [],
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

it("defaults to the single filtered project, then remembers the manual choice without a filter", async () => {
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

  // 单一项目筛选时优先跟随筛选，而不是上次新建选择的 Beta。
  mountModal();
  const projectTrigger = document.body.querySelector<HTMLButtonElement>(
    '[aria-label="项目：Alpha"]',
  );
  expect(projectTrigger).not.toBeNull();
  projectTrigger!.click();
  await nextTick();
  const beta = Array.from(
    document.body.querySelectorAll<HTMLButtonElement>(
      '[role="listbox"][aria-label="项目"] [role="option"]',
    ),
  ).find((option) => option.textContent?.trim() === "Beta");
  expect(beta).not.toBeUndefined();
  beta!.click();
  await nextTick();
  expect(localStorage.getItem("pm-create-task-project-v1")).toBe("Beta");

  app!.unmount();
  disposePinia(pinia!);
  host!.remove();
  app = undefined;
  pinia = undefined;
  host = undefined;
  window.history.replaceState(null, "", "/");
  // 清掉上一次挂载保存的筛选状态，模拟「没有筛选」的场景。
  localStorage.removeItem("pm-grid-filters-v1");

  // 没有筛选时仍记住上次手动选择的项目。
  mountModal();
  expect(
    document.body.querySelector('[aria-label="项目：Beta"]'),
  ).not.toBeNull();
});

it("ignores the project filter default when multiple projects are filtered", async () => {
  window.history.replaceState(null, "", "/?project=Alpha&project=Beta");
  localStorage.setItem("pm-create-task-project-v1", "Beta");
  pinia = createPinia();
  useMetaStore(pinia).projects = ["Alpha", "Beta"].map((name, sort_order) => ({
    name,
    color: "#3370ff",
    sort_order,
    local_path: "",
    git_url: "",
    created_at: "",
  }));
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(TaskCreateModal);
  app.use(pinia);
  app.mount(host);
  await nextTick();
  // 多项目筛选不适用该规则，回退到上次新建选择的项目。
  expect(
    document.body.querySelector('[aria-label="项目：Beta"]'),
  ).not.toBeNull();
});

const lastProject = () => localStorage.getItem("pm-create-task-project-v1");
const projectLabel = () =>
  document.body.querySelector("[aria-label^='项目：']")?.getAttribute("aria-label");
const twoProjects = () =>
  ["Alpha", "Beta"].map((name, sort_order) => ({
    name,
    color: "#3370ff",
    sort_order,
    local_path: "",
    git_url: "",
    created_at: "",
  })) as Project[];
/** 一个测试内共用一个 store，这样筛选状态能跨多次「打开弹窗」保留。 */
let lookupTasks: ReturnType<typeof useTaskStore> | undefined;
function setupStore(projects: Project[]) {
  pinia = createPinia();
  useMetaStore(pinia).projects = projects;
  lookupTasks = useTaskStore(pinia);
  return lookupTasks;
}
/** 只挂载一次弹窗；筛选变化后再调用即可拿到新的默认项目。 */
async function openCreateModal() {
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(TaskCreateModal);
  app.use(pinia!);
  app.mount(host);
  await nextTick();
}
/** 关掉当前弹窗（不卸载 pinia，筛选状态要留给下一次打开）。 */
async function closeCreateModal() {
  app?.unmount();
  app = undefined;
  host?.remove();
  host = undefined;
  await nextTick();
}
/** 关掉弹窗 → 改筛选 → 再打开，模拟用户真实的操作顺序。 */
async function reopenCreateModal(active?: string) {
  await closeCreateModal();
  if (active === undefined) lookupTasks!.clearFilters();
  else lookupTasks!.setProject(active);
  await nextTick();
  await openCreateModal();
}

/** 提交弹窗，等异步落库与收尾。createTask 由本文件的 mock 提供。 */
async function submitCreateModal() {
  Array.from(document.querySelectorAll<HTMLButtonElement>("button"))
    .find((button) => button.textContent?.trim() === "创建任务")!
    .click();
  for (let i = 0; i < 50; i++) await Promise.resolve();
  await nextTick();
}
/** 让 mock 的 createTask 回显传入的项目，便于断言「建在哪个项目下」。 */
function stubCreateTaskEchoingProject() {
  vi.mocked(api.createTask).mockImplementation(async (input) => {
    const { project, type, description } = input as {
      project: string;
      type: Task["type"];
      description: string;
    };
    return {
      id: "created-" + project,
      seq: 1,
      project,
      type,
      description,
      note: "",
      status: "未开始",
      priority: "中",
      submitter: "用户",
      submitter_name: "主机",
      created_at: "",
      finished_at: null,
      updated_at: "",
      position: 1,
      attachment_count: 0,
      predecessor_task_ids: [],
      unlock_task_ids: [],
    } as Task;
  });
}

it("筛选项目下建完任务后清除筛选，下次新建仍默认该项目", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  stubCreateTaskEchoingProject();
  setupStore(twoProjects());

  // 最初：没有任何筛选，新建弹窗默认落在项目列表第一项 Alpha。
  await openCreateModal();
  expect(projectLabel()).toBe("项目：Alpha");
  expect(lastProject()).toBeNull();
  await closeCreateModal();

  // 筛选 Beta → 新建任务：弹窗跟随筛选显示 Beta，并在该项目下真正建出任务。
  await reopenCreateModal("Beta");
  expect(projectLabel()).toBe("项目：Beta");
  await submitCreateModal();
  expect(api.createTask).toHaveBeenCalledWith(
    expect.objectContaining({ project: "Beta" }),
  );
  expect(lastProject()).toBe("Beta");
  await closeCreateModal();

  // 清除筛选后再新建：必须还是 Beta，而不是跳回 Alpha。
  await reopenCreateModal(undefined);
  expect(projectLabel()).toBe("项目：Beta");
});

it("打开弹窗本身不会改写「上次新建的项目」", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  localStorage.setItem("pm-create-task-project-v1", "Beta");
  setupStore(twoProjects());

  // 筛选 Alpha 打开弹窗：默认跟随筛选显示 Alpha，但没创建就等于没做选择，
  // 不能把 Alpha 覆盖成「上次新建的项目」。
  await reopenCreateModal("Alpha");
  expect(projectLabel()).toBe("项目：Alpha");
  expect(lastProject()).toBe("Beta");

  await reopenCreateModal(undefined);
  expect(projectLabel()).toBe("项目：Beta");
});

it("弹窗开着时清除筛选不会污染记忆", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  localStorage.setItem("pm-create-task-project-v1", "Beta");
  setupStore(twoProjects());

  // 筛 Alpha 打开弹窗后，弹窗还开着就把筛选清掉：旧实现里 watch(project)
  // 会跟着把项目改回 Beta 并回写记忆，等于凭空改写用户的选择。
  await reopenCreateModal("Alpha");
  expect(projectLabel()).toBe("项目：Alpha");
  lookupTasks!.clearFilters();
  await nextTick();
  // 记忆必须原封不动，仍是上次真正创建过的 Beta。
  expect(lastProject()).toBe("Beta");
  await closeCreateModal();

  // 清除筛选后再打开：没有筛选可跟随，回落到记忆里的 Beta。
  await openCreateModal();
  expect(projectLabel()).toBe("项目：Beta");
  expect(lastProject()).toBe("Beta");
});
