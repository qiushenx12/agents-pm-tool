<script setup lang="ts">
import type { Attachment } from "@/shared/types";
import UiDialog from "@/shared/UiDialog.vue";
import { api } from "../api/client";
import { attachmentKind } from "../attachmentKind";

defineProps<{ attachment: Attachment }>();
defineEmits<{ close: [] }>();
</script>

<template>
  <UiDialog
    :title="attachment.filename"
    :width="960"
    @close="$emit('close')"
  >
    <div class="media-preview">
      <img
        v-if="attachmentKind(attachment) === 'image'"
        :src="api.attachmentUrl(attachment.id)"
        :alt="attachment.filename"
      /><video
        v-else-if="attachmentKind(attachment) === 'video'"
        :src="api.attachmentUrl(attachment.id)"
        controls
        autoplay
      /><div
        v-else
        class="file-preview-fallback"
      >
        <p>该文件类型不支持在线预览。</p>
        <a
          :href="api.attachmentUrl(attachment.id)"
          :download="attachment.filename"
          class="btn btn-primary"
        >
          下载文件
        </a>
      </div>
    </div>
  </UiDialog>
</template>

<style scoped>
.file-preview-fallback {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  padding: 48px 24px;
  color: var(--text-secondary);
}
.file-preview-fallback p {
  margin: 0;
  font-size: 14px;
}
</style>
