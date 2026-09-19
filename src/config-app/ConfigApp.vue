<script setup lang="ts">
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  applyTheme,
  currentTheme,
  savedLocalTheme,
  toggleTheme,
} from "@/shared/theme";
import UiIcon from "@/shared/UiIcon.vue";
import WorkspaceSettingsForm from "@/shared/WorkspaceSettingsForm.vue";
import AppFeedback from "@/shared/AppFeedback.vue";
import { askConfirm, copyText, errorText, notify } from "@/shared/feedback";
import type {
  WorkspaceServerStatus,
  WorkspaceSettings,
} from "@/shared/types";
interface SaveSettingsResult {
  settings: WorkspaceSettings;
  restarted: boolean;
  port: number;
}
const desktop = isTauri();
const appWindow = desktop ? getCurrentWindow() : null;
// macOS 设置窗口用原生红绿灯（Overlay 标题栏叠在左上角），自绘按钮只留给 Windows；
// WKWebView 的 UA 含 "Mac OS X"，Windows WebView2 含 "Windows NT"，足够区分两端。
const nativeWindowControls =
  typeof navigator !== "undefined" && navigator.userAgent.includes("Mac OS X");
const settings = ref<WorkspaceSettings>({
  port: 17890,
  autostart: true,
  close_behavior: "keep_service",
  listen_scope: "local",
  agent_server_url: "",
});
const status = ref<WorkspaceServerStatus | null>(null);
const baseline = ref(""),
  loaded = ref(false),
  saving = ref(false),
  loading = ref(false),
  tokenBusy = ref(false),
  serviceBusy = ref(false),
  enterBusy = ref(false),
  error = ref("");
const dirty = computed(() => JSON.stringify(settings.value) !== baseline.value);
const theme = ref(currentTheme());
/** 本地切换：先应用，再写全局（网页界面会实时跟着变） */
async function switchTheme() {
  const next = toggleTheme();
  theme.value = next;
  if (!desktop) return; // 浏览器里打开本页：仅本地生效
  try {
    await invoke("set_theme", { theme: next });
  } catch (e) {
    error.value = errorText(e);
  }
}
function isInteractiveTarget(target: unknown) {
  if (typeof target !== "object" || target === null) return false;
  const closest = Reflect.get(target, "closest");
  return (
    typeof closest === "function" &&
    closest.call(
      target,
      "button,a,input,textarea,select,option,[data-tauri-drag-region='false']",
    ) !== null
  );
}
function startTitleBarDrag(event: MouseEvent) {
  if (
    event.button !== 0 ||
    event.detail !== 1 ||
    isInteractiveTarget(event.target)
  )
    return;
  void appWindow?.startDragging().catch(() => {});
}
function handleTitleBarDoubleClick(event: MouseEvent) {
  if (
    event.button !== 0 ||
    event.detail !== 2 ||
    isInteractiveTarget(event.target)
  )
    return;
  void appWindow?.toggleMaximize().catch(() => {});
}
async function load() {
  if (!desktop) return;
  loading.value = true;
  error.value = "";
  try {
    const [config, server] = await Promise.all([
      invoke<WorkspaceSettings>("get_settings"),
      invoke<WorkspaceServerStatus>("get_server_status"),
    ]);
    settings.value = config;
    status.value = server;
    baseline.value = JSON.stringify(config);
    loaded.value = true;
    // 全局主题对账：服务端有值就应用（各界面统一）；
    // 没有则把本机保存的偏好上报（老用户无感迁移）。
    if (config.theme === "light" || config.theme === "dark") {
      applyTheme(config.theme);
      theme.value = config.theme;
    } else {
      const local = savedLocalTheme();
      if (local) {
        theme.value = local;
        void invoke("set_theme", { theme: local }).catch(() => {});
      }
    }
  } catch (e) {
    error.value = errorText(e);
  } finally {
    loading.value = false;
  }
}
async function save() {
  if (!desktop || saving.value || serviceBusy.value) return;
  if (
    !Number.isInteger(settings.value.port) ||
    settings.value.port < 1024 ||
    settings.value.port > 65535
  ) {
    error.value = "请输入 1024–65535 之间的有效端口";
    return;
  }
  saving.value = true;
  error.value = "";
  try {
    const result = await invoke<SaveSettingsResult>("save_settings", {
      settings: settings.value,
    });
    settings.value = result.settings;
    baseline.value = JSON.stringify(result.settings);
    status.value = await invoke<WorkspaceServerStatus>("get_server_status");
    notify(result.restarted ? "设置已保存，服务已重启" : "设置已保存");
  } catch (e) {
    error.value = errorText(e);
  } finally {
    saving.value = false;
  }
}
async function regenerateToken() {
  if (
    !(await askConfirm(
      "重新生成 Agent token",
      "旧 token 将失效，后续 CLI 调用会读取最新凭据。",
      "重新生成",
    ))
  )
    return;
  tokenBusy.value = true;
  error.value = "";
  try {
    await invoke("regenerate_token");
    notify("Agent token 已更新");
  } catch (e) {
    error.value = errorText(e);
  } finally {
    tokenBusy.value = false;
  }
}
async function openWeb() {
  if (!status.value?.url) return;
  try {
    await shellOpen(status.value.url);
  } catch (e) {
    error.value = "打开网页失败：" + errorText(e);
  }
}
async function toggleService() {
  if (!desktop || !loaded.value || serviceBusy.value) return;
  const running = !status.value?.running;
  serviceBusy.value = true;
  error.value = "";
  try {
    status.value = await invoke<WorkspaceServerStatus>("set_server_running", {
      running,
    });
    notify(running ? "服务已启动" : "服务已停止");
  } catch (e) {
    error.value = `${running ? "启动" : "停止"}服务失败：${errorText(e)}`;
    // 命令可能已完成部分操作；重新读取一次，以真实状态为准。
    try {
      status.value = await invoke<WorkspaceServerStatus>("get_server_status");
    } catch {
      // 保留原状态与首个错误，避免二次失败掩盖真正原因。
    }
  } finally {
    serviceBusy.value = false;
  }
}
/** 在应用内打开网页同款界面（独立窗口），不走系统浏览器。 */
async function enterApp() {
  if (!status.value?.running || enterBusy.value || serviceBusy.value) return;
  enterBusy.value = true;
  error.value = "";
  try {
    await invoke("open_app_window");
    // 这里只做界面切换，不走 close()，避免 stop_all 配置把刚打开的应用一起退出。
    await appWindow?.hide();
  } catch (e) {
    error.value = errorText(e);
  } finally {
    enterBusy.value = false;
  }
}
async function closeWindow() {
  if (saving.value || serviceBusy.value) return;
  if (
    loaded.value &&
    dirty.value &&
    !(await askConfirm("关闭设置", "尚未保存的设置将被丢弃。", "关闭"))
  )
    return;
  await appWindow?.close();
}
let unlistenTheme: UnlistenFn | undefined;
onMounted(async () => {
  // 网页端切主题 → 服务端广播到这里，实时换肤
  if (desktop) {
    unlistenTheme = await listen<string | null>("theme-changed", (e) => {
      if (e.payload === "light" || e.payload === "dark") {
        applyTheme(e.payload);
        theme.value = e.payload;
      }
    });
  }
  void load();
});
onBeforeUnmount(() => {
  unlistenTheme?.();
});
</script>
<template>
  <div class="config-app">
    <header
      class="titlebar"
      :class="{ 'titlebar-native-controls': nativeWindowControls }"
      @mousedown="startTitleBarDrag"
      @dblclick="handleTitleBarDoubleClick"
    >
      <div class="titlebar-brand">
        <UiIcon name="layers" :size="17" /><span>Agents PM</span>
      </div>
      <div class="inline-actions">
        <button
          class="icon-btn"
          aria-label="切换明暗主题"
          title="切换明暗主题"
          @click="switchTheme()"
        >
          <UiIcon
            :name="theme === 'dark' ? 'sun' : 'moon'"
            :size="15"
          /></button
        ><template v-if="desktop && !nativeWindowControls"
          ><button
            class="icon-btn"
            aria-label="最小化"
            title="最小化"
            @click="appWindow?.minimize()"
          >
            <UiIcon name="minus" :size="14" /></button
          ><button
            class="icon-btn"
            aria-label="最大化或还原"
            title="最大化/还原"
            @click="appWindow?.toggleMaximize()"
          >
            <UiIcon name="window" :size="13" /></button
          ><button
            class="icon-btn danger"
            aria-label="关闭窗口"
            title="关闭"
            @click="closeWindow"
          >
            <UiIcon name="close" :size="15" /></button
        ></template>
      </div>
    </header>
    <main class="config-body">
      <div class="config-heading">
        <h1>工作区设置</h1>
        <p>管理本地服务与任务工作区</p>
      </div>
      <div v-if="!desktop" class="info-banner">
        请在桌面应用中管理服务设置。<a href="/">返回任务表</a>
      </div>
      <div v-if="error" class="form-error" role="alert">
        <UiIcon name="info" /><span>{{ error }}</span
        ><button v-if="!loaded" class="btn btn-sm" @click="load">重试</button>
      </div>
      <WorkspaceSettingsForm
        v-model="settings"
        :status="status"
        :loaded="loaded"
        :loading="loading"
        :busy="saving || serviceBusy"
        :token-busy="tokenBusy"
        :service-busy="serviceBusy"
        :enter-busy="enterBusy"
        :show-enter-app="desktop"
        show-service-toggle
        show-address-copy
        @open="openWeb"
        @enter-app="enterApp"
        @toggle-service="toggleService"
        @copy-address="copyText"
        @regenerate-token="regenerateToken"
      />
    </main>
    <footer class="config-footer">
      <span>{{
        serviceBusy
          ? status?.running
            ? "正在停止服务…"
            : "正在启动服务…"
          : saving
            ? "正在保存设置…"
          : loaded && dirty
            ? "有未保存的更改"
            : "设置保存在本机"
      }}</span>
      <div class="footer-actions">
        <button
          class="btn btn-primary"
          :disabled="!loaded || saving || serviceBusy || !dirty"
          @click="save"
        >
          {{ saving ? "保存中…" : "保存设置" }}
        </button>
      </div>
    </footer>
    <AppFeedback />
  </div>
</template>
<style scoped>
.config-app {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--card);
}
.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 42px;
  flex-shrink: 0;
  padding: 0 12px 0 16px;
  border-bottom: 1px solid var(--separator);
  user-select: none;
}
.titlebar-brand {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
  font-size: 12px;
}
.titlebar-brand > .ui-icon {
  color: var(--primary);
}
/* macOS：红绿灯叠在标题栏左上角，内容右移让位（红绿灯约占 70px） */
.titlebar-native-controls {
  padding-left: 80px;
}
.config-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 24px 28px;
}
.config-heading {
  margin-bottom: 22px;
}
.config-heading h1 {
  font-size: 21px;
  font-weight: 600;
}
.config-heading p {
  color: var(--text-tertiary);
  margin-top: 4px;
  font-size: 12px;
}
.config-body > .info-banner,
.config-body > .form-error {
  margin-bottom: 16px;
}
.config-footer {
  padding: 14px 28px;
  border-top: 1px solid var(--separator);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.config-footer > span {
  font-size: 11px;
  color: var(--text-tertiary);
}
.footer-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}
@media (max-width: 450px) {
  .config-body {
    padding: 20px;
  }
  .config-footer {
    padding: 12px 20px;
  }
}
</style>
