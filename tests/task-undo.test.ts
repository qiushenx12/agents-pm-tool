// @vitest-environment jsdom
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createPinia, disposePinia, setActivePinia, type Pinia } from "pinia";
import { beginTaskWrite, finishTaskWrite, isUndoShortcut, pendingTaskWrites, resetUndo, undoBusy, undoIds } from "@/grid-app/api/undoState";
import { useUndoStore } from "@/grid-app/stores/undoStore";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import { api } from "@/grid-app/api/client";
const feedback = vi.hoisted(() => ({ notify: vi.fn() }));
vi.mock("@/grid-app/api/client", () => ({ api: { undoOperation: vi.fn(), pageTasks: vi.fn() } }));
vi.mock("@/shared/feedback", () => ({ notify: feedback.notify, errorText: (e: Error) => e.message }));
let pinia: Pinia;
beforeEach(() => {
  vi.clearAllMocks(); vi.useFakeTimers(); resetUndo(); undoBusy.value = false; pendingTaskWrites.value = 0;
  window.history.replaceState(null, "", "/"); pinia = createPinia(); setActivePinia(pinia);
});
afterEach(() => { disposePinia(pinia); vi.useRealTimers(); });
function completed(id: number) { finishTaskWrite(beginTaskWrite(), new Response("{}", { headers: { "X-PM-Undo": String(id) } })); }
it("tracks committed operations in server order and ignores errors, noops and old login responses", () => {
  completed(9); completed(4); completed(0); completed(9);
  finishTaskWrite(beginTaskWrite(), new Response("{}", { status: 403 }));
  expect(undoIds.value).toEqual([4, 9]);
  const stale = beginTaskWrite(); resetUndo();
  finishTaskWrite(stale, new Response("{}", { headers: { "X-PM-Undo": "20" } }));
  expect(undoIds.value).toEqual([]); expect(pendingTaskWrites.value).toBe(0);
  completed(21); finishTaskWrite(beginTaskWrite(), new Response("{}"));
  expect(undoIds.value).toEqual([]);
});
it("keeps native text undo and ignores redo, key repeats and prevented events", () => {
  expect(isUndoShortcut(new KeyboardEvent("keydown", { key: "z", ctrlKey: true }))).toBe(true);
  expect(isUndoShortcut(new KeyboardEvent("keydown", { key: "z", metaKey: true }))).toBe(true);
  for (const options of [{ shiftKey: true }, { repeat: true }, { altKey: true }, { isComposing: true }])
    expect(isUndoShortcut(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, ...options }))).toBe(false);
  for (const html of ["<input>", "<textarea></textarea>", '<div contenteditable="true"><span>text</span></div>', '<div role="textbox"></div>']) {
    const host = document.createElement("div"); host.innerHTML = html; document.body.append(host);
    const target = host.querySelector("span") ?? host.firstElementChild!;
    const event = new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }); target.dispatchEvent(event);
    expect(isUndoShortcut(event)).toBe(false); host.remove();
  }
});
it("prevents concurrent undo, keeps failed entries and refreshes on success", async () => {
  completed(1); const store = useUndoStore();
  const tasks = useTaskStore(); const refresh = vi.spyOn(tasks, "scheduleRefresh");
  vi.mocked(api.undoOperation).mockRejectedValueOnce(new Error("字段已变化")).mockResolvedValueOnce([]);
  await store.undo(); expect(undoIds.value).toEqual([1]); expect(feedback.notify).toHaveBeenLastCalledWith("字段已变化", "error");
  pendingTaskWrites.value = 1; await store.undo(); expect(api.undoOperation).toHaveBeenCalledTimes(1); pendingTaskWrites.value = 0;
  const one = store.undo(); const two = store.undo(); await Promise.all([one, two]);
  expect(api.undoOperation).toHaveBeenCalledTimes(2); expect(undoIds.value).toEqual([]);
  expect(tasks.externalRevision).toBe(1); expect(refresh).toHaveBeenCalled();
});
