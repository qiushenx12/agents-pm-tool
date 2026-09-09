<script setup lang="ts">
import { ref } from "vue";
import UiIcon from "@/shared/UiIcon.vue";
import { useViewStore } from "../stores/viewStore";
const view = useViewStore(),
  dragging = ref("");
function start(event: DragEvent, key: string) {
  if (key === "description" || key === "note") {
    event.preventDefault();
    return;
  }
  dragging.value = key;
  event.dataTransfer?.setData("application/x-pm-column", key);
}
function drop(event: DragEvent, key: string) {
  view.moveBefore(
    event.dataTransfer?.getData("application/x-pm-column") || dragging.value,
    key,
  );
  dragging.value = "";
}
</script>
<template>
  <div class="menu-caption">显示字段 · 拖拽调整顺序</div>
  <div
    v-for="(column, index) in view.columns"
    :key="column.key"
    class="column-option"
    :class="{ dragging: dragging === column.key }"
    :draggable="column.key !== 'description' && column.key !== 'note'"
    @dragstart="start($event, column.key)"
    @dragover.prevent
    @drop.prevent="drop($event, column.key)"
    @dragend="dragging = ''"
  >
    <label class="column-option-label"
      ><UiIcon name="density" :size="12" class="subtle" /><input
        v-model="column.visible"
        type="checkbox"
        :disabled="column.key === 'description'"
      /><UiIcon :name="column.icon" :size="14" />{{ column.label }}</label
    >
    <span
      v-if="column.key === 'description' || column.key === 'note'"
      class="subtle column-fixed"
      >{{ column.key === "description" ? "固定主列" : "固定末列" }}</span
    >
    <template v-else
      ><button
        class="icon-btn"
        :aria-label="'前移字段：' + column.label"
        :disabled="index <= 1"
        @click="view.moveColumn(column.key, -1)"
      >
        <UiIcon name="up" :size="12" /></button
      ><button
        class="icon-btn"
        :aria-label="'后移字段：' + column.label"
        :disabled="index === view.columns.length - 1"
        @click="view.moveColumn(column.key, 1)"
      >
        <UiIcon name="down" :size="12" /></button
    ></template>
  </div>
  <div class="menu-divider"></div>
  <button class="menu-item" @click="view.reset()">
    <UiIcon name="refresh" :size="14" />恢复默认布局
  </button>
</template>
