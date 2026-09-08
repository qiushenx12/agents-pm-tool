<script setup lang="ts">
import { nextTick, ref } from "vue";
import { api } from "../api/client";
import { useMetaStore } from "../stores/metaStore";
import { useTaskStore } from "../stores/taskStore";
import UiDialog from "@/shared/UiDialog.vue";
import UiIcon from "@/shared/UiIcon.vue";
import UiPopover from "@/shared/UiPopover.vue";
import { askConfirm, errorText } from "@/shared/feedback";
const emit = defineEmits<{
  close: [];
  renamed: [oldName: string, newName: string];
}>();
const meta = useMetaStore(),
  tasks = useTaskStore();
const newName = ref(""),
  newPath = ref(""),
  newGit = ref(""),
  editing = ref(""),
  draft = ref(""),
  editingGit = ref(""),
  gitDraft = ref(""),
  error = ref(""),
  busy = ref(false);
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
async function run(action: () => Promise<unknown>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await action();
    await meta.refresh();
    tasks.scheduleRefresh();
  } catch (e) {
    error.value = errorText(e);
  } finally {
    busy.value = false;
  }
}
function create() {
  const name = newName.value.trim();
  if (!name) return;
  void run(async () => {
    await api.createProject({
      name,
      color: colors[meta.projects.length % colors.length],
      local_path: newPath.value.trim() || undefined,
      git_url: newGit.value.trim() || undefined,
    });
    newName.value = "";
    newPath.value = "";
    newGit.value = "";
  });
}
/** 弹系统文件夹选择框：新项目草稿填输入框，已有项目直接保存 */
function browse(target?: string) {
  void run(async () => {
    const r = await api.pickFolder();
    if (!r?.path) return;
    if (target) await api.patchProject(target, { local_path: r.path });
    else newPath.value = r.path;
  });
}
function startRename(name: string) {
  editing.value = name;
  draft.value = name;
  void nextTick(() =>
    document.querySelector<HTMLInputElement>("#project-rename")?.focus(),
  );
}
function startEditGit(project: { name: string; git_url: string }) {
  editingGit.value = project.name;
  gitDraft.value = project.git_url;
  void nextTick(() =>
    document.querySelector<HTMLInputElement>("#project-git-url")?.focus(),
  );
}
/** 保存 git 地址；清空输入即清除该项目的 git 地址 */
async function saveGit() {
  const name = editingGit.value,
    url = gitDraft.value.trim();
  if (!name) return;
  const current = meta.projects.find((p) => p.name === name)?.git_url ?? "";
  if (url === current) {
    editingGit.value = "";
    return;
  }
  await run(async () => {
    await api.patchProject(name, { git_url: url });
    editingGit.value = "";
  });
}
async function rename() {
  const old = editing.value,
    name = draft.value.trim();
  if (!old || !name || old === name) {
    editing.value = "";
    return;
  }
  await run(async () => {
    await api.patchProject(old, { new_name: name });
    editing.value = "";
    emit("renamed", old, name);
  });
}
async function move(index: number, direction: number) {
  const ordered = [...meta.projects];
  const destination = index + direction;
  if (destination < 0 || destination >= ordered.length) return;
  [ordered[index], ordered[destination]] = [
    ordered[destination],
    ordered[index],
  ];
  await run(async () => {
    for (let i = 0; i < ordered.length; i++)
      await api.patchProject(ordered[i].name, { sort_order: i });
  });
}
async function remove(name: string) {
  if (
    !(await askConfirm(
      "删除项目",
      "确定删除「" + name + "」？包含任务的项目需要先处理其中的任务。",
      "删除项目",
      true,
    ))
  )
    return;
  await run(async () => {
    await api.deleteProject(name);
    tasks.filters.project = tasks.filters.project.filter((p) => p !== name);
  });
}
async function close() {
  if (busy.value) return;
  if (
    (newName.value.trim() ||
      newPath.value.trim() ||
      newGit.value.trim() ||
      (editing.value && draft.value !== editing.value)) &&
    !(await askConfirm("关闭项目管理", "未保存的项目信息将被丢弃。", "关闭"))
  )
    return;
  emit("close");
}
</script>
<template>
  <UiDialog title="项目管理" :width="600" :busy="busy" @close="close">
    <p class="project-intro">
      按项目组织任务。项目名称和颜色会同步显示在任务表中。
    </p>
    <form class="project-create" @submit.prevent="create">
      <div class="project-create-row">
        <input
          v-model="newName"
          class="input"
          aria-label="新项目名称"
          placeholder="输入新项目名称"
          data-autofocus
          :disabled="busy"
        /><button class="btn btn-primary" :disabled="busy || !newName.trim()">
          <UiIcon name="plus" :size="15" />创建项目
        </button>
      </div>
      <div class="project-create-row">
        <input
          v-model="newPath"
          class="input"
          aria-label="新项目本地路径"
          placeholder="本地路径（可选），如 D:\code\my-project"
          :disabled="busy"
        /><button
          type="button"
          class="btn"
          :disabled="busy"
          title="选择文件夹"
          @click="browse()"
        >
          <UiIcon name="folder" :size="15" />选择文件夹
        </button>
      </div>
      <div class="project-create-row">
        <input
          v-model="newGit"
          class="input"
          aria-label="新项目 Git 地址"
          placeholder="Git 远端地址（可选），如 git@github.com:org/repo.git"
          :disabled="busy"
        />
      </div>
    </form>
    <div v-if="error || meta.error" class="form-error" role="alert">
      {{ error || meta.error }}
    </div>
    <div class="project-list-heading">
      <span>项目名称</span><span>{{ meta.projects.length }} 个项目</span>
    </div>
    <ul class="project-list">
      <li
        v-for="(project, index) in meta.projects"
        :key="project.name"
        class="project-item"
      >
        <UiPopover :width="198" label="项目颜色"
          ><template #trigger="{ toggle }"
            ><button
              class="project-color-button"
              :aria-label="'修改颜色：' + project.name"
              :disabled="busy"
              @click="toggle"
            >
              <UiIcon
                name="folder"
                :size="19"
                :style="{ color: project.color }"
              /></button></template
          ><template #default="{ close: closeMenu }"
            ><div class="menu-caption">项目颜色</div>
            <div class="color-palette">
              <button
                v-for="color in colors"
                :key="color"
                class="color-swatch"
                :aria-label="'颜色 ' + color"
                :aria-pressed="color === project.color"
                :style="{ background: color }"
                @click="
                  closeMenu();
                  run(() => api.patchProject(project.name, { color }));
                "
              >
                <UiIcon
                  v-if="color === project.color"
                  name="check"
                  :size="13"
                />
              </button></div></template
        ></UiPopover>
        <form
          v-if="editing === project.name"
          class="project-rename-form"
          @submit.prevent="rename"
        >
          <input
            id="project-rename"
            v-model="draft"
            class="input"
            aria-label="项目新名称"
            :disabled="busy"
            @keydown.esc.prevent.stop="editing = ''"
          /><button class="icon-btn" aria-label="保存项目名称" :disabled="busy">
            <UiIcon name="check" /></button
          ><button
            type="button"
            class="icon-btn"
            aria-label="取消重命名"
            :disabled="busy"
            @click="editing = ''"
          >
            <UiIcon name="close" />
          </button>
        </form>
        <div v-else class="project-item-main">
          <span class="project-item-name" :title="project.name">{{
            project.name
          }}</span>
          <form
            v-if="editingGit === project.name"
            class="project-git-form"
            @submit.prevent="saveGit"
          >
            <input
              id="project-git-url"
              v-model="gitDraft"
              class="input"
              aria-label="项目 Git 地址"
              placeholder="Git 远端地址，留空清除"
              :disabled="busy"
              @keydown.esc.prevent.stop="editingGit = ''"
            /><button
              class="icon-btn"
              aria-label="保存 Git 地址"
              :disabled="busy"
            >
              <UiIcon name="check" /></button
            ><button
              type="button"
              class="icon-btn"
              aria-label="取消编辑 Git 地址"
              :disabled="busy"
              @click="editingGit = ''"
            >
              <UiIcon name="close" />
            </button>
          </form>
          <template v-else>
            <span
              v-if="project.local_path"
              class="project-item-path"
              :title="project.local_path"
              >{{ project.local_path }}</span
            >
            <span
              v-if="project.git_url"
              class="project-item-path"
              :title="project.git_url"
              >{{ project.git_url }}</span
            >
          </template>
        </div>
        <div v-if="editing !== project.name" class="inline-actions">
          <button
            class="icon-btn"
            :aria-label="'编辑 Git 地址：' + project.name"
            title="编辑 Git 地址"
            :disabled="busy"
            @click="startEditGit(project)"
          >
            <UiIcon name="git" :size="14" /></button
          ><button
            class="icon-btn"
            :aria-label="'选择本地路径：' + project.name"
            title="选择本地路径"
            :disabled="busy"
            @click="browse(project.name)"
          >
            <UiIcon name="folder" :size="14" /></button
          ><button
            class="icon-btn"
            :aria-label="'重命名：' + project.name"
            title="重命名"
            :disabled="busy"
            @click="startRename(project.name)"
          >
            <UiIcon name="edit" :size="14" /></button
          ><button
            class="icon-btn"
            aria-label="上移项目"
            title="上移"
            :disabled="busy || index === 0"
            @click="move(index, -1)"
          >
            <UiIcon name="up" :size="14" /></button
          ><button
            class="icon-btn"
            aria-label="下移项目"
            title="下移"
            :disabled="busy || index === meta.projects.length - 1"
            @click="move(index, 1)"
          >
            <UiIcon name="down" :size="14" /></button
          ><button
            class="icon-btn danger"
            :aria-label="'删除项目：' + project.name"
            title="删除项目"
            :disabled="busy"
            @click="remove(project.name)"
          >
            <UiIcon name="trash" :size="14" />
          </button>
        </div>
      </li>
    </ul>
    <p v-if="!meta.projects.length" class="menu-empty">
      还没有项目，创建一个开始记录任务。
    </p>
    <template #footer
      ><span class="form-footer-hint">名称和颜色自动同步到已有任务</span
      ><button class="btn" :disabled="busy" @click="close">
        完成
      </button></template
    >
  </UiDialog>
</template>
