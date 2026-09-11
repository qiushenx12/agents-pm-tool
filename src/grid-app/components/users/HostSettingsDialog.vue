<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api } from "@/grid-app/api/client";
import { askConfirm, errorText, notify } from "@/shared/feedback";
import UiDialog from "@/shared/UiDialog.vue";
import WorkspaceSettingsForm from "@/shared/WorkspaceSettingsForm.vue";
import type {
  WorkspaceServerStatus,
  WorkspaceSettings,
} from "@/shared/types";

const emit = defineEmits<{
  close: [];
  restarting: [port: number];
}>();

const settings = ref<WorkspaceSettings>({
  port: 17890,
  autostart: true,
  close_behavior: "keep_service",
  listen_scope: "local",
  agent_server_url: "",
});
const status = ref<WorkspaceServerStatus | null>(null);
const baseline = ref("");
const loaded = ref(false);
const loading = ref(false);
const saving = ref(false);
const tokenBusy = ref(false);
const error = ref("");
const dirty = computed(
  () => loaded.value && JSON.stringify(settings.value) !== baseline.value,
);

async function load() {
  loading.value = true;
  error.value = "";
  try {
    const response = await api.getHostSettings();
    settings.value = response.settings;
    status.value = response.status;
    baseline.value = JSON.stringify(response.settings);
    loaded.value = true;
  } catch (cause) {
    error.value = errorText(cause);
  } finally {
    loading.value = false;
  }
}

async function save() {
  if (saving.value) return;
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
    const response = await api.putHostSettings(settings.value);
    settings.value = response.settings;
    status.value = response.status;
    baseline.value = JSON.stringify(response.settings);
    notify(response.restarted ? "设置已保存，服务正在重启" : "设置已保存");
    if (response.restarted) emit("restarting", response.port);
  } catch (cause) {
    error.value = errorText(cause);
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
    await api.regenerateAgentToken();
    notify("Agent token 已更新");
  } catch (cause) {
    error.value = errorText(cause);
  } finally {
    tokenBusy.value = false;
  }
}

function openWeb() {
  if (status.value?.url) window.open(status.value.url, "_blank", "noopener");
}

async function close() {
  if (
    dirty.value &&
    !(await askConfirm("关闭设置", "尚未保存的设置将被丢弃。", "关闭"))
  )
    return;
  emit("close");
}

onMounted(load);
</script>

<template>
  <UiDialog title="工作区设置" :width="620" :busy="saving" @close="close">
    <div v-if="error" class="form-error host-settings-error" role="alert">
      <span>{{ error }}</span>
      <button v-if="!loaded" class="btn btn-sm" @click="load">重试</button>
    </div>
    <WorkspaceSettingsForm
      v-model="settings"
      :status="status"
      :loaded="loaded"
      :loading="loading"
      :busy="saving"
      :token-busy="tokenBusy"
      open-label="打开网页"
      @open="openWeb"
      @regenerate-token="regenerateToken"
    />
    <template #footer>
      <span class="host-settings-footer-status">{{
        saving
          ? "正在保存设置…"
          : dirty
            ? "有未保存的更改"
            : "设置保存在本机"
      }}</span>
      <button class="btn" :disabled="saving" @click="close">取消</button>
      <button
        class="btn btn-primary"
        :disabled="!loaded || saving || !dirty"
        @click="save"
      >
        {{ saving ? "保存中…" : "保存设置" }}
      </button>
    </template>
  </UiDialog>
</template>

<style scoped>
.host-settings-error {
  margin-bottom: 16px;
}
.host-settings-error > span {
  flex: 1;
}
.host-settings-footer-status {
  margin-right: auto;
  color: var(--text-tertiary);
  font-size: 11px;
}
</style>
