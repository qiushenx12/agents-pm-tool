<script setup lang="ts">
import { computed, ref } from "vue";
import UiDialog from "@/shared/UiDialog.vue";
import UiIcon from "@/shared/UiIcon.vue";
import { askConfirm, errorText, notify } from "@/shared/feedback";
import { api } from "../api/client";
import { useMetaStore } from "../stores/metaStore";
import { useTaskStore } from "../stores/taskStore";

const props = defineProps<{ projectName?: string | null }>();
const emit = defineEmits<{
  close: [];
  renamed: [oldName: string, newName: string];
}>();
const meta = useMetaStore();
const tasks = useTaskStore();
const colors = [
  "#3370ff",
  "#34a874",
  "#f5a623",
  "#e85b50",
  "#9162db",
  "#6672dc",
  "#29a6a6",
  "#8d7866",
  "#8f959e",
  "#d861a9",
];
const current = props.projectName
  ? meta.projects.find((project) => project.name === props.projectName)
  : undefined;
const editing = !!props.projectName;
const initial = {
  name: current?.name ?? "",
  color:
    current?.color ?? colors[meta.projects.length % colors.length] ?? colors[0],
  localPath: current?.local_path ?? "",
  gitUrl: current?.git_url ?? "",
};
const name = ref(initial.name);
const color = ref(initial.color);
const localPath = ref(initial.localPath);
const gitUrl = ref(initial.gitUrl);
const busy = ref(false);
const error = ref(
  editing && !current ? "项目不存在或已经被删除，请关闭后重试" : "",
);
const dirty = computed(
  () =>
    name.value !== initial.name ||
    color.value !== initial.color ||
    localPath.value !== initial.localPath ||
    gitUrl.value !== initial.gitUrl,
);

async function browse() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const result = await api.pickFolder();
    if (result?.path) localPath.value = result.path;
  } catch (e) {
    error.value = errorText(e);
  } finally {
    busy.value = false;
  }
}

async function save() {
  if (busy.value) return;
  const nextName = name.value.trim();
  if (!nextName) {
    error.value = "请输入项目名称";
    return;
  }
  if (editing && !current) return;
  busy.value = true;
  error.value = "";
  try {
    if (editing) {
      await api.patchProject(current!.name, {
        new_name: nextName,
        color: color.value,
        local_path: localPath.value.trim(),
        git_url: gitUrl.value.trim(),
      });
      if (nextName !== current!.name)
        emit("renamed", current!.name, nextName);
      notify("项目设置已保存", "success");
    } else {
      await api.createProject({
        name: nextName,
        color: color.value,
        local_path: localPath.value.trim() || undefined,
        git_url: gitUrl.value.trim() || undefined,
      });
      notify("项目已创建", "success");
    }
    await meta.refresh();
    tasks.scheduleRefresh();
    emit("close");
  } catch (e) {
    error.value = errorText(e);
  } finally {
    busy.value = false;
  }
}

async function remove() {
  if (!current || busy.value) return;
  if (
    !(await askConfirm(
      "删除项目",
      "确定删除「" + current.name + "」？包含任务的项目需要先处理其中的任务。",
      "删除项目",
      true,
    ))
  )
    return;
  busy.value = true;
  error.value = "";
  try {
    await api.deleteProject(current.name);
    tasks.filters.project = tasks.filters.project.filter(
      (project) => project !== current.name,
    );
    await meta.refresh();
    tasks.scheduleRefresh();
    notify("项目已删除", "success");
    emit("close");
  } catch (e) {
    error.value = errorText(e);
  } finally {
    busy.value = false;
  }
}

async function close() {
  if (busy.value) return;
  if (
    dirty.value &&
    !(await askConfirm(
      editing ? "放弃项目更改" : "放弃新建项目",
      "尚未保存的项目信息将被丢弃。",
      "放弃",
    ))
  )
    return;
  emit("close");
}
</script>

<template>
  <UiDialog
    :title="editing ? '项目设置' : '新建项目'"
    :width="560"
    :busy="busy"
    @close="close"
  >
    <div class="project-dialog-hero">
      <span class="project-dialog-icon" :style="{ color }">
        <UiIcon name="folder" :size="24" />
      </span>
      <div>
        <span>{{ editing ? "管理当前项目" : "创建新的工作空间" }}</span>
        <strong>{{ name.trim() || "未命名项目" }}</strong>
        <p>
          {{
            editing
              ? "修改项目标识和代码位置，任务会继续保留在该项目中。"
              : "创建后会显示在左侧项目列表中。"
          }}
        </p>
      </div>
    </div>

    <form
      id="project-settings-form"
      class="project-settings-form"
      @submit.prevent="save"
    >
      <label class="form-field">
        <span>项目名称</span>
        <input
          v-model="name"
          class="input"
          aria-label="项目名称"
          placeholder="输入项目名称"
          data-autofocus
          :disabled="busy || (editing && !current)"
        />
      </label>

      <fieldset
        class="project-color-field"
        :disabled="busy || (editing && !current)"
      >
        <legend>项目颜色</legend>
        <div class="color-palette project-color-palette">
          <button
            v-for="option in colors"
            :key="option"
            type="button"
            class="color-swatch"
            :class="{ selected: color === option }"
            :aria-label="'选择颜色 ' + option"
            :aria-pressed="color === option"
            :style="{ background: option }"
            @click="color = option"
          >
            <UiIcon v-if="color === option" name="check" :size="13" />
          </button>
        </div>
      </fieldset>

      <label class="form-field">
        <span>本地目录 <small>可选</small></span>
        <div class="project-path-input">
          <input
            v-model="localPath"
            class="input"
            aria-label="项目本地路径"
            placeholder="如 D:\code\my-project"
            :disabled="busy || (editing && !current)"
          />
          <button
            type="button"
            class="btn"
            :disabled="busy || (editing && !current)"
            @click="browse"
          >
            <UiIcon name="folder" :size="15" />选择目录
          </button>
        </div>
        <span class="form-hint">用于从 Agent 任务中快速定位项目代码。</span>
      </label>

      <label class="form-field">
        <span>Git 远端地址 <small>可选</small></span>
        <input
          v-model="gitUrl"
          class="input"
          aria-label="项目 Git 地址"
          placeholder="如 git@github.com:org/repo.git"
          :disabled="busy || (editing && !current)"
        />
      </label>

      <div v-if="error || meta.error" class="form-error" role="alert">
        {{ error || meta.error }}
      </div>

      <section v-if="editing && current" class="project-danger-zone">
        <div>
          <strong>删除项目</strong>
          <p>只有未被任务引用的项目才能删除。</p>
        </div>
        <button
          type="button"
          class="btn btn-danger"
          :disabled="busy"
          @click="remove"
        >
          <UiIcon name="trash" :size="14" />删除项目
        </button>
      </section>
    </form>

    <template #footer>
      <button class="btn" :disabled="busy" @click="close">取消</button>
      <button
        class="btn btn-primary"
        type="submit"
        form="project-settings-form"
        :disabled="busy || !name.trim() || (editing && !current)"
      >
        <UiIcon :name="editing ? 'check' : 'plus'" :size="15" />{{
          editing ? "保存设置" : "创建项目"
        }}
      </button>
    </template>
  </UiDialog>
</template>
