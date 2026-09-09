<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { api } from "../api/client";
import { useMetaStore } from "../stores/metaStore";
import { useTaskStore } from "../stores/taskStore";
import UiDialog from "@/shared/UiDialog.vue";
import UiSelect from "@/shared/UiSelect.vue";
import UiIcon from "@/shared/UiIcon.vue";
import AttachmentUploader from "./AttachmentUploader.vue";
import { useUploadQueue } from "./useUploadQueue";
import { typeOptions } from "@/shared/taskOptions";
import { askConfirm, errorText, notify } from "@/shared/feedback";
import type { Task, TaskType } from "@/shared/types";
const emit = defineEmits<{
  close: [];
  created: [task: Task];
  "manage-projects": [];
}>();
const meta = useMetaStore(),
  tasks = useTaskStore(),
  queue = useUploadQueue();
const initialProject =
  tasks.filters.project.length === 1
    ? tasks.filters.project[0]
    : (meta.projects[0]?.name ?? "");
const project = ref(initialProject),
  type = ref<TaskType>("新增需求"),
  description = ref(""),
  note = ref("");
const createdTask = ref<Task | null>(null),
  error = ref(""),
  submitting = ref(false);
const projectOptions = computed(() =>
  meta.projects.map((p) => ({ value: p.name, color: p.color })),
);
watch(
  () => meta.projects,
  (list) => {
    if (!list.some((p) => p.name === project.value))
      project.value = list[0]?.name ?? "";
  },
);
const busy = computed(() => submitting.value || queue.busy.value);
function finish() {
  if (!createdTask.value) return;
  notify(
    queue.failed.value.length
      ? "任务已创建，未完成的附件可在详情中补充"
      : "任务已创建",
    queue.failed.value.length ? "info" : "success",
  );
  emit("created", createdTask.value);
}
async function close() {
  if (busy.value) return;
  if (createdTask.value) {
    if (
      queue.failed.value.length &&
      !(await askConfirm(
        "任务已创建",
        "部分附件尚未上传成功。离开后可在任务详情中重新选择并上传。",
        "稍后处理",
      ))
    )
      return;
    finish();
    return;
  }
  if (
    (description.value ||
      note.value ||
      queue.items.value.length ||
      type.value !== "新增需求" ||
      project.value !== initialProject) &&
    !(await askConfirm("放弃新建任务", "尚未提交的内容将被丢弃。", "放弃"))
  )
    return;
  emit("close");
}
async function submit() {
  if (busy.value) return;
  error.value = "";
  if (!project.value && !createdTask.value) {
    error.value = "请先创建或选择一个项目";
    return;
  }
  submitting.value = true;
  try {
    // Retain the created task across retries: attachment failure must never create a second record.
    if (!createdTask.value) {
      createdTask.value = await api.createTask({
        project: project.value,
        type: type.value,
        description: description.value,
        note: note.value,
      });
      tasks.acceptTask(createdTask.value);
    }
    const complete = await queue.upload(createdTask.value.id);
    await tasks.refresh();
    if (complete) finish();
    else
      error.value =
        "任务已创建，部分附件上传失败。可以重试，或稍后在详情中处理。";
  } catch (e) {
    error.value = errorText(e);
  } finally {
    submitting.value = false;
  }
}
</script>
<template>
  <UiDialog title="新建任务" :width="580" :busy="busy" @close="close">
    <div class="form-grid">
      <div class="form-field">
        <label>项目 <span class="required">*</span></label
        ><UiSelect
          v-model="project"
          :options="projectOptions"
          label="项目"
          :disabled="busy || !!createdTask"
        /><button
          v-if="!meta.projects.length"
          class="text-button"
          @click="emit('manage-projects')"
        >
          <UiIcon name="plus" :size="13" />创建第一个项目
        </button>
      </div>
      <div class="form-field">
        <label>任务类型 <span class="required">*</span></label
        ><UiSelect
          :model-value="type"
          :options="typeOptions"
          label="任务类型"
          :disabled="busy || !!createdTask"
          @update:model-value="type = $event as TaskType"
        />
      </div>
    </div>
    <div class="form-field">
      <label for="create-description">任务描述</label
      ><textarea
        id="create-description"
        v-model="description"
        class="input create-description"
        rows="5"
        placeholder="这项任务需要完成什么？"
        data-autofocus
        :disabled="busy || !!createdTask"
        @keydown.enter.ctrl.prevent="submit"
      />
      <p class="form-hint">写下目标、背景或验收要求，方便后续跟进。</p>
    </div>
    <div class="form-field">
      <label for="create-note">备注</label
      ><textarea
        id="create-note"
        v-model="note"
        class="input"
        rows="3"
        placeholder="补充记录（可选）"
        :disabled="busy || !!createdTask"
        @keydown.enter.ctrl.prevent="submit"
      />
    </div>
    <div class="form-field">
      <label>附件 <span class="subtle optional-label">可选</span></label
      ><AttachmentUploader
        :items="queue.items.value"
        :busy="busy"
        @add="queue.add"
        @remove="queue.remove"
      />
    </div>
    <div v-if="error" class="form-error" role="alert">
      <UiIcon name="info" />{{ error }}
    </div>
    <template #footer
      ><span class="form-footer-hint">Ctrl + Enter 创建</span
      ><button class="btn" :disabled="busy" @click="close">
        {{ createdTask ? "稍后处理" : "取消" }}</button
      ><button
        class="btn btn-primary"
        :disabled="busy || (!project && !createdTask)"
        @click="submit"
      >
        <span v-if="busy" class="spinner"></span
        >{{ busy ? "正在提交…" : createdTask ? "重试失败附件" : "创建任务" }}
      </button></template
    >
  </UiDialog>
</template>
