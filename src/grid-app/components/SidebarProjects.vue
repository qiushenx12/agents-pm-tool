<script setup lang="ts">
import { ref } from "vue";
import UiIcon from "@/shared/UiIcon.vue";
import { errorText, notify } from "@/shared/feedback";
import { api } from "../api/client";
import { useMetaStore } from "../stores/metaStore";

const props = defineProps<{ activeProject: string; isAdmin: boolean }>();
const emit = defineEmits<{
  select: [name: string];
  create: [];
  edit: [name: string];
}>();
const meta = useMetaStore();
const dragging = ref("");
const dropTarget = ref("");
const dropAfter = ref(false);
const reordering = ref(false);

function startDrag(event: DragEvent, name: string) {
  if (!props.isAdmin || reordering.value) {
    event.preventDefault();
    return;
  }
  dragging.value = name;
  event.dataTransfer?.setData("text/plain", name);
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}

function dragOver(event: DragEvent, name: string) {
  if (!dragging.value || dragging.value === name) return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  const row = event.currentTarget as HTMLElement;
  const bounds = row.getBoundingClientRect();
  dropTarget.value = name;
  dropAfter.value = event.clientY > bounds.top + bounds.height / 2;
}

function clearDrag() {
  dragging.value = "";
  dropTarget.value = "";
  dropAfter.value = false;
}

async function persistOrder(names: string[]) {
  if (reordering.value) return;
  const previous = [...meta.projects];
  if (names.every((name, index) => name === previous[index]?.name)) {
    clearDrag();
    return;
  }
  const byName = new Map(previous.map((project) => [project.name, project]));
  const ordered = names.map((name, index) => ({
    ...byName.get(name)!,
    sort_order: index,
  }));
  meta.projects = ordered;
  reordering.value = true;
  clearDrag();
  try {
    for (const project of ordered) {
      const before = byName.get(project.name);
      if (before?.sort_order !== project.sort_order)
        await api.patchProject(project.name, {
          sort_order: project.sort_order,
        });
    }
    await meta.refresh();
    notify("项目顺序已更新", "success");
  } catch (e) {
    meta.projects = previous;
    await meta.refresh();
    notify(errorText(e), "error");
  } finally {
    reordering.value = false;
  }
}

function drop(event: DragEvent, target: string) {
  event.preventDefault();
  const source = dragging.value || event.dataTransfer?.getData("text/plain");
  if (!source || source === target) {
    clearDrag();
    return;
  }
  const names = meta.projects.map((project) => project.name);
  const sourceIndex = names.findIndex((name) => name === source);
  if (sourceIndex < 0) {
    clearDrag();
    return;
  }
  names.splice(sourceIndex, 1);
  const targetIndex = names.findIndex((name) => name === target);
  names.splice(targetIndex + (dropAfter.value ? 1 : 0), 0, source);
  void persistOrder(names);
}

function moveWithKeyboard(name: string, direction: -1 | 1) {
  if (!props.isAdmin || reordering.value) return;
  const names = meta.projects.map((project) => project.name);
  const index = names.indexOf(name);
  const target = index + direction;
  if (index < 0 || target < 0 || target >= names.length) return;
  [names[index], names[target]] = [names[target], names[index]];
  void persistOrder(names);
}
</script>

<template>
  <nav class="workspace-nav project-nav" aria-label="项目列表">
    <div
      v-for="project in meta.projects"
      :key="project.name"
      class="project-nav-row"
      :class="{
        dragging: dragging === project.name,
        'drop-before': dropTarget === project.name && !dropAfter,
        'drop-after': dropTarget === project.name && dropAfter,
      }"
      :draggable="props.isAdmin && !reordering"
      @dragstart="startDrag($event, project.name)"
      @dragover="dragOver($event, project.name)"
      @drop="drop($event, project.name)"
      @dragend="clearDrag"
    >
      <span
        v-if="props.isAdmin"
        class="project-drag-handle"
        title="拖拽调整项目顺序"
      >
        <UiIcon name="grip" :size="13" />
      </span>
      <button
        class="nav-item project-nav-link"
        :class="{ active: props.activeProject === project.name }"
        :aria-current="props.activeProject === project.name ? 'page' : undefined"
        :title="project.name"
        @click="emit('select', project.name)"
        @keydown.alt.up.prevent="moveWithKeyboard(project.name, -1)"
        @keydown.alt.down.prevent="moveWithKeyboard(project.name, 1)"
      >
        <span class="project-symbol" :style="{ color: project.color }"
          ><UiIcon name="folder" /></span
        ><span class="nav-label">{{ project.name }}</span>
      </button>
      <button
        v-if="props.isAdmin"
        class="icon-btn project-row-actions"
        :aria-label="'项目设置：' + project.name"
        :title="'项目设置：' + project.name"
        draggable="false"
        @click.stop="emit('edit', project.name)"
      >
        <UiIcon name="more" :size="15" />
      </button>
    </div>
  </nav>
  <button
    v-if="props.isAdmin && !meta.projects.length && !meta.error"
    class="nav-item subtle"
    @click="emit('create')"
  >
    <UiIcon name="plus" /><span>创建第一个项目</span>
  </button>
</template>
