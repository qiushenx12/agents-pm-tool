// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, nextTick, type App } from "vue";
import { h } from "vue";
import ConfigApp from "@/config-app/ConfigApp.vue";
import UiIcon from "@/shared/UiIcon.vue";

const {
  invoke,
  shellOpen,
  running,
  lanUrl,
  serverTheme,
  listenTheme,
  hideWindow,
} = vi.hoisted(() => ({
  invoke: vi.fn(),
  shellOpen: vi.fn(),
  listenTheme: vi.fn(async () => () => {}),
  hideWindow: vi.fn(),
  running: { value: true },
  lanUrl: { value: "" },
  serverTheme: { value: undefined as string | null | undefined },
}));

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
  invoke,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenTheme }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: shellOpen }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
    hide: hideWindow,
  }),
}));

function serverStatus() {
  return {
    running: running.value,
    port: running.value ? 17890 : 0,
    url: running.value ? "http://127.0.0.1:17890" : "",
    lan_url: running.value ? lanUrl.value : "",
    data_dir: "C:/data",
  };
}

invoke.mockImplementation(async (command: string, args?: unknown) => {
  if (command === "get_settings") {
    return {
      port: 17890,
      autostart: true,
      close_behavior: "keep_service",
      listen_scope: "local",
      agent_server_url: "",
      theme: serverTheme.value,
    };
  }
  if (command === "get_server_status") {
    return serverStatus();
  }
  if (command === "set_server_running") {
    running.value = (args as { running: boolean }).running;
    return serverStatus();
  }
  return undefined;
});

let root: HTMLDivElement | undefined;
let app: App | undefined;

afterEach(() => {
  app?.unmount();
  root?.remove();
  document.querySelectorAll(".toast-stack").forEach((el) => el.remove());
  document.documentElement.removeAttribute("data-theme");
  localStorage.clear();
  running.value = true;
  lanUrl.value = "";
  serverTheme.value = undefined;
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: undefined,
  });
  vi.clearAllMocks();
});

/** 挂载设置窗口并等待 load() 落地（.service-address 只在 status 就绪后渲染） */
async function mountConfig() {
  // jsdom 没有 matchMedia；预置主题即可让 currentTheme 直接命中缓存分支。
  localStorage.setItem("pm-theme", "light");
  root = document.createElement("div");
  document.body.appendChild(root);
  app = createApp(ConfigApp);
  app.mount(root);
  await vi.waitFor(() =>
    expect(root!.querySelector(".service-address")).toBeTruthy(),
  );
  await nextTick();
  return root;
}

/** 参考图标：直接渲染一个 globe，用来对照顶部按钮用的是不是一个字形 */
function globePath() {
  const holder = document.createElement("div");
  document.body.appendChild(holder);
  createApp({ render: () => h(UiIcon, { name: "globe" }) }).mount(holder);
  const path = holder.querySelector("path")!.getAttribute("d");
  holder.remove();
  return path;
}

it("顶部的「打开网页」按钮用网页图标，并在系统浏览器打开本机地址", async () => {
  const mounted = await mountConfig();
  const openWeb = [...mounted.querySelectorAll(".service-overview button")].find(
    (button) => button.textContent?.includes("打开网页"),
  ) as HTMLButtonElement | undefined;

  expect(openWeb, "服务概览里应有「打开网页」按钮").toBeTruthy();
  expect(
    openWeb!.querySelector("path")!.getAttribute("d"),
    "「打开网页」应使用网页（globe）图标",
  ).toBe(globePath());
  expect(openWeb!.textContent).not.toContain("打开任务表");

  openWeb!.click();
  await vi.waitFor(() => expect(shellOpen).toHaveBeenCalledWith("http://127.0.0.1:17890"));
  expect(invoke).not.toHaveBeenCalledWith("open_app_window");
});

it("按启动/停止服务、进入应用、打开网页的顺序提供服务操作", async () => {
  const mounted = await mountConfig();
  const actions = [...mounted.querySelectorAll(".service-actions button")];
  expect(actions.map((button) => button.textContent?.trim())).toEqual([
    "停止服务",
    "进入应用",
    "打开网页",
  ]);

  (actions[0] as HTMLButtonElement).click();
  await vi.waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_server_running", {
      running: false,
    }),
  );
  await vi.waitFor(() =>
    expect(actions[0].textContent?.trim()).toBe("启动服务"),
  );
  expect((actions[1] as HTMLButtonElement).disabled).toBe(true);
  expect((actions[2] as HTMLButtonElement).disabled).toBe(true);

  (actions[0] as HTMLButtonElement).click();
  await vi.waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_server_running", {
      running: true,
    }),
  );
  await vi.waitFor(() =>
    expect(actions[0].textContent?.trim()).toBe("停止服务"),
  );
  expect((actions[1] as HTMLButtonElement).disabled).toBe(false);
  expect((actions[2] as HTMLButtonElement).disabled).toBe(false);
});

it("为本机和局域网地址分别提供复制按钮", async () => {
  lanUrl.value = "http://192.168.1.20:17890";
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
  const mounted = await mountConfig();

  const localCopy = mounted.querySelector<HTMLButtonElement>(
    '[aria-label="复制本机地址"]',
  );
  const lanCopy = mounted.querySelector<HTMLButtonElement>(
    '[aria-label="复制局域网地址"]',
  );
  expect(localCopy).not.toBeNull();
  expect(lanCopy).not.toBeNull();

  localCopy!.click();
  await vi.waitFor(() =>
    expect(writeText).toHaveBeenCalledWith("http://127.0.0.1:17890"),
  );
  lanCopy!.click();
  await vi.waitFor(() =>
    expect(writeText).toHaveBeenLastCalledWith("http://192.168.1.20:17890"),
  );
  expect(writeText).toHaveBeenCalledTimes(2);
});

it("「进入应用」位于服务开关和「打开网页」之间，点击后在应用内打开窗口", async () => {
  const mounted = await mountConfig();
  const actions = [...mounted.querySelectorAll(".service-actions button")];
  expect(actions.map((button) => button.textContent?.trim())).toEqual([
    "停止服务",
    "进入应用",
    "打开网页",
  ]);
  expect(
    [...mounted.querySelectorAll(".footer-actions button")].map((button) =>
      button.textContent?.trim(),
    ),
  ).toEqual(["保存设置"]);

  (actions[1] as HTMLButtonElement).click();
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("open_app_window"));
  await vi.waitFor(() => expect(hideWindow).toHaveBeenCalledOnce());
  expect(shellOpen).not.toHaveBeenCalled();
});

it("服务未运行时打开入口不可点，但仍可手动启动服务", async () => {
  running.value = false;
  const mounted = await mountConfig();
  await nextTick();

  const enter = [...mounted.querySelectorAll(".service-overview button")].find(
    (button) => button.textContent?.includes("进入应用"),
  ) as HTMLButtonElement;
  const openWeb = [...mounted.querySelectorAll(".service-overview button")].find(
    (button) => button.textContent?.includes("打开网页"),
  ) as HTMLButtonElement;
  const startService = [
    ...mounted.querySelectorAll(".service-overview button"),
  ].find((button) => button.textContent?.includes("启动服务")) as HTMLButtonElement;
  const localCopy = mounted.querySelector<HTMLButtonElement>(
    '[aria-label="复制本机地址"]',
  );

  expect(enter.disabled).toBe(true);
  expect(openWeb.disabled).toBe(true);
  expect(startService.disabled).toBe(false);
  expect(localCopy?.disabled).toBe(true);
  enter.click();
  expect(invoke).not.toHaveBeenCalledWith("open_app_window");
  running.value = true;
});

// ── 全局主题（设置窗口与网页界面共用一份） ────────────────

it("服务端已存主题时直接应用，不做迁移上报", async () => {
  serverTheme.value = "dark";
  await mountConfig();

  expect(
    document.documentElement.getAttribute("data-theme"),
    "应应用服务端存的暗色主题",
  ).toBe("dark");
  expect(invoke).not.toHaveBeenCalledWith("set_theme", expect.anything());
});

it("服务端未设置且本机存过偏好时，把本机偏好迁移为全局主题", async () => {
  serverTheme.value = null; // 服务端从没设置过
  await mountConfig(); // mountConfig 预置本机 pm-theme=light

  await vi.waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_theme", { theme: "light" }),
  );
  // 本机主题保持不变（本来就是 light）
  expect(document.documentElement.getAttribute("data-theme")).toBe(null);
});

it("点击主题按钮切换明暗并写入全局主题", async () => {
  serverTheme.value = "light";
  const mounted = await mountConfig();
  await nextTick();

  const toggle = mounted.querySelector(
    'button[aria-label="切换明暗主题"]',
  ) as HTMLButtonElement;
  expect(toggle).toBeTruthy();

  toggle.click();
  await vi.waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_theme", { theme: "dark" }),
  );
  expect(document.documentElement.getAttribute("data-theme")).toBe("dark");

  toggle.click();
  await vi.waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_theme", { theme: "light" }),
  );
  expect(document.documentElement.getAttribute("data-theme")).toBe(null);
});
