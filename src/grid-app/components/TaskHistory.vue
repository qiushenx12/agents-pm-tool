<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { api } from "../api/client";
import { useTaskStore } from "../stores/taskStore";
import { errorText } from "@/shared/feedback";
import { formatDateTime, type TaskHistoryEntry } from "@/shared/types";

const props = defineProps<{ taskId: string; revision: string }>();
const tasks = useTaskStore();
const items = ref<TaskHistoryEntry[]>([]);
const loading = ref(false), error = ref("");
const nextBefore = ref<number | null>(null);
let requestId = 0;
let retryMore = false;
const fields: Record<string, string> = {
  task: "任务", project: "项目", type: "任务类型", description: "描述", note: "备注",
  status: "状态", priority: "优先级", assignee_user_id: "负责人", owner_user_id: "提交账号",
  finished_at: "完成时间", position: "手动排序", predecessor_task_ids: "子任务",
  unlock_task_ids: "父级任务", attachments: "附件",
};
function valueText(value: unknown, field: string): string {
  if (value === null || value === undefined || value === "") return "—";
  if (Array.isArray(value)) {
    return value.map((item: unknown) => {
      if (field === "attachments" && typeof item === "object" && item !== null && "filename" in item)
        return String(item.filename);
      return String(item);
    }).join("\n") || "—";
  }
  if (typeof value === "object") {
    if (field === "task" && "description" in value) return String(value.description || "未填写任务描述");
    return JSON.stringify(value);
  }
  if (field === "assignee_user_id") return `Agent（${String(value)}）`;
  return String(value);
}
function actionText(entry: TaskHistoryEntry, field: string) {
  if (entry.action === "undo") return "撤销 · " + (fields[field] ?? field);
  if (field === "task") return entry.action === "delete" ? "删除任务" : "创建任务";
  return fields[field] ?? field;
}
async function load(more = false) {
  const id = ++requestId;
  retryMore = more;
  loading.value = true;
  error.value = "";
  if (!more) { items.value = []; nextBefore.value = null; }
  try {
    const page = await api.taskHistory(props.taskId, more ? nextBefore.value ?? undefined : undefined);
    if (id !== requestId) return;
    items.value = more ? [...items.value, ...page.items] : page.items;
    nextBefore.value = page.next_before;
  } catch (e) {
    if (id === requestId) error.value = errorText(e);
  } finally {
    if (id === requestId) loading.value = false;
  }
}
watch(() => [props.taskId, props.revision, tasks.externalRevision], () => void load(), { immediate: true });
onBeforeUnmount(() => { requestId++; });
</script>

<template>
  <section class="task-history" aria-label="任务操作历史" :aria-busy="loading">
    <div class="history-heading">
      <span>操作历史</span>
      <button class="btn btn-ghost btn-sm" :disabled="loading" @click="load()">刷新</button>
    </div>
    <div v-if="error" class="form-error" role="alert">
      {{ error }}<button class="btn btn-sm" :disabled="loading" @click="load(retryMore)">重试</button>
    </div>
    <div v-if="items.length" class="history-scroll">
      <table class="history-table">
        <thead><tr><th scope="col">时间</th><th scope="col">操作人</th><th scope="col">字段</th><th scope="col">变更前</th><th scope="col" class="history-arrow-col" aria-hidden="true"></th><th scope="col">变更后</th></tr></thead>
        <tbody v-for="entry in items" :key="entry.operation_id">
          <tr v-for="(change, index) in entry.changes" :key="change.field">
            <td v-if="index === 0" :rowspan="entry.changes.length" class="history-time">{{ formatDateTime(entry.created_at) }}</td>
            <td v-if="index === 0" :rowspan="entry.changes.length" class="history-actor">
              {{ entry.source === 'agent' ? `Agent（${entry.actor_name}）` : entry.actor_name }}
            </td>
            <td>{{ actionText(entry, change.field) }}</td>
            <td class="history-value history-before">{{ valueText(change.before, change.field) }}</td>
            <td class="history-arrow" aria-hidden="true">→</td>
            <td class="history-value">{{ valueText(change.after, change.field) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-if="loading" class="history-empty" role="status">正在加载历史…</p>
    <p v-else-if="!items.length && !error" class="history-empty">暂无可见的操作历史。启用历史功能后的操作会记录在这里。</p>
    <button v-if="nextBefore && !error" class="btn btn-sm history-more" :disabled="loading" @click="load(true)">加载更早记录</button>
  </section>
</template>

<style scoped>
.history-heading { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; color: var(--text-secondary); }
.history-scroll { overflow-x: auto; }
.history-table { width: 100%; min-width: 530px; table-layout: fixed; border-collapse: collapse; text-align: left; }
.history-table th { color: var(--text-secondary); font-weight: 400; padding: 0 10px 14px; font-size: 12px; }
.history-table th:first-child { width: 90px; }
.history-table th:nth-child(2) { width: 90px; }
.history-table th:nth-child(3) { width: 80px; }
.history-table th.history-arrow-col { width: 30px; padding-left: 0; padding-right: 0; }
.history-table td.history-arrow { padding-left: 0; padding-right: 0; color: var(--text-tertiary); text-align: center; }
.history-table td { vertical-align: top; padding: 16px 10px; line-height: 1.65; overflow-wrap: anywhere; }
.history-table tbody { border-top: 1px solid var(--separator); }
.history-value { white-space: pre-wrap; }
.history-before, .history-time { color: var(--text-secondary); }
.history-time { font-size: 12px; }
.history-empty { padding: 36px 16px; color: var(--text-tertiary); text-align: center; line-height: 1.8; }
.history-more { display: block; margin: 20px auto; }
</style>
