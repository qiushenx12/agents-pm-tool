<script setup lang="ts">
import { ref } from "vue";
import UiIcon from "@/shared/UiIcon.vue";
import { useViewStore } from "../stores/viewStore";
const view = useViewStore(),
  dragging = ref(""),
  dropBefore = ref("");
function start(event: DragEvent, key: string) {
  dragging.value = key;
  dropBefore.value = "";
  event.dataTransfer?.setData("application/x-pm-column", key);
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}
function over(event: DragEvent, key: string) {
  if (!dragging.value || key === dragging.value) {
    dropBefore.value = "";
    return;
  }
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  dropBefore.value = key;
}
function leave(event: DragEvent, key: string) {
  const next = event.relatedTarget;
  if (next instanceof Node && (event.currentTarget as HTMLElement).contains(next))
    return;
  if (dropBefore.value === key) dropBefore.value = "";
}
function end() {
  dragging.value = "";
  dropBefore.value = "";
}
function drop(event: DragEvent, key: string) {
  view.moveBefore(
    event.dataTransfer?.getData("application/x-pm-column") || dragging.value,
    key,
  );
  end();
}
</script>
<template>
  <div class="menu-caption">显示字段 · 拖拽调整顺序</div>
  <div
    v-for="(column, index) in view.columns"
    :key="column.key"
    class="column-option"
    :class="{
      dragging: dragging === column.key,
      'column-drop-before': dropBefore === column.key,
    }"
    draggable="true"
    @dragstart="start($event, column.key)"
    @dragover="over($event, column.key)"
    @dragleave="leave($event, column.key)"
    @drop.prevent="drop($event, column.key)"
    @dragend="end"
  >
    <label class="column-option-label"
      ><UiIcon name="density" :size="12" class="subtle" /><input
        v-model="column.visible"
        type="checkbox"
        :disabled="column.key === 'description'"
      /><UiIcon :name="column.icon" :size="14" />{{ column.label }}</label
    >
    <span v-if="column.key === 'description'" class="subtle column-fixed"
      >始终显示</span
    >
    <button
      class="icon-btn"
      :aria-label="'前移字段：' + column.label"
      :disabled="index === 0"
      @click="view.moveColumn(column.key, -1)"
    >
      <UiIcon name="up" :size="12" />
    </button>
    <button
      class="icon-btn"
      :aria-label="'后移字段：' + column.label"
      :disabled="index === view.columns.length - 1"
      @click="view.moveColumn(column.key, 1)"
    >
      <UiIcon name="down" :size="12" />
    </button>
  </div>
  <div
    class="column-option"
    :class="{ 'column-drop-before': dropBefore === 'actions' }"
    @dragover="over($event, 'actions')"
    @dragleave="leave($event, 'actions')"
    @drop.prevent="drop($event, 'actions')"
  >
    <label class="column-option-label"
      ><UiIcon name="density" :size="12" class="subtle" /><input
        type="checkbox"
        checked
        disabled
      /><UiIcon name="more" :size="14" />操作</label
    ><span class="subtle column-fixed">固定末列</span>
  </div>
  <div class="menu-divider"></div>
  <button class="menu-item" @click="view.reset()">
    <UiIcon name="refresh" :size="14" />恢复默认布局
  </button>
</template>
