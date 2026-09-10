<script setup lang="ts">
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { computed, onMounted, ref } from "vue";
import { currentTheme, toggleTheme } from "@/shared/theme";
import UiIcon from "@/shared/UiIcon.vue";
import UiSelect from "@/shared/UiSelect.vue";
import AppFeedback from "@/shared/AppFeedback.vue";
import { askConfirm, errorText, notify } from "@/shared/feedback";
interface Settings {
  port: number;
  autostart: boolean;
  close_behavior: string;
  listen_scope: string;
  agent_server_url: string;
}
interface ServerStatus {
  running: boolean;
  port: number;
  url: string;
  lan_url: string;
  data_dir: string;
}
interface SaveSettingsResult {
  settings: Settings;
  restarted: boolean;
  port: number;
}
const desktop = isTauri();
const appWindow = desktop ? getCurrentWindow() : null;
const settings = ref<Settings>({
  port: 17890,
  autostart: true,
  close_behavior: "keep_service",
  listen_scope: "local",
  agent_server_url: "",
});
const status = ref<ServerStatus | null>(null);
const baseline = ref(""),
  loaded = ref(false),
  saving = ref(false),
  loading = ref(false),
  tokenBusy = ref(false),
  error = ref("");
const dirty = computed(() => JSON.stringify(settings.value) !== baseline.value);
const theme = ref(currentTheme());
const closeOptions = [
  { value: "keep_service", label: "保持服务运行（托盘常驻）" },
  { value: "stop_all", label: "退出应用并停止服务" },
];
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
      invoke<Settings>("get_settings"),
      invoke<ServerStatus>("get_server_status"),
    ]);
    settings.value = config;
    status.value = server;
    baseline.value = JSON.stringify(config);
    loaded.value = true;
  } catch (e) {
    error.value = errorText(e);
  } finally {
    loading.value = false;
  }
}
async function save() {
  if (!desktop || saving.value) return;
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
    status.value = await invoke<ServerStatus>("get_server_status");
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
    error.value = "打开任务表失败：" + errorText(e);
  }
}
async function closeWindow() {
  if (saving.value) return;
  if (
    loaded.value &&
    dirty.value &&
    !(await askConfirm("关闭设置", "尚未保存的设置将被丢弃。", "关闭"))
  )
    return;
  await appWindow?.close();
}
onMounted(load);
</script>
<template>
  <div class="config-app">
    <header
      class="titlebar"
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
          @click="theme = toggleTheme()"
        >
          <UiIcon
            :name="theme === 'dark' ? 'sun' : 'moon'"
            :size="15"
          /></button
        ><template v-if="desktop"
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
      <section class="service-overview">
        <div class="service-overview-top">
          <span class="service-icon"><UiIcon name="grid" :size="22" /></span>
          <div>
            <h2>任务工作区</h2>
            <p class="service-status">
              <span class="status-dot" :class="{ on: status?.running }"></span
              >{{
                loading
                  ? "正在连接服务…"
                  : status?.running
                    ? "服务运行中"
                    : desktop
                      ? "服务未运行"
                      : "桌面服务设置"
              }}
            </p>
          </div>
          <button
            class="btn btn-primary btn-sm"
            :disabled="!status?.running"
            @click="openWeb"
          >
            打开任务表<UiIcon name="expand" :size="13" />
          </button>
        </div>
        <div v-if="status" class="service-address">
          <span>本机地址</span><code>{{ status.url || "—" }}</code
          ><template v-if="status.lan_url"
            ><span>局域网地址</span><code>{{ status.lan_url }}</code></template
          >
        </div>
      </section>
      <section class="settings-section">
        <h2><UiIcon name="settings" :size="15" />常用设置</h2>
        <fieldset :disabled="!loaded || saving">
          <div class="settings-row">
            <div>
              <label for="autostart">启动时自动开启服务</label>
              <p>打开应用即可使用任务工作区</p>
            </div>
            <input
              id="autostart"
              v-model="settings.autostart"
              type="checkbox"
              role="switch"
              class="switch-input"
            />
          </div>
          <div class="settings-row vertical">
            <label>关闭窗口时</label
            ><UiSelect
              v-model="settings.close_behavior"
              :options="closeOptions"
              label="关闭窗口时"
              :disabled="!loaded || saving"
            />
          </div>
        </fieldset>
      </section>
      <section class="settings-section">
        <h2><UiIcon name="layers" :size="15" />连接设置</h2>
        <fieldset :disabled="!loaded || saving">
          <div class="settings-row">
            <div>
              <label for="service-port">服务端口</label>
              <p>修改后保存会重新启动服务</p>
            </div>
            <input
              id="service-port"
              v-model.number="settings.port"
              class="input port-input"
              type="number"
              min="1024"
              max="65535"
            />
          </div>
          <div class="settings-row vertical">
            <label>访问范围</label
            ><label class="scope-option"
              ><input
                v-model="settings.listen_scope"
                type="radio"
                value="local"
              />
              <div>
                <strong>仅本机</strong><span>仅当前电脑可以访问工作区</span>
              </div></label
            ><label class="scope-option"
              ><input
                v-model="settings.listen_scope"
                type="radio"
                value="lan"
              />
              <div>
                <strong>局域网</strong><span>允许同一网络中的设备访问</span>
              </div></label
            >
            <div v-if="settings.listen_scope === 'lan'" class="scope-warning">
              局域网用户需要注册并经管理员授权。请在可信网络中使用，且不要复用重要口令。
            </div>
          </div>
          <div class="settings-row vertical">
            <label for="agent-server-url">远程 Agent 服务地址（可选）</label>
            <p>多网卡时可手工指定给远程用户的地址；留空会自动探测。</p>
            <input
              id="agent-server-url"
              v-model="settings.agent_server_url"
              class="input"
              placeholder="例如 http://192.168.1.10:17890"
            />
          </div>
        </fieldset>
      </section>
      <section class="settings-section">
        <h2><UiIcon name="bot" :size="15" />Agent 接入</h2>
        <p class="section-description">Agent 通过 pm-cli 读取和推进任务。</p>
        <button
          class="btn btn-sm"
          :disabled="!loaded || tokenBusy || saving"
          @click="regenerateToken"
        >
          <UiIcon name="refresh" :size="13" />{{
            tokenBusy ? "正在更新…" : "重新生成 Agent token"
          }}
        </button>
        <div v-if="status" class="data-directory">
          <span>数据目录</span><code>{{ status.data_dir }}</code>
        </div>
      </section>
    </main>
    <footer class="config-footer">
      <span>{{
        saving
          ? "正在保存设置…"
          : loaded && dirty
            ? "有未保存的更改"
            : "设置保存在本机"
      }}</span
      ><button
        class="btn btn-primary"
        :disabled="!loaded || saving || !dirty"
        @click="save"
      >
        {{ saving ? "保存中…" : "保存设置" }}
      </button>
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
.service-overview {
  background: var(--sidebar-bg);
  border: 1px solid var(--separator);
  border-radius: 8px;
  padding: 16px;
}
.service-overview-top {
  display: flex;
  gap: 10px;
  align-items: center;
}
.service-overview-top h2 {
  font-size: 13px;
  font-weight: 600;
}
.service-overview-top > .btn {
  margin-left: auto;
}
.service-icon {
  display: grid;
  place-items: center;
  width: 36px;
  height: 38px;
  border-radius: 7px;
  background: var(--tag-teal-bg);
  color: var(--tag-teal-fg);
}
.service-status {
  display: flex;
  align-items: center;
  gap: 5px;
  color: var(--text-secondary);
  font-size: 11px;
  margin-top: 4px;
}
.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-tertiary);
}
.status-dot.on {
  background: var(--success);
}
.service-address {
  display: grid;
  grid-template-columns: 72px 1fr;
  gap: 5px;
  font-size: 11px;
  padding-top: 14px;
  margin-top: 14px;
  border-top: 1px solid var(--separator);
  color: var(--text-secondary);
}
.service-address code {
  overflow-wrap: anywhere;
  font: 11px var(--font-mono);
}
.settings-section {
  padding: 24px 0 20px;
  border-bottom: 1px solid var(--separator);
}
.settings-section h2 {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 14px;
}
.settings-section h2 > .ui-icon {
  color: var(--text-tertiary);
}
.settings-section fieldset {
  border: 0;
  min-width: 0;
}
.settings-section fieldset:disabled {
  opacity: 0.6;
}
.settings-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 16px;
}
.settings-row:last-child {
  margin-bottom: 0;
}
.settings-row p {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-top: 4px;
}
.settings-row.vertical {
  flex-direction: column;
  align-items: stretch;
  gap: 9px;
}
.port-input {
  width: 105px;
  flex-shrink: 0;
}
.switch-input {
  appearance: none;
  width: 30px;
  height: 18px;
  border-radius: 10px;
  background: var(--input-border);
  position: relative;
  cursor: pointer;
}
.switch-input::after {
  content: "";
  position: absolute;
  left: 2px;
  top: 2px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 3px #0002;
  transition: transform 0.12s;
}
.switch-input:checked {
  background: var(--primary);
}
.switch-input:checked::after {
  transform: translateX(12px);
}
.scope-option {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 5px 0;
  cursor: pointer;
}
.scope-option > div {
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.scope-option strong {
  font-size: 12px;
  font-weight: 400;
}
.scope-option span {
  color: var(--text-tertiary);
  font-size: 11px;
}
.scope-warning {
  font-size: 11px;
  background: var(--tag-orange-bg);
  color: var(--tag-orange-fg);
  padding: 8px 10px;
  border-radius: 5px;
}
.section-description {
  font-size: 12px;
  color: var(--text-secondary);
  margin-bottom: 12px;
}
.data-directory {
  display: flex;
  flex-direction: column;
  gap: 5px;
  font-size: 11px;
  color: var(--text-tertiary);
  margin-top: 16px;
}
.data-directory code {
  font: 10px/1.6 var(--font-mono);
  overflow-wrap: anywhere;
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
@media (max-width: 450px) {
  .config-body {
    padding: 20px;
  }
  .service-overview {
    padding: 12px;
  }
  .config-footer {
    padding: 12px 20px;
  }
}
</style>
