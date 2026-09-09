<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from "vue";
import { useTaskStore } from "../stores/taskStore";
import { askConfirm, errorText } from "@/shared/feedback";
const props = defineProps<{
  taskId: string;
  value: string;
  inline?: boolean;
  field?: "description" | "note";
}>();
const emit = defineEmits<{ close: []; saved: [] }>();
const tasks = useTaskStore();
const draft = ref(props.value),
  baseline = ref(props.value),
  error = ref(""),
  saving = ref(false);
const input = ref<HTMLTextAreaElement>();
const root = ref<HTMLElement>();
let ended = false;
const field = computed(() => props.field ?? "description");
const fieldLabel = computed(() =>
  field.value === "note" ? "备注" : "任务描述",
);
const changedExternally = computed(() => props.value !== baseline.value);
const dirty = computed(() => draft.value !== baseline.value);
async function save(explicit = true) {
  if (saving.value || ended) return;
  if (!dirty.value) {
    ended = true;
    emit("close");
    return;
  }
  if (changedExternally.value) {
    if (!explicit) return;
    if (
      !(await askConfirm(
        fieldLabel.value + "已更新",
        "其他操作已修改了这段" +
          fieldLabel.value +
          "。你的草稿仍被保留，确认用当前草稿覆盖最新内容？",
        "保存草稿",
      ))
    )
      return;
  }
  saving.value = true;
  error.value = "";
  try {
    await tasks.updateTask(
      props.taskId,
      field.value === "note"
        ? { note: draft.value }
        : { description: draft.value },
    );
    ended = true;
    emit("saved");
    emit("close");
  } catch (e) {
    error.value = errorText(e);
  } finally {
    saving.value = false;
  }
}
function cancel() {
  if (!saving.value) {
    ended = true;
    emit("close");
  }
}
async function blur(e: FocusEvent) {
  if (!props.inline || root.value?.contains(e.relatedTarget as Node)) return;
  await nextTick();
  if (!root.value?.contains(document.activeElement)) void save(false);
}
onMounted(() => {
  input.value?.focus();
  input.value?.setSelectionRange(draft.value.length, draft.value.length);
});
defineExpose({ dirty, saving, save, cancel });
</script>
<template>
  <div
    ref="root"
    class="description-editor"
    :class="{ 'inline-editor': inline }"
    @focusout="blur"
    @click.stop
    @keydown.stop
  >
    <div v-if="changedExternally" class="info-banner">
      {{ fieldLabel }}已被其他操作更新，当前草稿已保留。保存前请确认。
    </div>
    <textarea
      ref="input"
      v-model="draft"
      class="input"
      :aria-label="'编辑' + fieldLabel"
      rows="5"
      :disabled="saving"
      @keydown.esc.prevent="cancel"
      @keydown.enter.ctrl.prevent="save()"
      @keydown.enter.meta.prevent="save()"
    />
    <div v-if="error" class="form-error" role="alert">
      {{ error }}，输入已保留。
    </div>
    <div class="editor-footer">
      <span>Ctrl + Enter 保存 · Esc 取消</span>
      <div class="inline-actions">
        <button class="btn btn-sm btn-ghost" :disabled="saving" @click="cancel">
          取消</button
        ><button
          class="btn btn-primary btn-sm"
          :disabled="saving"
          @click="save()"
        >
          {{ saving ? "保存中…" : error ? "重试保存" : "保存" }}
        </button>
      </div>
    </div>
  </div>
</template>
