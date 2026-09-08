<script setup lang="ts">
import UiDialog from "./UiDialog.vue";
import UiIcon from "./UiIcon.vue";
import {
  notices,
  dismissNotice,
  confirmation,
  settleConfirm,
} from "./feedback";
</script>
<template>
  <Teleport to="body"
    ><div class="toast-stack" aria-live="polite">
      <div
        v-for="n in notices"
        :key="n.id"
        class="toast"
        :class="'toast-' + n.kind"
      >
        <UiIcon :name="n.kind === 'success' ? 'check' : 'info'" /><span>{{
          n.text
        }}</span
        ><button
          class="icon-btn"
          aria-label="关闭提示"
          @click="dismissNotice(n.id)"
        >
          <UiIcon name="close" :size="14" />
        </button>
      </div></div
  ></Teleport>
  <UiDialog
    v-if="confirmation"
    :title="confirmation.title"
    :width="420"
    @close="settleConfirm(false)"
    ><p class="confirm-text">{{ confirmation.text }}</p>
    <template #footer
      ><button class="btn" data-autofocus @click="settleConfirm(false)">
        取消</button
      ><button
        class="btn"
        :class="confirmation.danger ? 'btn-danger' : 'btn-primary'"
        @click="settleConfirm(true)"
      >
        {{ confirmation.label }}
      </button></template
    ></UiDialog
  >
</template>
