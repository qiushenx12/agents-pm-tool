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
        v-else
        :src="api.attachmentUrl(attachment.id)"
        controls
        autoplay
      />
    </div>
  </UiDialog>
</template>
