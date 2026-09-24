<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { api } from "../api/client";
import { buildRelationForest, buildRelationGraph, CARD_HEIGHT, CARD_WIDTH, type NodePosition } from "../relationGraph";
import { errorText } from "@/shared/feedback";
import { statusTones } from "@/shared/taskOptions";
import UiIcon from "@/shared/UiIcon.vue";
import { useTaskStore } from "../stores/taskStore";
import type { Task } from "@/shared/types";

const props = defineProps<{ taskId: string | null; revision: number; selectedTaskId?: string | null }>();
const emit = defineEmits<{ openDetail: [task: Task]; clearDetail: [] }>();
const tasks = useTaskStore();
const records = ref<Task[]>([]);
const loading = ref(false);
const error = ref("");
const nodePositions = ref<Record<string, NodePosition>>({});
const graph = computed(() => {
  const positions = new Map(Object.entries(nodePositions.value));
  return props.taskId
    ? buildRelationGraph(records.value, props.taskId, positions)
    : buildRelationForest(records.value, positions);
});
const selectedId = ref<string | null>(null);
// 平移/缩放状态（声明提前，watch immediate 回调里要用）
const panX = ref(0);
const panY = ref(0);
const zoom = ref(1);
let viewPristine = true;
let requestId = 0;
let controller: AbortController | undefined;

function resetView() {
  viewPristine = true;
  nodePositions.value = {};
  panX.value = 0;
  panY.value = 0;
  zoom.value = 1;
}

function refreshGraph() {
  resetView();
  void load();
}

async function load() {
  controller?.abort();
  const current = ++requestId;
  controller = new AbortController();
  loading.value = true;
  error.value = "";
  try {
    // 项目范围跟随任务表的当前筛选（侧栏当前项目），只作用于多树总览；
    // 单树模式始终展示该任务所在的整棵树
    const query =
      !props.taskId && tasks.filters.project.length
        ? { project: [...tasks.filters.project] }
        : {};
    const result = await api.listTasks(query, controller.signal);
    if (current === requestId) records.value = result;
  } catch (cause) {
    if (current === requestId && !(cause instanceof Error && cause.name === "AbortError"))
      error.value = errorText(cause);
  } finally {
    if (current === requestId) loading.value = false;
  }
}
watch(
  () => props.taskId,
  (id) => {
    controller?.abort();
    requestId++;
    records.value = [];
    loading.value = false;
    error.value = "";
    selectedId.value = id;
    // 切换任务时重置视图，避免旧任务的平移/缩放状态残留
    resetView();
    void load();
  },
  { immediate: true },
);
watch(() => props.selectedTaskId, (id) => {
  if (id !== undefined) selectedId.value = id;
}, { immediate: true });
watch(() => props.revision, () => {
  void load();
});
// 任务表的项目筛选变化（侧栏或关联图页签下拉）：重新加载并重置视图，
// 已被筛掉的选中卡片一并取消
watch(
  () => tasks.filters.project,
  () => {
    if (selectedId.value) {
      selectedId.value = null;
      emit("clearDetail");
    }
    resetView();
    void load();
  },
  { deep: true },
);

// ========== 画布平移（中键拖拽）与缩放（滚轮） ==========
const scrollEl = ref<HTMLElement | null>(null);
const viewportW = ref(0);
const viewportH = ref(0);
const ZOOM_MIN = 0.25;
const ZOOM_MAX = 3;
const ZOOM_STEP = 1.1;

// 默认多树视图初次打开时尽量完整呈现；手动缩放或拖动后保留用户的视角。
watch([graph, viewportW, viewportH], () => {
  if (props.taskId || !viewPristine || !graph.value.nodes.length || !viewportW.value || !viewportH.value) return;
  const width = graph.value.bounds.right - graph.value.bounds.left;
  const height = graph.value.bounds.bottom - graph.value.bounds.top;
  zoom.value = Math.max(ZOOM_MIN, Math.min(1, (viewportW.value - 96) / width, (viewportH.value - 96) / height));
}, { flush: "post" });

const centerX = computed(() => viewportW.value / 2 - zoom.value * (graph.value.bounds.left + graph.value.bounds.right) / 2);
const centerY = computed(() => viewportH.value / 2 - zoom.value * (graph.value.bounds.top + graph.value.bounds.bottom) / 2);

const canvasTransform = computed(
  () => `translate(${centerX.value + panX.value}px, ${centerY.value + panY.value}px) scale(${zoom.value})`,
);

// 用 ResizeObserver 监听视口尺寸（.relation-scroll 是 v-else 分支，onMounted 时可能还不存在）
let resizeObserver: ResizeObserver | undefined;
function bindResizeObserver() {
  if (!scrollEl.value) return;
  viewportW.value = scrollEl.value.clientWidth;
  viewportH.value = scrollEl.value.clientHeight;
  if (typeof ResizeObserver === "undefined") return;
  resizeObserver = new ResizeObserver((entries) => {
    const entry = entries[0];
    if (entry) {
      viewportW.value = entry.contentRect.width;
      viewportH.value = entry.contentRect.height;
    }
  });
  resizeObserver.observe(scrollEl.value);
}
watch(scrollEl, () => {
  resizeObserver?.disconnect();
  resizeObserver = undefined;
  bindResizeObserver();
}, { immediate: true });

// 中键拖拽平移
const panning = ref(false);
let panStart: { pointerId: number; startX: number; startY: number; panX: number; panY: number } | null = null;
const draggingId = ref<string | null>(null);
let dragStart: { pointerId: number; taskId: string; startX: number; startY: number; x: number; y: number; element: HTMLElement } | null = null;
let suppressClick = false;

/** 拖动吸附的容差（屏幕像素，除以缩放换算回画布坐标）。 */
const SNAP_THRESHOLD = 8;
/** 卡片间距：拖动时保持这个间距，连线才有走线通道。 */
const CARD_GAP = 24;

/**
 * 自动布局位置（不含手动拖动）。吸附的两个用途：
 * ① 原始位置本身必须是吸附点——否则拖走一点就再也没法精确放回原位；
 * ② 判断卡片是否回到了原位，是就把手动覆盖删掉，让它重新跟随布局。
 */
const autoPositions = computed(() => {
  const layout = props.taskId
    ? buildRelationGraph(records.value, props.taskId)
    : buildRelationForest(records.value);
  return new Map(layout.nodes.map((node) => [node.task.id, { x: node.x, y: node.y }]));
});

/** 当前吸附到的对齐线（画布坐标），null 表示该轴没有吸附，用于画参考线。 */
const snapGuides = ref<{ x: number | null; y: number | null }>({
  x: null,
  y: null,
});

/** 在候选值里找最近的一个，超出容差不吸附。等距时取靠前的候选，所以自带位置放最前。 */
function snapAxis(value: number, targets: number[], threshold: number) {
  let best: number | null = null;
  let bestDistance = Infinity;
  for (const target of targets) {
    const distance = Math.abs(target - value);
    if (distance <= threshold && distance < bestDistance) {
      best = target;
      bestDistance = distance;
    }
  }
  return best;
}

/**
 * 把拖动中的位置吸附到「与其他卡片左边缘对齐」或「与自己的原始位置重合」。
 * 卡片尺寸一致，所以边缘对齐等价于 x 相同 / y 相同。
 */
function snapDrag(taskId: string, raw: { x: number; y: number }) {
  const own = autoPositions.value.get(taskId);
  const threshold = SNAP_THRESHOLD / zoom.value;
  const others = graph.value.nodes.filter((node) => node.task.id !== taskId);
  const x = snapAxis(
    raw.x,
    [...(own ? [own.x] : []), ...others.map((node) => node.x)],
    threshold,
  );
  const y = snapAxis(
    raw.y,
    [...(own ? [own.y] : []), ...others.map((node) => node.y)],
    threshold,
  );
  return { x: x ?? raw.x, y: y ?? raw.y, guideX: x, guideY: y };
}

/** 卡片之间保持 CARD_GAP 的间隙（不含自己）。 */
function overlapsNode(taskId: string, x: number, y: number) {
  return graph.value.nodes.some(
    (node) =>
      node.task.id !== taskId &&
      x < node.x + CARD_WIDTH + CARD_GAP &&
      x + CARD_WIDTH + CARD_GAP > node.x &&
      y < node.y + CARD_HEIGHT + CARD_GAP &&
      y + CARD_HEIGHT + CARD_GAP > node.y,
  );
}

/** 拖回原位（与自动布局位置重合）时撤掉手动覆盖，卡片重新跟随布局。 */
function forgetRestoredPosition(taskId: string) {
  const own = autoPositions.value.get(taskId);
  const current = nodePositions.value[taskId];
  if (!own || !current) return;
  if (Math.abs(current.x - own.x) > 0.5 || Math.abs(current.y - own.y) > 0.5)
    return;
  const rest = { ...nodePositions.value };
  delete rest[taskId];
  nodePositions.value = rest;
}

function beginNodeDrag(event: PointerEvent, taskId: string) {
  if (event.button !== 0 || dragStart) return;
  const node = graph.value.nodes.find((item) => item.task.id === taskId);
  if (!node) return;
  const element = event.currentTarget as HTMLElement;
  dragStart = { pointerId: event.pointerId, taskId, startX: event.clientX, startY: event.clientY, x: node.x, y: node.y, element };
  element.setPointerCapture?.(event.pointerId);
}

function onMouseDown(event: MouseEvent) {
  // 阻止浏览器中键自动滚动（autoscroll）
  if (event.button === 1) event.preventDefault();
}

function onPointerDown(event: PointerEvent) {
  if (event.button !== 1 || !scrollEl.value) return;
  event.preventDefault();
  panStart = {
    pointerId: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    panX: panX.value,
    panY: panY.value,
  };
  scrollEl.value.setPointerCapture(event.pointerId);
  panning.value = true;
}

function onPointerMove(event: PointerEvent) {
  if (dragStart && event.pointerId === dragStart.pointerId) {
    const dx = event.clientX - dragStart.startX;
    const dy = event.clientY - dragStart.startY;
    if (Math.hypot(dx, dy) <= 4 && !draggingId.value) return;
    draggingId.value = dragStart.taskId;
    const raw = {
      x: dragStart.x + dx / zoom.value,
      y: dragStart.y + dy / zoom.value,
    };
    // 吸附后若与别的卡片重叠，退回未吸附的原始位置；仍然重叠就整帧不动
    let next = snapDrag(dragStart.taskId, raw);
    if (overlapsNode(dragStart.taskId, next.x, next.y)) next = { ...raw, guideX: null, guideY: null };
    if (overlapsNode(dragStart.taskId, next.x, next.y)) return;
    viewPristine = false;
    const oldCenterX = centerX.value;
    const oldCenterY = centerY.value;
    nodePositions.value = { ...nodePositions.value, [dragStart.taskId]: { x: next.x, y: next.y } };
    snapGuides.value = { x: next.guideX, y: next.guideY };
    panX.value += oldCenterX - centerX.value;
    panY.value += oldCenterY - centerY.value;
    return;
  }
  if (!panStart || event.pointerId !== panStart.pointerId) return;
  const dx = event.clientX - panStart.startX;
  const dy = event.clientY - panStart.startY;
  if (dx || dy) viewPristine = false;
  panX.value = panStart.panX + dx;
  panY.value = panStart.panY + dy;
}

function endPointer(event: PointerEvent) {
  if (dragStart && event.pointerId === dragStart.pointerId) {
    if (draggingId.value) {
      suppressClick = true;
      setTimeout(() => { suppressClick = false; }, 0);
      forgetRestoredPosition(dragStart.taskId);
    }
    if (dragStart.element.hasPointerCapture?.(event.pointerId)) dragStart.element.releasePointerCapture(event.pointerId);
    dragStart = null;
    draggingId.value = null;
    snapGuides.value = { x: null, y: null };
    return;
  }
  if (!panStart || event.pointerId !== panStart.pointerId) return;
  // 如果确实移动了，抑制随后的 click（避免误选中节点）
  const moved = Math.hypot(event.clientX - panStart.startX, event.clientY - panStart.startY) > 4;
  if (moved) {
    suppressClick = true;
    setTimeout(() => { suppressClick = false; }, 0);
  }
  panStart = null;
  panning.value = false;
  if (scrollEl.value?.hasPointerCapture(event.pointerId)) scrollEl.value.releasePointerCapture(event.pointerId);
}

// 滚轮缩放：以鼠标位置为中心
function onWheel(event: WheelEvent) {
  if (!scrollEl.value) return;
  event.preventDefault();
  viewPristine = false;
  const rect = scrollEl.value.getBoundingClientRect();
  // 鼠标在画布容器内的坐标
  const mx = event.clientX - rect.left;
  const my = event.clientY - rect.top;
  // 当前缩放中心（考虑 centerX/centerY 偏移）
  const cx = (mx - centerX.value - panX.value) / zoom.value;
  const cy = (my - centerY.value - panY.value) / zoom.value;
  const next = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom.value * (event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP)));
  // 调整 pan 使缩放中心保持在鼠标位置
  zoom.value = next;
  panX.value = mx - centerX.value - cx * next;
  panY.value = my - centerY.value - cy * next;
}

function selectNode(task: Task) {
  if (suppressClick) {
    suppressClick = false;
    return;
  }
  selectedId.value = task.id;
  emit("openDetail", task);
}

/** 卡片配色与任务表状态列同源：都取自 shared/taskOptions.ts 的 statusTones */
function statusTone(status: Task["status"]): string {
  return statusTones[status] ?? "gray";
}

function clearSelection(event: MouseEvent) {
  if ((event.target as Element).closest(".relation-card")) return;
  if (suppressClick) { suppressClick = false; return; }
  selectedId.value = null;
  emit("clearDetail");
}

onBeforeUnmount(() => {
  requestId++;
  controller?.abort();
  resizeObserver?.disconnect();
});
</script>

<template>
  <section class="relation-view" aria-label="任务关联图">
    <div class="relation-toolbar">
      <div class="relation-toolbar-copy">
        <strong>任务关联图</strong>
        <span class="relation-count">{{ taskId ? "当前任务所在的关联树" : "全部未验收通过的关联树" }}</span>
        <template v-if="graph.nodes.length">
          <span class="relation-count">{{ graph.nodes.length }} 个节点 · {{ graph.edges.length }} 条关联</span>
          <span class="relation-hint">单击查看详情 · 拖动卡片（自动对齐）· 中键平移 · 滚轮缩放</span>
        </template>
      </div>
      <div class="relation-actions">
        <button type="button" class="icon-btn" aria-label="刷新关联图" title="重新加载关联任务并重置布局" @click="refreshGraph">
          <UiIcon name="refresh" :size="15" />
        </button>
      </div>
    </div>
    <div v-if="error" class="relation-message error-banner" role="alert">
      加载关联图失败：{{ error }}
      <button class="btn btn-sm" @click="load">重试</button>
    </div>
    <div v-else-if="loading && !records.length" class="relation-message">正在加载关联任务…</div>
    <div v-else-if="!graph.nodes.length" class="relation-message">
      {{ taskId ? "任务不可见、已删除，或没有可见的关联任务。" : `暂无${tasks.filters.project.length === 1 ? `项目「${tasks.filters.project[0]}」的` : ""}未验收通过的关联任务树。可从任务表右键查看已验收通过的任务树。` }}
    </div>
    <div
      v-else
      ref="scrollEl"
      class="relation-scroll"
      :class="{ 'relation-scroll-panning': panning, 'relation-scroll-dragging': draggingId }"
      tabindex="0"
      aria-label="关联图画布，中键拖动平移，滚轮缩放"
      @mousedown="onMouseDown"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="endPointer"
      @pointercancel="endPointer"
      @wheel="onWheel"
      @click="clearSelection"
    >
      <div
        class="relation-canvas"
        :style="{
          width: graph.width + 'px',
          height: graph.height + 'px',
          transform: canvasTransform,
        }"
      >
        <svg class="relation-lines" :width="graph.width" :height="graph.height" aria-hidden="true">
          <path v-for="edge in graph.edges" :key="edge.from + ':' + edge.to" class="relation-line" :d="edge.path" />
          <line
            v-if="snapGuides.x !== null"
            class="relation-guide"
            :x1="snapGuides.x"
            :y1="graph.bounds.top"
            :x2="snapGuides.x"
            :y2="graph.bounds.bottom"
          />
          <line
            v-if="snapGuides.y !== null"
            class="relation-guide"
            :x1="graph.bounds.left"
            :y1="snapGuides.y"
            :x2="graph.bounds.right"
            :y2="snapGuides.y"
          />
        </svg>
        <button
          v-for="node in graph.nodes"
          :key="node.task.id"
          class="relation-card"
          :class="{ 'relation-card-selected': node.task.id === selectedId }"
          type="button"
          :data-tone="statusTone(node.task.status)"
          :aria-pressed="node.task.id === selectedId"
          :style="{ left: node.x + 'px', top: node.y + 'px' }"
          :aria-label="`查看任务 ${node.task.id}：${node.task.description}`"
          title="单击打开任务详情"
          @pointerdown="beginNodeDrag($event, node.task.id)"
          @click="selectNode(node.task)"
          @keydown.enter="selectNode(node.task)"
        >
          <span class="relation-card-top">
            <span class="relation-card-id">{{ node.task.id }}</span>
            <span class="relation-card-status">{{ node.task.status }}</span>
          </span>
          <span class="relation-card-description">{{ node.task.description || "未填写任务描述" }}</span>
          <span class="relation-card-project">{{ node.task.project }}</span>
        </button>
      </div>
    </div>
  </section>
</template>
