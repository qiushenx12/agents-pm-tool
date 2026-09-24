<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import UiIcon from "@/shared/UiIcon.vue";
import UiPopover from "@/shared/UiPopover.vue";
import type { Task } from "@/shared/types";

const props = withDefaults(
  defineProps<{
    modelValue: readonly string[];
    options: readonly Task[];
    label: string;
    excludeId?: string;
    disabled?: boolean;
    loading?: boolean;
    field?: boolean;
  }>(),
  { excludeId: "", disabled: false, loading: false, field: false },
);
const emit = defineEmits<{
  "update:modelValue": [value: string[]];
  open: [];
  "trigger-click": [];
  locate: [id: string];
}>();
const search = ref("");
const searchField = ref<HTMLInputElement>();
function clearSearch() {
  search.value = "";
  searchField.value?.focus();
}
const keyword = computed(() => search.value.trim().toLowerCase());
const displayValue = computed(() => props.modelValue.join(","));
const candidates = computed(() =>
  props.options.filter((task) => task.id !== props.excludeId),
);
// 无输入时只列出已勾选的任务，未勾选时交给空态提示引导用户输入。
const checkedTasks = computed(() =>
  candidates.value.filter((task) => props.modelValue.includes(task.id)),
);
// 输入关键词后只显示命中项，不再固定保留未命中的已勾选任务。
const filtered = computed(() => {
  const value = keyword.value;
  if (!value) return checkedTasks.value;
  return candidates.value.filter(
    (task) =>
      task.id.toLowerCase().includes(value) ||
      task.description.toLowerCase().includes(value),
  );
});

function toggleValue(id: string) {
  if (props.disabled) return;
  const next = props.modelValue.includes(id)
    ? props.modelValue.filter((value) => value !== id)
    : [...props.modelValue, id];
  emit("update:modelValue", next);
}
function toggleFromClick(event: MouseEvent, open: boolean, toggle: () => void) {
  emit("trigger-click");
  // 双击计数大于 1 时保持刚打开的列表，避免用户看到它一闪而过。
  if (open && event.detail > 1) return;
  search.value = "";
  if (!open) emit("open");
  toggle();
  // 列表默认只列已勾选任务，打开后聚焦搜索框，用户可以直接输入任务 ID。
  if (!open) void focusSearch();
}
async function focusSearch() {
  await nextTick();
  searchField.value?.focus({ preventScroll: true });
}
const listPopover = ref<InstanceType<typeof UiPopover>>();
const contextMenu = ref<InstanceType<typeof UiPopover>>();
const contextTask = ref<Task | null>(null);
function openContextMenu(event: MouseEvent, task: Task) {
  // 仅在已有勾选内容时提供“定位任务”，否则保留浏览器默认右键菜单。
  if (props.disabled || !props.modelValue.length) return;
  event.preventDefault();
  event.stopPropagation();
  contextTask.value = task;
  void contextMenu.value?.openAt(event.clientX, event.clientY);
}
function locateContextTask(closeMenu: () => void) {
  const id = contextTask.value?.id;
  closeMenu();
  if (!id) return;
  listPopover.value?.close();
  emit("locate", id);
}
</script>

<template>
  <UiPopover
    ref="listPopover"
    :width="360"
    :label="label"
    :anchor-class="field ? 'dependency-anchor' : 'select-anchor'"
  >
    <template #trigger="{ toggle, open }">
      <button
        type="button"
        class="select-trigger dependency-trigger"
        :class="{ 'select-field': field }"
        :disabled="disabled"
        :aria-label="label + '：' + (displayValue || '未选择')"
        :aria-expanded="open"
        aria-haspopup="listbox"
        :title="displayValue"
        @click="toggleFromClick($event, open, toggle)"
      >
        <span class="dependency-value" :class="{ subtle: !displayValue }">{{
          displayValue || "—"
        }}</span>
        <UiIcon name="chevron" :size="13" class="select-chevron" />
      </button>
    </template>
    <template #default>
      <div class="menu-caption">{{ label }} · 可多选</div>
      <div class="menu-search">
        <UiIcon name="search" />
        <input
          ref="searchField"
          v-model="search"
          :aria-label="'搜索' + label"
          placeholder="搜索任务 ID 或描述…"
        />
        <button
          v-if="search"
          type="button"
          class="menu-search-clear"
          aria-label="清空搜索"
          title="清空搜索"
          @click="clearSearch"
        >
          <UiIcon name="close" :size="13" />
        </button>
      </div>
      <div class="dependency-options" role="listbox" :aria-label="label" aria-multiselectable="true">
        <button
          v-for="task in filtered"
          :key="task.id"
          type="button"
          class="menu-item dependency-option"
          role="option"
          :aria-selected="modelValue.includes(task.id)"
          :disabled="disabled"
          @click="toggleValue(task.id)"
          @contextmenu="openContextMenu($event, task)"
        >
          <span class="dependency-option-text">
            <strong>{{ task.id }}</strong>
            <small>{{ task.description || "未填写任务描述" }}</small>
          </span>
          <UiIcon v-if="modelValue.includes(task.id)" name="check" class="menu-check" />
        </button>
        <div v-if="loading" class="menu-empty"><span class="spinner"></span>正在加载任务</div>
        <div v-else-if="!filtered.length" class="menu-empty">
          {{ keyword ? "没有匹配的任务" : "输入任务 ID 或描述，开始搜索任务" }}
        </div>
      </div>
    </template>
  </UiPopover>
  <UiPopover ref="contextMenu" :width="150" label="任务操作">
    <template #trigger></template>
    <template #default="{ close }">
      <button class="menu-item" @click="locateContextTask(close)">
        <UiIcon name="locate" />定位任务
      </button>
    </template>
  </UiPopover>
</template>

<style scoped>
.dependency-trigger {
  min-width: 0;
  width: 100%;
}
.menu-search input {
  flex: 1;
  min-width: 0;
}
.menu-search-clear {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 18px;
  height: 18px;
  padding: 0;
  border: 0;
  border-radius: 50%;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
}
.menu-search-clear:hover,
.menu-search-clear:focus-visible {
  background: var(--hover);
  color: var(--text-primary);
}
.dependency-value {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dependency-options {
  max-height: 300px;
  overflow: auto;
}
.dependency-option {
  align-items: center;
}
.dependency-option-text {
  display: grid;
  min-width: 0;
  text-align: left;
}
.dependency-option-text strong,
.dependency-option-text small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dependency-option-text small {
  color: var(--text-secondary);
  font-weight: 400;
}
</style>
