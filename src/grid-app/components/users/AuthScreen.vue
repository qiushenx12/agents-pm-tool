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
const error = ref("");
const busy = ref(false);
const isLoopback = computed(() =>
  ["127.0.0.1", "localhost", "::1", "[::1]"].includes(location.hostname),
);

async function submit() {
  if (!username.value.trim() || !password.value) return;
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
      <div class="auth-brand"><UiIcon name="layers" :size="25" /></div>
      <h1 id="auth-title">Agents PM Tool</h1>
      <p>登录后访问已授权的项目与任务。</p>

      <button
        v-if="isLoopback"
        class="btn btn-primary auth-host-button"
        :disabled="busy"
        @click="hostLogin"
      >
        <UiIcon name="window" />以主机账号进入
      </button>
      <div v-if="isLoopback" class="auth-divider"><span>或使用命名账号</span></div>

      <div class="auth-tabs" role="tablist">
        <button
          type="button"
          :class="{ active: mode === 'login' }"
          @click="mode = 'login'"
        >
          登录
        </button>
        <button
          type="button"
          :class="{ active: mode === 'register' }"
          @click="mode = 'register'"
        >
          注册
        </button>
      </div>
      <form class="auth-form" @submit.prevent="submit">
        <label>用户名<input v-model="username" class="input" maxlength="64" autocomplete="username" /></label>
        <label
          >密码<input
            v-model="password"
            class="input"
            type="password"
            minlength="8"
            maxlength="256"
            :autocomplete="mode === 'login' ? 'current-password' : 'new-password'"
        /></label>
        <div v-if="error" class="form-error" role="alert">{{ error }}</div>
        <button
          class="btn btn-primary"
          :disabled="busy || !username.trim() || password.length < 8"
        >
          {{ busy ? "请稍候…" : mode === "login" ? "登录" : "注册并登录" }}
        </button>
      </form>
      <p v-if="mode === 'register'" class="auth-tip">
        注册后默认没有项目权限，请联系管理员授权。
      </p>
    </section>
  </main>
</template>
