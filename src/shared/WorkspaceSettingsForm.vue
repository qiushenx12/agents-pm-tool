<script setup lang="ts">
import UiIcon from "./UiIcon.vue";
import UiSelect from "./UiSelect.vue";
import type { WorkspaceServerStatus, WorkspaceSettings } from "./types";

withDefaults(
  defineProps<{
    status: WorkspaceServerStatus | null;
    loaded: boolean;
    loading?: boolean;
    busy?: boolean;
    tokenBusy?: boolean;
    openLabel?: string;
    showAddressCopy?: boolean;
  }>(),
  {
    loading: false,
    busy: false,
    tokenBusy: false,
    openLabel: "打开网页",
    showAddressCopy: false,
  },
);
const settings = defineModel<WorkspaceSettings>({ required: true });
defineEmits<{
  open: [];
  regenerateToken: [];
  copyAddress: [value: string];
}>();

const closeOptions = [
  { value: "keep_service", label: "保持服务运行（托盘常驻）" },
  { value: "stop_all", label: "退出应用并停止服务" },
];
</script>

<template>
  <div class="workspace-settings-form">
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
                  : "服务未运行"
            }}
          </p>
        </div>
        <button
          class="btn btn-primary btn-sm"
          :disabled="!status?.running"
          @click="$emit('open')"
        >
          {{ openLabel }}<UiIcon name="globe" :size="13" />
        </button>
      </div>
      <div v-if="status" class="service-address">
        <div
          class="service-address-row"
          :class="{ 'has-copy': showAddressCopy }"
        >
          <span>本机地址</span><code>{{ status.url || "—" }}</code
          ><button
            v-if="showAddressCopy"
            type="button"
            class="icon-btn address-copy"
            aria-label="复制本机地址"
            title="复制本机地址"
            :disabled="!status.url"
            @click="$emit('copyAddress', status.url)"
          >
            <UiIcon name="copy" :size="13" />
          </button>
        </div>
        <div
          v-if="status.lan_url"
          class="service-address-row"
          :class="{ 'has-copy': showAddressCopy }"
        >
          <span>局域网地址</span><code>{{ status.lan_url }}</code
          ><button
            v-if="showAddressCopy"
            type="button"
            class="icon-btn address-copy"
            aria-label="复制局域网地址"
            title="复制局域网地址"
            @click="$emit('copyAddress', status.lan_url)"
          >
            <UiIcon name="copy" :size="13" />
          </button>
        </div>
      </div>
    </section>

    <section class="settings-section">
      <h2><UiIcon name="settings" :size="15" />常用设置</h2>
      <fieldset :disabled="!loaded || busy">
        <div class="settings-row">
          <div>
            <label for="workspace-autostart">启动时自动开启服务</label>
            <p>打开应用即可使用任务工作区</p>
          </div>
          <input
            id="workspace-autostart"
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
            :disabled="!loaded || busy"
          />
        </div>
      </fieldset>
    </section>

    <section class="settings-section">
      <h2><UiIcon name="layers" :size="15" />连接设置</h2>
      <fieldset :disabled="!loaded || busy">
        <div class="settings-row">
          <div>
            <label for="workspace-service-port">服务端口</label>
            <p>修改后保存会重新启动服务</p>
          </div>
          <input
            id="workspace-service-port"
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
            ><input v-model="settings.listen_scope" type="radio" value="local" />
            <div>
              <strong>仅本机</strong><span>仅当前电脑可以访问工作区</span>
            </div></label
          ><label class="scope-option"
            ><input v-model="settings.listen_scope" type="radio" value="lan" />
            <div>
              <strong>局域网</strong><span>允许同一网络中的设备访问</span>
            </div></label
          >
          <div v-if="settings.listen_scope === 'lan'" class="scope-warning">
            局域网用户需要注册并经管理员授权。请在可信网络中使用，且不要复用重要口令。
          </div>
        </div>
        <div class="settings-row vertical">
          <label for="workspace-agent-server-url">远程 Agent 服务地址（可选）</label>
          <p>多网卡时可手工指定给远程用户的地址；留空会自动探测。</p>
          <input
            id="workspace-agent-server-url"
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
        :disabled="!loaded || tokenBusy || busy"
        @click="$emit('regenerateToken')"
      >
        <UiIcon name="refresh" :size="13" />{{
          tokenBusy ? "正在更新…" : "重新生成 Agent token"
        }}
      </button>
      <div v-if="status" class="data-directory">
        <span>数据目录</span><code>{{ status.data_dir }}</code>
      </div>
    </section>
  </div>
</template>

<style scoped>
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
  display: flex;
  flex-direction: column;
  gap: 5px;
  font-size: 11px;
  padding-top: 14px;
  margin-top: 14px;
  border-top: 1px solid var(--separator);
  color: var(--text-secondary);
}
.service-address-row {
  display: grid;
  grid-template-columns: 72px minmax(0, 1fr);
  align-items: center;
  gap: 5px;
}
.service-address-row.has-copy {
  grid-template-columns: 72px minmax(0, 1fr) 24px;
}
.service-address code {
  overflow-wrap: anywhere;
  font: 11px var(--font-mono);
}
.address-copy {
  width: 24px;
  height: 24px;
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
@media (max-width: 450px) {
  .service-overview {
    padding: 12px;
  }
}
</style>
