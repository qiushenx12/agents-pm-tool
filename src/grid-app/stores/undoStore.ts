import { computed } from "vue";
import { defineStore } from "pinia";
import { api } from "../api/client";
import { pendingTaskWrites, undoBusy, undoIds } from "../api/undoState";
import { useTaskStore } from "./taskStore";
import { errorText, notify } from "@/shared/feedback";

export const useUndoStore = defineStore("undo", () => {
  const tasks = useTaskStore();
  const available = computed(() => undoIds.value.length > 0);
  const busy = computed(() => undoBusy.value || pendingTaskWrites.value > 0 || tasks.saving);
  async function undo() {
    if (busy.value) return;
    const id = undoIds.value[undoIds.value.length - 1];
    if (id === undefined) { notify("当前页面没有可撤销的任务操作"); return; }
    undoBusy.value = true;
    try {
      const restored = await api.undoOperation(id);
      if (!undoIds.value.includes(id)) return;
      undoIds.value = undoIds.value.filter(value => value !== id);
      restored.forEach(task => tasks.acceptTask(task));
      tasks.externalRevision++;
      tasks.scheduleRefresh();
      notify("已撤销上一步操作");
    } catch (e) {
      notify(errorText(e), "error");
    } finally {
      undoBusy.value = false;
    }
  }
  return { available, busy, undo };
});
