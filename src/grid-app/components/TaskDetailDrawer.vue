<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { api, ApiRequestError } from "../api/client";
import { useTaskStore } from "../stores/taskStore";
import UiDialog from "@/shared/UiDialog.vue";
import UiIcon from "@/shared/UiIcon.vue";
import UiPopover from "@/shared/UiPopover.vue";
import TaskField from "./TaskField.vue";
import TaskHistory from "./TaskHistory.vue";
import TaskMultiSelect from "./TaskMultiSelect.vue";
import DescriptionEditor from "./DescriptionEditor.vue";
import AttachmentUploader from "./AttachmentUploader.vue";
import AttachmentPreviewDialog from "./AttachmentPreviewDialog.vue";
import { useUploadQueue, formatSize } from "./useUploadQueue";
import { attachmentKind } from "../attachmentKind";
import { askConfirm, copyText, errorText, notify } from "@/shared/feedback";
import { formatDateTime, type Task, type Attachment } from "@/shared/types";
const props = withDefaults(defineProps<{ task: Task; navigation?: boolean }>(), {
  navigation: true,
});
const emit = defineEmits<{ close: []; navigate: [task: Task] }>();
const activeTab = ref<"detail" | "history">("detail");
const tasks = useTaskStore(),
  queue = useUploadQueue();
const attachments = ref<Attachment[]>([]),
  attachmentError = ref(""),
  attachmentLoading = ref(false),
  showUpload = ref(false);
const editingField = ref<"description" | "note" | null>(null),
  editor = ref<InstanceType<typeof DescriptionEditor>>();
const preview = ref<Attachment | null>(null);
const missing = ref(false),
  detailError = ref("");
const current = computed(() => tasks.records[props.task.id] ?? props.task);
const dependencyOptions = ref<Task[]>([]);
const dependencyOptionsLoading = ref(false);
async function loadDependencyOptions() {
  if (dependencyOptionsLoading.value) return;
  dependencyOptionsLoading.value = true;
  try {
    dependencyOptions.value = await api.listTasks();
  } catch (e) {
    notify("加载关联任务失败：" + errorText(e), "error");
  } finally {
    dependencyOptionsLoading.value = false;
  }
}
async function updateDependencies(
  field: "predecessor_task_ids" | "unlock_task_ids",
  ids: string[],
) {
  try {
    await tasks.updateTask(current.value.id, { [field]: ids });
    void loadDependencyOptions();
  } catch (e) {
    notify(errorText(e), "error");
  }
}
const index = computed(() =>
  tasks.tasks.findIndex((t) => t.id === props.task.id),
);
const busy = computed(() => queue.busy.value || !!tasks.pending[props.task.id]);
let loadId = 0;
let detailRequest = 0;
async function refreshCurrent() {
  const id = ++detailRequest,
    taskId = props.task.id;
  const before = tasks.records[taskId];
  if (tasks.pending[taskId]) return;
  try {
    // Read independently of the table's filters, which may now exclude this open task.
    const latest = await api.getTask(taskId);
    if (
      id !== detailRequest ||
      taskId !== props.task.id ||
      tasks.pending[taskId] ||
      tasks.records[taskId] !== before
    )
      return;
    missing.value = false;
    detailError.value = "";
    if (latest) tasks.acceptTask(latest);
  } catch (e) {
    if (id === detailRequest) {
      if (e instanceof ApiRequestError && e.status === 404)
        missing.value = true;
      else detailError.value = errorText(e);
    }
  }
}
async function loadAttachments() {
  const id = ++loadId,
    taskId = props.task.id;
  attachmentError.value = "";
  attachmentLoading.value = true;
  try {
    const result = await api.listAttachments(taskId);
    if (id === loadId) attachments.value = result;
  } catch (e) {
    if (id === loadId) attachmentError.value = errorText(e);
  } finally {
    if (id === loadId) attachmentLoading.value = false;
  }
}
watch(
  () => props.task.id,
  () => {
    detailRequest++;
    activeTab.value = "detail";
    missing.value = false;
    detailError.value = "";
    attachments.value = [];
    editingField.value = null;
    queue.items.value = [];
    showUpload.value = false;
    preview.value = null;
    void loadAttachments();
    void refreshCurrent();
  },
  { immediate: true },
);
watch(
  () => tasks.externalRevision,
  () => {
    if (!queue.busy.value) void loadAttachments();
    void refreshCurrent();
  },
);
onBeforeUnmount(() => {
  loadId++;
  detailRequest++;
});
async function canLeave() {
  if (busy.value) return false;
  if (
    (editingField.value && editor.value?.dirty) ||
    queue.items.value.some((item) => item.state !== "done")
  )
    return askConfirm(
      "离开任务详情",
      "未保存的描述、备注或未上传的附件将被丢弃。",
      "离开",
    );
  return true;
}
async function close() {
  if (await canLeave()) emit("close");
}
async function navigate(direction: number) {
  if (!(await canLeave())) return;
  const task = await tasks.adjacentTask(props.task.id, direction as -1 | 1);
  if (task) emit("navigate", task);
}
async function upload() {
  await queue.upload(props.task.id);
  await loadAttachments();
  tasks.scheduleRefresh();
  if (!queue.failed.value.length) {
    queue.clearDone();
    showUpload.value = false;
    notify("附件已上传");
  }
}
async function removeAttachment(attachment: Attachment) {
  if (
    !(await askConfirm(
      "删除附件",
      "确定永久删除「" + attachment.filename + "」？",
      "删除附件",
      true,
    ))
  )
    return;
  try {
    await api.deleteAttachment(attachment.id);
    await loadAttachments();
    tasks.scheduleRefresh();
    notify("附件已删除");
  } catch (e) {
    attachmentError.value = errorText(e);
  }
}
async function removeTask() {
  if (
    !(await askConfirm(
      "删除任务",
      "该任务及其附件将被永久删除，确认继续？",
      "删除任务",
      true,
    ))
  )
    return;
  try {
    await tasks.removeTask(props.task.id);
    emit("close");
    notify("任务已删除");
  } catch (e) {
    notify(errorText(e), "error");
  }
}
async function clearAssignee() {
  if (
    !(await askConfirm(
      "清除负责人",
      "清除后任意 Agent 都可以继续修改该任务，确认清除负责人？",
      "清除",
    ))
  )
    return;
  try {
    await tasks.updateTask(props.task.id, { assignee_user_id: null });
    notify("负责人已清除");
  } catch (e) {
    notify(errorText(e), "error");
  }
}
</script>
<template>
  <UiDialog title="任务详情" drawer :busy="busy" @close="close">
    <template #header-actions
      ><button
        v-if="navigation"
        class="icon-btn"
        aria-label="上一条任务"
        title="上一条任务"
        :disabled="(index === 0 && tasks.page === 1) || busy || tasks.loading"
        @click="navigate(-1)"
      >
        <UiIcon name="up" /></button
      ><button
        v-if="navigation"
        class="icon-btn"
        aria-label="下一条任务"
        title="下一条任务"
        :disabled="
          (index === tasks.tasks.length - 1 && tasks.page === tasks.pages) ||
          busy ||
          tasks.loading
        "
        @click="navigate(1)"
      >
        <UiIcon name="down" /></button
      ><UiPopover align="right" :width="180" label="详情操作"
        ><template #trigger="{ toggle }"
          ><button class="icon-btn" aria-label="更多任务操作" @click="toggle">
            <UiIcon name="more" /></button></template
        ><template #default="{ close: closeMenu }"
          ><button
            class="menu-item"
            @click="
              copyText(current.id);
              closeMenu();
            "
          >
            <UiIcon name="copy" />复制任务 ID</button
          ><button
            class="menu-item danger-text"
            :disabled="busy"
            @click="
              closeMenu();
              removeTask();
            "
          >
            <UiIcon name="trash" />删除任务
          </button></template
        ></UiPopover
      ></template
    >
    <div class="detail-identifier">
      <span class="tag tag-gray">任务</span><span>{{ current.id }}</span
      ><button
        class="icon-btn"
        aria-label="复制任务 ID"
        title="复制任务 ID"
        @click="copyText(current.id)"
      >
        <UiIcon name="copy" :size="13" />
      </button>
    </div>
    <h3 class="detail-title">
      {{ current.description.split("\n")[0] || "未填写任务描述" }}
    </h3>
    <div v-if="missing" class="form-error" role="alert">
      任务已被其他操作删除。当前保留最后一次内容，便于复制。
    </div>
    <div v-else-if="detailError" class="form-error" role="alert">
      {{ detailError
      }}<button class="btn btn-sm" @click="refreshCurrent">重试</button>
    </div>
    <div v-if="navigation && index < 0" class="info-banner detail-outside-filter">
      此任务不在当前页中，仍可在这里查看和编辑。
    </div>
    <div class="detail-tabs" role="tablist" aria-label="任务详情分页">
      <button id="task-detail-tab" role="tab" :aria-selected="activeTab === 'detail'" aria-controls="task-detail-panel" @click="activeTab = 'detail'">详情</button>
      <button id="task-history-tab" role="tab" :aria-selected="activeTab === 'history'" aria-controls="task-history-panel" @click="activeTab = 'history'">历史</button>
    </div>
    <div v-if="activeTab === 'history'" id="task-history-panel" role="tabpanel" aria-labelledby="task-history-tab">
      <TaskHistory :task-id="current.id" :revision="JSON.stringify(current)" />
    </div>
    <div v-show="activeTab === 'detail'" id="task-detail-panel" role="tabpanel" aria-labelledby="task-detail-tab">
    <div class="detail-properties">
      <div class="property-row">
        <span><UiIcon name="folder" />项目</span
        ><TaskField :task="current" field="project" :editable="!missing" form />
      </div>
      <div class="property-row">
        <span><UiIcon name="tag" />任务类型</span
        ><TaskField :task="current" field="type" :editable="!missing" form />
      </div>
      <div class="property-row">
        <span><UiIcon name="flag" />优先级</span
        ><TaskField :task="current" field="priority" :editable="!missing" form />
      </div>
      <div class="property-row">
        <span><UiIcon name="circle" />当前状态</span
        ><TaskField :task="current" field="status" :editable="!missing" form />
      </div>
      <div class="property-row">
        <span><UiIcon name="git" />子任务 ID</span>
        <TaskMultiSelect
          :model-value="current.predecessor_task_ids ?? []"
          :options="
            dependencyOptions.filter(
              (task) => !(current.unlock_task_ids ?? []).includes(task.id),
            )
          "
          label="子任务 ID"
          :exclude-id="current.id"
          :disabled="missing || busy"
          :loading="dependencyOptionsLoading"
          @open="loadDependencyOptions"
          @update:model-value="updateDependencies('predecessor_task_ids', $event)"
        />
      </div>
      <div class="property-row">
        <span><UiIcon name="git" />父级任务 ID</span>
        <TaskMultiSelect
          :model-value="current.unlock_task_ids ?? []"
          :options="
            dependencyOptions.filter(
              (task) => !(current.predecessor_task_ids ?? []).includes(task.id),
            )
          "
          label="父级任务 ID"
          :exclude-id="current.id"
          :disabled="missing || busy"
          :loading="dependencyOptionsLoading"
          @open="loadDependencyOptions"
          @update:model-value="updateDependencies('unlock_task_ids', $event)"
        />
      </div>
      <div class="property-row">
        <span><UiIcon name="user" />提交人</span>
        <div
          class="submitter-tag"
          :class="current.submitter === 'Agent' ? 'agent' : 'human'"
        >
          <span class="submitter-avatar"
            ><UiIcon
              :name="current.submitter === 'Agent' ? 'bot' : 'user'"
              :size="12" /></span
          >{{ current.submitter_name || current.submitter }}
        </div>
      </div>
      <div class="property-row">
        <span><UiIcon name="bot" />负责人</span>
        <div class="assignee-value">
          <div v-if="current.assignee_name" class="submitter-tag agent">
            <span class="submitter-avatar"
              ><UiIcon name="bot" :size="12" /></span
            >{{ current.assignee_name }}
          </div>
          <span v-else class="detail-date">—</span>
          <button
            v-if="current.assignee_user_id && !missing"
            class="btn btn-ghost btn-sm"
            :disabled="busy"
            title="清除后任意 Agent 都可以继续修改该任务"
            @click="clearAssignee"
          >
            清除
          </button>
        </div>
      </div>
      <div class="property-row">
        <span><UiIcon name="clock" />创建时间</span>
        <div class="detail-date">{{ formatDateTime(current.created_at) }}</div>
      </div>
      <div class="property-row">
        <span><UiIcon name="check" />完成时间</span>
        <div class="detail-date">
          {{ formatDateTime(current.finished_at) || "—" }}
        </div>
      </div>
    </div>
    <section class="detail-section">
      <div class="section-heading">
        <h3><UiIcon name="text" />任务描述</h3>
        <button
          v-if="!editingField && !missing"
          class="btn btn-ghost btn-sm"
          @click="editingField = 'description'"
        >
          <UiIcon name="edit" :size="14" />编辑
        </button>
      </div>
      <DescriptionEditor
        v-if="editingField === 'description'"
        ref="editor"
        :task-id="current.id"
        :value="current.description"
        @close="editingField = null"
      />
      <p
        v-else
        class="detail-description"
        :class="{ subtle: !current.description }"
      >
        {{
          current.description || "暂无描述，添加背景和验收要求，让任务更清晰。"
        }}
      </p>
    </section>
    <section class="detail-section">
      <div class="section-heading">
        <h3><UiIcon name="edit" />备注</h3>
        <button
          v-if="!editingField && !missing"
          class="btn btn-ghost btn-sm"
          @click="editingField = 'note'"
        >
          <UiIcon name="edit" :size="14" />编辑
        </button>
      </div>
      <DescriptionEditor
        v-if="editingField === 'note'"
        ref="editor"
        :task-id="current.id"
        :value="current.note"
        field="note"
        @close="editingField = null"
      />
      <p
        v-else
        class="detail-description"
        :class="{ subtle: !current.note }"
      >
        {{ current.note || "暂无备注。" }}
      </p>
    </section>
    <section class="detail-section">
      <div class="section-heading">
        <h3>
          <UiIcon name="attachment" />附件<span class="section-count">{{
            attachments.length
          }}</span>
        </h3>
        <button
          class="btn btn-ghost btn-sm"
          :disabled="queue.busy.value || missing"
          @click="showUpload = !showUpload"
        >
          <UiIcon name="plus" :size="14" />添加附件
        </button>
      </div>
      <div v-if="attachmentError" class="form-error" role="alert">
        {{ attachmentError
        }}<button class="btn btn-sm" @click="loadAttachments">重试</button>
      </div>
      <div v-if="showUpload || queue.items.value.length" class="detail-upload">
        <AttachmentUploader
          :items="queue.items.value"
          :busy="queue.busy.value"
          @add="queue.add"
          @remove="queue.remove"
        /><button
          v-if="queue.items.value.length"
          class="btn btn-primary btn-sm"
          :disabled="queue.busy.value || missing"
          @click="upload"
        >
          {{
            queue.busy.value
              ? "上传中…"
              : queue.failed.value.length
                ? "重试上传"
                : "上传附件"
          }}
        </button>
      </div>
      <div
        v-if="attachmentLoading && !attachments.length"
        class="attachment-empty"
      >
        <span class="spinner"></span>正在加载附件
      </div>
      <ul v-else-if="attachments.length" class="attachment-list">
        <li
          v-for="attachment in attachments"
          :key="attachment.id"
          class="attachment-item"
        >
          <button
            v-if="attachmentKind(attachment) !== 'file'"
            class="attachment-thumbnail"
            :aria-label="'预览：' + attachment.filename"
            @click="preview = attachment"
          >
            <img
              v-if="attachmentKind(attachment) === 'image'"
              :src="api.attachmentUrl(attachment.id)"
              :alt="attachment.filename"
            /><UiIcon v-else name="expand" :size="24" /></button
          ><span v-else class="attachment-file-icon"
            ><UiIcon name="file" :size="25"
          /></span>
          <div class="attachment-meta">
            <a
              :href="api.attachmentUrl(attachment.id)"
              :download="attachment.filename"
              >{{ attachment.filename }}</a
            ><span>{{ formatSize(attachment.size) }}</span>
          </div>
          <a
            class="icon-btn"
            :aria-label="'下载：' + attachment.filename"
            :href="api.attachmentUrl(attachment.id)"
            :download="attachment.filename"
            ><UiIcon name="download" :size="15" /></a
          ><button
            class="icon-btn danger"
            :aria-label="'删除附件：' + attachment.filename"
            @click="removeAttachment(attachment)"
          >
            <UiIcon name="trash" :size="15" />
          </button>
        </li>
      </ul>
      <div v-else-if="!showUpload && !attachmentError" class="attachment-empty">
        <UiIcon name="attachment" :size="20" /><span>暂无附件</span
        ><span class="subtle">添加截图、文档或视频，补充任务信息。</span>
      </div>
    </section>
    </div>
    <template #footer
      ><span class="detail-updated"
        >最后更新 {{ formatDateTime(current.updated_at) }}</span
      ><button class="btn btn-sm" :disabled="busy" @click="close">
        完成
      </button></template
    >
  </UiDialog>
  <AttachmentPreviewDialog
    v-if="preview"
    :attachment="preview"
    @close="preview = null"
  />
</template>

<style scoped>
.detail-tabs { display: flex; gap: 28px; border-bottom: 1px solid var(--separator); margin-bottom: 24px; }
.detail-tabs button { padding: 12px 2px; border: 0; border-bottom: 3px solid transparent; background: transparent; color: var(--text-secondary); cursor: pointer; font: inherit; }
.detail-tabs button[aria-selected="true"] { color: var(--primary); border-bottom-color: var(--primary); }
</style>
