<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api } from "../../api/client";
import { copyText, errorText } from "@/shared/feedback";
import UiDialog from "@/shared/UiDialog.vue";
import UiIcon from "@/shared/UiIcon.vue";
import type { AgentAccess, LocalSkillTarget, User } from "@/shared/types";

const props = defineProps<{ user: User }>();
const emit = defineEmits<{
  close: [];
  userChanged: [user: User];
  accessChanged: [access: AgentAccess];
}>();
const access = ref<AgentAccess>();
const targets = ref<LocalSkillTarget[]>([]);
const username = ref(props.user.username);
const error = ref("");
const busy = ref(false);

async function load() {
  busy.value = true;
  error.value = "";
  try {
    access.value = await api.getAgentAccess();
    emit("accessChanged", access.value);
    if (props.user.is_host) {
      targets.value = await api.listLocalSkills();
    }
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

async function regenerate() {
  busy.value = true;
  try {
    access.value = await api.regenerateAgentToken();
    emit("accessChanged", access.value);
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

async function revoke() {
  busy.value = true;
  try {
    await api.revokeAgentToken();
    if (access.value) access.value.token = null;
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

async function install() {
  busy.value = true;
  try {
    targets.value = await api.installLocalSkills();
    access.value = await api.getAgentAccess();
    emit("accessChanged", access.value);
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

async function rename() {
  const value = username.value.trim();
  if (!value || value === props.user.username) return;
  busy.value = true;
  try {
    emit("userChanged", await api.patchUser(props.user.id, { username: value }));
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

function download() {
  window.location.href = "/api/web/skill/download";
}

onMounted(load);
</script>

<template>
  <UiDialog title="我的账号与 Agent 访问" :width="700" :busy="busy" @close="emit('close')">
    <section class="user-section">
      <h3>账号</h3>
      <form class="inline-edit" @submit.prevent="rename">
        <input v-model="username" class="input" aria-label="我的用户名" />
        <button class="btn" :disabled="busy || !username.trim()">保存名字</button>
      </form>
      <p class="muted">角色：{{ user.role }}<span v-if="user.is_host"> · 内置主机账号</span></p>
    </section>

    <section class="user-section">
      <h3>远程连接</h3>
      <p>{{ access?.access_instructions || "正在加载…" }}</p>
      <label>服务地址<input class="input code-input" readonly :value="access?.server_url" /></label>
      <label
        >Agent token<input
          class="input code-input"
          readonly
          :value="access?.token || '尚未签发或已吊销'"
      /></label>
      <div class="inline-actions">
        <button v-if="access?.token" class="btn" @click="copyText(access.token)">
          <UiIcon name="copy" />复制 token
        </button>
        <button class="btn" :disabled="busy" @click="regenerate">
          <UiIcon name="refresh" />{{ access?.token ? "重新生成" : "生成 token" }}
        </button>
        <button v-if="access?.token" class="btn btn-danger" :disabled="busy" @click="revoke">
          吊销
        </button>
      </div>
    </section>

    <section class="user-section">
      <h3>pm-cli-skill</h3>
      <p class="muted">下载包包含 SKILL.md 与匹配当前服务版本的 pm-cli.exe。</p>
      <div class="inline-actions">
        <button class="btn" @click="download"><UiIcon name="download" />下载 skill</button>
        <button v-if="user.is_host && targets.length" class="btn btn-primary" :disabled="busy" @click="install">
          一键安装/更新 {{ targets.length }} 个本机前端
        </button>
      </div>
      <ul v-if="user.is_host" class="skill-targets">
        <li v-for="target in targets" :key="target.path">
          <strong>{{ target.frontend }}</strong><span>{{ target.installed ? `已安装 ${target.version || ''}` : "未安装" }}</span>
          <code>{{ target.path }}</code>
        </li>
      </ul>
    </section>
    <div v-if="error" class="form-error" role="alert">{{ error }}</div>
  </UiDialog>
</template>
