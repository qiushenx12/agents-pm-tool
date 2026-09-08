<script setup lang="ts">
import { computed, ref } from "vue";
import UiSelect from "@/shared/UiSelect.vue";
import { useMetaStore } from "../stores/metaStore";
import { useTaskStore } from "../stores/taskStore";
import { statusOptions, typeOptions } from "@/shared/taskOptions";
import { errorText } from "@/shared/feedback";
import type { Task, TaskStatus, TaskType } from "@/shared/types";
const props = withDefaults(
  defineProps<{
    task: Task;
    field: "project" | "type" | "status";
    editable?: boolean;
    form?: boolean;
  }>(),
  { editable: true },
);
const tasks = useTaskStore(),
  meta = useMetaStore();
const error = ref("");
const options = computed(() =>
  props.field === "project"
    ? meta.projects.map((p) => ({ value: p.name, color: p.color, tone: "" }))
    : props.field === "type"
      ? typeOptions
      : statusOptions,
);
const option = computed(() =>
  options.value.find((o) => o.value === props.task[props.field]),
);
const label = computed(
  () =>
    ({ project: "项目", type: "任务类型", status: "当前状态" })[props.field],
);
async function update(value: string) {
  error.value = "";
  try {
    await tasks.updateTask(
      props.task.id,
      props.field === "project"
        ? { project: value }
        : props.field === "type"
          ? { type: value as TaskType }
          : { status: value as TaskStatus },
    );
  } catch (e) {
    error.value = errorText(e);
  }
}
</script>
<template>
  <div class="task-field" :class="{ 'form-task-field': form }">
    <UiSelect
      v-if="editable"
      :model-value="task[field]"
      :options="options"
      :label="label"
      :field="!form"
      :disabled="!!tasks.pending[task.id]"
      @update:model-value="update"
    />
    <span
      v-else
      class="field-value"
      :class="option?.tone ? 'tag tag-' + option.tone : ''"
      ><span
        v-if="field === 'project'"
        class="option-dot"
        :style="{ background: meta.projectColor(task.project) }"
      ></span
      >{{ task[field] }}</span
    >
    <span
      v-if="tasks.pending[task.id] && editable"
      class="field-pending"
      title="保存中"
      ><span class="spinner"></span
    ></span>
    <div v-if="error" class="field-error" role="alert" :title="error">
      {{ error }} · 点击字段重试
    </div>
  </div>
</template>
