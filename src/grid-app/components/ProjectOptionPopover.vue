<script setup lang="ts">
import { nextTick, ref } from "vue";
import { api } from "@/grid-app/api/client";
import { useMetaStore } from "@/grid-app/stores/metaStore";
import { useTaskStore } from "@/grid-app/stores/taskStore";

const emit = defineEmits<{ close: [] }>();

const metaStore = useMetaStore();
const taskStore = useTaskStore();

const newName = ref("");
const editingName = ref<string | null>(null);
const editingText = ref("");
const error = ref("");
const busy = ref(false);

const COLORS = [
  "#007AFF", "#34C759", "#FF9500", "#FF3B30", "#AF52DE",
  "#5856D6", "#00C7BE", "#A2845E", "#8E8E93", "#FF2D55",
];

async function run(fn: () => Promise<unknown>) {
  busy.value = true;
  error.value = "";
  try {
    await fn();
    await metaStore.refresh();
    await taskStore.refresh();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    busy.value = false;
  }
}

function createOption() {
  const name = newName.value.trim();
  if (!name || busy.value) return;
  void run(async () => {
    await api.createProject({ name });
    newName.value = "";
  });
}

function startRename(name: string) {
  editingName.value = name;
  editingText.value = name;
  void nextTick(() => {
    document.querySelector<HTMLInputElement>(".rename-input")?.focus();
  });
}

function commitRename(oldName: string) {
  const new_name = editingText.value.trim();
  editingName.value = null;
  if (!new_name || new_name === oldName || busy.value) return;
  void run(() => api.patchProject(oldName, { new_name }));
}

function changeColor(name: string, color: string) {
  if (busy.value) return;
  void run(() => api.patchProject(name, { color }));
}

function move(name: string, dir: -1 | 1) {
  const list = metaStore.projects;
  const i = list.findIndex((p) => p.name === name);
  const j = i + dir;
  if (i < 0 || j < 0 || j >= list.length || busy.value) return;
  void run(async () => {
    await api.patchProject(list[i].name, { sort_order: list[j].sort_order });
    await api.patchProject(list[j].name, { sort_order: list[i].sort_order });
  });
}

function remove(name: string) {
  if (busy.value) return;
  if (!confirm(`确定删除项目选项「${name}」？`)) return;
  void run(() => api.deleteProject(name));
}
</script>

<template>
  <div class="popover-mask" @click.self="emit('close')">
    <div class="option-popover" @click.stop>
      <div class="option-popover-title">项目选项</div>

      <div class="option-create">
        <input
          v-model="newName"
          class="input"
          placeholder="输入新选项名称，回车创建"
          @keydown.enter="createOption"
        />
      </div>

      <div v-if="error" class="option-error">{{ error }}</div>

      <ul class="option-list">
        <li v-for="(p, i) in metaStore.projects" :key="p.name" class="option-item">
          <span class="option-dot" :style="{ background: p.color }"></span>
          <input
            v-if="editingName === p.name"
            v-model="editingText"
            class="input rename-input"
            @keydown.enter="commitRename(p.name)"
            @keydown.esc="editingName = null"
            @blur="commitRename(p.name)"
          />
          <span v-else class="option-name" @dblclick="startRename(p.name)">{{ p.name }}</span>

          <span class="option-actions">
            <button class="icon-btn" title="重命名" @click="startRename(p.name)">✎</button>
            <button class="icon-btn" title="上移" :disabled="i === 0" @click="move(p.name, -1)">↑</button>
            <button class="icon-btn" title="下移" :disabled="i === metaStore.projects.length - 1" @click="move(p.name, 1)">↓</button>
            <button class="icon-btn danger" title="删除" @click="remove(p.name)">✕</button>
          </span>

          <span class="option-colors">
            <button
              v-for="c in COLORS"
              :key="c"
              class="color-swatch"
              :class="{ active: p.color === c }"
              :style="{ background: c }"
              @click="changeColor(p.name, c)"
            ></button>
          </span>
        </li>
      </ul>
    </div>
  </div>
</template>
