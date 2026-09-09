<script setup lang="ts">
import type { Task } from "@/shared/types";
import UiIcon from "@/shared/UiIcon.vue";
import { TASK_ROW_ACTIONS } from "../taskActions";

defineProps<{ task: Task }>();
</script>

<template>
  <div class="task-actions">
    <button
      v-for="action in TASK_ROW_ACTIONS"
      :key="action.key"
      type="button"
      class="btn btn-sm btn-ghost task-action-button"
      :title="action.title"
      :aria-label="action.title + '：' + task.id"
      @click.stop="action.run(task)"
    >
      <UiIcon :name="action.icon" :size="13" />{{ action.label }}
    </button>
  </div>
</template>
