<script setup lang="ts">
import { reactive, ref } from "vue";
import { api } from "@/grid-app/api/client";
import ProjectOptionPopover from "@/grid-app/components/ProjectOptionPopover.vue";
import StatusSelect from "@/grid-app/components/StatusSelect.vue";
import SubmitterTag from "@/grid-app/components/SubmitterTag.vue";
import { useMetaStore } from "@/grid-app/stores/metaStore";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import { formatDateTime, type Task, type TaskType } from "@/shared/types";

const emit = defineEmits<{ "open-detail": [task: Task] }>();

const taskStore = useTaskStore();
const metaStore = useMetaStore();

// ── 列定义与列宽拖拽 ──────────────────────────────────────
const columns = reactive([
  { key: "id", label: "ID", width: 190, sortable: false },
  { key: "project", label: "项目", width: 140, sortable: false },
  { key: "type", label: "任务类型", width: 100, sortable: false },
  { key: "description", label: "任务描述", width: 320, sortable: false },
  { key: "status", label: "当前状态", width: 110, sortable: false },
  { key: "submitter", label: "提交人", width: 80, sortable: false },
  { key: "attachments", label: "附件", width: 70, sortable: false },
  { key: "created_at", label: "创建时间", width: 150, sortable: true },
  { key: "finished_at", label: "完成时间", width: 150, sortable: true },
]);

const showProjectPopover = ref(false);

function onHeaderClick(col: (typeof columns)[number]) {
  if (col.key === "project") {
    showProjectPopover.value = true;
    return;
  }
  if (col.sortable) {
    taskStore.toggleSort(col.key as "created_at" | "finished_at");
  }
}

function sortIndicator(key: string) {
  if (taskStore.filters.sort_by !== key) return "";
  return taskStore.filters.sort_order === "asc" ? " ↑" : " ↓";
}

let resizeState: { index: number; startX: number; startWidth: number } | null = null;

function startResize(e: MouseEvent, index: number) {
  e.preventDefault();
  e.stopPropagation();
  resizeState = { index, startX: e.clientX, startWidth: columns[index].width };
  window.addEventListener("mousemove", onResizing);
  window.addEventListener("mouseup", stopResize, { once: true });
}

function onResizing(e: MouseEvent) {
  if (!resizeState) return;
  const delta = e.clientX - resizeState.startX;
  columns[resizeState.index].width = Math.max(60, resizeState.startWidth + delta);
}

function stopResize() {
  resizeState = null;
  window.removeEventListener("mousemove", onResizing);
}

// ── 行内编辑 ─────────────────────────────────────────────
const editingCell = ref<string | null>(null); // `${taskId}:description`
const editingText = ref("");

function startEditDescription(task: Task) {
  editingCell.value = `${task.id}:description`;
  editingText.value = task.description;
}

async function commitDescription(task: Task) {
  editingCell.value = null;
  const description = editingText.value.trim();
  if (description === task.description) return;
  try {
    await api.patchTask(task.id, { description });
    await taskStore.refresh();
  } catch (e) {
    alert(e instanceof Error ? e.message : String(e));
  }
}

async function onProjectChange(task: Task, e: Event) {
  const project = (e.target as HTMLSelectElement).value;
  if (project === task.project) return;
  try {
    await api.patchTask(task.id, { project });
    await taskStore.refresh();
  } catch (err) {
    alert(err instanceof Error ? err.message : String(err));
    await taskStore.refresh();
  }
}

async function onTypeChange(task: Task, e: Event) {
  const type = (e.target as HTMLSelectElement).value as TaskType;
  if (type === task.type) return;
  try {
    await api.patchTask(task.id, { type });
    await taskStore.refresh();
  } catch (err) {
    alert(err instanceof Error ? err.message : String(err));
    await taskStore.refresh();
  }
}

async function onDelete(task: Task) {
  if (!confirm(`确定删除任务「${task.id}」？此操作不可恢复。`)) return;
  try {
    await api.deleteTask(task.id);
    await taskStore.refresh();
  } catch (e) {
    alert(e instanceof Error ? e.message : String(e));
  }
}
</script>

<template>
  <div class="grid-wrap">
    <table class="task-grid">
      <colgroup>
        <col v-for="c in columns" :key="c.key" :style="{ width: c.width + 'px' }" />
      </colgroup>
      <thead>
        <tr>
          <th
            v-for="(c, i) in columns"
            :key="c.key"
            :class="{ sortable: c.sortable || c.key === 'project' }"
            @click="onHeaderClick(c)"
          >
            {{ c.label }}{{ sortIndicator(c.key) }}<span v-if="c.key === 'project'"> ▾</span>
            <span class="col-resize" @mousedown="startResize($event, i)"></span>
          </th>
          <th class="col-actions"></th>
        </tr>
      </thead>
      <tbody>
        <tr v-if="taskStore.loading && !taskStore.tasks.length">
          <td :colspan="columns.length + 1" class="grid-placeholder">加载中…</td>
        </tr>
        <tr v-else-if="!taskStore.tasks.length">
          <td :colspan="columns.length + 1" class="grid-placeholder">
            暂无任务，点击右上角「新建任务」开始
          </td>
        </tr>
        <tr
          v-for="t in taskStore.tasks"
          :key="t.id"
          class="task-row"
          @click="emit('open-detail', t)"
        >
          <td class="cell-id">{{ t.id }}</td>

          <td @click.stop>
            <span class="option-dot" :style="{ background: metaStore.projectColor(t.project) }"></span>
            <select class="cell-select" :value="t.project" @change="onProjectChange(t, $event)">
              <option v-for="p in metaStore.projects" :key="p.name" :value="p.name">
                {{ p.name }}
              </option>
            </select>
          </td>

          <td @click.stop>
            <select class="cell-select" :value="t.type" @change="onTypeChange(t, $event)">
              <option v-for="tp in metaStore.taskTypes" :key="tp" :value="tp">{{ tp }}</option>
            </select>
          </td>

          <td class="cell-desc" @click.stop="startEditDescription(t)">
            <textarea
              v-if="editingCell === `${t.id}:description`"
              v-model="editingText"
              class="desc-editor"
              rows="3"
              @blur="commitDescription(t)"
              @keydown.esc="editingCell = null"
              @keydown.enter.ctrl="commitDescription(t)"
              @click.stop
            ></textarea>
            <span v-else class="desc-text">{{ t.description || "（空）" }}</span>
          </td>

          <td @click.stop><StatusSelect :task="t" /></td>

          <td><SubmitterTag :submitter="t.submitter" /></td>

          <td class="cell-attach">
            <span v-if="t.attachment_count" class="attach-count">📎 {{ t.attachment_count }}</span>
            <span v-else class="attach-none">—</span>
          </td>

          <td class="cell-time">{{ formatDateTime(t.created_at) }}</td>
          <td class="cell-time">{{ formatDateTime(t.finished_at) || "—" }}</td>

          <td class="col-actions" @click.stop>
            <button class="icon-btn danger" title="删除任务" @click="onDelete(t)">✕</button>
          </td>
        </tr>
      </tbody>
    </table>

    <ProjectOptionPopover v-if="showProjectPopover" @close="showProjectPopover = false" />
  </div>
</template>
