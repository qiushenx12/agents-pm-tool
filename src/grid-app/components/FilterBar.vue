<script setup lang="ts">
import { ref } from "vue";
import { useMetaStore } from "@/grid-app/stores/metaStore";
import { useTaskStore } from "@/grid-app/stores/taskStore";

const taskStore = useTaskStore();
const metaStore = useMetaStore();

const openDropdown = ref<string | null>(null);

function toggleDropdown(key: string) {
  openDropdown.value = openDropdown.value === key ? null : key;
}

function closeDropdown() {
  openDropdown.value = null;
}

function onKeywordInput(e: Event) {
  taskStore.filters.keyword = (e.target as HTMLInputElement).value;
}

const groups = [
  { key: "project" as const, label: "项目" },
  { key: "type" as const, label: "类型" },
  { key: "status" as const, label: "状态" },
  { key: "submitter" as const, label: "提交人" },
];

function optionsOf(key: string): string[] {
  switch (key) {
    case "project":
      return metaStore.projects.map((p) => p.name);
    case "type":
      return [...metaStore.taskTypes];
    case "status":
      return [...metaStore.taskStatuses];
    case "submitter":
      return [...metaStore.submitters];
    default:
      return [];
  }
}

function selectedCount(key: "project" | "type" | "status" | "submitter") {
  return taskStore.filters[key].length;
}
</script>

<template>
  <div class="filter-bar" @click.self="closeDropdown">
    <div v-for="g in groups" :key="g.key" class="filter-group">
      <button
        class="filter-btn"
        :class="{ active: selectedCount(g.key) > 0 }"
        @click.stop="toggleDropdown(g.key)"
      >
        {{ g.label }}{{ selectedCount(g.key) ? `(${selectedCount(g.key)})` : "" }} ▾
      </button>
      <div v-if="openDropdown === g.key" class="filter-dropdown" @click.stop>
        <label v-for="opt in optionsOf(g.key)" :key="opt" class="filter-option">
          <input
            type="checkbox"
            :checked="taskStore.filters[g.key].includes(opt as never)"
            @change="taskStore.toggleFilter(g.key, opt)"
          />
          <span
            v-if="g.key === 'project'"
            class="option-dot"
            :style="{ background: metaStore.projectColor(opt) }"
          ></span>
          {{ opt }}
        </label>
        <div v-if="!optionsOf(g.key).length" class="filter-empty">暂无选项</div>
      </div>
    </div>

    <input
      class="input filter-keyword"
      :value="taskStore.filters.keyword"
      placeholder="搜索 ID / 描述…"
      @input="onKeywordInput"
    />
  </div>
</template>
