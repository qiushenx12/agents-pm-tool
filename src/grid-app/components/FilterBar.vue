<script setup lang="ts">
import { computed, ref, watch } from "vue";
import UiIcon from "@/shared/UiIcon.vue";
import ColumnSettings from "./ColumnSettings.vue";
import type { GroupField } from "@/shared/types";
import { priorityTones } from "@/shared/taskOptions";
import UiPopover from "@/shared/UiPopover.vue";
import { useMetaStore } from "../stores/metaStore";
import { useTaskStore } from "../stores/taskStore";
import { useViewStore } from "../stores/viewStore";
const emit = defineEmits<{ create: [] }>();
const tasks = useTaskStore(),
  meta = useMetaStore(),
  view = useViewStore();
const priorityTone = (value: string) => priorityTones[value] ?? "gray";
const groups = [
  { key: "project", label: "项目", icon: "folder" },
  { key: "type", label: "任务类型", icon: "tag" },
  { key: "priority", label: "优先级", icon: "flag" },
  { key: "status", label: "当前状态", icon: "review" },
  { key: "submitter", label: "提交人", icon: "user" },
] as const;
type ConditionKey = (typeof groups)[number]["key"];
function options(key: ConditionKey): readonly string[] {
  return key === "project"
    ? meta.projects.map((p) => p.name)
    : key === "type"
      ? meta.taskTypes
      : key === "priority"
        ? meta.priorities
        : key === "status"
          ? meta.taskStatuses
          : meta.submitters;
}
/** 提交人组拆两层：大类（用户/Agent）与具体用户名（含已选但暂无候选的）。 */
const submitterUserOptions = computed(() => {
  const selected = tasks.filters.submitter.filter(
    (v) => !meta.submitters.includes(v as never),
  );
  return [...new Set([...meta.submitterNames, ...selected])];
});
const activeConditionKeys = ref<ConditionKey[]>(
  groups
    .filter((group) => tasks.filters[group.key].length > 0)
    .map((group) => group.key),
);
const activeGroups = computed(() =>
  activeConditionKeys.value.flatMap((key) => {
    const group = groups.find((item) => item.key === key);
    return group ? [group] : [];
  }),
);
const availableGroups = computed(() =>
  groups.filter((group) => !activeConditionKeys.value.includes(group.key)),
);
/** 外部恢复出的非空筛选要补成条件行；值清空后则保留条件，等待用户重新选择。 */
watch(
  () => groups.map((group) => tasks.filters[group.key].length),
  (lengths) => {
    groups.forEach((group, index) => {
      if (
        lengths[index] > 0 &&
        !activeConditionKeys.value.includes(group.key)
      ) {
        activeConditionKeys.value.push(group.key);
      }
    });
  },
);
function addCondition(key: ConditionKey) {
  if (!activeConditionKeys.value.includes(key))
    activeConditionKeys.value.push(key);
}
function toggleFilterBuilder(toggle: () => void, open: boolean) {
  if (!open && activeConditionKeys.value.length === 0) addCondition(groups[0].key);
  toggle();
}
function fieldChoices(current: ConditionKey) {
  return groups.filter(
    (group) =>
      group.key === current || !activeConditionKeys.value.includes(group.key),
  );
}
function clearConditionValues(key: ConditionKey) {
  (tasks.filters[key] as string[]).splice(0);
  if (key === "status") tasks.filters.status_mode = "include";
}
function replaceCondition(current: ConditionKey, next: ConditionKey) {
  if (current === next) return;
  const index = activeConditionKeys.value.indexOf(current);
  if (index < 0 || activeConditionKeys.value.includes(next)) return;
  clearConditionValues(current);
  activeConditionKeys.value.splice(index, 1, next);
}
function removeCondition(key: ConditionKey) {
  const index = activeConditionKeys.value.indexOf(key);
  if (index >= 0) activeConditionKeys.value.splice(index, 1);
  clearConditionValues(key);
}
function clearAllConditions() {
  tasks.clearFilters();
  activeConditionKeys.value = [];
}
function optionSections(key: ConditionKey) {
  if (key === "submitter") {
    return [
      { label: "提交来源", values: [...meta.submitters] as string[] },
      { label: "具体提交人", values: submitterUserOptions.value },
    ].filter((section) => section.values.length > 0);
  }
  const selected = tasks.filters[key] as string[];
  return [
    {
      label: "",
      values: [...new Set([...options(key), ...selected])],
    },
  ];
}
function conditionSummary(key: ConditionKey) {
  const values = tasks.filters[key] as string[];
  if (!values.length) return `选择${groups.find((group) => group.key === key)?.label}`;
  return values.length === 1 ? values[0] : `${values[0]} 等 ${values.length} 项`;
}
const filterValueCount = computed(() =>
  groups.reduce(
    (total, group) => total + tasks.filters[group.key].length,
    0,
  ),
);
const sorts = [
  { value: "created_at", label: "创建时间", icon: "clock" },
  { value: "finished_at", label: "完成时间", icon: "clock" },
  { value: "updated_at", label: "更新时间", icon: "clock" },
  { value: "priority", label: "优先级", icon: "flag" },
  { value: "seq", label: "创建顺序", icon: "sort" },
  { value: "manual", label: "手动排序", icon: "grip" },
] as const;
/** 切入手动排序时先以当前视图为基线重铺位置，再从现状开始拖 */
async function chooseSort(value: (typeof sorts)[number]["value"]) {
  if (value === "manual" && tasks.filters.sort_by !== "manual") {
    try {
      await tasks.rebaseManualOrder();
    } catch {
      return; // 已 toast，保持当前排序
    }
  }
  tasks.filters.sort_by = value;
}
function frozenInput(event: Event) {
  const input = event.target as HTMLInputElement;
  if (/^[0-7]$/.test(input.value)) view.frozenColumns = Number(input.value);
  else input.value = String(view.frozenColumns);
}
function frozenBeforeInput(event: InputEvent) {
  const input = event.target as HTMLInputElement;
  if (event.data === null) return;
  const start = input.selectionStart ?? input.value.length;
  const end = input.selectionEnd ?? input.value.length;
  const next = input.value.slice(0, start) + event.data + input.value.slice(end);
  if (!/^[0-7]$/.test(next))
    event.preventDefault();
}
</script>
<template>
  <div class="table-toolbar">
    <button class="btn btn-primary btn-sm" @click="emit('create')">
      <UiIcon name="plus" :size="15" />新建任务
    </button>
    <span class="toolbar-divider"></span>
    <UiPopover :width="560" label="筛选任务"
      ><template #trigger="{ toggle, open }"
        ><button
          class="btn btn-ghost btn-sm"
          :class="{ active: tasks.activeFilterCount > 0 || open }"
          :aria-expanded="open"
          @click="toggleFilterBuilder(toggle, open)"
        >
          <UiIcon name="filter" :size="15" />筛选<span
            v-if="filterValueCount"
            class="tool-count"
            >{{ filterValueCount }}</span
          >
        </button></template
      ><template #default>
        <div class="menu-title filter-builder-title">
          <span class="filter-builder-heading"
            >设置筛选条件<UiIcon name="info" :size="14"
          /></span>
          <span>同一字段可多选；当前状态支持包含或不包含</span>
        </div>
        <div v-if="activeGroups.length" class="filter-condition-list">
          <div
            v-for="group in activeGroups"
            :key="group.key"
            class="filter-condition"
            :data-filter-key="group.key"
          >
            <UiPopover :width="210" label="选择筛选字段" anchor-class="filter-field-anchor"
              ><template #trigger="{ toggle, open: fieldOpen }"
                ><button
                  type="button"
                  class="filter-condition-control filter-field-control"
                  :aria-expanded="fieldOpen"
                  :aria-label="`更换筛选字段：${group.label}`"
                  @click="toggle"
                >
                  <UiIcon :name="group.icon" :size="14" />
                  <span>{{ group.label }}</span>
                  <UiIcon name="chevron" :size="13" /></button
                ></template
              ><template #default="{ close }">
                <div class="menu-caption">选择筛选字段</div>
                <button
                  v-for="choice in fieldChoices(group.key)"
                  :key="choice.key"
                  type="button"
                  class="menu-item"
                  :aria-selected="choice.key === group.key"
                  @click="
                    replaceCondition(group.key, choice.key);
                    close();
                  "
                >
                  <UiIcon :name="choice.icon" :size="14" />{{ choice.label
                  }}<UiIcon
                    v-if="choice.key === group.key"
                    name="check"
                    class="menu-check"
                  /></button></template
            ></UiPopover>
            <UiPopover
              v-if="group.key === 'status'"
              :width="150"
              label="当前状态筛选方式"
              anchor-class="filter-operator-anchor"
            >
              <template #trigger="{ toggle, open: modeOpen }"
                ><button
                  type="button"
                  class="filter-condition-control filter-operator-control"
                  aria-label="切换当前状态筛选方式"
                  :aria-expanded="modeOpen"
                  @click="toggle"
                >
                  {{ tasks.filters.status_mode === "exclude" ? "不包含" : "包含" }}
                  <UiIcon name="chevron" :size="13" /></button
                ></template
              ><template #default="{ close }">
                <button
                  v-for="mode in [
                    { value: 'include', label: '包含' },
                    { value: 'exclude', label: '不包含' },
                  ] as const"
                  :key="mode.value"
                  type="button"
                  class="menu-item"
                  :aria-selected="tasks.filters.status_mode === mode.value"
                  @click="
                    tasks.filters.status_mode = mode.value;
                    close();
                  "
                >
                  {{ mode.label
                  }}<UiIcon
                    v-if="tasks.filters.status_mode === mode.value"
                    name="check"
                    class="menu-check"
                  /></button></template
            ></UiPopover>
            <span v-else class="filter-condition-control filter-operator-static"
              >等于</span
            >
            <UiPopover :width="270" :label="`选择${group.label}`" anchor-class="filter-value-anchor"
              ><template #trigger="{ toggle, open: valueOpen }"
                ><button
                  type="button"
                  class="filter-condition-control filter-value-control"
                  :class="{
                    placeholder: !(tasks.filters[group.key] as string[]).length,
                  }"
                  :aria-expanded="valueOpen"
                  @click="toggle"
                >
                  <span>{{ conditionSummary(group.key) }}</span>
                  <UiIcon name="chevron" :size="13" /></button
                ></template
              ><template #default>
                <div class="menu-title filter-value-title">选择{{ group.label }}</div>
                <div class="filter-value-options">
                  <template
                    v-for="section in optionSections(group.key)"
                    :key="section.label"
                  >
                    <div v-if="section.label" class="menu-caption filter-value-caption">
                      {{ section.label }}
                    </div>
                    <label
                      v-for="option in section.values"
                      :key="option"
                      class="menu-item"
                      ><input
                        type="checkbox"
                        :checked="
                          (tasks.filters[group.key] as string[]).includes(option)
                        "
                        @change="tasks.toggleFilter(group.key, option)"
                      /><span
                        v-if="group.key === 'project'"
                        class="option-dot"
                        :style="{ background: meta.projectColor(option) }"
                      ></span
                      ><span
                        v-else-if="group.key === 'priority'"
                        class="option-dot"
                        :class="'dot-' + priorityTone(option)"
                      ></span
                      ><span>{{ option }}</span></label
                    >
                  </template>
                  <div
                    v-if="!optionSections(group.key).length"
                    class="menu-empty"
                  >
                    暂无可选项
                  </div>
                </div></template
            ></UiPopover>
            <button
              type="button"
              class="icon-btn filter-condition-remove"
              :aria-label="`移除${group.label}筛选条件`"
              @click="removeCondition(group.key)"
            >
              <UiIcon name="close" :size="13" />
            </button>
          </div>
        </div>
        <div v-else class="filter-builder-empty">尚未添加筛选条件</div>
        <div class="filter-builder-actions">
          <UiPopover :width="220" label="添加筛选条件"
            ><template #trigger="{ toggle, open: addOpen }"
              ><button
                type="button"
                class="add-filter-condition"
                :aria-expanded="addOpen"
                @click="toggle"
              >
                <UiIcon name="plus" :size="15" />添加条件
              </button></template
            ><template #default="{ close }">
              <div class="menu-caption">选择筛选字段</div>
              <button
                v-for="group in availableGroups"
                :key="group.key"
                type="button"
                class="menu-item"
                @click="
                  addCondition(group.key);
                  close();
                "
              >
                <UiIcon :name="group.icon" :size="14" />{{ group.label }}
              </button>
              <div v-if="!availableGroups.length" class="menu-empty">
                所有字段均已添加
              </div></template
          ></UiPopover>
          <button
            v-if="filterValueCount"
            type="button"
            class="clear-filters filter-builder-clear"
            @click="clearAllConditions"
          >
            清空条件
          </button>
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
          @click="chooseSort(sort.value)"
        >
          <UiIcon :name="sort.icon" :size="14" />{{ sort.label
          }}<UiIcon
            v-if="tasks.filters.sort_by === sort.value"
            name="check"
            class="menu-check"
          />
        </button>
        <div v-if="tasks.filters.sort_by === 'manual'" class="menu-empty">
          以当前顺序为起点，拖动行首把手调整
        </div>
        <template v-else>
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
          </button>
        </template> </template
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
    <UiPopover :width="220" label="冻结列设置"
      ><template #trigger="{ toggle, open }"
        ><button
          class="btn btn-ghost btn-sm"
          :aria-expanded="open"
          @click="toggle"
        >
          <UiIcon name="columns" :size="15" />冻结
        </button></template
      ><template #default>
        <label class="freeze-setting" for="frozen-column-count">
          <span>冻结左侧列数</span>
          <input
            id="frozen-column-count"
            type="text"
            inputmode="numeric"
            pattern="[0-7]"
            maxlength="1"
            autocomplete="off"
            aria-label="冻结左侧列数"
            :value="view.frozenColumns"
            @focus="($event.target as HTMLInputElement).select()"
            @beforeinput="frozenBeforeInput"
            @input="frozenInput"
          />
        </label>
        <div class="menu-empty">0–7 列；宽度不足时自动减少实际冻结列数</div>
      </template></UiPopover>
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
</template>
