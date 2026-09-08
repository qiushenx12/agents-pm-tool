<script setup lang="ts">
import { computed } from "vue";
import UiIcon from "@/shared/UiIcon.vue";
import ColumnSettings from "./ColumnSettings.vue";
import type { GroupField } from "@/shared/types";
import UiPopover from "@/shared/UiPopover.vue";
import { useMetaStore } from "../stores/metaStore";
import { useTaskStore } from "../stores/taskStore";
import { useViewStore } from "../stores/viewStore";
const emit = defineEmits<{ create: [] }>();
const tasks = useTaskStore(),
  meta = useMetaStore(),
  view = useViewStore();
const groups = [
  { key: "project", label: "项目" },
  { key: "type", label: "任务类型" },
  { key: "status", label: "当前状态" },
  { key: "submitter", label: "提交人" },
] as const;
function options(key: string): readonly string[] {
  return key === "project"
    ? meta.projects.map((p) => p.name)
    : key === "type"
      ? meta.taskTypes
      : key === "status"
        ? meta.taskStatuses
        : meta.submitters;
}
const chips = computed(() =>
  groups.flatMap((g) =>
    tasks.filters[g.key].map((value) => ({
      key: g.key,
      label: g.label,
      value,
    })),
  ),
);
const sorts = [
  { value: "created_at", label: "创建时间" },
  { value: "finished_at", label: "完成时间" },
  { value: "updated_at", label: "更新时间" },
  { value: "seq", label: "创建顺序" },
] as const;
</script>
<template>
  <div class="table-toolbar">
    <button class="btn btn-primary btn-sm" @click="emit('create')">
      <UiIcon name="plus" :size="15" />新建任务
    </button>
    <span class="toolbar-divider"></span>
    <UiPopover :width="300" label="筛选任务"
      ><template #trigger="{ toggle, open }"
        ><button
          class="btn btn-ghost btn-sm"
          :class="{ active: tasks.activeFilterCount > 0 || open }"
          :aria-expanded="open"
          @click="toggle"
        >
          <UiIcon name="filter" :size="15" />筛选<span
            v-if="chips.length"
            class="tool-count"
            >{{ chips.length }}</span
          >
        </button></template
      ><template #default>
        <div class="menu-title">
          筛选记录<span>同类多选，满足任一条件</span>
        </div>
        <div v-for="group in groups" :key="group.key" class="filter-section">
          <div class="menu-caption">{{ group.label }}</div>
          <label
            v-for="option in options(group.key)"
            :key="option"
            class="menu-item"
            ><input
              type="checkbox"
              :checked="(tasks.filters[group.key] as string[]).includes(option)"
              @change="tasks.toggleFilter(group.key, option)"
            /><span
              v-if="group.key === 'project'"
              class="option-dot"
              :style="{ background: meta.projectColor(option) }"
            ></span
            ><span>{{ option }}</span></label
          >
          <div v-if="!options(group.key).length" class="menu-empty">
            暂无项目
          </div>
        </div>
      </template></UiPopover
    >
    <UiPopover label="排序设置"
      ><template #trigger="{ toggle, open }"
        ><button
          class="btn btn-ghost btn-sm"
          :aria-expanded="open"
          @click="toggle"
        >
          <UiIcon name="sort" :size="15" />排序
        </button></template
      ><template #default>
        <div class="menu-caption">排序字段</div>
        <button
          v-for="sort in sorts"
          :key="sort.value"
          class="menu-item"
          @click="tasks.filters.sort_by = sort.value"
        >
          <UiIcon name="clock" :size="14" />{{ sort.label
          }}<UiIcon
            v-if="tasks.filters.sort_by === sort.value"
            name="check"
            class="menu-check"
          />
        </button>
        <div class="menu-divider"></div>
        <button class="menu-item" @click="tasks.filters.sort_order = 'asc'">
          <UiIcon name="up" />升序<UiIcon
            v-if="tasks.filters.sort_order === 'asc'"
            name="check"
            class="menu-check"
          /></button
        ><button class="menu-item" @click="tasks.filters.sort_order = 'desc'">
          <UiIcon name="down" />降序<UiIcon
            v-if="tasks.filters.sort_order === 'desc'"
            name="check"
            class="menu-check"
          />
        </button> </template
    ></UiPopover>
    <UiPopover :width="310" label="字段设置"
      ><template #trigger="{ toggle, open }"
        ><button
          class="btn btn-ghost btn-sm"
          :aria-expanded="open"
          @click="toggle"
        >
          <UiIcon name="columns" :size="15" />字段
        </button></template
      ><template #default><ColumnSettings /></template
    ></UiPopover>
    <UiPopover :width="210" label="分组设置"
      ><template #trigger="{ toggle, open }"
        ><button
          class="btn btn-ghost btn-sm"
          :class="{ active: !!tasks.filters.group_by }"
          :aria-expanded="open"
          @click="toggle"
        >
          <UiIcon name="layers" :size="15" />分组
        </button></template
      ><template #default="{ close }">
        <div class="menu-caption">按字段分组</div>
        <button
          class="menu-item"
          @click="
            tasks.filters.group_by = '';
            close();
          "
        >
          不分组<UiIcon
            v-if="!tasks.filters.group_by"
            name="check"
            class="menu-check"
          /></button
        ><button
          v-for="group in groups"
          :key="group.key"
          class="menu-item"
          @click="
            tasks.filters.group_by = group.key as GroupField;
            close();
          "
        >
          {{ group.label
          }}<UiIcon
            v-if="tasks.filters.group_by === group.key"
            name="check"
            class="menu-check"
          />
        </button> </template
    ></UiPopover>
    <UiPopover :width="180" label="行高设置"
      ><template #trigger="{ toggle, open }"
        ><button
          class="btn btn-ghost btn-sm density-button"
          :aria-expanded="open"
          @click="toggle"
        >
          <UiIcon name="density" :size="15" />行高
        </button></template
      ><template #default="{ close }"
        ><div class="menu-caption">行高</div>
        <button
          v-for="item in [
            { value: 32, label: '紧凑' },
            { value: 36, label: '标准' },
            { value: 44, label: '宽松' },
          ]"
          :key="item.value"
          class="menu-item"
          @click="
            view.density = item.value;
            close();
          "
        >
          {{ item.label
          }}<UiIcon
            v-if="view.density === item.value"
            name="check"
            class="menu-check"
          /></button></template
    ></UiPopover>
    <div class="table-search">
      <UiIcon name="search" :size="15" /><input
        id="task-search"
        v-model="tasks.filters.keyword"
        aria-label="搜索任务"
        placeholder="搜索 ID 或任务描述"
      /><button
        v-if="tasks.filters.keyword"
        class="icon-btn"
        aria-label="清空搜索"
        @click="tasks.filters.keyword = ''"
      >
        <UiIcon name="close" :size="12" /></button
      ><kbd v-else>Ctrl K</kbd>
    </div>
  </div>
  <div v-if="chips.length || tasks.filters.keyword" class="filter-chips">
    <span class="subtle">筛选条件</span
    ><button
      v-for="chip in chips"
      :key="chip.key + chip.value"
      class="filter-chip"
      :aria-label="'移除筛选：' + chip.label + ' ' + chip.value"
      @click="tasks.toggleFilter(chip.key, chip.value)"
    >
      {{ chip.label }}<span>{{ chip.value }}</span
      ><UiIcon name="close" :size="12" /></button
    ><button
      v-if="tasks.filters.keyword"
      class="filter-chip"
      @click="tasks.filters.keyword = ''"
    >
      搜索<span>{{ tasks.filters.keyword }}</span
      ><UiIcon name="close" :size="12" /></button
    ><button class="clear-filters" @click="tasks.clearFilters()">
      清空条件
    </button>
  </div>
</template>
