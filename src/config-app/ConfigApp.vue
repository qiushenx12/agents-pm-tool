<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { currentTheme, toggleTheme } from "@/shared/theme";
import { onMounted, ref } from "vue";

interface Settings {
  port: number;
  autostart: boolean;
  close_behavior: string; // "keep_service" | "stop_all"
  listen_scope: string; // "local"（127.0.0.1）| "lan"（0.0.0.0）
}

interface ServerStatus {
  running: boolean;
  port: number;
  url: string;
  lan_url: string; // LAN 模式下的局域网地址，local 模式为空
  data_dir: string;
}

interface SaveSettingsResult {
  settings: Settings;
  restarted: boolean;
  port: number;
}

const settings = ref<Settings>({
  port: 17890,
  autostart: true,
  close_behavior: "keep_service",
  listen_scope: "local",
});
const status = ref<ServerStatus | null>(null);
const message = ref("");
const error = ref("");
const saving = ref(false);

const appWindow = getCurrentWindow();
const theme = ref(currentTheme());

function onToggleTheme() {
  theme.value = toggleTheme();
}

// 顶栏拖拽：Tauri v2 的 data-tauri-drag-region HTML 属性在自绘标题栏里行为不可靠，
// 照搬 cc-launcher：手动监听 mousedown 调 startDragging()，并排除按钮等交互元素。
function isInteractiveTarget(target: unknown): boolean {
  if (typeof target !== "object" || target === null) return false;
  const closest = Reflect.get(target, "closest");
  if (typeof closest !== "function") return false;
  return (
    closest.call(
      target,
      "button, a, input, textarea, select, option, [data-tauri-drag-region='false']",
    ) !== null
  );
}

function startTitleBarDrag(event: MouseEvent) {
  // 只响应左键单击（detail=1 排除双击），且不点在按钮上
  if (event.button !== 0 || event.detail !== 1) return;
  if (isInteractiveTarget(event.target)) return;
  void appWindow.startDragging().catch(() => {});
}

function handleTitleBarDoubleClick(event: MouseEvent) {
  if (event.button !== 0 || event.detail !== 2) return;
  if (isInteractiveTarget(event.target)) return;
  void appWindow.toggleMaximize().catch(() => {});
}

async function load() {
  settings.value = await invoke<Settings>("get_settings");
  status.value = await invoke<ServerStatus>("get_server_status");
}

async function save() {
  saving.value = true;
  message.value = "";
  error.value = "";
  try {
    const result = await invoke<SaveSettingsResult>("save_settings", { settings: settings.value });
    settings.value = result.settings;
    status.value = await invoke<ServerStatus>("get_server_status");
    // 提示语区分是否重启了服务（review P3-3）
    if (result.restarted) {
      const host = result.settings.listen_scope === "lan" ? "0.0.0.0" : "127.0.0.1";
      message.value = `已保存，服务已重启并监听 ${host}:${result.port}`;
    } else {
      message.value = "已保存（端口与监听范围未变，服务无需重启）";
    }
  } catch (e) {
    error.value = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  } finally {
    saving.value = false;
  }
}

async function regenerateToken() {
  error.value = "";
  try {
    await invoke("regenerate_token");
    message.value = "Agent token 已重新生成并生效";
  } catch (e) {
    error.value = String(e);
  }
}

async function openWeb() {
  error.value = "";
  if (!status.value?.url) return;
  try {
    await shellOpen(status.value.url);
  } catch (e) {
    error.value = `打开网页失败：${typeof e === "string" ? e : e instanceof Error ? e.message : String(e)}`;
  }
}

onMounted(load);
</script>

<template>
  <div class="config-app">
    <div
      class="titlebar"
      @mousedown="startTitleBarDrag"
      @dblclick="handleTitleBarDoubleClick"
    >
      <span class="titlebar-title">Agents PM Tool 设置</span>
      <div class="titlebar-actions">
        <button class="icon-btn" title="切换明暗主题" @click="onToggleTheme">
          {{ theme === "dark" ? "☀" : "☾" }}
        </button>
        <button class="icon-btn" title="最小化" @click="appWindow.minimize()">—</button>
        <button class="icon-btn" title="最大化/还原" @click="appWindow.toggleMaximize()">▢</button>
        <button class="icon-btn danger" title="关闭" @click="appWindow.close()">✕</button>
      </div>
    </div>

    <main class="config-body">
      <section class="config-section">
        <h3>服务状态</h3>
        <div class="status-line">
          <span class="status-dot" :class="status?.running ? 'on' : 'off'"></span>
          <span v-if="status?.running">服务运行中：{{ status.url }}</span>
          <span v-else>服务未运行</span>
        </div>
        <div class="status-line muted" v-if="status?.running && status?.lan_url">
          局域网访问：{{ status.lan_url }}
        </div>
        <div class="status-line muted" v-if="status">数据目录：{{ status.data_dir }}</div>
        <button class="btn btn-sm" :disabled="!status?.running" @click="openWeb">打开网页</button>
      </section>

      <section class="config-section">
        <h3>基础配置</h3>
        <label class="form-label">HTTP 端口（修改后保存将重启服务）</label>
        <input v-model.number="settings.port" class="input port-input" type="number" min="1024" max="65535" />

        <label class="form-label">服务监听范围（修改后保存将重启服务）</label>
        <label class="check-line">
          <input v-model="settings.listen_scope" type="radio" value="local" />
          本地（127.0.0.1，仅本机可访问）
        </label>
        <label class="check-line">
          <input v-model="settings.listen_scope" type="radio" value="lan" />
          局域网（0.0.0.0，同网段设备可访问）
        </label>
        <p v-if="settings.listen_scope === 'lan'" class="warn-hint">
          ⚠ 局域网模式下 /api/web/* 无鉴权，同网段任何设备都能增删任务，请确认网络环境可信。
        </p>

        <label class="check-line">
          <input v-model="settings.autostart" type="checkbox" />
          启动 app 后自动开启服务
        </label>

        <label class="form-label">关闭窗口时</label>
        <select v-model="settings.close_behavior" class="input">
          <option value="keep_service">保持服务运行（托盘常驻）</option>
          <option value="stop_all">退出 app 并停止服务</option>
        </select>

        <div class="config-actions">
          <button class="btn btn-primary" :disabled="saving" @click="save">
            {{ saving ? "保存中…" : "保存" }}
          </button>
        </div>
      </section>

      <section class="config-section">
        <h3>Agent 接入</h3>
        <p class="muted">pm-cli 通过 data/runtime.json 中的 token 访问受限 API。</p>
        <button class="btn btn-sm" @click="regenerateToken">重新生成 Agent token</button>
      </section>

      <div v-if="message" class="msg ok">{{ message }}</div>
      <div v-if="error" class="msg err">{{ error }}</div>
    </main>
  </div>
</template>

<style scoped>
.config-app {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 44px;          /* 固定高度，否则 flex 布局下会被 config-body 吃掉 */
  flex-shrink: 0;        /* 禁止被压缩 */
  padding: 0 12px;
  background: var(--card);
  border-bottom: 1px solid var(--separator);
  user-select: none;
}
.titlebar-title {
  font-weight: 600;
}
.titlebar-actions {
  display: flex;
  gap: 4px;
}
.config-body {
  flex: 1;
  overflow: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.config-section {
  background: var(--card);
  border: 1px solid var(--separator);
  border-radius: var(--radius);
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.config-section h3 {
  font-size: var(--font-size-base);
}
.status-line {
  display: flex;
  align-items: center;
  gap: 8px;
}
.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.status-dot.on { background: var(--success); }
.status-dot.off { background: var(--danger); }
.muted {
  color: var(--text-secondary);
  font-size: var(--font-size-small);
  word-break: break-all;
}
.port-input {
  width: 140px;
}
.warn-hint {
  color: var(--warning);
  font-size: var(--font-size-small);
  margin: 0 0 4px;
  padding: 6px 8px;
  background: color-mix(in srgb, var(--warning) 12%, transparent);
  border-radius: var(--radius);
  border: 1px solid color-mix(in srgb, var(--warning) 35%, transparent);
}
.check-line {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
.config-actions {
  display: flex;
  justify-content: flex-end;
  margin-top: 4px;
}
.form-label {
  font-size: var(--font-size-small);
  color: var(--text-secondary);
}
.input {
  padding: 6px 10px;
  border: 1px solid var(--input-border);
  border-radius: var(--radius-sm);
  background: var(--input-bg);
  color: var(--text-primary);
}
.btn {
  align-self: flex-start;
  padding: 6px 14px;
  border: 1px solid var(--input-border);
  border-radius: var(--radius-sm);
  background: var(--card);
  color: var(--text-primary);
  cursor: pointer;
}
.btn-primary {
  background: var(--primary);
  border-color: var(--primary);
  color: #fff;
}
.btn-sm {
  padding: 3px 10px;
  font-size: var(--font-size-small);
}
.icon-btn {
  border: none;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 2px 8px;
}
.icon-btn.danger:hover {
  color: var(--danger);
}
.msg {
  font-size: var(--font-size-small);
}
.msg.ok { color: var(--success); }
.msg.err { color: var(--danger); }
</style>
