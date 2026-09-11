<script setup lang="ts">
import { computed, ref } from "vue";
import { useTaskStore } from "../stores/taskStore";
import { useMetaStore } from "../stores/metaStore";
import UiIcon from "@/shared/UiIcon.vue";
import UiPopover from "@/shared/UiPopover.vue";
import UiDialog from "@/shared/UiDialog.vue";
import { statusOptions, typeOptions, priorityOptions } from "@/shared/taskOptions";
import { askConfirm, errorText, notify } from "@/shared/feedback";
import type { Task, TaskBatchRequest, TaskBatchResult } from "@/shared/types";
const tasks = useTaskStore(),
  meta = useMetaStore();
const result = ref<TaskBatchResult | null>(null),
  recipe = ref<TaskBatchRequest | null>(null),
  operation = ref(""),
  error = ref("");
const names = ref<Record<string, string>>({});
const failures = computed(
  () => result.value?.results.filter((item) => item.error) ?? [],
);
const fields = computed(() => [
  { key: "status", label: "修改状态", options: statusOptions },
  {
    key: "project",
    label: "移动项目",
    options: meta.projects.map((p) => ({ value: p.name, color: p.color })),
  },
  { key: "type", label: "修改类型", options: typeOptions },
  { key: "priority", label: "修改优先级", options: priorityOptions },
]);
async function run(request: TaskBatchRequest, label: string) {
  if (tasks.saving) return;
  error.value = "";
  operation.value = label;
  recipe.value = request;
  for (const id of request.ids)
    if (tasks.selection[id])
      names.value[id] = tasks.selection[id].description || id;
  try {
    const response = await tasks.applyBatch(request);
    if (response.failed) result.value = response;
    else {
      result.value = null;
      notify(label + "完成，共 " + response.succeeded + " 条任务");
    }
  } catch (e) {
    error.value = errorText(e) + "。请求结果尚未确认，请检查记录后重试。";
  }
}
function update(field: string, value: string) {
  const patch = { [field]: value } as Partial<
    Pick<Task, "project" | "status" | "type" | "priority">
  >;
  void run(
    { action: "update", ids: [...tasks.selectedIds], patch },
    field === "project" ? "移动项目" : "批量修改",
  );
}
async function remove() {
  const ids = [...tasks.selectedIds];
  if (
    !(await askConfirm(
      "批量删除 " + ids.length + " 条任务",
      "将永久删除已选择的任务及其附件，包括其他页面中选中的记录。",
      "删除 " + ids.length + " 条任务",
      true,
    ))
  )
    return;
  void run({ action: "delete", ids }, "批量删除");
}
async function retry() {
  if (!recipe.value || !failures.value.length) return;
  const request = {
    ...recipe.value,
    ids: failures.value.map((item) => item.id),
  };
  if (
    request.action === "delete" &&
    !(await askConfirm(
      "重试删除失败项",
      "仅重试 " +
        request.ids.length +
        " 条失败任务，已成功的任务不会重复处理。",
      "重试删除",
      true,
    ))
  )
    return;
  void run(request, operation.value);
}
</script>
<template>
  <div
    v-if="tasks.selectedIds.length"
    class="bulk-toolbar"
    role="region"
    aria-label="批量操作"
  >
    <div class="bulk-count">
      <UiIcon name="check" :size="15" />已选择
      <strong>{{ tasks.selectedIds.length }}</strong> 条<span class="subtle"
        >可跨页选择</span
      >
    </div>
    <UiPopover
      v-for="field in fields"
      :key="field.key"
      :width="230"
      :label="field.label"
      ><template #trigger="{ toggle, open }"
        ><button
          class="btn btn-sm"
          :disabled="tasks.saving"
          :aria-expanded="open"
          @click="toggle"
        >
          {{ field.label
          }}<UiIcon name="chevron" :size="12" /></button></template
      ><template #default="{ close }"
        ><div class="menu-caption">
          {{ field.label }} · {{ tasks.selectedIds.length }} 条任务
        </div>
        <button
          v-for="option in field.options"
          :key="option.value"
          class="menu-item"
          @click="
            close();
            update(field.key, option.value);
          "
        >
          {{ option.value }}
        </button></template
      ></UiPopover
    >
    <button
      class="btn btn-sm btn-danger"
      :disabled="tasks.saving"
      @click="remove"
    >
      <UiIcon name="trash" :size="13" />删除
    </button>
    <span v-if="tasks.batchBusy" class="bulk-progress" role="status"
      ><span class="spinner"></span>正在处理…</span
    >
    <button
      class="btn btn-sm btn-ghost bulk-clear"
      :disabled="tasks.batchBusy"
      @click="tasks.clearSelection()"
    >
      取消选择
    </button>
  </div>
  <div v-if="error" class="workspace-error error-banner" role="alert">
    <UiIcon name="info" /><span>{{ error }}</span
    ><button class="icon-btn" aria-label="关闭批量操作错误" @click="error = ''">
      <UiIcon name="close" :size="13" />
    </button>
  </div>
  <UiDialog
    v-if="result"
    :title="operation + '结果'"
    :width="540"
    :busy="tasks.batchBusy"
    @close="result = null"
  >
    <div class="batch-summary">
      <span class="tag tag-green">成功 {{ result.succeeded }} 条</span
      ><span class="tag tag-red">失败 {{ result.failed }} 条</span>
    </div>
    <p class="form-hint">成功项已完成，失败项保留选择。重试只处理以下任务。</p>
    <ul class="batch-failures">
      <li v-for="item in failures" :key="item.id">
        <strong>{{ names[item.id] || item.id }}</strong
        ><code>{{ item.id }}</code
        ><span>{{ item.error?.message }}</span>
      </li>
    </ul>
    <div v-if="error" class="form-error" role="alert">{{ error }}</div>
    <template #footer
      ><button class="btn" :disabled="tasks.batchBusy" @click="result = null">
        稍后处理</button
      ><button class="btn btn-primary" :disabled="tasks.saving" @click="retry">
        {{ tasks.batchBusy ? "正在重试…" : "重试失败项" }}
      </button></template
    >
  </UiDialog>
</template>
