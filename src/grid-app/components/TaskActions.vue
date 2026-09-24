<script setup lang="ts">
import type { Task } from "@/shared/types";
import UiIcon from "@/shared/UiIcon.vue";
import { useViewStore } from "../stores/viewStore";

defineProps<{ task: Task }>();
const view = useViewStore();
</script>

<template>
  <div class="task-actions">
    <button
      v-for="action in view.visibleRowActions"
      :key="action.key"
      type="button"
      class="btn btn-sm task-action-button"
      :title="action.title"
      :aria-label="action.title + '：' + task.id"
      @click.stop="action.run(task)"
    >
      <UiIcon :name="action.icon" :size="13" />{{ action.label }}
    </button>
  </div>
</template>
