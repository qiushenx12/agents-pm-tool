<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { storeToRefs } from "pinia";
import { GROUP_FIELDS, type GroupField } from "@/shared/types";
import UiIcon from "@/shared/UiIcon.vue";
import UiPopover from "@/shared/UiPopover.vue";
import TaskField from "./TaskField.vue";
import TaskActions from "./TaskActions.vue";
import DescriptionEditor from "./DescriptionEditor.vue";
import AttachmentPreviewDialog from "./AttachmentPreviewDialog.vue";
import { api } from "../api/client";
import { attachmentKind } from "../attachmentKind";
import { buildAgentIdPrompt } from "../taskActions";
import { useTaskStore } from "../stores/taskStore";
import { useViewStore } from "../stores/viewStore";
import { askConfirm, copyText, errorText, notify } from "@/shared/feedback";
import { formatDateTime, type Attachment, type Task } from "@/shared/types";
defineProps<{ quickCreating?: boolean }>();
const emit = defineEmits<{
  "open-detail": [task: Task];
  create: [];
  "quick-create": [];
}>();
const tasks = useTaskStore(),
  view = useViewStore();
const root = ref<HTMLElement>();
const active = ref<{ id: string; key: string } | null>(null);
const editingId = ref<string | null>(null);
const editingKey = ref<"description" | "note">("description");
const editingTask = ref<Task | null>(null);
const preview = ref<Attachment | null>(null);
const editor = ref<InstanceType<typeof DescriptionEditor>>();
const editorPosition = ref({ left: "0px", top: "0px" });
const { page, pages } = storeToRefs(tasks);
const collapsedGroups = ref(new Set<string>());
const groupLabels: Record<GroupField, string> = {
  project: "项目",
  status: "当前状态",
  type: "任务类型",
  submitter: "提交人",
  priority: "优先级",
};
const pageGroups = computed(() => {
  const field = tasks.filters.group_by;
  if (!field) return [{ value: "", count: tasks.total, items: tasks.tasks }];
  const buckets = new Map<string, Task[]>();
  tasks.tasks.forEach((task) => {
    const value = task[field];
    if (!buckets.has(value)) buckets.set(value, []);
    buckets.get(value)!.push(task);
  });
  return Array.from(buckets, ([value, items]) => ({
    value,
    items,
    count:
      tasks.groups.find((group) => group.value === value)?.count ??
      items.length,
  }));
});
const rows = computed(() =>
  pageGroups.value.flatMap((group) =>
    collapsedGroups.value.has(group.value) ? [] : group.items,
  ),
);
const rowIndexes = computed(
  () => new Map(rows.value.map((task, index) => [task.id, index])),
);
const ordinal = (id: string) =>
  (page.value - 1) * tasks.pageSize +
  tasks.tasks.findIndex((task) => task.id === id) +
  1;
const rowIndex = (id: string) => rowIndexes.value.get(id) ?? 0;
const allSelected = computed(
  () =>
    rows.value.length > 0 &&
    rows.value.every((task) => !!tasks.selection[task.id]),
);
const someSelected = computed(() =>
  rows.value.some((task) => !!tasks.selection[task.id]),
);
const allCollapsed = computed(
  () =>
    pageGroups.value.length > 0 &&
    pageGroups.value.every((group) => collapsedGroups.value.has(group.value)),
);
function toggleGroup(value: string) {
  if (collapsedGroups.value.has(value)) collapsedGroups.value.delete(value);
  else collapsedGroups.value.add(value);
}
function toggleGroups() {
  collapsedGroups.value = allCollapsed.value
    ? new Set()
    : new Set(tasks.groups.map((group) => group.value));
}
function selectPage() {
  if (allSelected.value)
    rows.value.forEach((task) => tasks.toggleSelection(task, false));
  else {
    const available = 500 - tasks.selectedIds.length;
    const unselected = rows.value.filter((task) => !tasks.selection[task.id]);
    unselected
      .slice(0, available)
      .forEach((task) => tasks.toggleSelection(task, true));
    if (unselected.length > available)
      notify("每次最多选择 500 条任务", "info");
  }
}
watch(
  () => tasks.filters.group_by,
  () => {
    collapsedGroups.value = new Set();
  },
);
const draggingColumn = ref("");
function dragColumn(event: DragEvent, key: string) {
  if (
    key === "description" ||
    (event.target as HTMLElement).closest("button,.col-resize,.popover-anchor")
  ) {
    event.preventDefault();
    return;
  }
  draggingColumn.value = key;
  event.dataTransfer?.setData("application/x-pm-column", key);
}
function dropColumn(event: DragEvent, key: string) {
  view.moveBefore(
    event.dataTransfer?.getData("application/x-pm-column") ||
      draggingColumn.value,
    key,
  );
  draggingColumn.value = "";
}
// ── 行拖拽（手动排序） ──────────────────────────────────
const draggingTask = ref("");
const dropSlot = ref<{ id: string; edge: "above" | "below" } | null>(null);
const canDragRows = computed(
  () =>
    tasks.filters.sort_by === "manual" &&
    tasks.isCurrentPage &&
    !tasks.batchBusy,
);
/** 计算落在 task 上/下半行时的前后邻居；不允许时返回 null */
function dropTarget(event: DragEvent, task: Task) {
  const dragged = draggingTask.value;
  if (!dragged || dragged === task.id) return null;
  // 分组视图只允许同组内拖动（跨组不涉及字段变更，避免误导）
  const field = tasks.filters.group_by;
  const draggedTask = tasks.records[dragged];
  if (field && draggedTask && draggedTask[field] !== task[field]) return null;
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const above = event.clientY < rect.top + rect.height / 2;
  const list = rows.value;
  const at = list.findIndex((t) => t.id === task.id);
  if (at < 0) return null;
  const prevId = above ? (list[at - 1]?.id ?? null) : task.id;
  const nextId = above ? task.id : (list[at + 1]?.id ?? null);
  if (prevId === dragged || nextId === dragged || (!prevId && !nextId))
    return null; // 原位
  return { prevId, nextId, edge: above ? ("above" as const) : ("below" as const) };
}
function dragRow(event: DragEvent, task: Task) {
  if (!canDragRows.value) {
    event.preventDefault();
    return;
  }
  draggingTask.value = task.id;
  if (event.dataTransfer) {
    event.dataTransfer.setData("application/x-pm-task", task.id);
    event.dataTransfer.effectAllowed = "move";
  }
}
function dragOverRow(event: DragEvent, task: Task) {
  const target = dropTarget(event, task);
  if (!target) return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  dropSlot.value = { id: task.id, edge: target.edge };
}
function dropRow(event: DragEvent, task: Task) {
  const dragged = draggingTask.value;
  const target = dropTarget(event, task);
  draggingTask.value = "";
  dropSlot.value = null;
  if (!dragged || !target) return;
  event.preventDefault();
  void tasks.moveTask(dragged, target.prevId, target.nextId);
}
function dragRowEnd() {
  draggingTask.value = "";
  dropSlot.value = null;
}
// ── 附件单元格拖放 ────────────────────────────────────
const attachmentDragTarget = ref("");
const attachmentUploads = ref<
  Record<string, { completed: number; total: number }>
>({});
const attachmentPreviews = ref<Record<string, Attachment[]>>({});
const attachmentPreviewCounts = new Map<string, number>();
const attachmentPreviewLoads = new Map<string, Promise<void>>();
function visibleAttachmentPreviews(attachments: Attachment[]) {
  return attachments.slice(0, attachments.length > 3 ? 2 : 3);
}
function hiddenAttachmentCount(attachments: Attachment[]) {
  return attachments.length - visibleAttachmentPreviews(attachments).length;
}
function loadAttachmentPreviews(task: Task) {
  const count = task.attachment_count ?? 0;
  if (!count) {
    delete attachmentPreviews.value[task.id];
    attachmentPreviewCounts.set(task.id, 0);
    return;
  }
  if (
    attachmentPreviewCounts.get(task.id) === count ||
    attachmentPreviewLoads.has(task.id)
  )
    return;
  const request = (async () => {
    try {
      const attachments = await api.listAttachments(task.id);
      if (!tasks.tasks.some((current) => current.id === task.id)) return;
      attachmentPreviews.value[task.id] = attachments;
      attachmentPreviewCounts.set(task.id, count);
    } catch {
      // 列表仍保留附件数量作为降级展示，后续计数变化时自动重试。
      if (tasks.tasks.some((current) => current.id === task.id)) {
        delete attachmentPreviews.value[task.id];
        attachmentPreviewCounts.set(task.id, count);
      }
    } finally {
      attachmentPreviewLoads.delete(task.id);
      const latest = tasks.tasks.find((current) => current.id === task.id);
      if (
        latest &&
        (latest.attachment_count ?? 0) > 0 &&
        attachmentPreviewCounts.get(task.id) !== latest.attachment_count
      )
        loadAttachmentPreviews(latest);
    }
  })();
  attachmentPreviewLoads.set(task.id, request);
}
watch(
  () =>
    tasks.tasks
      .map((task) => `${task.id}:${task.attachment_count ?? 0}`)
      .join("|"),
  () => {
    const visibleIds = new Set(tasks.tasks.map((task) => task.id));
    Object.keys(attachmentPreviews.value).forEach((id) => {
      if (!visibleIds.has(id)) {
        delete attachmentPreviews.value[id];
        attachmentPreviewCounts.delete(id);
      }
    });
    tasks.tasks.forEach(loadAttachmentPreviews);
  },
  { immediate: true },
);
function isFileDrag(event: DragEvent) {
  const transfer = event.dataTransfer;
  return (
    !!transfer &&
    (transfer.files.length > 0 || Array.from(transfer.types).includes("Files"))
  );
}
function dragOverAttachments(event: DragEvent, task: Task) {
  if (!isFileDrag(event)) return;
  event.preventDefault();
  event.stopPropagation();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
  attachmentDragTarget.value = task.id;
}
function leaveAttachmentCell(event: DragEvent, task: Task) {
  if (attachmentDragTarget.value !== task.id) return;
  const cell = event.currentTarget as HTMLElement;
  if (
    !(event.relatedTarget instanceof Node) ||
    !cell.contains(event.relatedTarget)
  )
    attachmentDragTarget.value = "";
}
function dropAttachments(event: DragEvent, task: Task) {
  if (!isFileDrag(event)) return;
  event.preventDefault();
  event.stopPropagation();
  attachmentDragTarget.value = "";
  const files = Array.from(event.dataTransfer?.files ?? []);
  if (files.length) void uploadAttachments(task, files);
}
function clipboardFiles(event: ClipboardEvent) {
  const transfer = event.clipboardData;
  if (!transfer) return [];
  const files = Array.from(transfer.files);
  if (files.length) return files;
  return Array.from(transfer.items)
    .filter((item) => item.kind === "file")
    .map((item) => item.getAsFile())
    .filter((file): file is File => file !== null);
}
function pasteAttachments(event: ClipboardEvent, task: Task) {
  const files = clipboardFiles(event);
  if (!files.length) return;
  event.preventDefault();
  event.stopPropagation();
  void uploadAttachments(task, files);
}
async function uploadAttachments(task: Task, files: File[]) {
  if (attachmentUploads.value[task.id]) {
    notify("该任务的附件正在上传，请稍后再试", "info");
    return;
  }
  const baseline = task.attachment_count ?? 0;
  attachmentUploads.value[task.id] = { completed: 0, total: files.length };
  let uploaded = 0;
  let lastError = "";
  for (const file of files) {
    try {
      await api.uploadAttachment(task.id, file);
      uploaded++;
    } catch (e) {
      lastError = errorText(e);
    } finally {
      const progress = attachmentUploads.value[task.id];
      if (progress) progress.completed++;
    }
  }
  delete attachmentUploads.value[task.id];
  if (uploaded) {
    const latest = tasks.records[task.id] ?? task;
    tasks.acceptTask({
      ...latest,
      attachment_count: Math.max(
        latest.attachment_count ?? baseline,
        baseline + uploaded,
      ),
    });
    tasks.scheduleRefresh();
  }
  const failed = files.length - uploaded;
  if (!failed) notify(`已上传 ${uploaded} 个附件`);
  else if (uploaded)
    notify(
      `已上传 ${uploaded} 个附件，${failed} 个失败：${lastError}`,
      "error",
    );
  else notify(`附件上传失败：${lastError}`, "error");
}
watch(rows, (current) => {
  if (active.value && !current.some((t) => t.id === active.value?.id))
    active.value = null;
});
watch(
  () => view.visibleColumns.map((c) => c.key),
  (keys) => {
    if (active.value && !keys.includes(active.value.key))
      active.value.key = "description";
  },
);
const ACTION_COLUMN_WIDTH = 116;
const widths = computed(
  () =>
    view.visibleColumns.reduce((n, c) => n + c.width, 64) +
    ACTION_COLUMN_WIDTH,
);
const currentEditingTask = computed(() =>
  editingId.value
    ? (tasks.records[editingId.value] ?? editingTask.value)
    : null,
);
function selected(task: Task, key: string) {
  return active.value?.id === task.id && active.value.key === key;
}
async function select(task: Task, key: string, focus = true) {
  active.value = { id: task.id, key };
  if (focus) {
    await nextTick();
    cell(task.id, key)?.focus({ preventScroll: true });
  }
}
function cell(id: string, key: string) {
  return root.value?.querySelector<HTMLElement>(
    '[data-task-id="' + CSS.escape(id) + '"][data-column="' + key + '"]',
  );
}
function clearActiveCellOnOutsideClick(event: MouseEvent) {
  if (!active.value || !(event.target instanceof Element)) return;
  const clickedCell = event.target.closest<HTMLElement>(
    '[role="gridcell"][data-task-id]',
  );
  if (
    (clickedCell && root.value?.contains(clickedCell)) ||
    event.target.closest(".grid-cell-editor")
  )
    return;
  const activeCell = cell(active.value.id, active.value.key);
  active.value = null;
  if (activeCell && document.activeElement === activeCell) activeCell.blur();
}
function startEdit(task: Task, key: "description" | "note") {
  if (editingId.value && editingId.value !== task.id) return;
  editingTask.value = task;
  editingId.value = task.id;
  editingKey.value = key;
  void select(task, key, false);
  const rect = cell(task.id, key)?.getBoundingClientRect();
  editorPosition.value = {
    left:
      Math.max(12, Math.min(rect?.left ?? 100, window.innerWidth - 432)) + "px",
    top:
      Math.max(12, Math.min(rect?.top ?? 100, window.innerHeight - 280)) + "px",
  };
}
function closeEditor() {
  const restore = !!document.activeElement?.closest(".grid-cell-editor");
  const id = editingId.value,
    key = editingKey.value;
  editingId.value = null;
  editingTask.value = null;
  void nextTick(() => {
    if (id && restore) cell(id, key)?.focus({ preventScroll: true });
  });
}
function activate(task: Task, key: string) {
  if (key === "description" || key === "note") startEdit(task, key);
  else if (["project", "type", "status", "priority"].includes(key))
    cell(task.id, key)
      ?.querySelector<HTMLButtonElement>(".select-trigger")
      ?.click();
  else emit("open-detail", task);
}
function keyboard(e: KeyboardEvent, index: number, key: string) {
  if (
    (e.target as HTMLElement).closest("button,input,textarea") ||
    e.isComposing
  )
    return;
  const col = view.visibleColumns.findIndex((c) => c.key === key);
  if (e.altKey && e.key === "Enter") {
    e.preventDefault();
    emit("open-detail", rows.value[index]);
    return;
  }
  if (e.key === "Enter" || e.key === "F2") {
    e.preventDefault();
    activate(rows.value[index], key);
    return;
  }
  if ((e.ctrlKey || e.metaKey) && e.key === "c") {
    e.preventDefault();
    const task = rows.value[index];
    void copyText(
      key === "attachments"
        ? String(task.attachment_count ?? 0)
        : String(task[key as keyof Task] ?? ""),
    );
    return;
  }
  const keys = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Tab"];
  if (!keys.includes(e.key)) return;
  let nextRow = index,
    nextCol = col;
  if (e.key === "ArrowUp") nextRow--;
  if (e.key === "ArrowDown") nextRow++;
  if (e.key === "ArrowLeft") nextCol--;
  if (e.key === "ArrowRight") nextCol++;
  if (e.key === "Tab") {
    nextCol += e.shiftKey ? -1 : 1;
    if (nextCol < 0) {
      nextRow--;
      nextCol = view.visibleColumns.length - 1;
    }
    if (nextCol >= view.visibleColumns.length) {
      nextRow++;
      nextCol = 0;
    }
    if (nextRow < 0 || nextRow >= rows.value.length) return;
  }
  e.preventDefault();
  nextRow = Math.max(0, Math.min(rows.value.length - 1, nextRow));
  nextCol = Math.max(0, Math.min(view.visibleColumns.length - 1, nextCol));
  void select(rows.value[nextRow], view.visibleColumns[nextCol].key);
  void nextTick(() =>
    cell(
      rows.value[nextRow].id,
      view.visibleColumns[nextCol].key,
    )?.scrollIntoView({ block: "nearest", inline: "nearest" }),
  );
}
let stopResize: (() => void) | undefined;
function resize(e: PointerEvent, key: string) {
  const column = view.columns.find((c) => c.key === key)!;
  const x = e.clientX,
    width = column.width;
  const move = (event: PointerEvent) => {
    column.width = Math.max(
      key === "description" ? 260 : 80,
      Math.min(800, width + event.clientX - x),
    );
  };
  stopResize = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", stopResize!);
    stopResize = undefined;
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", stopResize);
  e.preventDefault();
  e.stopPropagation();
}
onMounted(() => document.addEventListener("click", clearActiveCellOnOutsideClick));
onBeforeUnmount(() => {
  stopResize?.();
  document.removeEventListener("click", clearActiveCellOnOutsideClick);
});
async function remove(task: Task) {
  if (
    !(await askConfirm(
      "删除任务",
      "确定删除「" +
        (task.description.slice(0, 40) || task.id) +
        "」？任务及附件将永久删除。",
      "删除任务",
      true,
    ))
  )
    return;
  try {
    await tasks.removeTask(task.id);
    notify("任务已删除");
  } catch (e) {
    notify(errorText(e), "error");
  }
}
async function changePage(value: number) {
  if (!Number.isFinite(value) || tasks.saving || editor.value?.saving) return;
  if (
    editingId.value &&
    editor.value?.dirty &&
    !(await askConfirm("离开当前编辑", "尚未保存的任务描述将被丢弃。", "离开"))
  )
    return;
  editingId.value = null;
  await tasks.setPage(value);
  root.value?.scrollTo({ top: 0 });
  active.value = null;
}
async function jumpPage(event: Event) {
  const input = event.target as HTMLInputElement;
  await changePage(Number(input.value));
  input.value = String(page.value);
}
async function reveal(id: string) {
  const result = await tasks.reveal(id);
  if (!result?.anchor_found) return;
  const task = tasks.tasks.find((t) => t.id === id);
  if (!task) return;
  if (tasks.filters.group_by)
    collapsedGroups.value.delete(task[tasks.filters.group_by]);
  await nextTick();
  await select(task, "description");
  cell(id, "description")?.scrollIntoView({ block: "nearest" });
}
const sorts = ["created_at", "finished_at", "priority"] as const;
defineExpose({ reveal });
</script>
<template>
  <div v-if="tasks.filters.group_by" class="group-strip">
    <span
      ><UiIcon name="layers" :size="13" />按{{
        groupLabels[tasks.filters.group_by]
      }}分组 · 共 {{ tasks.groups.length }} 组</span
    ><button class="text-button" @click="toggleGroups">
      {{ allCollapsed ? "全部展开" : "全部收起" }}</button
    ><span class="subtle">分组数量按完整筛选结果统计</span>
  </div>
  <div
    ref="root"
    class="grid-wrap"
    :style="{ '--row-height': view.density + 'px', '--index-width': '64px' }"
    :aria-busy="tasks.loading"
  >
    <table
      class="task-grid"
      role="grid"
      aria-label="任务表"
      :style="{ width: widths + 'px' }"
    >
      <colgroup>
        <col style="width: 64px" />
        <col
          v-for="column in view.visibleColumns"
          :key="column.key"
          :style="{ width: column.width + 'px' }"
        />
        <col />
      </colgroup>
      <thead>
        <tr>
          <th class="row-index pinned-index">
            <input
              type="checkbox"
              aria-label="全选本页可见任务"
              :checked="allSelected"
              :indeterminate="someSelected && !allSelected"
              :disabled="
                tasks.loading ||
                tasks.batchBusy ||
                !tasks.isCurrentPage ||
                !rows.length
              "
              @change="selectPage"
            />
          </th>
          <th
            v-for="column in view.visibleColumns"
            :key="column.key"
            :data-column="column.key"
            :class="{
              'pinned-description': column.key === 'description',
              'column-dragging': draggingColumn === column.key,
            }"
            :draggable="column.key !== 'description'"
            @dragstart="dragColumn($event, column.key)"
            @dragover.prevent
            @drop.prevent="dropColumn($event, column.key)"
            @dragend="draggingColumn = ''"
          >
            <div class="column-heading">
              <UiIcon :name="column.icon" :size="14" /><span>{{
                column.label
              }}</span
              ><button
                v-if="tasks.filters.sort_by === column.key"
                class="sort-toggle"
                :aria-label="
                  tasks.filters.sort_order === 'asc'
                    ? '点击改为降序排列'
                    : '点击改为升序排列'
                "
                :title="
                  tasks.filters.sort_order === 'asc'
                    ? '点击改为降序排列'
                    : '点击改为升序排列'
                "
                @click="
                  tasks.filters.sort_order =
                    tasks.filters.sort_order === 'asc' ? 'desc' : 'asc'
                "
              >
                <UiIcon
                  :name="tasks.filters.sort_order === 'asc' ? 'up' : 'down'"
                  :size="12"
                /></button
              ><UiPopover
                :width="200"
                align="right"
                :label="column.label + '字段设置'"
                ><template #trigger="{ toggle }"
                  ><button
                    class="icon-btn column-menu"
                    :aria-label="column.label + '字段设置'"
                    @click="toggle"
                  >
                    <UiIcon name="chevron" :size="12" /></button></template
                ><template #default="{ close }">
                  <div class="menu-caption">{{ column.label }}</div>
                  <button
                    v-if="GROUP_FIELDS.includes(column.key as GroupField)"
                    class="menu-item"
                    @click="
                      tasks.filters.group_by = column.key as GroupField;
                      close();
                    "
                  >
                    <UiIcon name="layers" :size="14" />按此字段分组
                  </button>
                  <template v-if="column.key !== 'description'"
                    ><button
                      class="menu-item"
                      :disabled="
                        view.columns.findIndex((c) => c.key === column.key) <= 1
                      "
                      @click="
                        view.moveColumn(column.key, -1);
                        close();
                      "
                    >
                      <UiIcon name="left" :size="14" />向前移动</button
                    ><button
                      class="menu-item"
                      :disabled="
                        view.columns.findIndex((c) => c.key === column.key) ===
                        view.columns.length - 1
                      "
                      @click="
                        view.moveColumn(column.key, 1);
                        close();
                      "
                    >
                      <UiIcon name="right" :size="14" />向后移动
                    </button></template
                  >
                  <template
                    v-if="sorts.includes(column.key as (typeof sorts)[number])"
                    ><button
                      class="menu-item"
                      @click="
                        tasks.filters.sort_by =
                          column.key as (typeof sorts)[number];
                        tasks.filters.sort_order = 'asc';
                        close();
                      "
                    >
                      <UiIcon name="up" />升序排列</button
                    ><button
                      class="menu-item"
                      @click="
                        tasks.filters.sort_by =
                          column.key as (typeof sorts)[number];
                        tasks.filters.sort_order = 'desc';
                        close();
                      "
                    >
                      <UiIcon name="down" />降序排列
                    </button>
                    <div class="menu-divider"></div
                  ></template>
                  <button
                    v-if="column.key !== 'description'"
                    class="menu-item"
                    @click="
                      column.visible = false;
                      close();
                    "
                  >
                    <UiIcon name="columns" />隐藏此字段</button
                  ><button
                    class="menu-item"
                    @click="
                      column.width = column.key === 'description' ? 380 : 150;
                      close();
                    "
                  >
                    <UiIcon name="refresh" />重置列宽
                  </button>
                </template></UiPopover
              >
            </div>
            <span
              class="col-resize"
              @pointerdown="resize($event, column.key)"
            ></span>
          </th>
          <th
            class="task-actions-column"
            data-column="actions"
            @dragover.prevent
            @drop.prevent="dropColumn($event, 'actions')"
          >
            <div class="column-heading task-actions-heading">
              <UiIcon name="more" :size="14" /><span>操作</span>
            </div>
          </th>
        </tr>
      </thead>
      <tbody>
        <template v-for="group in pageGroups" :key="group.value">
          <tr v-if="tasks.filters.group_by" class="group-row">
            <td :colspan="view.visibleColumns.length + 2">
              <div class="group-heading">
                <button
                  class="group-toggle"
                  :aria-expanded="!collapsedGroups.has(group.value)"
                  :aria-label="'分组：' + group.value"
                  @click="toggleGroup(group.value)"
                >
                  <UiIcon
                    :name="
                      collapsedGroups.has(group.value) ? 'right' : 'chevron'
                    "
                    :size="13"
                  /><strong>{{ group.value }}</strong
                  ><span>{{ group.count }} 条</span></button
                ><span v-if="group.count > group.items.length" class="subtle"
                  >本页 {{ group.items.length }} 条</span
                >
              </div>
            </td>
          </tr>
          <template v-if="!collapsedGroups.has(group.value)">
            <tr
              v-for="task in group.items"
              :key="task.id"
              class="task-row"
              :class="{
                'row-selected': active?.id === task.id,
                'row-checked': !!tasks.selection[task.id],
                'row-dragging': draggingTask === task.id,
                'drop-above': dropSlot?.id === task.id && dropSlot.edge === 'above',
                'drop-below': dropSlot?.id === task.id && dropSlot.edge === 'below',
              }"
              @dragover="dragOverRow($event, task)"
              @drop="dropRow($event, task)"
            >
              <td class="row-index pinned-index">
                <span
                  class="row-grip"
                  :class="{ 'grip-ready': canDragRows }"
                  :draggable="canDragRows"
                  :title="canDragRows ? '拖动排序' : '排序方式设为手动排序后可拖动'"
                  @dragstart="dragRow($event, task)"
                  @dragend="dragRowEnd"
                  ><UiIcon name="grip" :size="14" /></span
                ><input
                  class="row-checkbox"
                  type="checkbox"
                  :aria-label="
                    '选择任务：' + (task.description.slice(0, 32) || task.id)
                  "
                  :checked="!!tasks.selection[task.id]"
                  :disabled="
                    tasks.loading || tasks.batchBusy || !tasks.isCurrentPage
                  "
                  @change="tasks.toggleSelection(task)"
                /><span class="row-number">{{ ordinal(task.id) }}</span
                ><UiPopover :width="190" label="任务操作"
                  ><template #trigger="{ toggle }"
                    ><button
                      class="icon-btn row-menu"
                      :aria-label="'任务操作：' + task.id"
                      @click="toggle"
                    >
                      <UiIcon name="more" /></button></template
                  ><template #default="{ close }"
                    ><button
                      class="menu-item"
                      @click="
                        emit('open-detail', task);
                        close();
                      "
                    >
                      <UiIcon name="expand" />展开详情</button
                    ><button
                      class="menu-item"
                      @click="
                        copyText(task.id);
                        close();
                      "
                    >
                      <UiIcon name="copy" />复制任务 ID
                    </button>
                    <div class="menu-divider"></div>
                    <button
                      class="menu-item danger-text"
                      @click="
                        close();
                        remove(task);
                      "
                    >
                      <UiIcon name="trash" />删除任务
                    </button></template
                  ></UiPopover
                >
              </td>
              <td
                v-for="(column, colIndex) in view.visibleColumns"
                :key="column.key"
                role="gridcell"
                :data-task-id="task.id"
                :data-column="column.key"
                :aria-label="
                  column.key === 'attachments'
                    ? `附件 ${task.attachment_count ?? 0} 个，可拖入或粘贴文件上传`
                    : column.label
                "
                :aria-selected="selected(task, column.key)"
                :tabindex="
                  selected(task, column.key) ||
                  (!active && rowIndex(task.id) === 0 && colIndex === 0)
                    ? 0
                    : -1
                "
                :class="{
                  'pinned-description': column.key === 'description',
                  'cell-selected': selected(task, column.key),
                  'cell-editing':
                    editingId === task.id && column.key === editingKey,
                  'attachment-drop-target':
                    column.key === 'attachments' &&
                    attachmentDragTarget === task.id,
                  'attachment-uploading':
                    column.key === 'attachments' &&
                    !!attachmentUploads[task.id],
                }"
                @click="
                  column.key === 'description'
                    ? startEdit(task, 'description')
                    : select(task, column.key)
                "
                @dblclick="
                  column.key !== 'description' && activate(task, column.key)
                "
                @keydown="keyboard($event, rowIndex(task.id), column.key)"
                @dragover="
                  column.key === 'attachments' &&
                  dragOverAttachments($event, task)
                "
                @dragleave="
                  column.key === 'attachments' &&
                  leaveAttachmentCell($event, task)
                "
                @drop="
                  column.key === 'attachments' && dropAttachments($event, task)
                "
                @paste="
                  column.key === 'attachments' && pasteAttachments($event, task)
                "
              >
                <div
                  v-if="column.key === 'description'"
                  class="description-cell"
                >
                  <span
                    class="description-text"
                    :class="{ subtle: !task.description }"
                    :title="task.description"
                    >{{ task.description || "未填写任务描述" }}</span
                  ><button
                    class="icon-btn expand-record"
                    :aria-label="
                      '展开任务：' + (task.description.slice(0, 28) || task.id)
                    "
                    title="展开详情"
                    @click.stop="emit('open-detail', task)"
                  >
                    <UiIcon name="expand" :size="13" />
                  </button>
                </div>
                <TaskField
                  v-else-if="
                    column.key === 'project' ||
                    column.key === 'type' ||
                    column.key === 'status' ||
                    column.key === 'priority'
                  "
                  :task="task"
                  :field="column.key"
                  :editable="
                    selected(task, column.key) &&
                    tasks.isCurrentPage &&
                    !tasks.batchBusy
                  "
                />
                <span
                  v-else-if="column.key === 'note'"
                  class="note-cell"
                  :class="{ subtle: !task.note }"
                  :title="task.note"
                  >{{ task.note || "—" }}</span
                >
                <span
                  v-else-if="column.key === 'submitter'"
                  class="submitter-tag"
                  :class="task.submitter === 'Agent' ? 'agent' : 'human'"
                  ><span class="submitter-avatar"
                    ><UiIcon
                      :name="task.submitter === 'Agent' ? 'bot' : 'user'"
                      :size="12" /></span
                  ><span class="submitter-label" :title="task.submitter_name || task.submitter">{{
                    task.submitter_name || task.submitter
                  }}</span></span
                >
                <span
                  v-else-if="column.key === 'attachments'"
                  class="attachment-cell"
                  :class="{ subtle: !task.attachment_count }"
                  ><span
                    v-if="attachmentUploads[task.id]"
                    class="spinner"
                  ></span
                  ><UiIcon
                    v-else-if="attachmentDragTarget === task.id"
                    name="upload"
                    :size="14"
                  /><template v-else-if="attachmentPreviews[task.id]?.length"
                    ><span class="attachment-preview-list"
                      ><template
                        v-for="attachment in visibleAttachmentPreviews(
                          attachmentPreviews[task.id],
                        )"
                        :key="attachment.id"
                        ><button
                          v-if="attachmentKind(attachment) !== 'file'"
                          type="button"
                          class="attachment-preview"
                          :class="
                            'attachment-preview-' + attachmentKind(attachment)
                          "
                          :title="attachment.filename"
                          :aria-label="'预览：' + attachment.filename"
                          @click.stop="preview = attachment"
                          @dblclick.stop
                        >
                          <img
                            v-if="attachmentKind(attachment) === 'image'"
                            :src="api.attachmentUrl(attachment.id)"
                            :alt="attachment.filename"
                          /><UiIcon v-else name="video" :size="14" /></button
                        ><span
                          v-else
                          class="attachment-preview attachment-preview-file"
                          :title="attachment.filename"
                          ><UiIcon name="file" :size="14" /></span
                      ></template
                      ><span
                        v-if="
                          hiddenAttachmentCount(
                            attachmentPreviews[task.id],
                          ) > 0
                        "
                        class="attachment-preview-overflow"
                        >+{{
                          hiddenAttachmentCount(attachmentPreviews[task.id])
                        }}</span
                      ></span
                    ></template
                  ><UiIcon
                    v-else-if="task.attachment_count"
                    name="attachment"
                    :size="14"
                  /><template v-if="attachmentUploads[task.id]"
                    >{{ attachmentUploads[task.id].completed }}/{{
                      attachmentUploads[task.id].total
                    }}</template
                  ><template v-else-if="attachmentDragTarget === task.id"
                    >松开上传</template
                  ><template
                    v-else-if="!attachmentPreviews[task.id]?.length"
                    >{{ task.attachment_count || "—" }}</template
                  ></span
                >
                <div v-else-if="column.key === 'id'" class="id-cell">
                  <span class="cell-id" :title="task.id">{{ task.id }}</span
                  ><button
                    type="button"
                    class="icon-btn id-prompt-button"
                    :aria-label="'复制任务 Prompt：' + task.id"
                    title="复制任务 Prompt"
                    @click.stop="copyText(buildAgentIdPrompt(task))"
                  >
                    <UiIcon name="copy" :size="13" />
                  </button>
                </div>
                <span v-else class="cell-time">{{
                  formatDateTime(
                    task[column.key as "created_at" | "finished_at"],
                  ) || "—"
                }}</span>
              </td>
              <td
                role="gridcell"
                class="task-actions-column"
                data-column="actions"
              >
                <TaskActions :task="task" />
              </td>
            </tr>
          </template></template
        >
      </tbody>
    </table>
    <div v-if="!tasks.tasks.length" class="grid-empty">
      <span v-if="tasks.loading" class="spinner"></span
      ><span v-else class="empty-icon"
        ><UiIcon
          :name="
            tasks.error ? 'info' : tasks.activeFilterCount ? 'search' : 'grid'
          "
          :size="32"
      /></span>
      <h3>
        {{
          tasks.loading
            ? "正在加载任务"
            : tasks.error
              ? "暂时无法加载任务"
              : tasks.activeFilterCount
                ? "没有符合条件的任务"
                : "从第一项任务开始"
        }}
      </h3>
      <p>
        {{
          tasks.error
            ? "请检查服务连接后重新加载。"
            : tasks.activeFilterCount
              ? "试试其他关键词，或清空筛选条件。"
              : "把需求、问题和待办记录下来，随时跟进进展。"
        }}
      </p>
      <button
        v-if="!tasks.loading"
        class="btn btn-sm"
        :class="{ 'btn-primary': !tasks.error && !tasks.activeFilterCount }"
        @click="
          tasks.error
            ? tasks.refresh()
            : tasks.activeFilterCount
              ? tasks.clearFilters()
              : emit('create')
        "
      >
        {{
          tasks.error
            ? "重新加载"
            : tasks.activeFilterCount
              ? "清空筛选"
              : "新建任务"
        }}
      </button>
    </div>
    <button
      v-else
      class="add-record-row"
      :disabled="quickCreating"
      @click="emit('quick-create')"
    >
      <span v-if="quickCreating" class="spinner"></span
      ><UiIcon v-else name="plus" :size="15" />{{
        quickCreating ? "正在添加…" : "添加一条任务"
      }}
    </button>
  </div>
  <Teleport to="body"
    ><div
      v-if="editingId && currentEditingTask"
      class="grid-cell-editor"
      :style="editorPosition"
    >
      <div v-if="!rows.some((t) => t.id === editingId)" class="info-banner">
        任务已不在当前结果中，草稿仍然保留。
      </div>
      <DescriptionEditor
        ref="editor"
        :task-id="editingId"
        :value="currentEditingTask[editingKey]"
        :field="editingKey"
        inline
        @close="closeEditor"
      /></div
  ></Teleport>
  <footer class="grid-footer">
    <span
      >共 <strong>{{ tasks.total }}</strong> 条记录<span
        v-if="tasks.activeFilterCount"
      >
        · 已筛选</span
      ></span
    ><span class="save-indicator" role="status"
      ><span v-if="tasks.saving || tasks.loading" class="spinner"></span
      ><UiIcon
        v-else-if="tasks.initialized && !tasks.error"
        name="check"
        :size="13"
      />{{
        tasks.saving
          ? "正在保存"
          : tasks.loading
            ? "正在更新"
            : tasks.error
              ? "加载失败"
              : "更改自动保存"
      }}</span
    >
    <div class="pagination">
      <button
        class="icon-btn"
        :disabled="page === 1 || tasks.loading || tasks.saving"
        aria-label="上一页"
        @click="changePage(page - 1)"
      >
        <UiIcon name="left" :size="14" /></button
      ><label class="page-input-label"
        ><input
          class="page-input"
          type="number"
          aria-label="跳转页码"
          :value="page"
          min="1"
          :max="pages"
          @change="jumpPage"
        /><span>/ {{ pages }} 页</span></label
      ><button
        class="icon-btn"
        :disabled="page === pages || tasks.loading || tasks.saving"
        aria-label="下一页"
        @click="changePage(page + 1)"
      >
        <UiIcon name="right" :size="14" />
      </button>
    </div>
    <UiPopover :width="150" align="right" label="每页条数"
      ><template #trigger="{ toggle }"
        ><button
          class="btn btn-sm btn-ghost page-size-button"
          :disabled="tasks.loading || tasks.saving"
          @click="toggle"
        >
          {{ tasks.pageSize }} 条/页<UiIcon
            name="chevron"
            :size="12"
          /></button></template
      ><template #default="{ close }"
        ><button
          v-for="size in [50, 100, 200]"
          :key="size"
          class="menu-item"
          @click="
            tasks.setPageSize(size);
            close();
          "
        >
          {{ size }} 条/页<UiIcon
            v-if="tasks.pageSize === size"
            name="check"
            class="menu-check"
          /></button></template
    ></UiPopover>
  </footer>
  <AttachmentPreviewDialog
    v-if="preview"
    :attachment="preview"
    @close="preview = null"
  />
</template>
