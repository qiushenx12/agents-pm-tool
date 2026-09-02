<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { currentTheme, toggleTheme } from "@/shared/theme";
import { subscribeTaskEvents } from "@/grid-app/api/client";
import FilterBar from "@/grid-app/components/FilterBar.vue";
import TaskCreateModal from "@/grid-app/components/TaskCreateModal.vue";
import TaskDetailDrawer from "@/grid-app/components/TaskDetailDrawer.vue";
import TaskGrid from "@/grid-app/components/TaskGrid.vue";
import { useMetaStore } from "@/grid-app/stores/metaStore";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import type { Task } from "@/shared/types";

const taskStore = useTaskStore();
const metaStore = useMetaStore();

const showCreate = ref(false);
const detailTask = ref<Task | null>(null);
const theme = ref(currentTheme());

function onToggleTheme() {
  theme.value = toggleTheme();
}

let unsubscribe: (() => void) | undefined;

onMounted(async () => {
  await metaStore.refresh();
  await taskStore.refresh();
  unsubscribe = subscribeTaskEvents(() => {
    void taskStore.refresh();
    void metaStore.refresh();
  });
});

onBeforeUnmount(() => unsubscribe?.());
</script>

<template>
  <div class="grid-app">
    <header class="grid-toolbar">
      <h1 class="grid-title">任务表</h1>
      <div class="toolbar-actions">
        <button class="btn" title="切换明暗主题" @click="onToggleTheme">
          {{ theme === "dark" ? "☀ 浅色" : "☾ 深色" }}
        </button>
        <button class="btn btn-primary" @click="showCreate = true">＋ 新建任务</button>
      </div>
    </header>

    <FilterBar />

    <div v-if="taskStore.error" class="grid-error">{{ taskStore.error }}</div>

    <TaskGrid @open-detail="(t) => (detailTask = t)" />

    <TaskCreateModal
      v-if="showCreate"
      @close="showCreate = false"
      @created="showCreate = false"
    />

    <TaskDetailDrawer
      v-if="detailTask"
      :task="detailTask"
      @close="detailTask = null"
    />
  </div>
</template>
