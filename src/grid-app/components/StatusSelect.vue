<script setup lang="ts">
import { computed } from "vue";
import { api } from "@/grid-app/api/client";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import { TASK_STATUSES, type Task, type TaskStatus } from "@/shared/types";

const props = defineProps<{ task: Task }>();
const taskStore = useTaskStore();

const statusClass = computed(() => {
  const map: Record<TaskStatus, string> = {
    未开始: "status-todo",
    进行中: "status-doing",
    待验证: "status-review",
    已完成: "status-done",
    验收未通过: "status-rejected",
    验收通过: "status-accepted",
  };
  return map[props.task.status];
});

async function onChange(e: Event) {
  const status = (e.target as HTMLSelectElement).value as TaskStatus;
  if (status === props.task.status) return;
  try {
    await api.patchTask(props.task.id, { status });
    await taskStore.refresh();
  } catch (err) {
    alert(err instanceof Error ? err.message : String(err));
    await taskStore.refresh();
  }
}
</script>

<template>
  <select
    class="status-select"
    :class="statusClass"
    :value="task.status"
    @click.stop
    @change="onChange"
  >
    <option v-for="s in TASK_STATUSES" :key="s" :value="s">{{ s }}</option>
  </select>
</template>
