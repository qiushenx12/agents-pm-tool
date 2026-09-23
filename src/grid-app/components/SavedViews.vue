<script setup lang="ts">
import { ref } from "vue";
import UiIcon from "@/shared/UiIcon.vue";
import UiPopover from "@/shared/UiPopover.vue";
import UiDialog from "@/shared/UiDialog.vue";
import { useSavedViewStore } from "../stores/savedViewStore";
import { askConfirm, errorText, notify } from "@/shared/feedback";
const saved = useSavedViewStore();
defineProps<{ mode: "table" | "graph" }>();
const emit = defineEmits<{ "update:mode": [mode: "table" | "graph"] }>();
const dialog = ref<"save" | "rename" | null>(null),
  name = ref(""),
  targetId = ref(""),
  error = ref("");
function openSave() {
  dialog.value = "save";
  name.value = "";
  error.value = "";
}
function openRename(id: string, oldName: string) {
  dialog.value = "rename";
  targetId.value = id;
  name.value = oldName;
  error.value = "";
}
function submit() {
  try {
    if (dialog.value === "rename") saved.rename(targetId.value, name.value);
    else saved.save(name.value);
    dialog.value = null;
    notify("方案已保存");
  } catch (e) {
    error.value = errorText(e);
  }
}
async function remove(id: string, label: string) {
  if (
    !(await askConfirm(
      "删除筛选方案",
      "删除「" + label + "」？任务记录不受影响。",
      "删除方案",
    ))
  )
    return;
  try {
    saved.remove(id);
    notify("方案已删除");
  } catch (e) {
    notify(errorText(e), "error");
  }
}
function update() {
  try {
    saved.update();
    notify("方案已更新");
  } catch (e) {
    notify(errorText(e), "error");
  }
}
</script>
<template>
  <div class="view-tabs saved-view-bar" role="tablist" aria-label="任务视图">
    <UiPopover :width="300" label="筛选方案"
      ><template #trigger="{ toggle, open }"
        ><button
          class="view-tab view-selector"
          :class="{ active: mode === 'table' }"
          role="tab"
          :aria-selected="mode === 'table'"
          :aria-expanded="open"
          aria-label="切换筛选方案"
          @click="emit('update:mode', 'table'); toggle()"
        >
          <UiIcon name="grid" :size="15" /><span>{{
            saved.active?.name || "任务表"
          }}</span
          ><UiIcon name="chevron" :size="12" /></button></template
      ><template #default="{ close }">
        <div class="menu-caption">筛选方案 · 当前浏览器</div>
        <button
          class="menu-item"
          @click="
            saved.activate('');
            close();
          "
        >
          <UiIcon name="grid" />默认任务表<UiIcon
            v-if="!saved.activeId"
            name="check"
            class="menu-check"
          />
        </button>
        <div v-if="saved.views.length" class="menu-divider"></div>
        <div v-for="view in saved.views" :key="view.id" class="saved-view-item">
          <button
            class="menu-item"
            @click="
              saved.activate(view.id);
              close();
            "
          >
            <UiIcon name="filter" :size="14" /><span>{{ view.name }}</span
            ><UiIcon
              v-if="saved.activeId === view.id"
              name="check"
              class="menu-check"
            /></button
          ><button
            class="icon-btn"
            :aria-label="'重命名方案：' + view.name"
            title="重命名"
            @click="
              close();
              openRename(view.id, view.name);
            "
          >
            <UiIcon name="edit" :size="13" /></button
          ><button
            class="icon-btn danger"
            :aria-label="'删除方案：' + view.name"
            title="删除方案"
            @click="
              close();
              remove(view.id, view.name);
            "
          >
            <UiIcon name="trash" :size="13" />
          </button>
        </div>
        <div class="menu-divider"></div>
        <button
          class="menu-item"
          @click="
            close();
            openSave();
          "
        >
          <UiIcon name="plus" :size="14" />保存当前条件为新方案
        </button>
      </template></UiPopover
    >
    <span v-if="mode === 'table' && saved.dirty" class="view-dirty">已修改</span
    ><button v-if="mode === 'table' && saved.dirty" class="text-button" @click="update">
      更新方案
    </button>
    <button
      class="view-tab graph-view-tab"
      :class="{ active: mode === 'graph' }"
      type="button"
      role="tab"
      aria-label="关联图"
      :aria-selected="mode === 'graph'"
      @click="emit('update:mode', 'graph')"
    >
      <UiIcon name="git" :size="15" />关联图
    </button>
  </div>
  <UiDialog
    v-if="dialog"
    :title="dialog === 'save' ? '保存筛选方案' : '重命名筛选方案'"
    :width="420"
    @close="dialog = null"
    ><form @submit.prevent="submit">
      <div class="form-field">
        <label for="view-name">方案名称</label
        ><input
          id="view-name"
          v-model="name"
          class="input"
          maxlength="40"
          placeholder="例如：本周待验证"
          data-autofocus
        />
        <p class="form-hint">
          保存筛选、排序和分组，仅在当前浏览器与访问地址中可用。
        </p>
      </div>
      <div v-if="error" class="form-error" role="alert">{{ error }}</div>
    </form>
    <template #footer
      ><button class="btn" @click="dialog = null">取消</button
      ><button class="btn btn-primary" :disabled="!name.trim()" @click="submit">
        保存方案
      </button></template
    ></UiDialog
  >
</template>
