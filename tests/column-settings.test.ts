// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it } from "vitest";
import { createPinia, disposePinia, setActivePinia, type Pinia } from "pinia";
import { createApp, h, nextTick, type App } from "vue";
import ColumnSettings from "@/grid-app/components/ColumnSettings.vue";
import { useViewStore } from "@/grid-app/stores/viewStore";

let app: App<Element>;
let host: HTMLElement;
let pinia: Pinia;

beforeEach(() => {
  localStorage.clear();
  pinia = createPinia();
  setActivePinia(pinia);
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(ColumnSettings) });
  app.use(pinia);
  app.mount(host);
});

afterEach(() => {
  app.unmount();
  host.remove();
  disposePinia(pinia);
});

it("shows notes as movable and actions as the fixed final column", () => {
  const options = Array.from(
    host.querySelectorAll<HTMLElement>(".column-option"),
  );
  const note = options.find((option) => option.textContent?.includes("备注"));
  const description = options.find((option) =>
    option.textContent?.includes("任务描述"),
  );
  const actions = options.find((option) =>
    option.textContent?.includes("操作"),
  );

  expect(note?.draggable).toBe(true);
  expect(note?.textContent).not.toContain("固定末列");
  expect(description?.draggable).toBe(true);
  expect(description?.textContent).toContain("始终显示");
  expect(description?.querySelector<HTMLInputElement>('input[type="checkbox"]'))
    .toMatchObject({ checked: true, disabled: true });
  expect(actions).toBe(options[options.length - 1]);
  expect(actions?.draggable).toBe(false);
  expect(actions?.textContent).toContain("固定末列");
  expect(
    actions?.querySelector<HTMLInputElement>('input[type="checkbox"]'),
  ).toMatchObject({ checked: true, disabled: true });
});

it("marks the insertion row while dragging and clears it after dropping", async () => {
  const options = Array.from(host.querySelectorAll<HTMLElement>(".column-option"));
  const source = options.find((option) => option.textContent?.includes("备注"))!;
  const target = options.find((option) => option.textContent?.includes("优先级"))!;
  const description = options.find((option) =>
    option.textContent?.includes("任务描述"),
  )!;
  const actions = options.find((option) => option.textContent?.includes("操作"))!;
  const view = useViewStore(pinia);

  source.dispatchEvent(new Event("dragstart", { bubbles: true, cancelable: true }));
  target.dispatchEvent(new Event("dragover", { bubbles: true, cancelable: true }));
  await nextTick();
  expect(target.classList.contains("column-drop-before")).toBe(true);

  description.dispatchEvent(new Event("dragover", { bubbles: true, cancelable: true }));
  await nextTick();
  expect(description.classList.contains("column-drop-before")).toBe(true);

  description.dispatchEvent(new Event("drop", { bubbles: true, cancelable: true }));
  await nextTick();
  expect(host.querySelector(".column-drop-before")).toBeNull();
  expect(view.columns[0].key).toBe("note");

  description.dispatchEvent(new Event("dragstart", { bubbles: true, cancelable: true }));
  actions.dispatchEvent(new Event("dragover", { bubbles: true, cancelable: true }));
  actions.dispatchEvent(new Event("drop", { bubbles: true, cancelable: true }));
  await nextTick();
  expect(view.columns.at(-1)?.key).toBe("description");
});
