<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { currentTheme, toggleTheme } from "@/shared/theme";
import { api, subscribeTaskEvents } from "@/grid-app/api/client";
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
import SidebarProjects from "./components/SidebarProjects.vue";
import TaskGrid from "./components/TaskGrid.vue";
import AuthScreen from "./components/users/AuthScreen.vue";
import UserManagementDialog from "./components/users/UserManagementDialog.vue";
import AgentAccessDialog from "./components/users/AgentAccessDialog.vue";
import { useMetaStore } from "./stores/metaStore";
import { useTaskStore } from "./stores/taskStore";
import { useViewStore } from "./stores/viewStore";
import type { Task, TaskStatus, User } from "@/shared/types";
import { setAgentPromptAccess } from "./taskActions";

const tasks = useTaskStore(),
  meta = useMetaStore(),
  view = useViewStore();
const showCreate = ref(false),
  showProjects = ref(false),
  showUsers = ref(false),
  showAgentAccess = ref(false);
const projectDialogName = ref<string | null>(null);
const quickCreating = ref(false);
const currentUser = ref<User | null>(null);
const authLoading = ref(true);
const detailTask = ref<Task | null>(null);
const theme = ref(currentTheme());
const table = ref<InstanceType<typeof TaskGrid>>();
const savedViews = useSavedViewStore();
const activeProject = computed(() =>
  tasks.filters.project.length === 1 ? tasks.filters.project[0] : "",
);
const title = computed(() => activeProject.value || "任务管理");
const isAdmin = computed(() =>
  ["admin", "super_admin"].includes(currentUser.value?.role ?? ""),
);
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
async function quickCreate() {
  if (quickCreating.value) return;
  const project = activeProject.value || meta.projects[0]?.name || "";
  if (!project) {
    notify("请先创建或选择一个项目", "error");
    return;
  }
  quickCreating.value = true;
  try {
    const task = await api.createTask({
      project,
      type: "新增需求",
      description: "",
      note: "",
    });
    onCreated(task);
    notify("任务已创建", "success");
  } catch (e) {
    notify(errorText(e), "error");
  } finally {
    quickCreating.value = false;
  }
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
function openProjectCreate() {
  projectDialogName.value = null;
  showProjects.value = true;
}
function openProjectSettings(name: string) {
  projectDialogName.value = name;
  showProjects.value = true;
}
function closeProjects() {
  showProjects.value = false;
  projectDialogName.value = null;
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
async function startWorkspace(user: User) {
  currentUser.value = user;
  void meta.refresh();
  void tasks.refresh();
  try {
    setAgentPromptAccess(await api.getAgentAccess());
  } catch {
    setAgentPromptAccess();
  }
  unsubscribe?.();
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
}

async function initialize() {
  try {
    await startWorkspace(await api.me());
  } catch {
    currentUser.value = null;
  } finally {
    authLoading.value = false;
  }
}

async function authenticated(user: User) {
  authLoading.value = true;
  await startWorkspace(user);
  authLoading.value = false;
}

async function logout() {
  try {
    await api.logout();
  } finally {
    unsubscribe?.();
    unsubscribe = undefined;
    currentUser.value = null;
    showProjects.value = false;
    showUsers.value = false;
    showAgentAccess.value = false;
    setAgentPromptAccess();
  }
}

onMounted(() => {
  void initialize();
  document.addEventListener("keydown", keyboard);
});
onBeforeUnmount(() => {
  unsubscribe?.();
  clearTimeout(metaTimer);
  document.removeEventListener("keydown", keyboard);
});
</script>
<template>
  <div v-if="authLoading" class="auth-screen">
    <div class="auth-loading"><UiIcon name="layers" :size="28" />正在检查登录状态…</div>
  </div>
  <AuthScreen v-else-if="!currentUser" @authenticated="authenticated" />
  <div v-else class="workspace" :class="{ 'sidebar-collapsed': view.collapsed }">
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
        <SidebarProjects
          :active-project="activeProject"
          :is-admin="isAdmin"
          @select="tasks.setProject"
          @create="openProjectCreate"
          @edit="openProjectSettings"
        />
      </div>
      <div class="sidebar-footer">
        <button v-if="isAdmin" class="nav-item" title="用户管理" @click="showUsers = true">
          <UiIcon name="user" /><span>用户管理</span></button
        ><button class="nav-item" title="Agent 访问" @click="showAgentAccess = true">
          <UiIcon name="bot" /><span>我的 Agent 访问</span></button
        ><button
          class="nav-item"
          :title="theme === 'dark' ? '切换浅色主题' : '切换深色主题'"
          @click="theme = toggleTheme()"
        >
          <UiIcon :name="theme === 'dark' ? 'sun' : 'moon'" /><span>{{
            theme === "dark" ? "浅色模式" : "深色模式"
          }}</span>
        </button>
        <button class="local-profile profile-button" title="退出登录" @click="logout">
          <span class="profile-avatar">{{ currentUser.username.slice(0, 1) }}</span>
          <div><strong>{{ currentUser.username }}</strong><span>{{ currentUser.role }} · 点击退出</span></div>
        </button>
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
        :quick-creating="quickCreating"
        @open-detail="detailTask = $event"
        @create="showCreate = true"
        @quick-create="quickCreate"
      />
    </main>
    <TaskCreateModal
      v-if="showCreate"
      @close="showCreate = false"
      @created="onCreated"
      @manage-projects="openProjectCreate"
    />
    <TaskDetailDrawer
      v-if="detailTask"
      :task="detailTask"
      @close="detailTask = null"
      @navigate="detailTask = $event"
    />
    <ProjectOptionPopover
      v-if="showProjects && isAdmin"
      :project-name="projectDialogName"
      @close="closeProjects"
      @renamed="renamed"
    />
    <UserManagementDialog
      v-if="showUsers && isAdmin"
      :current-user="currentUser"
      @close="showUsers = false"
    />
    <AgentAccessDialog
      v-if="showAgentAccess"
      :user="currentUser"
      @close="showAgentAccess = false"
      @user-changed="currentUser = $event"
      @access-changed="setAgentPromptAccess($event)"
    />
    <AppFeedback />
  </div>
</template>
