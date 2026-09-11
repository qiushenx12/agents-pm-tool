// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createApp, h, nextTick, reactive, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import DescriptionEditor from "@/grid-app/components/DescriptionEditor.vue";
import { api } from "@/grid-app/api/client";
const feedback = vi.hoisted(() => ({
  askConfirm: vi.fn(),
  notify: vi.fn(),
}));
vi.mock("@/grid-app/api/client", () => ({
  api: { patchTask: vi.fn(), pageTasks: vi.fn() },
}));
vi.mock("@/shared/feedback", () => ({
  askConfirm: feedback.askConfirm,
  notify: feedback.notify,
  errorText: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
}));
let app: App, pinia: Pinia, host: HTMLElement;
const flush = async () => {
  for (let i = 0; i < 24; i++) await Promise.resolve();
  await nextTick();
};
function mountEditor(inline = true) {
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
            inline,
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
async function leaveEditor(el: HTMLTextAreaElement) {
  const outside = document.createElement("button");
  document.body.append(outside);
  el.focus();
  outside.focus();
  await flush();
  outside.remove();
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  feedback.askConfirm.mockResolvedValue(true);
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
  it("shows only the input inline, keeps failed input, and retries on the next focusout", async () => {
    mountEditor();
    const el = input("保留这个草稿");
    vi.mocked(api.patchTask)
      .mockRejectedValueOnce(new Error("保存失败"))
      .mockResolvedValueOnce({ id: "1", description: "保留这个草稿" } as never);
    expect(host.querySelector(".editor-footer")).toBeNull();
    expect(host.querySelector("button")).toBeNull();
    await leaveEditor(el);
    expect(host.querySelector("textarea")?.value).toBe("保留这个草稿");
    expect(host.textContent).toContain("输入已保留");
    await leaveEditor(el);
    expect(api.patchTask).toHaveBeenCalledTimes(2);
    expect(host.querySelector("textarea")).toBeNull();
  });
  it("preserves dirty input and asks before an external update is overwritten on focusout", async () => {
    const state = mountEditor(),
      el = input("本地草稿");
    feedback.askConfirm.mockResolvedValueOnce(false);
    state.value = "外部最新描述";
    await nextTick();
    expect(el.value).toBe("本地草稿");
    expect(host.textContent).toContain("其他操作更新");
    await leaveEditor(el);
    expect(feedback.askConfirm).toHaveBeenCalledOnce();
    expect(api.patchTask).not.toHaveBeenCalled();
    expect(state.open).toBe(true);
  });
  it("keeps explicit controls in the non-inline detail editor", () => {
    mountEditor(false);
    expect(host.querySelector(".editor-footer")).not.toBeNull();
    expect(host.textContent).toContain("取消");
    expect(host.textContent).toContain("保存");
  });
});

it("updates the note when the editor is used for the note field", async () => {
  mountNoteEditor();
  const el = input("新的备注");
  vi.mocked(api.patchTask).mockResolvedValueOnce({
    id: "1",
    note: "新的备注",
  } as never);
  await leaveEditor(el);
  expect(api.patchTask).toHaveBeenCalledWith("1", { note: "新的备注" });
  expect(host.querySelector("textarea")).toBeNull();
});
