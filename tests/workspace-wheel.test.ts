// @vitest-environment jsdom
import { afterAll, afterEach, beforeAll, expect, it, vi } from "vitest";
import { createApp, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import GridApp from "@/grid-app/GridApp.vue";

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
    listProjects: vi.fn().mockResolvedValue([]),
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

// 工具栏与视图标签栏对本次验证无关，替换成最简桩件，避免引入额外的接口调用。
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
  const { defineComponent, h } = await import("vue");
  return {
    default: defineComponent({
      setup() {
        return () => h("div", { class: "view-tabs" }, "视图标签栏");
      },
    }),
  };
});

vi.mock("@/grid-app/components/BulkActions.vue", async () => {
  const { defineComponent } = await import("vue");
  return { default: defineComponent({ template: "<div />" }) };
});

vi.mock("@/shared/AppFeedback.vue", async () => {
  const { defineComponent } = await import("vue");
  return { default: defineComponent({ template: "<div />" }) };
});

/** 折叠区域的自然高度（jsdom 不做布局，得手动喂给组件）。 */
const HEADER_NATURAL = 120;
/** 一格鼠标滚轮的位移。 */
const NOTCH = 100;
/** 折叠总行程 = 自然高度 × 3，所以约三格收完。 */
const TRAVEL = HEADER_NATURAL * 3;

const originalRect = HTMLElement.prototype.getBoundingClientRect;

beforeAll(() => {
  // jsdom 里所有元素高度都是 0，折叠需要真实高度才能算，这里按需伪造。
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
    const height = this.classList.contains("workspace-collapsible-inner")
      ? HEADER_NATURAL
      : 0;
    return {
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: 0,
      bottom: height,
      width: 0,
      height,
      toJSON: () => ({}),
    } as DOMRect;
  };
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

afterAll(() => {
  HTMLElement.prototype.getBoundingClientRect = originalRect;
  vi.unstubAllGlobals();
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

async function mountWorkspace() {
  localStorage.setItem("pm-theme", "light");
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp(GridApp);
  app.use(pinia);
  app.mount(host);

  await vi.waitFor(() => {
    const element = host.querySelector<HTMLElement>(".workspace-collapsible");
    // 量到自然高度后才会写上像素高度；初始状态就是完整高度。
    expect(element?.style.height).toBe(`${HEADER_NATURAL}px`);
  });
  return host.querySelector<HTMLElement>(".workspace-collapsible")!;
}

function wheel(target: Element, deltaY: number) {
  const event = new WheelEvent("wheel", {
    deltaY,
    bubbles: true,
    cancelable: true,
  });
  target.dispatchEvent(event);
  return event;
}

/** 期望高度：偏移 / 总行程 的进度线性折算。 */
function expectedHeight(offset: number) {
  const clamped = Math.min(TRAVEL, Math.max(0, offset));
  return HEADER_NATURAL * (1 - clamped / TRAVEL);
}

function heightOf(element: HTMLElement) {
  return Number.parseFloat(element.style.height || "0");
}

it("在表格上方滚动时逐步收起，滚满行程才收到底", async () => {
  const collapsible = await mountWorkspace();
  const header = host.querySelector<HTMLElement>(".workspace-header")!;

  const first = wheel(header, NOTCH);
  await nextTick();

  // 上方区域自身不滚动，这次滚轮被折叠接手。
  expect(first.defaultPrevented).toBe(true);
  // 一格只收掉一部分 —— 这就是"逐步收起，不是一步到位"。
  expect(heightOf(collapsible)).toBeCloseTo(expectedHeight(NOTCH), 5);
  expect(heightOf(collapsible)).toBeGreaterThan(0);
  expect(heightOf(collapsible)).toBeLessThan(HEADER_NATURAL);
  expect(collapsible.classList.contains("is-collapsed")).toBe(false);

  wheel(header, NOTCH);
  await nextTick();
  expect(heightOf(collapsible)).toBeCloseTo(expectedHeight(2 * NOTCH), 5);

  wheel(header, NOTCH);
  await nextTick();
  expect(heightOf(collapsible)).toBeCloseTo(expectedHeight(3 * NOTCH), 5);

  wheel(header, NOTCH);
  await nextTick();
  expect(heightOf(collapsible)).toBe(0);
  expect(collapsible.classList.contains("is-collapsed")).toBe(true);
});

it("收到底后继续向下滚不再变化", async () => {
  const collapsible = await mountWorkspace();
  const header = host.querySelector<HTMLElement>(".workspace-header")!;

  for (let i = 0; i < 5; i += 1) wheel(header, NOTCH);
  await nextTick();
  expect(heightOf(collapsible)).toBe(0);

  wheel(header, NOTCH);
  await nextTick();
  expect(heightOf(collapsible)).toBe(0);
  expect(collapsible.classList.contains("is-collapsed")).toBe(true);
});

it("向上滚按原路退回，回到顶部完全展开", async () => {
  const collapsible = await mountWorkspace();
  const header = host.querySelector<HTMLElement>(".workspace-header")!;

  for (let i = 0; i < 4; i += 1) wheel(header, NOTCH);
  await nextTick();
  expect(heightOf(collapsible)).toBe(0);

  wheel(header, -NOTCH);
  await nextTick();
  expect(heightOf(collapsible)).toBeCloseTo(expectedHeight(TRAVEL - NOTCH), 5);
  expect(collapsible.classList.contains("is-collapsed")).toBe(false);

  // 一路退回全展开，多滚的不会把高度撑过自然高度。
  for (let i = 0; i < 5; i += 1) wheel(header, -NOTCH);
  await nextTick();
  expect(heightOf(collapsible)).toBe(HEADER_NATURAL);
});

it("表格那几块上的滚轮不折叠页头", async () => {
  const collapsible = await mountWorkspace();
  const wrap = host.querySelector<HTMLElement>(".grid-wrap")!;

  const event = wheel(wrap.querySelector<HTMLElement>("table")!, NOTCH);
  await nextTick();

  expect(event.defaultPrevented).toBe(false);
  expect(heightOf(collapsible)).toBe(HEADER_NATURAL);
});

it("表格自身滚动不再触发折叠", async () => {
  const collapsible = await mountWorkspace();
  const wrap = host.querySelector<HTMLElement>(".grid-wrap")!;

  Object.defineProperty(wrap, "scrollTop", { value: 400, configurable: true });
  Object.defineProperty(wrap, "scrollHeight", {
    value: 2000,
    configurable: true,
  });
  Object.defineProperty(wrap, "clientHeight", {
    value: 600,
    configurable: true,
  });
  wrap.dispatchEvent(new Event("scroll"));
  await nextTick();

  expect(heightOf(collapsible)).toBe(HEADER_NATURAL);
  expect(collapsible.classList.contains("is-collapsed")).toBe(false);
});
