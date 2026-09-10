// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import GridApp from "@/grid-app/GridApp.vue";
import { api } from "@/grid-app/api/client";
import { useTaskStore } from "@/grid-app/stores/taskStore";

const { createdTask, quickTask, reveal } = vi.hoisted(() => ({
  createdTask: {
    id: "new-task",
    seq: 1,
    project: "测试项目",
    type: "新增需求" as const,
    description: "创建后保持在列表",
    note: "",
    status: "未开始" as const,
    submitter: "用户" as const,
    created_at: "",
    updated_at: "",
    finished_at: null,
    position: 1,
  },
  quickTask: {
    id: "quick-task",
    seq: 2,
    project: "测试项目",
    type: "新增需求" as const,
    description: "",
    note: "",
    status: "未开始" as const,
    submitter: "用户" as const,
    created_at: "",
    updated_at: "",
    finished_at: null,
    position: 2,
  },
  reveal: vi.fn(),
}));

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
      skill_ready: false,
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

vi.mock("@/grid-app/components/FilterBar.vue", async () => {
  const { defineComponent, h } = await import("vue");
  return {
    default: defineComponent({
      emits: ["create"],
      setup(_, { emit }) {
        return () =>
          h(
            "button",
            { "data-testid": "open-create", onClick: () => emit("create") },
            "新建",
          );
      },
    }),
  };
});

vi.mock("@/grid-app/components/TaskCreateModal.vue", async () => {
  const { defineComponent, h } = await import("vue");
  return {
    default: defineComponent({
      emits: ["created"],
      setup(_, { emit }) {
        return () =>
          h(
            "button",
            {
              "data-testid": "complete-create",
              onClick: () => emit("created", createdTask),
            },
            "完成创建",
          );
      },
    }),
  };
});

vi.mock("@/grid-app/components/TaskGrid.vue", async () => {
  const { defineComponent, h } = await import("vue");
  return {
    default: defineComponent({
      props: { quickCreating: Boolean },
      emits: ["quick-create"],
      setup(props, { emit, expose }) {
        expose({ reveal });
        return () =>
          h("div", { "data-testid": "task-grid" }, [
            h(
              "button",
              {
                "data-testid": "quick-create",
                disabled: props.quickCreating,
                onClick: () => emit("quick-create"),
              },
              "快捷新增",
            ),
          ]);
      },
    }),
  };
});

vi.mock("@/grid-app/components/TaskDetailDrawer.vue", async () => {
  const { defineComponent, h } = await import("vue");
  return {
    default: defineComponent({
      setup() {
        return () => h("div", { "data-testid": "task-detail-drawer" });
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

vi.mock("@/grid-app/components/ProjectOptionPopover.vue", async () => {
  const { defineComponent, h } = await import("vue");
  return {
    default: defineComponent({
      props: { projectName: String },
      setup(props) {
        return () =>
          h(
            "div",
            { "data-testid": "project-dialog" },
            props.projectName || "new-project",
          );
      },
    }),
  };
});

vi.mock("@/shared/AppFeedback.vue", async () => {
  const { defineComponent } = await import("vue");
  return { default: defineComponent({ template: "<div />" }) };
});

let app: App;
let pinia: Pinia;
let host: HTMLElement;

afterEach(() => {
  app?.unmount();
  disposePinia(pinia);
  host?.remove();
  vi.clearAllMocks();
  localStorage.clear();
});

it("does not open the detail drawer after creating a task", async () => {
  localStorage.setItem("pm-theme", "light");
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(GridApp);
  app.use(pinia);
  app.mount(host);
  await vi.waitFor(() => {
    expect(
      host.querySelector<HTMLButtonElement>('[data-testid="open-create"]'),
    ).not.toBeNull();
  });

  host
    .querySelector<HTMLButtonElement>('[data-testid="open-create"]')!
    .click();
  await nextTick();
  host
    .querySelector<HTMLButtonElement>('[data-testid="complete-create"]')!
    .click();
  await nextTick();

  expect(host.querySelector('[data-testid="complete-create"]')).toBeNull();
  expect(host.querySelector('[data-testid="task-detail-drawer"]')).toBeNull();
  expect(useTaskStore(pinia).records[createdTask.id]).toMatchObject(createdTask);
  expect(reveal).toHaveBeenCalledWith(createdTask.id);
});

it("creates a blank task directly from the list footer", async () => {
  vi.mocked(api.createTask).mockResolvedValueOnce(quickTask);
  localStorage.setItem("pm-theme", "light");
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(GridApp);
  app.use(pinia);
  app.mount(host);
  await vi.waitFor(() => {
    expect(
      host.querySelector<HTMLButtonElement>('[data-testid="quick-create"]'),
    ).not.toBeNull();
  });

  host
    .querySelector<HTMLButtonElement>('[data-testid="quick-create"]')!
    .click();

  await vi.waitFor(() => {
    expect(api.createTask).toHaveBeenCalledWith({
      project: "测试项目",
      type: "新增需求",
      description: "",
      note: "",
    });
    expect(useTaskStore(pinia).records[quickTask.id]).toMatchObject(quickTask);
  });
  expect(host.querySelector('[data-testid="complete-create"]')).toBeNull();
  expect(reveal).toHaveBeenCalledWith(quickTask.id);
});

it("opens settings from a project's three-dot button without a footer management entry", async () => {
  localStorage.setItem("pm-theme", "light");
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(GridApp);
  app.use(pinia);
  app.mount(host);
  await vi.waitFor(() => {
    expect(
      host.querySelector<HTMLButtonElement>(
        '[aria-label="项目设置：测试项目"]',
      ),
    ).not.toBeNull();
  });

  expect(host.querySelector('[title="项目管理"]')).toBeNull();
  host
    .querySelector<HTMLButtonElement>('[aria-label="项目设置：测试项目"]')!
    .click();
  await nextTick();

  expect(
    host.querySelector('[data-testid="project-dialog"]')?.textContent,
  ).toBe("测试项目");
});
