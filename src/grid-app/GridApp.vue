<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { currentTheme, toggleTheme } from "@/shared/theme";
import { subscribeTaskEvents } from "@/grid-app/api/client";
import UiIcon from "@/shared/UiIcon.vue";
import AppFeedback from "@/shared/AppFeedback.vue";
import FilterBar from "./components/FilterBar.vue";
import SavedViews from "./components/SavedViews.vue";
import BulkActions from "./components/BulkActions.vue";
import { useSavedViewStore } from "./stores/savedViewStore";
import { errorText, notify } from "@/shared/feedback";
import TaskCreateModal from "./components/TaskCreateModal.vue";
import TaskDetailDrawer from "./components/TaskDetailDrawer.vue";
import ProjectOptionPopover from "./components/ProjectOptionPopover.vue";
import TaskGrid from "./components/TaskGrid.vue";
import { useMetaStore } from "./stores/metaStore";
import { useTaskStore } from "./stores/taskStore";
import { useViewStore } from "./stores/viewStore";
import type { Task, TaskStatus } from "@/shared/types";

const tasks = useTaskStore(),
  meta = useMetaStore(),
  view = useViewStore();
const showCreate = ref(false),
  showProjects = ref(false);
const detailTask = ref<Task | null>(null);
const theme = ref(currentTheme());
const table = ref<InstanceType<typeof TaskGrid>>();
const savedViews = useSavedViewStore();
const activeProject = computed(() =>
  tasks.filters.project.length === 1 ? tasks.filters.project[0] : "",
);
const title = computed(() => activeProject.value || "任务管理");
const presets: { label: string; icon: string; status?: TaskStatus }[] = [
  { label: "全部任务", icon: "grid" },
  { label: "待验证", icon: "review", status: "待验证" },
  { label: "验收未通过", icon: "circle", status: "验收未通过" },
];
function presetActive(status?: TaskStatus) {
  const f = tasks.filters;
  return (
    !f.project.length &&
    !f.type.length &&
    !f.submitter.length &&
    !f.keyword &&
    (status
      ? f.status.length === 1 && f.status[0] === status
      : !f.status.length)
  );
}
function onCreated(task: Task) {
  showCreate.value = false;
  const latest = tasks.records[task.id] ?? task;
  tasks.acceptTask(latest);
  table.value?.reveal(task.id);
}
function renamed(oldName: string, newName: string) {
  try {
    savedViews.renameProject(oldName, newName);
  } catch (e) {
    notify(errorText(e), "error");
  }
  tasks.filters.project = tasks.filters.project.map((p) =>
    p === oldName ? newName : p,
  );
}
let unsubscribe: (() => void) | undefined;
let metaTimer: ReturnType<typeof setTimeout> | undefined;
function keyboard(e: KeyboardEvent) {
  if (
    (e.ctrlKey || e.metaKey) &&
    e.key.toLowerCase() === "k" &&
    !document.querySelector('[role="dialog"]')
  ) {
    e.preventDefault();
    document.querySelector<HTMLInputElement>("#task-search")?.focus();
  }
}
onMounted(() => {
  void meta.refresh();
  void tasks.refresh();
  unsubscribe = subscribeTaskEvents(
    () => {
      tasks.externalRevision++;
      tasks.scheduleRefresh();
      clearTimeout(metaTimer);
      metaTimer = setTimeout(() => {
        void meta.refresh();
      }, 180);
    },
    (state) => {
      tasks.connection = state;
    },
  );
  document.addEventListener("keydown", keyboard);
});
onBeforeUnmount(() => {
  unsubscribe?.();
  clearTimeout(metaTimer);
  document.removeEventListener("keydown", keyboard);
});
</script>
<template>
  <div class="workspace" :class="{ 'sidebar-collapsed': view.collapsed }">
    <aside class="workspace-sidebar" aria-label="工作区导航">
      <div class="brand">
        <span class="brand-mark"><UiIcon name="layers" :size="22" /></span>
        <div class="brand-copy">
          <strong>Agents PM</strong><span>项目与任务工作台</span>
        </div>
        <button
          class="icon-btn collapse-control"
          aria-label="收起侧栏"
          title="收起侧栏"
          @click="view.collapsed = true"
        >
          <UiIcon name="sidebar" />
        </button>
      </div>
      <div class="sidebar-content">
        <div class="nav-section-label">工作空间</div>
        <nav class="workspace-nav">
          <button
            v-for="preset in presets"
            :key="preset.label"
            class="nav-item"
            :class="{ active: presetActive(preset.status) }"
            :aria-current="presetActive(preset.status) ? 'page' : undefined"
            :title="preset.label"
            @click="tasks.setPreset(preset.status)"
          >
            <UiIcon :name="preset.icon" /><span>{{ preset.label }}</span>
          </button>
        </nav>
        <div class="nav-section-label project-section-label">
          <span>项目</span
          ><button
            class="icon-btn"
            aria-label="管理项目"
            title="管理项目"
            @click="showProjects = true"
          >
            <UiIcon name="plus" :size="14" />
          </button>
        </div>
        <nav class="workspace-nav project-nav">
          <button
            v-for="project in meta.projects"
            :key="project.name"
            class="nav-item"
            :class="{ active: activeProject === project.name }"
            :aria-current="activeProject === project.name ? 'page' : undefined"
            :title="project.name"
            @click="tasks.setProject(project.name)"
          >
            <span class="project-symbol" :style="{ color: project.color }"
              ><UiIcon name="folder" /></span
            ><span class="nav-label">{{ project.name }}</span>
          </button>
        </nav>
        <button
          v-if="!meta.projects.length && !meta.error"
          class="nav-item subtle"
          @click="showProjects = true"
        >
          <UiIcon name="plus" /><span>创建第一个项目</span>
        </button>
      </div>
      <div class="sidebar-footer">
        <button class="nav-item" title="项目管理" @click="showProjects = true">
          <UiIcon name="settings" /><span>项目管理</span></button
        ><button
          class="nav-item"
          :title="theme === 'dark' ? '切换浅色主题' : '切换深色主题'"
          @click="theme = toggleTheme()"
        >
          <UiIcon :name="theme === 'dark' ? 'sun' : 'moon'" /><span>{{
            theme === "dark" ? "浅色模式" : "深色模式"
          }}</span>
        </button>
        <div class="local-profile">
          <span class="profile-avatar">本</span>
          <div><strong>本地工作区</strong><span>Agents PM Tool</span></div>
        </div>
      </div>
    </aside>
    <main class="workspace-main">
      <header class="workspace-header">
        <div class="breadcrumb">
          <button
            class="icon-btn"
            :aria-label="view.collapsed ? '展开侧栏' : '收起侧栏'"
            :title="view.collapsed ? '展开侧栏' : '收起侧栏'"
            @click="view.collapsed = !view.collapsed"
          >
            <UiIcon name="sidebar" /></button
          ><span>工作空间</span><UiIcon name="right" :size="12" /><span>{{
            title
          }}</span>
        </div>
        <div class="page-heading">
          <div class="page-identity">
            <span class="page-icon"><UiIcon name="grid" :size="23" /></span>
            <div>
              <h1>{{ title }}</h1>
              <p>
                {{
                  activeProject
                    ? "项目任务与进展，集中在这里"
                    : "让每一项需求、问题与进展都有迹可循"
                }}
              </p>
            </div>
          </div>
          <div class="connection-state" :class="tasks.connection" role="status">
            <span class="connection-dot"></span
            >{{
              tasks.connection === "live"
                ? "实时同步已连接"
                : tasks.connection === "reconnecting"
                  ? "连接恢复中"
                  : "正在连接"
            }}
          </div>
        </div>
      </header>
      <SavedViews />
      <FilterBar @create="showCreate = true" />
      <BulkActions />
      <div
        v-if="tasks.error || meta.error"
        class="workspace-error error-banner"
        role="alert"
      >
        <UiIcon name="info" /><span>{{ tasks.error || meta.error }}</span
        ><button
          class="btn btn-sm"
          @click="
            tasks.refresh();
            meta.refresh();
          "
        >
          重新加载
        </button>
      </div>
      <div
        v-if="tasks.connection === 'reconnecting'"
        class="connection-banner"
        role="status"
      >
        实时连接暂时中断，正在自动重连；每 10 秒检查一次更新。
      </div>
      <TaskGrid
        ref="table"
        @open-detail="detailTask = $event"
        @create="showCreate = true"
      />
    </main>
    <TaskCreateModal
      v-if="showCreate"
      @close="showCreate = false"
      @created="onCreated"
      @manage-projects="showProjects = true"
    />
    <TaskDetailDrawer
      v-if="detailTask"
      :task="detailTask"
      @close="detailTask = null"
      @navigate="detailTask = $event"
    />
    <ProjectOptionPopover
      v-if="showProjects"
      @close="showProjects = false"
      @renamed="renamed"
    />
    <AppFeedback />
  </div>
</template>
