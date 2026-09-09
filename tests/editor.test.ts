// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createApp, h, nextTick, reactive, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import DescriptionEditor from "@/grid-app/components/DescriptionEditor.vue";
import { api } from "@/grid-app/api/client";
vi.mock("@/grid-app/api/client", () => ({
  api: { patchTask: vi.fn(), pageTasks: vi.fn() },
}));
let app: App, pinia: Pinia, host: HTMLElement;
const flush = async () => {
  for (let i = 0; i < 24; i++) await Promise.resolve();
  await nextTick();
};
function mountEditor() {
  const state = reactive({ value: "原始描述", open: true });
  pinia = createPinia();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({
    render: () =>
      state.open
        ? h(DescriptionEditor, {
            taskId: "1",
            value: state.value,
            inline: true,
            onClose: () => {
              state.open = false;
            },
          })
        : null,
  });
  app.use(pinia);
  app.mount(host);
  return state;
}
function mountNoteEditor() {
  const state = reactive({ value: "原始备注", open: true });
  pinia = createPinia();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({
    render: () =>
      state.open
        ? h(DescriptionEditor, {
            taskId: "1",
            value: state.value,
            field: "note",
            inline: true,
            onClose: () => {
              state.open = false;
            },
          })
        : null,
  });
  app.use(pinia);
  app.mount(host);
  return state;
}
function input(value: string) {
  const el = host.querySelector("textarea")!;
  el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  return el;
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [],
    total: 0,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  });
  window.history.replaceState(null, "", "/");
});
afterEach(() => {
  app?.unmount();
  disposePinia(pinia);
  host?.remove();
  vi.useRealTimers();
});
describe("description editor draft safety", () => {
  it("Escape cancels without submitting on subsequent focusout", async () => {
    const state = mountEditor(),
      el = input("不要保存");
    el.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    el.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    await flush();
    expect(state.open).toBe(false);
    expect(api.patchTask).not.toHaveBeenCalled();
  });
  it("keeps input on failure and retries one explicit save", async () => {
    mountEditor();
    input("保留这个草稿");
    vi.mocked(api.patchTask)
      .mockRejectedValueOnce(new Error("保存失败"))
      .mockResolvedValueOnce({ id: "1", description: "保留这个草稿" } as never);
    host.querySelector<HTMLButtonElement>(".btn-primary")!.click();
    await flush();
    expect(host.querySelector("textarea")?.value).toBe("保留这个草稿");
    expect(host.textContent).toContain("输入已保留");
    host.querySelector<HTMLButtonElement>(".btn-primary")!.click();
    await flush();
    expect(api.patchTask).toHaveBeenCalledTimes(2);
    expect(host.querySelector("textarea")).toBeNull();
  });
  it("preserves dirty input across an external update and never overwrites it on blur", async () => {
    const state = mountEditor(),
      el = input("本地草稿");
    state.value = "外部最新描述";
    await nextTick();
    expect(el.value).toBe("本地草稿");
    expect(host.textContent).toContain("其他操作更新");
    el.blur();
    el.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    await flush();
    expect(api.patchTask).not.toHaveBeenCalled();
    expect(state.open).toBe(true);
  });
});

it("updates the note when the editor is used for the note field", async () => {
  mountNoteEditor();
  input("新的备注");
  vi.mocked(api.patchTask).mockResolvedValueOnce({
    id: "1",
    note: "新的备注",
  } as never);
  host.querySelector<HTMLButtonElement>(".btn-primary")!.click();
  await flush();
  expect(api.patchTask).toHaveBeenCalledWith("1", { note: "新的备注" });
  expect(host.querySelector("textarea")).toBeNull();
});
