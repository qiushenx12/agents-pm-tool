<script setup lang="ts">
import { computed, ref } from "vue";
import { api } from "../../api/client";
import { errorText } from "@/shared/feedback";
import UiIcon from "@/shared/UiIcon.vue";
import type { User } from "@/shared/types";

const emit = defineEmits<{ authenticated: [user: User] }>();
const mode = ref<"login" | "register">("login");
const username = ref("");
const password = ref("");
const confirmPassword = ref("");
const error = ref("");
const busy = ref(false);
const isLoopback = computed(() =>
  ["127.0.0.1", "localhost", "::1", "[::1]"].includes(location.hostname),
);
const passwordMismatch = computed(
  () =>
    mode.value === "register" &&
    confirmPassword.value.length > 0 &&
    password.value !== confirmPassword.value,
);
const canSubmit = computed(
  () =>
    username.value.trim().length > 0 &&
    password.value.length >= 8 &&
    (mode.value === "login" ||
      (confirmPassword.value.length >= 8 && !passwordMismatch.value)),
);

function setMode(next: "login" | "register") {
  mode.value = next;
  error.value = "";
  if (next === "login") confirmPassword.value = "";
}

async function submit() {
  if (!canSubmit.value) {
    if (mode.value === "register") {
      error.value = passwordMismatch.value
        ? "两次输入的密码不一致，请重新确认"
        : "请完整填写用户名、密码和确认密码";
    }
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    const response =
      mode.value === "login"
        ? await api.login({ username: username.value, password: password.value })
        : await api.register({
            username: username.value,
            password: password.value,
          });
    emit("authenticated", response.user);
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

async function hostLogin() {
  busy.value = true;
  error.value = "";
  try {
    const response = await api.hostLogin();
    emit("authenticated", response.user);
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <main class="auth-screen">
    <section class="auth-card" aria-labelledby="auth-title">
      <aside class="auth-intro-panel">
        <div class="auth-product-mark"><UiIcon name="layers" :size="26" /></div>
        <div>
          <span class="auth-product-label">AGENTS PM TOOL</span>
          <h1>让用户与 Agent<br />在同一个工作台协作</h1>
          <p>任务、权限与进度都留在你的本地环境中。</p>
        </div>
        <ul class="auth-feature-list">
          <li><span><UiIcon name="folder" :size="15" /></span><div><strong>按项目授权</strong><small>只查看和操作被授权的项目</small></div></li>
          <li><span><UiIcon name="settings" :size="15" /></span><div><strong>细粒度权限</strong><small>字段、状态和附件权限独立控制</small></div></li>
          <li><span><UiIcon name="bot" :size="15" /></span><div><strong>独立 Agent 接入</strong><small>每位用户拥有自己的访问凭据</small></div></li>
        </ul>
        <div class="auth-local-note"><UiIcon name="check" :size="14" />数据默认保存在本机</div>
      </aside>

      <div class="auth-form-panel">
        <header class="auth-form-heading">
          <span>{{ mode === "login" ? "欢迎回来" : "创建新账号" }}</span>
          <h2 id="auth-title">{{ mode === "login" ? "登录工作台" : "注册账号" }}</h2>
          <p>{{ mode === "login" ? "使用命名账号继续访问任务。" : "请确认两次密码输入一致，注册后等待管理员授权。" }}</p>
        </header>

        <button
          v-if="isLoopback"
          class="btn btn-primary auth-host-button"
          :disabled="busy"
          @click="hostLogin"
        >
          <UiIcon name="window" />以主机账号快速进入
        </button>
        <div v-if="isLoopback" class="auth-divider"><span>或使用命名账号</span></div>

        <div class="auth-tabs" role="tablist" aria-label="账号操作">
          <button
            type="button"
            role="tab"
            :aria-selected="mode === 'login'"
            :class="{ active: mode === 'login' }"
            @click="setMode('login')"
          >
            登录
          </button>
          <button
            type="button"
            role="tab"
            :aria-selected="mode === 'register'"
            :class="{ active: mode === 'register' }"
            @click="setMode('register')"
          >
            注册
          </button>
        </div>
        <form class="auth-form" @submit.prevent="submit">
          <label>
            <span>用户名</span>
            <input v-model="username" class="input" maxlength="64" autocomplete="username" placeholder="输入用户名" />
          </label>
          <label>
            <span>密码</span>
            <input
              v-model="password"
              class="input"
              type="password"
              minlength="8"
              maxlength="256"
              :autocomplete="mode === 'login' ? 'current-password' : 'new-password'"
              :placeholder="mode === 'login' ? '输入密码' : '至少 8 个字符'"
            />
          </label>
          <label v-if="mode === 'register'">
            <span>确认密码</span>
            <input
              v-model="confirmPassword"
              class="input"
              :class="{ 'input-error': passwordMismatch }"
              type="password"
              minlength="8"
              maxlength="256"
              autocomplete="new-password"
              placeholder="再次输入密码"
            />
            <small v-if="passwordMismatch" class="auth-field-error">两次输入的密码不一致</small>
            <small v-else class="auth-field-hint">请再次输入相同密码，避免因输入错误无法登录。</small>
          </label>
          <div v-if="error" class="form-error" role="alert">{{ error }}</div>
          <button class="btn btn-primary auth-submit" :disabled="busy || !canSubmit">
            {{ busy ? "请稍候…" : mode === "login" ? "登录" : "创建账号并登录" }}
          </button>
        </form>
        <p v-if="mode === 'register'" class="auth-tip">
          <UiIcon name="info" :size="14" />注册后默认没有项目权限，请联系管理员授权。
        </p>
      </div>
    </section>
  </main>
</template>
