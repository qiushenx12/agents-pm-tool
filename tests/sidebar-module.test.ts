// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import GridApp from "@/grid-app/GridApp.vue";
import { useViewStore } from "@/grid-app/stores/viewStore";

vi.mock("@/grid-app/api/client", () => ({
  api: {
    me: vi.fn().mockResolvedValue({
      id: "host",
      username: "主机",
      role: "super_admin",
      created_at: "",
      disabled: false,
      is_host: true,
    }),
    getAgentAccess: vi.fn().mockResolvedValue({
      server_url: "http://127.0.0.1:17890",
      token: "token",
      access_instructions: "本机访问",
    }),
    listProjects: vi.fn().mockResolvedValue([
      {
        name: "测试项目",
        color: "#2563eb",
        sort_order: 0,
        local_path: "",
        git_url: "",
        created_at: "",
      },
    ]),
    listSubmitterNames: vi.fn().mockResolvedValue([]),
    createTask: vi.fn(),
    pageTasks: vi.fn().mockResolvedValue({
      items: [],
      total: 0,
      page: 1,
      page_size: 100,
      groups: [],
      anchor_found: null,
    }),
  },
  subscribeTaskEvents: vi.fn(() => vi.fn()),
}));

// 工具栏和反馈层与侧栏结构无关，替换成最简桩件，避免触发额外请求。
vi.mock("@/grid-app/components/FilterBar.vue", async () => {
  const { defineComponent, h } = await import("vue");
  return {
    default: defineComponent({
      emits: ["create"],
      setup(_props, { emit }) {
        return () => h("button", { onClick: () => emit("create") }, "新建任务");
      },
    }),
  };
});

vi.mock("@/grid-app/components/SavedViews.vue", async () => {
  const { defineComponent } = await import("vue");
  return { default: defineComponent({ template: "<div />" }) };
});

vi.mock("@/grid-app/components/BulkActions.vue", async () => {
  const { defineComponent } = await import("vue");
  return { default: defineComponent({ template: "<div />" }) };
});

vi.mock("@/shared/AppFeedback.vue", async () => {
  const { defineComponent } = await import("vue");
  return { default: defineComponent({ template: "<div />" }) };
});

// 新建项目弹窗内容与侧栏入口无关，用桩件确认入口确实打开了它。
vi.mock("@/grid-app/components/ProjectOptionPopover.vue", async () => {
  const { defineComponent, h } = await import("vue");
  return {
    default: defineComponent({
      setup() {
        return () => h("div", { "data-testid": "project-dialog" });
      },
    }),
  };
});

let app: App;
let pinia: Pinia;
let host: HTMLElement;

afterEach(() => {
  app?.unmount();
  if (pinia) disposePinia(pinia);
  host?.remove();
  vi.clearAllMocks();
  localStorage.clear();
});

async function mountWorkspace(projectsOpen: boolean) {
  localStorage.setItem("pm-theme", "light");
  localStorage.setItem(
    "pm-table-view-v1",
    JSON.stringify({ collapsed: false, projectsOpen }),
  );
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(GridApp);
  app.use(pinia);
  app.mount(host);

  await vi.waitFor(() => {
    expect(host.querySelector(".project-nav-row")).not.toBeNull();
  });
  return host.querySelector<HTMLButtonElement>(".module-toggle")!;
}

function panel() {
  return host.querySelector<HTMLElement>("#sidebar-projects")!;
}

it("侧栏只保留一个纯文字的 Agents PM Tool 模块，不再有工作空间入口", async () => {
  const toggle = await mountWorkspace(true);
  const sidebar = host.querySelector<HTMLElement>(".workspace-sidebar")!;

  expect(toggle.textContent!.trim()).toBe("Agents PM Tool");
  for (const label of ["工作空间", "全部任务", "待验证", "验收未通过"]) {
    expect(sidebar.textContent).not.toContain(label);
  }
  expect(host.querySelector(".breadcrumb")!.textContent).toContain(
    "Agents PM Tool",
  );
  expect(host.querySelector(".breadcrumb")!.textContent).not.toContain(
    "工作空间",
  );
  // 标题本身就是折叠开关：没有品牌图标、副标题、折叠箭头和项目小标题。
  expect(toggle.closest(".sidebar-module-head")).not.toBeNull();
  expect(sidebar.querySelector(".brand-mark")).toBeNull();
  expect(sidebar.querySelector(".module-chevron")).toBeNull();
  expect(sidebar.querySelector(".nav-section-label")).toBeNull();
  expect(panel().querySelector(".project-nav-row")).not.toBeNull();
});

it("标题右侧的加号用于新建项目", async () => {
  await mountWorkspace(true);
  const head = host.querySelector<HTMLElement>(".sidebar-module-head")!;
  const add = head.querySelector<HTMLButtonElement>('[aria-label="新建项目"]')!;

  expect(add).not.toBeNull();
  // 现在只有侧栏标题这一个新建项目入口。
  expect(host.querySelectorAll('[aria-label="新建项目"]')).toHaveLength(1);

  add.click();
  await nextTick();

  expect(host.querySelector('[data-testid="project-dialog"]')).not.toBeNull();
});

it("点击 Agents PM Tool 标题收展项目列表，并把选择写进本机偏好", async () => {
  const toggle = await mountWorkspace(true);
  expect(toggle.getAttribute("aria-expanded")).toBe("true");
  expect(panel().style.display).not.toBe("none");

  toggle.click();
  await nextTick();

  expect(toggle.getAttribute("aria-expanded")).toBe("false");
  expect(panel().style.display).toBe("none");
  expect(toggle.title).toBe("展开项目");
  expect(useViewStore(pinia).projectsOpen).toBe(false);
  expect(
    JSON.parse(localStorage.getItem("pm-table-view-v1")!).projectsOpen,
  ).toBe(false);

  toggle.click();
  await nextTick();
  expect(panel().style.display).not.toBe("none");
  expect(useViewStore(pinia).projectsOpen).toBe(true);
});

it("按上次保存的状态打开侧栏", async () => {
  const toggle = await mountWorkspace(false);

  expect(toggle.getAttribute("aria-expanded")).toBe("false");
  expect(toggle.title).toBe("展开项目");
  // 收起只隐藏展开区，项目条目仍在文档里，展开时无需重新拉取项目。
  expect(panel().querySelector(".project-nav-row")).not.toBeNull();
});

it("侧栏收成图标栏后没有折叠开关，项目列表保持可见", async () => {
  localStorage.setItem("pm-theme", "light");
  localStorage.setItem(
    "pm-table-view-v1",
    JSON.stringify({ collapsed: true, projectsOpen: false }),
  );
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(GridApp);
  app.use(pinia);
  app.mount(host);

  await vi.waitFor(() => {
    expect(host.querySelector(".project-nav-row")).not.toBeNull();
  });

  expect(host.querySelector(".workspace")!.classList).toContain(
    "sidebar-collapsed",
  );
  // 图标栏里标题被隐藏，模块的收起状态不适用，项目图标照常可点。
  expect(panel().style.display).not.toBe("none");
  expect(host.querySelector(".sidebar-module-head")).not.toBeNull();
});
