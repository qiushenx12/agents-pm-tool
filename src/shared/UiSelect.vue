<script setup lang="ts">
import { computed, ref } from "vue";
import UiPopover from "./UiPopover.vue";
import UiIcon from "./UiIcon.vue";
export interface SelectOption {
  value: string;
  label?: string;
  color?: string;
  tone?: string;
}
const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: readonly SelectOption[];
    label: string;
    field?: boolean;
    disabled?: boolean;
    placeholder?: string;
  }>(),
  { placeholder: "请选择" },
);
const emit = defineEmits<{
  "update:modelValue": [value: string];
  /** 用户在下拉列表里显式点选某项（与 v-model 变化区分：默认值不会触发） */
  pick: [value: string];
}>();
const search = ref("");
const selected = computed(() =>
  props.options.find((o) => o.value === props.modelValue),
);
const filtered = computed(() =>
  props.options.filter((o) =>
    (o.label ?? o.value).toLowerCase().includes(search.value.toLowerCase()),
  ),
);
</script>
<template>
  <UiPopover :width="240" :label="label"
    ><template #trigger="{ toggle, open }">
      <button
        type="button"
        class="select-trigger"
        :class="{ 'select-field': field }"
        :disabled="disabled"
        :aria-label="label + '：' + (modelValue || placeholder)"
        :aria-expanded="open"
        aria-haspopup="listbox"
        @click="
          search = '';
          toggle();
        "
      >
        <span
          class="field-value"
          :class="selected?.tone ? 'tag tag-' + selected.tone : ''"
          ><span
            v-if="selected?.color"
            class="option-dot"
            :style="{ background: selected.color }"
          ></span
          >{{ selected?.label || modelValue || placeholder }}</span
        ><UiIcon
          name="chevron"
          :size="13"
          class="select-chevron"
        /></button></template
    ><template #default="{ close }">
      <div class="menu-caption">{{ label }}</div>
      <div v-if="options.length > 6" class="menu-search">
        <UiIcon name="search" /><input
          v-model="search"
          :aria-label="'搜索' + label"
          placeholder="搜索选项…"
        />
      </div>
      <div role="listbox" :aria-label="label">
        <button
          v-for="option in filtered"
          :key="option.value"
          type="button"
          class="menu-item"
          role="option"
          :aria-selected="option.value === modelValue"
          @click="
            emit('update:modelValue', option.value);
            emit('pick', option.value);
            close();
          "
        >
          <span
            class="field-value"
            :class="option.tone ? 'tag tag-' + option.tone : ''"
            ><span
              v-if="option.color"
              class="option-dot"
              :style="{ background: option.color }"
            ></span
            >{{ option.label || option.value }}</span
          ><UiIcon
            v-if="option.value === modelValue"
            name="check"
            class="menu-check"
          />
        </button>
      </div>
      <div v-if="!filtered.length" class="menu-empty">没有匹配的选项</div>
    </template></UiPopover
  >
</template>
