<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { api } from "../api/client";
import { buildRelationGraph } from "../relationGraph";
import { errorText } from "@/shared/feedback";
import UiIcon from "@/shared/UiIcon.vue";
import type { Task } from "@/shared/types";

const props = defineProps<{ projects: string[]; revision: number }>();
const emit = defineEmits<{ openDetail: [task: Task] }>();
const records = ref<Task[]>([]);
const loading = ref(false);
const error = ref("");
const showIsolated = ref(false);
const graph = computed(() => buildRelationGraph(records.value, showIsolated.value));
let requestId = 0;
let controller: AbortController | undefined;

async function load() {
  controller?.abort();
  const current = ++requestId;
  controller = new AbortController();
  loading.value = true;
  error.value = "";
  try {
    const result = await api.listTasks(
      { project: props.projects.length ? [...props.projects] : undefined },
      controller.signal,
    );
    if (current === requestId) records.value = result;
  } catch (cause) {
    if (current === requestId && !(cause instanceof Error && cause.name === "AbortError"))
      error.value = errorText(cause);
  } finally {
    if (current === requestId) loading.value = false;
  }
}
watch(
  () => props.projects.join("\0"),
  () => {
    records.value = [];
    void load();
  },
  { immediate: true },
);
watch(() => props.revision, () => void load());
onBeforeUnmount(() => {
  requestId++;
  controller?.abort();
});
</script>

<template>
  <section class="relation-view" aria-label="任务关联图">
    <div class="relation-toolbar">
      <div>
        <strong>任务关联图</strong>
        <span class="relation-count">{{ graph.nodes.length }} 个节点 · {{ graph.edges.length }} 条关联</span>
        <span class="relation-count">双击节点查看详情</span>
      </div>
      <div class="relation-actions">
        <label class="relation-toggle">
          <input v-model="showIsolated" type="checkbox" />显示无关联任务
          <span v-if="graph.isolatedCount">({{ graph.isolatedCount }})</span>
        </label>
        <button type="button" class="icon-btn" aria-label="刷新关联图" title="刷新关联图" @click="load">
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
      {{ records.length ? "当前范围内暂无任务关联，可勾选“显示无关联任务”。" : "当前范围内暂无任务。" }}
    </div>
    <div v-else class="relation-scroll" tabindex="0" aria-label="关联图画布，可滚动查看">
      <div class="relation-canvas" :style="{ width: graph.width + 'px', height: graph.height + 'px' }">
        <svg class="relation-lines" :width="graph.width" :height="graph.height" aria-hidden="true">
          <path v-for="edge in graph.edges" :key="edge.from + ':' + edge.to" class="relation-line" :d="edge.path" />
        </svg>
        <button
          v-for="node in graph.nodes"
          :key="node.task.id"
          class="relation-card"
          type="button"
          :data-status="node.task.status"
          :style="{ left: node.x + 'px', top: node.y + 'px' }"
          :aria-label="`查看任务 ${node.task.id}：${node.task.description}`"
          title="双击打开任务详情"
          @dblclick="emit('openDetail', node.task)"
          @keydown.enter="emit('openDetail', node.task)"
        >
          <span class="relation-card-top">
            <span class="relation-card-id">{{ node.task.id }}</span>
            <span class="relation-card-status">{{ node.task.status }}</span>
          </span>
          <span class="relation-card-description">{{ node.task.description || "未填写任务描述" }}</span>
          <span v-if="projects.length !== 1" class="relation-card-project">{{ node.task.project }}</span>
        </button>
      </div>
    </div>
  </section>
</template>
