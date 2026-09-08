<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref } from "vue";
import { enterLayer, leaveLayer, isTopLayer, focusables } from "./layers";
const props = withDefaults(
  defineProps<{ width?: number; align?: "left" | "right"; label?: string }>(),
  { width: 240, align: "left", label: "选项" },
);
const open = ref(false);
const anchor = ref<HTMLElement>();
const panel = ref<HTMLElement>();
const position = ref({
  left: "0px",
  top: "0px",
  width: "240px",
  maxHeight: "400px",
  zIndex: 200,
});
let layer = 0;
let previous: HTMLElement | null = null;
function place() {
  if (!anchor.value || !panel.value) return;
  const r = anchor.value.getBoundingClientRect();
  const width = Math.min(props.width, window.innerWidth - 24);
  const below = window.innerHeight - r.bottom - 16;
  const above = r.top - 16;
  const flip = below < Math.min(panel.value.scrollHeight, 240) && above > below;
  const maxHeight = Math.max(80, flip ? above : below);
  const height = Math.min(panel.value.scrollHeight, maxHeight);
  position.value = {
    left:
      Math.max(
        12,
        Math.min(
          props.align === "right" ? r.right - width : r.left,
          window.innerWidth - width - 12,
        ),
      ) + "px",
    top: Math.max(12, flip ? r.top - height - 6 : r.bottom + 6) + "px",
    width: width + "px",
    maxHeight: maxHeight + "px",
    zIndex: 200 + layer,
  };
}
function close(restore = true) {
  if (!open.value) return;
  open.value = false;
  leaveLayer(layer);
  window.removeEventListener("resize", place);
  window.removeEventListener("scroll", place, true);
  document.removeEventListener("pointerdown", outside, true);
  document.removeEventListener("keydown", keyboard, true);
  if (restore && previous?.isConnected) {
    const cell = previous.closest<HTMLElement>('[role="gridcell"]');
    (cell ?? previous).focus({ preventScroll: true });
  }
}
async function toggle() {
  if (open.value) {
    close();
    return;
  }
  previous = document.activeElement as HTMLElement;
  layer = enterLayer();
  open.value = true;
  await nextTick();
  place();
  const selected = panel.value?.querySelector<HTMLElement>(
    '[aria-selected="true"]',
  );
  (selected ?? focusables(panel.value)[0] ?? panel.value)?.focus({
    preventScroll: true,
  });
  window.addEventListener("resize", place);
  window.addEventListener("scroll", place, true);
  document.addEventListener("pointerdown", outside, true);
  document.addEventListener("keydown", keyboard, true);
}
function outside(e: PointerEvent) {
  if (!isTopLayer(layer)) return;
  const target = e.target as Node;
  if (!anchor.value?.contains(target) && !panel.value?.contains(target))
    close(false);
}
function keyboard(e: KeyboardEvent) {
  if (!isTopLayer(layer)) return;
  if (e.key === "Escape") {
    e.preventDefault();
    e.stopImmediatePropagation();
    close();
  }
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    const items = focusables(panel.value);
    if (!items.length) return;
    e.preventDefault();
    e.stopPropagation();
    const i = items.indexOf(document.activeElement as HTMLElement);
    items[
      (i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length
    ].focus();
  }
  if (e.key === "Tab") {
    const items = focusables(panel.value);
    const i = items.indexOf(document.activeElement as HTMLElement);
    if ((e.shiftKey && i <= 0) || (!e.shiftKey && i === items.length - 1)) {
      e.preventDefault();
      close();
    }
  }
}
onBeforeUnmount(() => close(false));
defineExpose({ toggle, close });
</script>
<template>
  <span ref="anchor" class="popover-anchor" @click.stop @keydown.stop
    ><slot name="trigger" :toggle="toggle" :open="open"
  /></span>
  <Teleport to="body"
    ><div
      v-if="open"
      ref="panel"
      class="ui-popover"
      :style="position"
      :aria-label="label"
      role="group"
      tabindex="-1"
      @click.stop
      @keydown.stop
    >
      <slot :close="close" /></div
  ></Teleport>
</template>
