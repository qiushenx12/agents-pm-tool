<script setup lang="ts">
import UiIcon from "@/shared/UiIcon.vue";
import { TASK_ROW_ACTIONS } from "../taskActions";
import { useViewStore } from "../stores/viewStore";
// 右键操作列表头弹出的按钮勾选列表。默认全选；至少保留一个按钮（见 viewStore）。
const view = useViewStore();
</script>

<template>
  <div class="menu-caption">操作列按钮</div>
  <label
    v-for="action in TASK_ROW_ACTIONS"
    :key="action.key"
    class="column-option"
  >
    <span class="column-option-label"
      ><input
        type="checkbox"
        :checked="!view.hiddenActions.includes(action.key)"
        :disabled="!view.canToggleRowAction(action.key)"
        :aria-label="action.label"
        @change="view.toggleRowAction(action.key)"
      /><UiIcon :name="action.icon" :size="14" />{{ action.label }}</span
    ><span
      v-if="!view.canToggleRowAction(action.key)"
      class="subtle column-fixed"
      >至少保留一个</span
    >
  </label>
</template>
