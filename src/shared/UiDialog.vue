<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { enterLayer, leaveLayer, isTopLayer, focusables } from "./layers";
import UiIcon from "./UiIcon.vue";
const props = withDefaults(
  defineProps<{
    title: string;
    drawer?: boolean;
    busy?: boolean;
    width?: number;
  }>(),
  { width: 520 },
);
const emit = defineEmits<{ close: [] }>();
const panel = ref<HTMLElement>();
const layer = enterLayer();
const previous = document.activeElement as HTMLElement | null;
const drawerWidth = ref(Number(localStorage.getItem("pm-drawer-width")) || 620);
const titleId = "dialog-title-" + layer;
function close() {
  if (!props.busy) emit("close");
}
function keyboard(e: KeyboardEvent) {
  if (!isTopLayer(layer) || e.defaultPrevented) return;
  if (e.key === "Escape") {
    e.preventDefault();
    e.stopImmediatePropagation();
    close();
  }
  if (e.key === "Tab") {
    const items = focusables(panel.value);
    if (!items.length) {
      e.preventDefault();
      panel.value?.focus();
      return;
    }
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (e.shiftKey && i <= 0) {
      e.preventDefault();
      items[items.length - 1].focus();
    } else if (!e.shiftKey && (i < 0 || i === items.length - 1)) {
      e.preventDefault();
      items[0].focus();
    }
  }
}
let resizeEnd: (() => void) | undefined;
function resize(e: PointerEvent) {
  const x = e.clientX,
    width = drawerWidth.value;
  const move = (event: PointerEvent) => {
    drawerWidth.value = Math.min(
      window.innerWidth - 32,
      Math.max(440, width + x - event.clientX),
    );
  };
  resizeEnd = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", resizeEnd!);
    localStorage.setItem("pm-drawer-width", String(drawerWidth.value));
    resizeEnd = undefined;
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", resizeEnd);
  e.preventDefault();
}
onMounted(() => {
  (
    panel.value?.querySelector<HTMLElement>("[data-autofocus]") ?? panel.value
  )?.focus();
  document.addEventListener("keydown", keyboard);
});
onBeforeUnmount(() => {
  resizeEnd?.();
  leaveLayer(layer);
  document.removeEventListener("keydown", keyboard);
  if (previous?.isConnected) previous.focus({ preventScroll: true });
});
</script>
<template>
  <Teleport to="body">
    <div
      class="dialog-mask"
      :class="{ 'is-drawer': drawer }"
      :style="{ zIndex: 200 + layer }"
      @click.self="close"
    >
      <section
        ref="panel"
        class="ui-dialog"
        :class="{ 'drawer-panel': drawer }"
        :style="{ width: (drawer ? drawerWidth : width) + 'px' }"
        role="dialog"
        aria-modal="true"
        :aria-labelledby="titleId"
        tabindex="-1"
      >
        <div
          v-if="drawer"
          class="drawer-resize"
          role="separator"
          aria-label="调整详情宽度"
          aria-orientation="vertical"
          @pointerdown="resize"
        ></div>
        <header class="dialog-header">
          <h2 :id="titleId">{{ title }}</h2>
          <div class="inline-actions">
            <slot name="header-actions" /><button
              class="icon-btn"
              aria-label="关闭"
              title="关闭 · Esc"
              :disabled="busy"
              @click="close"
            >
              <UiIcon name="close" />
            </button>
          </div>
        </header>
        <div class="dialog-body"><slot /></div>
        <footer v-if="$slots.footer" class="dialog-footer">
          <slot name="footer" />
        </footer>
      </section>
    </div>
  </Teleport>
</template>
