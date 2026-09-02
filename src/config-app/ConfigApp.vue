<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onMounted, ref } from "vue";

interface Settings {
  port: number;
  autostart: boolean;
  close_behavior: string; // "keep_service" | "stop_all"
}

interface ServerStatus {
  running: boolean;
  port: number;
  url: string;
  data_dir: string;
}

const settings = ref<Settings>({ port: 17890, autostart: true, close_behavior: "keep_service" });
const status = ref<ServerStatus | null>(null);
const message = ref("");
const error = ref("");
const saving = ref(false);

const appWindow = getCurrentWindow();

async function load() {
  settings.value = await invoke<Settings>("get_settings");
  status.value = await invoke<ServerStatus>("get_server_status");
}

async function save() {
  saving.value = true;
  message.value = "";
  error.value = "";
  try {
    settings.value = await invoke<Settings>("save_settings", { settings: settings.value });
    status.value = await invoke<ServerStatus>("get_server_status");
    message.value = "已保存，服务已按新配置运行";
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
  if (status.value?.url) await open(status.value.url);
}

onMounted(load);
</script>

<template>
  <div class="config-app">
    <div class="titlebar" data-tauri-drag-region>
      <span class="titlebar-title">Agents PM Tool 设置</span>
      <div class="titlebar-actions">
        <button class="icon-btn" @click="appWindow.minimize()">—</button>
        <button class="icon-btn danger" @click="appWindow.close()">✕</button>
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
        <div class="status-line muted" v-if="status">数据目录：{{ status.data_dir }}</div>
        <button class="btn btn-sm" :disabled="!status?.running" @click="openWeb">打开网页</button>
      </section>

      <section class="config-section">
        <h3>基础配置</h3>
        <label class="form-label">HTTP 端口（保存后自动重启服务）</label>
        <input v-model.number="settings.port" class="input port-input" type="number" min="1024" max="65535" />

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
  padding: 8px 12px;
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
