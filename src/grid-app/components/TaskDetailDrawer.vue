<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api } from "@/grid-app/api/client";
import { useMetaStore } from "@/grid-app/stores/metaStore";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import { formatDateTime, type Attachment, type Task } from "@/shared/types";

const props = defineProps<{ task: Task }>();
const emit = defineEmits<{ close: [] }>();

const metaStore = useMetaStore();
const taskStore = useTaskStore();

const attachments = ref<Attachment[]>([]);
const error = ref("");
const uploading = ref(false);

const current = computed(
  () => taskStore.tasks.find((t) => t.id === props.task.id) ?? props.task,
);

async function refreshAttachments() {
  attachments.value = await api.listAttachments(props.task.id);
}

onMounted(refreshAttachments);

function isImage(a: Attachment) {
  return /^image\//.test(a.mime ?? "") || /\.(png|jpe?g|gif|webp)$/i.test(a.filename);
}

function isVideo(a: Attachment) {
  return /^video\//.test(a.mime ?? "") || /\.(mp4|mov)$/i.test(a.filename);
}

function formatSize(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

async function onUpload(e: Event) {
  const input = e.target as HTMLInputElement;
  const files = Array.from(input.files ?? []);
  input.value = "";
  if (!files.length) return;
  error.value = "";
  uploading.value = true;
  try {
    for (const f of files) await api.uploadAttachment(props.task.id, f);
    await refreshAttachments();
    await taskStore.refresh();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    uploading.value = false;
  }
}

async function onDeleteAttachment(a: Attachment) {
  if (!confirm(`删除附件「${a.filename}」？`)) return;
  try {
    await api.deleteAttachment(a.id);
    await refreshAttachments();
    await taskStore.refresh();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}
</script>

<template>
  <div class="drawer-mask" @click.self="emit('close')">
    <aside class="drawer">
      <header class="drawer-header">
        <h2>任务详情</h2>
        <button class="icon-btn" @click="emit('close')">✕</button>
      </header>

      <div class="drawer-body">
        <div class="detail-row">
          <span class="detail-label">ID</span>
          <span class="cell-id">{{ current.id }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">项目</span>
          <span>
            <span class="option-dot" :style="{ background: metaStore.projectColor(current.project) }"></span>
            {{ current.project }}
          </span>
        </div>
        <div class="detail-row">
          <span class="detail-label">类型</span>
          <span>{{ current.type }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">状态</span>
          <span>{{ current.status }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">提交人</span>
          <span>{{ current.submitter }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">创建时间</span>
          <span>{{ formatDateTime(current.created_at) }}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">完成时间</span>
          <span>{{ formatDateTime(current.finished_at) || "—" }}</span>
        </div>

        <div class="detail-section">
          <h3>任务描述</h3>
          <p class="detail-desc">{{ current.description || "（空）" }}</p>
        </div>

        <div class="detail-section">
          <h3>
            附件（{{ attachments.length }}）
            <label class="btn btn-sm upload-btn" :class="{ disabled: uploading }">
              {{ uploading ? "上传中…" : "上传附件" }}
              <input type="file" multiple hidden :disabled="uploading" @change="onUpload" />
            </label>
          </h3>
          <div v-if="error" class="form-error">{{ error }}</div>
          <ul class="attach-list">
            <li v-for="a in attachments" :key="a.id" class="attach-item">
              <img
                v-if="isImage(a)"
                class="attach-preview"
                :src="api.attachmentUrl(a.id)"
                :alt="a.filename"
              />
              <video
                v-else-if="isVideo(a)"
                class="attach-preview"
                :src="api.attachmentUrl(a.id)"
                controls
              ></video>
              <div class="attach-meta">
                <a :href="api.attachmentUrl(a.id)" :download="a.filename">{{ a.filename }}</a>
                <span class="attach-size">{{ formatSize(a.size) }}</span>
              </div>
              <button class="icon-btn danger" title="删除附件" @click="onDeleteAttachment(a)">✕</button>
            </li>
            <li v-if="!attachments.length" class="attach-empty">暂无附件</li>
          </ul>
        </div>
      </div>
    </aside>
  </div>
</template>
