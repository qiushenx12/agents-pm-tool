// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createPinia, disposePinia, setActivePinia, type Pinia } from "pinia";
import { createApp, nextTick, type App } from "vue";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import SavedViews from "@/grid-app/components/SavedViews.vue";

let app: App | undefined;
let host: HTMLElement;
let pinia: Pinia;
let style: HTMLStyleElement;

beforeEach(() => {
  localStorage.clear();
  window.history.replaceState(null, "", "/");
  pinia = createPinia();
  setActivePinia(pinia);
  host = document.createElement("div");
  document.body.append(host);
  style = document.createElement("style");
  // jsdom does not ship the browser's native button face; model it so the regression is observable.
  style.textContent = "button { background: rgb(190, 190, 190); }\n" +
    readFileSync(resolve("src/grid-app/grid.css"), "utf8");
  document.head.append(style);
});

afterEach(() => {
  app?.unmount();
  app = undefined;
  host.remove();
  style.remove();
  disposePinia(pinia);
  vi.restoreAllMocks();
});

it("keeps the graph tab transparent and leaves only the active tab underlined", () => {
  app = createApp(SavedViews, { mode: "graph" });
  app.mount(host);
  const graph = host.querySelector<HTMLButtonElement>(".graph-view-tab")!;
  const table = host.querySelector<HTMLButtonElement>(".view-selector")!;
  expect(getComputedStyle(graph).backgroundColor).toBe("rgba(0, 0, 0, 0)");
  expect(getComputedStyle(graph).borderBottomColor).not.toBe("transparent");
  expect(getComputedStyle(table).borderBottomColor).toBe("rgba(0, 0, 0, 0)");
});

it("shows a graph tab in place of the external save button while keeping save inside the list", async () => {
  const onMode = vi.fn();
  app = createApp(SavedViews, { mode: "table", "onUpdate:mode": onMode });
  app.mount(host);

  expect(host.querySelector(".save-view-button")).toBeNull();
  const graph = host.querySelector<HTMLButtonElement>("[role=tab][aria-label=关联图]");
  expect(graph).not.toBeNull();
  graph!.click();
  expect(onMode).toHaveBeenCalledWith("graph");

  host.querySelector<HTMLButtonElement>("[aria-label=切换筛选方案]")!.click();
  await nextTick();
  expect(document.body.textContent).toContain("保存当前条件为新方案");
});
