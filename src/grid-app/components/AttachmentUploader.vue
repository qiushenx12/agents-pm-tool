<script setup lang="ts">
import { ref } from "vue";
import UiIcon from "@/shared/UiIcon.vue";
import { formatSize, type UploadItem } from "./useUploadQueue";
defineProps<{ items: UploadItem[]; busy: boolean }>();
const emit = defineEmits<{ add: [files: File[]]; remove: [id: number] }>();
const input = ref<HTMLInputElement>();
const dragging = ref(false);
function choose(e: Event) {
  const el = e.target as HTMLInputElement;
  emit("add", Array.from(el.files ?? []));
  el.value = "";
}
function drop(e: DragEvent) {
  dragging.value = false;
  emit("add", Array.from(e.dataTransfer?.files ?? []));
}
</script>
<template>
  <div
    class="upload-zone"
    :class="{ dragging, 'upload-disabled': busy }"
    @dragover.prevent="dragging = !busy"
    @dragleave.prevent="dragging = false"
    @drop.prevent="!busy && drop($event)"
  >
    <UiIcon name="upload" :size="22" />
    <div>
      <button
        type="button"
        class="text-button"
        :disabled="busy"
        @click="input?.click()"
      >
        选择文件</button
      ><span class="muted"> 或拖拽文件到这里</span>
      <p class="form-hint">支持图片、视频及文档附件</p>
    </div>
    <input
      ref="input"
      type="file"
      multiple
      hidden
      :disabled="busy"
      aria-label="选择附件"
      @change="choose"
    />
  </div>
  <ul v-if="items.length" class="upload-list">
    <li v-for="item in items" :key="item.id">
      <UiIcon name="file" />
      <div class="upload-file-info">
        <span>{{ item.file.name }}</span
        ><small :class="{ 'danger-text': item.state === 'error' }">{{
          item.state === "error"
            ? item.error
            : item.state === "uploading"
              ? "上传中…"
              : item.state === "done"
                ? "上传完成"
                : formatSize(item.file.size)
        }}</small>
      </div>
      <span v-if="item.state === 'uploading'" class="spinner"></span
      ><UiIcon
        v-else-if="item.state === 'done'"
        name="check"
        class="success-text"
      /><button
        v-else
        class="icon-btn"
        :aria-label="'移除附件：' + item.file.name"
        :disabled="busy"
        @click="emit('remove', item.id)"
      >
        <UiIcon name="close" :size="14" />
      </button>
    </li>
  </ul>
</template>
