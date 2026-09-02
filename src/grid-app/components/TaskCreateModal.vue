<script setup lang="ts">
import { ref } from "vue";
import { api } from "@/grid-app/api/client";
import { useMetaStore } from "@/grid-app/stores/metaStore";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import type { TaskType } from "@/shared/types";

const emit = defineEmits<{ close: []; created: [] }>();

const metaStore = useMetaStore();
const taskStore = useTaskStore();

const project = ref(metaStore.projects[0]?.name ?? "");
const type = ref<TaskType>("新增需求");
const description = ref("");
const files = ref<File[]>([]);
const error = ref("");
const submitting = ref(false);

function onFiles(e: Event) {
  files.value = Array.from((e.target as HTMLInputElement).files ?? []);
}

async function submit() {
  error.value = "";
  if (!project.value) {
    error.value = "请选择项目";
    return;
  }
  submitting.value = true;
  try {
    const task = await api.createTask({
      project: project.value,
      type: type.value,
      description: description.value,
    });
    for (const f of files.value) {
      await api.uploadAttachment(task.id, f);
    }
    await taskStore.refresh();
    emit("created");
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <div class="modal-mask" @click.self="emit('close')">
    <div class="modal">
      <h2 class="modal-title">新建任务</h2>

      <label class="form-label">项目 *</label>
      <select v-model="project" class="input">
        <option v-for="p in metaStore.projects" :key="p.name" :value="p.name">
          {{ p.name }}
        </option>
      </select>

      <label class="form-label">任务类型 *</label>
      <select v-model="type" class="input">
        <option v-for="t in metaStore.taskTypes" :key="t" :value="t">{{ t }}</option>
      </select>

      <label class="form-label">任务描述</label>
      <textarea v-model="description" class="input" rows="5" placeholder="描述任务内容…"></textarea>

      <label class="form-label">附件</label>
      <input type="file" multiple @change="onFiles" />
      <div v-if="files.length" class="file-list">
        <div v-for="f in files" :key="f.name" class="file-item">{{ f.name }}</div>
      </div>

      <div v-if="error" class="form-error">{{ error }}</div>

      <div class="modal-actions">
        <button class="btn" @click="emit('close')">取消</button>
        <button class="btn btn-primary" :disabled="submitting" @click="submit">
          {{ submitting ? "创建中…" : "创建" }}
        </button>
      </div>
    </div>
  </div>
</template>
