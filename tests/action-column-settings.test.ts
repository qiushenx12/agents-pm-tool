// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it } from "vitest";
import { createPinia, disposePinia, setActivePinia, type Pinia } from "pinia";
import { createApp, h, nextTick, type App } from "vue";
import ActionColumnSettings from "@/grid-app/components/ActionColumnSettings.vue";
import { TASK_ROW_ACTIONS } from "@/grid-app/taskActions";
import { useViewStore } from "@/grid-app/stores/viewStore";

const KEYS = TASK_ROW_ACTIONS.map((action) => action.key);
let app: App | undefined;
let host: HTMLElement;
let pinia: Pinia;

beforeEach(() => {
  localStorage.clear();
  pinia = createPinia();
  setActivePinia(pinia);
  host = document.createElement("div");
  document.body.append(host);
});

afterEach(() => {
  app?.unmount();
  app = undefined;
  host?.remove();
  disposePinia(pinia);
  localStorage.clear();
});

function mount() {
  app = createApp({ render: () => h(ActionColumnSettings) });
  app.use(pinia);
  app.mount(host);
}

function boxes() {
  return [...host.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
}

it("新装用户默认两个按钮都显示", () => {
  mount();
  const view = useViewStore();
  expect(view.hiddenActions).toEqual([]);
  expect(view.visibleRowActions.map((action) => action.key)).toEqual(KEYS);
  expect(boxes().map((box) => box.checked)).toEqual([true, true]);
  expect(boxes().map((box) => box.disabled)).toEqual([false, false]);
});

it("可以逐个隐藏按钮，但最后一个不允许隐藏", async () => {
  mount();
  const view = useViewStore();

  boxes()[0].click();
  await nextTick();
  expect(view.hiddenActions).toEqual([KEYS[0]]);
  expect(view.visibleRowActions.map((action) => action.key)).toEqual([
    KEYS[1],
  ]);
  expect(boxes()[0].checked).toBe(false);

  // 只剩一个按钮：另一个变成不可取消，并给出提示
  expect(view.canToggleRowAction(KEYS[1])).toBe(false);
  expect(boxes()[1].disabled).toBe(true);
  expect(host.textContent).toContain("至少保留一个");
  boxes()[1].click();
  await nextTick();
  expect(view.visibleRowActions.map((action) => action.key)).toEqual([
    KEYS[1],
  ]);

  // 已隐藏的可以重新显示，恢复后两个都能再取消
  boxes()[0].click();
  await nextTick();
  expect(view.visibleRowActions.map((action) => action.key)).toEqual(KEYS);
  expect(boxes().map((box) => box.disabled)).toEqual([false, false]);
});

it("隐藏状态写进本地偏好，重新打开仍然生效", async () => {
  mount();
  useViewStore().toggleRowAction(KEYS[1]);
  await nextTick();
  expect(
    JSON.parse(localStorage.getItem("pm-table-view-v1") || "{}").hiddenActions,
  ).toEqual([KEYS[1]]);

  disposePinia(pinia);
  pinia = createPinia();
  setActivePinia(pinia);
  expect(useViewStore().hiddenActions).toEqual([KEYS[1]]);
  expect(useViewStore().visibleRowActions.map((action) => action.key)).toEqual([
    KEYS[0],
  ]);
});

it("偏好被改坏成全部隐藏时回落到全部显示", () => {
  localStorage.setItem(
    "pm-table-view-v1",
    JSON.stringify({ hiddenActions: KEYS }),
  );
  expect(useViewStore().visibleRowActions.map((action) => action.key)).toEqual(
    KEYS,
  );
});

it("恢复默认布局会把按钮一并还原", async () => {
  mount();
  const view = useViewStore();
  view.toggleRowAction(KEYS[0]);
  await nextTick();
  view.reset();
  await nextTick();
  expect(view.hiddenActions).toEqual([]);
  expect(view.visibleRowActions.map((action) => action.key)).toEqual(KEYS);
});
