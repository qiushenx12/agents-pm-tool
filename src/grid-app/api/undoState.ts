import { ref } from "vue";

// Per-page, per-login history. The server remains the authority for ownership and conflicts.
export const undoIds = ref<number[]>([]);
export const undoBusy = ref(false);
export const pendingTaskWrites = ref(0);
let epoch = 0;
export function resetUndo() { epoch++; undoIds.value = []; }
export function beginTaskWrite() { pendingTaskWrites.value++; return epoch; }
export function finishTaskWrite(startEpoch: number, response?: Response) {
  pendingTaskWrites.value--;
  if (startEpoch !== epoch || !response?.ok) return;
  const header = response.headers.get("X-PM-Undo");
  if (header === null) { resetUndo(); return; }
  const id = Number(header);
  if (Number.isSafeInteger(id) && id > 0 && !undoIds.value.includes(id)) {
    undoIds.value = [...undoIds.value, id].sort((a, b) => a - b).slice(-100);
  }
}
export function isUndoShortcut(event: KeyboardEvent) {
  if (event.defaultPrevented || event.isComposing || event.repeat || event.altKey || event.shiftKey ||
      !(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "z") return false;
  const target = event.target;
  return !(target instanceof Element && target.closest('input,textarea,select,[contenteditable]:not([contenteditable="false"]),[role="textbox"]'));
}
