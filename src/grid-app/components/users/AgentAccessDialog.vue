<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api } from "../../api/client";
import { copyText, errorText } from "@/shared/feedback";
import FrontendLogo from "@/shared/FrontendLogo.vue";
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
const installing = ref<LocalSkillTarget["frontend_id"] | null>(null);
const opening = ref<LocalSkillTarget["frontend_id"] | null>(null);

const roleNames = {
  super_admin: "超级管理员",
  admin: "管理员",
  user: "普通用户",
} as const;

const frontendCards = computed(() =>
  [
    {
      id: "codex" as const,
      title: "Codex",
    },
    {
      id: "claude_code" as const,
      title: "Claude Code",
    },
    {
      id: "workbuddy" as const,
      title: "WorkBuddy",
    },
    {
      id: "opencode" as const,
      title: "OpenCode",
    },
    {
      id: "cursor" as const,
      title: "Cursor",
    },
    {
      id: "pi" as const,
      title: "Pi",
    },
    {
      id: "deepseek_harness" as const,
      title: "DeepSeek Harness",
    },
  ].map((frontend) => {
    const detected = targets.value.filter(
      (target) => target.frontend_id === frontend.id,
    );
    const primary = detected.find((target) => target.installed) ?? detected[0];
    return {
      ...frontend,
      primary,
      extraCount: Math.max(detected.length - 1, 0),
      allInstalled:
        detected.length > 0 && detected.every((target) => target.installed),
      someInstalled: detected.some((target) => target.installed),
    };
  }),
);

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

async function install(frontend: LocalSkillTarget["frontend_id"]) {
  busy.value = true;
  installing.value = frontend;
  error.value = "";
  try {
    targets.value = await api.installLocalSkills(frontend);
    access.value = await api.getAgentAccess();
    emit("accessChanged", access.value);
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    installing.value = null;
    busy.value = false;
  }
}

async function refreshTargets() {
  busy.value = true;
  error.value = "";
  try {
    targets.value = await api.listLocalSkills();
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

async function openDirectory(frontend: LocalSkillTarget["frontend_id"]) {
  busy.value = true;
  opening.value = frontend;
  error.value = "";
  try {
    await api.openLocalSkillDirectory(frontend);
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    opening.value = null;
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

onMounted(load);
</script>

<template>
  <UiDialog title="我的账号与 Agent 访问" :width="780" :busy="busy" @close="emit('close')">
    <div class="agent-access-layout">
      <section class="agent-account-summary">
        <div class="agent-account-avatar">{{ user.username.slice(0, 1) }}</div>
        <div class="agent-account-copy">
          <strong>{{ user.username }}</strong>
          <span>{{ roleNames[user.role] }}<template v-if="user.is_host"> · 内置主机账号</template></span>
        </div>
        <form class="agent-account-edit" @submit.prevent="rename">
          <input v-model="username" class="input" aria-label="我的用户名" maxlength="64" />
          <button class="btn btn-sm" :disabled="busy || !username.trim() || username.trim() === user.username">
            保存名字
          </button>
        </form>
      </section>

      <section class="agent-access-card connection-card">
        <header class="agent-card-header">
          <div class="agent-card-icon"><UiIcon name="bot" :size="19" /></div>
          <div>
            <h3>Agent 连接凭据</h3>
            <p>pm-cli 使用以下地址和当前账号的独立 token 访问任务。</p>
          </div>
        </header>
        <div class="agent-access-note">
          <UiIcon name="info" :size="15" />
          <span>{{ access?.access_instructions || "正在读取接入信息…" }}</span>
        </div>
        <div class="agent-credentials">
          <div class="agent-credential-row">
            <span>服务地址</span>
            <code>{{ access?.server_url || "—" }}</code>
            <button v-if="access?.server_url" class="icon-btn" title="复制服务地址" aria-label="复制服务地址" @click="copyText(access.server_url)">
              <UiIcon name="copy" :size="14" />
            </button>
          </div>
          <div class="agent-credential-row token-row">
            <span>Agent token</span>
            <code :class="{ empty: !access?.token }">{{ access?.token || "尚未签发或已吊销" }}</code>
            <button v-if="access?.token" class="icon-btn" title="复制 token" aria-label="复制 Agent token" @click="copyText(access.token)">
              <UiIcon name="copy" :size="14" />
            </button>
          </div>
        </div>
        <div class="agent-card-actions">
          <button class="btn" :disabled="busy" @click="regenerate">
            <UiIcon name="refresh" />{{ access?.token ? "重新生成 token" : "生成 token" }}
          </button>
          <button v-if="access?.token" class="btn btn-danger" :disabled="busy" @click="revoke">
            吊销 token
          </button>
        </div>
      </section>

      <section class="agent-access-card skill-install-card">
        <header class="skill-install-header">
          <div>
            <h3>pm-cli-skill</h3>
            <p>为每个本机 Agent 前端分别安装，也可以下载完整包手动部署。</p>
          </div>
          <div class="inline-actions">
            <button v-if="user.is_host" class="btn btn-sm" :disabled="busy" @click="refreshTargets">
              <UiIcon name="refresh" :size="13" />重新检测
            </button>
            <a class="btn btn-sm" href="/api/web/skill/download" download>
              <UiIcon name="download" :size="13" />下载 ZIP
            </a>
          </div>
        </header>

        <div v-if="user.is_host" class="skill-frontend-grid">
          <article
            v-for="frontend in frontendCards"
            :key="frontend.id"
            class="skill-frontend-card"
            :class="`frontend-${frontend.id}`"
            :data-frontend="frontend.id"
          >
            <header>
              <span class="skill-frontend-mark"><FrontendLogo :id="frontend.id" :size="20" /></span>
              <strong>{{ frontend.title }}</strong>
              <span
                class="skill-state"
                :class="{ ready: frontend.allInstalled, partial: frontend.someInstalled && !frontend.allInstalled }"
              >
                {{
                  !frontend.primary
                    ? "未检测到"
                    : frontend.allInstalled
                      ? "已就绪"
                      : frontend.someInstalled
                        ? "部分已安装"
                        : "可安装"
                }}
              </span>
            </header>
            <code
              v-if="frontend.primary"
              class="skill-path"
              :title="
                frontend.primary.path +
                (frontend.extraCount ? ` 等 ${frontend.extraCount + 1} 个目录` : '')
              "
            >
              <span>{{ frontend.primary.path }}</span>
              <em v-if="frontend.primary.version">v{{ frontend.primary.version }}</em>
            </code>
            <span v-else class="skill-path empty">
              启动或安装 {{ frontend.title }} 后重新检测
            </span>
            <div class="skill-card-actions">
              <button
                class="btn btn-primary skill-install-button"
                :disabled="busy || !frontend.primary"
                @click="install(frontend.id)"
              >
                <UiIcon :name="frontend.allInstalled ? 'refresh' : 'download'" />
                {{
                  installing === frontend.id
                    ? "安装中…"
                    : frontend.allInstalled
                      ? "更新"
                      : "安装"
                }}
              </button>
              <button
                class="icon-btn skill-open-button"
                :disabled="busy || !frontend.primary"
                :title="opening === frontend.id ? '正在打开…' : `打开 ${frontend.title} 的 skill 目录`"
                :aria-label="`打开 ${frontend.title} 的 skill 目录`"
                @click="openDirectory(frontend.id)"
              >
                <UiIcon name="folder" :size="15" />
              </button>
            </div>
          </article>
        </div>
        <div v-else class="remote-skill-hint">
          <UiIcon name="download" :size="19" />
          <div><strong>在 Agent 所在电脑安装</strong><span>下载 ZIP 后解压到对应前端的 skill 目录。</span></div>
        </div>
      </section>
    </div>
    <div v-if="error" class="form-error" role="alert">{{ error }}</div>
  </UiDialog>
</template>
