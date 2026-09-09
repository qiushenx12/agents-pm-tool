// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it } from "vitest";
import { createPinia, disposePinia, setActivePinia, type Pinia } from "pinia";
import { createApp, h, type App } from "vue";
import ColumnSettings from "@/grid-app/components/ColumnSettings.vue";

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
  const actions = options.find((option) =>
    option.textContent?.includes("操作"),
  );

  expect(note?.draggable).toBe(true);
  expect(note?.textContent).not.toContain("固定末列");
  expect(actions).toBe(options[options.length - 1]);
  expect(actions?.draggable).toBe(false);
  expect(actions?.textContent).toContain("固定末列");
  expect(
    actions?.querySelector<HTMLInputElement>('input[type="checkbox"]'),
  ).toMatchObject({ checked: true, disabled: true });
});
