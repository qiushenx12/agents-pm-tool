import { computed, ref } from "vue";
import { api } from "../api/client";
import { errorText } from "@/shared/feedback";
export interface UploadItem {
  id: number;
  file: File;
  state: "ready" | "uploading" | "done" | "error";
  error: string;
}
export function useUploadQueue() {
  const items = ref<UploadItem[]>([]);
  const busy = ref(false);
  let nextId = 0;
  const failed = computed(() =>
    items.value.filter((item) => item.state === "error"),
  );
  function add(files: File[]) {
    if (busy.value) return;
    for (const file of files) {
      if (
        !items.value.some(
          (item) =>
            item.file.name === file.name &&
            item.file.size === file.size &&
            item.file.lastModified === file.lastModified,
        )
      ) {
        items.value.push({ id: ++nextId, file, state: "ready", error: "" });
      }
    }
  }
  function remove(id: number) {
    if (!busy.value) items.value = items.value.filter((item) => item.id !== id);
  }
  async function upload(taskId: string) {
    if (busy.value) return false;
    busy.value = true;
    try {
      for (const item of items.value) {
        if (item.state === "done") continue;
        item.state = "uploading";
        item.error = "";
        try {
          await api.uploadAttachment(taskId, item.file);
          item.state = "done";
        } catch (e) {
          item.state = "error";
          item.error = errorText(e);
        }
      }
      return items.value.every((item) => item.state === "done");
    } finally {
      busy.value = false;
    }
  }
  function clearDone() {
    items.value = items.value.filter((item) => item.state !== "done");
  }
  return { items, busy, failed, add, remove, upload, clearDone };
}
export function formatSize(n: number) {
  return n < 1024
    ? n + " B"
    : n < 1048576
      ? (n / 1024).toFixed(1) + " KB"
      : (n / 1048576).toFixed(1) + " MB";
}
