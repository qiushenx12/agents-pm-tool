// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, type App, type Component } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import ProjectOptionPopover from "@/grid-app/components/ProjectOptionPopover.vue";
import SidebarProjects from "@/grid-app/components/SidebarProjects.vue";
import { api } from "@/grid-app/api/client";
import { useMetaStore } from "@/grid-app/stores/metaStore";
import type { Project } from "@/shared/types";

vi.mock("@/grid-app/api/client", () => ({
  api: {
    listProjects: vi.fn(),
    listSubmitterNames: vi.fn().mockResolvedValue([]),
    createProject: vi.fn(),
    patchProject: vi.fn(),
    deleteProject: vi.fn(),
    pickFolder: vi.fn(),
    pageTasks: vi.fn().mockResolvedValue({
      items: [],
      total: 0,
      page: 1,
      page_size: 100,
      groups: [],
      anchor_found: null,
    }),
  },
}));

let app: App | undefined;
let pinia: Pinia | undefined;
let host: HTMLDivElement | undefined;

const project = (name: string, sortOrder: number): Project => ({
  name,
  color: "#3370ff",
  sort_order: sortOrder,
  local_path: `D:\\code\\${name}`,
  git_url: `https://example.com/${name}.git`,
  created_at: "2026-09-10 10:00:00",
});

function mount(component: Component, props = {}, projects: Project[] = []) {
  pinia = createPinia();
  useMetaStore(pinia).projects = projects;
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(component, props);
  app.use(pinia);
  app.mount(host);
}

function changeInput(label: string, value: string) {
  const input = document.body.querySelector<HTMLInputElement>(
    `[aria-label="${label}"]`,
  )!;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

afterEach(() => {
  app?.unmount();
  if (pinia) disposePinia(pinia);
  host?.remove();
  document.body
    .querySelectorAll(".dialog-mask")
    .forEach((element) => element.remove());
  app = undefined;
  pinia = undefined;
  host = undefined;
  vi.clearAllMocks();
  localStorage.clear();
});

it("opens per-project settings from the sidebar and reorders by drag", async () => {
  const edit = vi.fn();
  let serverProjects = [
    project("Alpha", 0),
    project("Beta", 1),
    project("Gamma", 2),
  ];
  vi.mocked(api.listProjects).mockImplementation(async () => [
    ...serverProjects,
  ]);
  vi.mocked(api.patchProject).mockImplementation(async (name, patch) => {
    const current = serverProjects.find((item) => item.name === name)!;
    Object.assign(current, patch);
    serverProjects = [...serverProjects].sort(
      (left, right) => left.sort_order - right.sort_order,
    );
    return current;
  });

  mount(
    SidebarProjects,
    {
      activeProject: "Alpha",
      isAdmin: true,
      onEdit: edit,
    },
    serverProjects,
  );
  const meta = useMetaStore(pinia!);
  await vi.waitFor(() =>
    expect(host!.querySelectorAll(".project-nav-row")).toHaveLength(3),
  );

  host!
    .querySelector<HTMLButtonElement>('[aria-label="项目设置：Alpha"]')!
    .click();
  expect(edit).toHaveBeenCalledWith("Alpha");
  expect(host!.textContent).not.toContain("项目管理");
  // 项目小标题和它上面的新建入口已经移到侧栏标题行，列表里不再重复出现。
  expect(host!.querySelector(".nav-section-label")).toBeNull();
  expect(host!.querySelector('[aria-label="新建项目"]')).toBeNull();

  const rows = host!.querySelectorAll<HTMLElement>(".project-nav-row");
  rows[1].dispatchEvent(
    new Event("dragstart", { bubbles: true, cancelable: true }),
  );
  rows[0].dispatchEvent(
    new MouseEvent("dragover", { bubbles: true, cancelable: true, clientY: 0 }),
  );
  rows[0].dispatchEvent(
    new Event("drop", { bubbles: true, cancelable: true }),
  );

  await vi.waitFor(() => expect(api.patchProject).toHaveBeenCalledTimes(2));
  expect(meta.projects.map((item) => item.name)).toEqual([
    "Beta",
    "Alpha",
    "Gamma",
  ]);
  expect(api.patchProject).toHaveBeenCalledWith("Beta", { sort_order: 0 });
  expect(api.patchProject).toHaveBeenCalledWith("Alpha", { sort_order: 1 });
});

it("只有项目列表为空时才在列表内提示创建项目", async () => {
  const create = vi.fn();

  mount(SidebarProjects, {
    activeProject: "",
    isAdmin: true,
    onCreate: create,
  });

  const entry = await vi.waitFor(() => {
    const button = host!.querySelector<HTMLButtonElement>(".nav-item.subtle");
    expect(button).not.toBeNull();
    return button!;
  });
  expect(entry.textContent).toContain("创建第一个项目");
  entry.click();

  expect(create).toHaveBeenCalledOnce();
});

it("edits all existing project settings in the redesigned dialog", async () => {
  const renamed = vi.fn();
  const close = vi.fn();
  let saved = project("Alpha", 0);
  vi.mocked(api.listProjects).mockImplementation(async () => [{ ...saved }]);
  vi.mocked(api.patchProject).mockImplementation(async (_, patch) => {
    saved = {
      ...saved,
      name: patch.new_name ?? saved.name,
      color: patch.color ?? saved.color,
      local_path: patch.local_path ?? saved.local_path,
      git_url: patch.git_url ?? saved.git_url,
    };
    return { ...saved };
  });

  mount(
    ProjectOptionPopover,
    {
      projectName: "Alpha",
      onRenamed: renamed,
      onClose: close,
    },
    [{ ...saved }],
  );

  expect(document.body.textContent).toContain("项目设置");
  expect(document.body.textContent).toContain("删除项目");
  changeInput("项目名称", "Renamed");
  changeInput("项目本地路径", "D:\\workspace\\renamed");
  changeInput("项目 Git 地址", "git@example.com:team/renamed.git");
  document.body
    .querySelector<HTMLButtonElement>('[aria-label="选择颜色 #e85b50"]')!
    .click();
  Array.from(document.body.querySelectorAll<HTMLButtonElement>("button"))
    .find((button) => button.textContent?.includes("保存设置"))!
    .click();

  await vi.waitFor(() => expect(api.patchProject).toHaveBeenCalledOnce());
  expect(api.patchProject).toHaveBeenCalledWith("Alpha", {
    new_name: "Renamed",
    color: "#e85b50",
    local_path: "D:\\workspace\\renamed",
    git_url: "git@example.com:team/renamed.git",
  });
  expect(renamed).toHaveBeenCalledWith("Alpha", "Renamed");
  await vi.waitFor(() => expect(close).toHaveBeenCalledOnce());
});

it("creates a project from the dedicated creation dialog", async () => {
  const close = vi.fn();
  const created = project("New Project", 0);
  vi.mocked(api.listProjects).mockResolvedValue([created]);
  vi.mocked(api.createProject).mockResolvedValue(created);

  mount(ProjectOptionPopover, { onClose: close });
  changeInput("项目名称", "New Project");
  changeInput("项目本地路径", "D:\\workspace\\new-project");
  changeInput("项目 Git 地址", "https://example.com/new-project.git");
  document.body
    .querySelector<HTMLFormElement>("#project-settings-form")!
    .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));

  await vi.waitFor(() => expect(api.createProject).toHaveBeenCalledOnce());
  expect(api.createProject).toHaveBeenCalledWith({
    name: "New Project",
    color: "#3370ff",
    local_path: "D:\\workspace\\new-project",
    git_url: "https://example.com/new-project.git",
  });
  await vi.waitFor(() => expect(close).toHaveBeenCalledOnce());
});
