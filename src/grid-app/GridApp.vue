<script setup lang="ts">
import { invoke, isTauri } from "@tauri-apps/api/core";
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  applyTheme,
  currentTheme,
  reconcileTheme,
  toggleTheme,
  type ThemeTransport,
} from "@/shared/theme";
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
import HostSettingsDialog from "./components/users/HostSettingsDialog.vue";
import { useMetaStore } from "./stores/metaStore";
import { useTaskStore } from "./stores/taskStore";
import { useViewStore } from "./stores/viewStore";
import type { Task, User } from "@/shared/types";
import { setAgentPromptAccess } from "./taskActions";
import { openWorkspaceSettings } from "./settingsNavigation";

const tasks = useTaskStore(),
  meta = useMetaStore(),
  view = useViewStore();
const showCreate = ref(false),
  showProjects = ref(false),
  showUsers = ref(false),
  showAgentAccess = ref(false),
  showHostSettings = ref(false);
const projectDialogName = ref<string | null>(null);
const quickCreating = ref(false);
const currentUser = ref<User | null>(null);
const authLoading = ref(true);
const detailTask = ref<Task | null>(null);
const desktop = isTauri();
const theme = ref(currentTheme());
/** 全局主题传输层：网页端走 HTTP（GET 公开、PUT 需登录） */
const themeTransport: ThemeTransport = {
  fetch: async () => {
    const t = (await api.getAppearance()).theme;
    return t === "light" || t === "dark" ? t : null;
  },
  push: (t) => api.putAppearance(t),
};
/** 与全局主题对账：有值则应用，没有则上报本机偏好；失败静默，下次再试。 */
async function syncTheme() {
  const t = await reconcileTheme(themeTransport);
  if (t) theme.value = t;
}
/** 其他界面切了主题（SSE 带值下发）→ 直接应用 */
function applyRemoteTheme(value: string | null) {
  if (value === "light" || value === "dark") {
    applyTheme(value);
    theme.value = value;
  }
}
/** 本地切换：先应用，再上报全局（设置窗口会实时跟着变） */
async function switchTheme() {
  const next = toggleTheme();
  theme.value = next;
  try {
    await api.putAppearance(next);
  } catch (e) {
    notify(errorText(e), "error");
  }
}
const table = ref<InstanceType<typeof TaskGrid>>();
const savedViews = useSavedViewStore();
const activeProject = computed(() =>
  tasks.filters.project.length === 1 ? tasks.filters.project[0] : "",
);
const title = computed(() => activeProject.value || "任务管理");
const isAdmin = computed(() =>
  ["admin", "super_admin"].includes(currentUser.value?.role ?? ""),
);
/** 侧栏收成图标栏后标题（折叠开关）会被隐藏，项目列表就保持可见。 */
const projectsVisible = computed(() => view.projectsOpen || view.collapsed);
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

/** 系统浏览器显示 Web 弹窗；应用内窗口则关闭自身并切回桌面设置窗口。 */
async function openHostSettings() {
  try {
    await openWorkspaceSettings(
      desktop,
      () => {
        showHostSettings.value = true;
      },
      () => invoke("open_settings_window"),
    );
  } catch (cause) {
    notify(errorText(cause), "error");
  }
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
  // 登录后再对账一次主题：未登录时 PUT 被拦，迁移在这里补齐
  void syncTheme();
  // 先与服务端对账视图设置，再拉列表：这样打开时看到的就是两端共用的分组/排序/筛选。
  await tasks.syncViewState();
  void tasks.refresh();
  try {
    setAgentPromptAccess(await api.getAgentAccess());
  } catch {
    setAgentPromptAccess();
  }
  unsubscribe?.();
  unsubscribe = subscribeTaskEvents(
    () => {
      // 顺带对账主题：SSE 断线重连/轮询期间错过的主题变化也靠这里补齐
      void syncTheme();
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
    applyRemoteTheme,
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
    tasks.resetViewStateSync();
    // 视图设置跟账号走：别把上一个人的分组/排序/筛选带给下一个登录的人
    tasks.applyFilters({});
    currentUser.value = null;
    showProjects.value = false;
    showUsers.value = false;
    showAgentAccess.value = false;
    showHostSettings.value = false;
    setAgentPromptAccess();
  }
}

/** 修改服务端口后，等 graceful restart 交还监听端口，再把主机网页接到新地址。 */
function reconnectToSettingsPort(port: number) {
  showHostSettings.value = false;
  window.setTimeout(() => {
    const target = new URL(window.location.href);
    target.hostname = "127.0.0.1";
    target.port = String(port);
    window.location.assign(target);
  }, 1200);
}

onMounted(() => {
  void initialize();
  // 未登录也先拉全局主题（GET 公开），登录页就能正确着色
  void syncTheme();
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
      <div class="sidebar-module">
        <div class="sidebar-module-head">
          <button
            type="button"
            class="module-toggle"
            :aria-expanded="view.projectsOpen"
            aria-controls="sidebar-projects"
            :title="view.projectsOpen ? '收起项目' : '展开项目'"
            @click="view.projectsOpen = !view.projectsOpen"
          >
            Agents PM Tool
          </button>
          <button
            v-if="isAdmin"
            type="button"
            class="icon-btn"
            aria-label="新建项目"
            title="新建项目"
            @click="openProjectCreate"
          >
            <UiIcon name="plus" :size="14" />
          </button>
        </div>
        <div
          id="sidebar-projects"
          v-show="projectsVisible"
          class="sidebar-content"
        >
          <SidebarProjects
            :active-project="activeProject"
            :is-admin="isAdmin"
            @select="tasks.setProject"
            @create="openProjectCreate"
            @edit="openProjectSettings"
          />
        </div>
      </div>
      <div class="sidebar-footer">
        <button
          v-if="currentUser.is_host"
          class="nav-item"
          title="工作区设置"
          @click="openHostSettings"
        >
          <UiIcon name="settings" /><span>工作区设置</span>
        </button>
        <button v-if="isAdmin" class="nav-item" title="用户管理" @click="showUsers = true">
          <UiIcon name="user" /><span>用户管理</span></button
        ><button class="nav-item" title="Agent 访问" @click="showAgentAccess = true">
          <UiIcon name="bot" /><span>我的 Agent 访问</span></button
        ><button
          class="nav-item"
          :title="theme === 'dark' ? '切换浅色主题' : '切换深色主题'"
          @click="switchTheme()"
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
          ><span>Agents PM Tool</span><UiIcon name="right" :size="12" /><span>{{
            title
          }}</span>
        </div>
        <SavedViews />
      </header>
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
    <HostSettingsDialog
      v-if="showHostSettings && currentUser.is_host"
      @close="showHostSettings = false"
      @restarting="reconnectToSettingsPort"
    />
    <AppFeedback />
  </div>
</template>
