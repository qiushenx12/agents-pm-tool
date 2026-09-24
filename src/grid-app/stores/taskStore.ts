import { defineStore } from "pinia";
import { computed, nextTick, onScopeDispose, ref, watch } from "vue";
import { api } from "@/grid-app/api/client";
import { errorText, notify } from "@/shared/feedback";
import type {
  Task,
  TaskPageQuery,
  TaskPage,
  TaskGroupCount,
  TaskBatchRequest,
} from "@/shared/types";
import {
  FILTER_KEYS,
  filterFingerprint,
  readFiltersFromUrl,
  readSavedFilters,
  sanitizeFilters,
  saveFilters,
  urlHasFilterParams,
  writeFiltersToUrl,
  type FilterState,
} from "./filters";
import {
  PAGE_SIZES,
  readPageSize,
  rememberPageSize,
} from "./pageSize";
export type { FilterState } from "./filters";
export const useTaskStore = defineStore("tasks", () => {
  const tasks = ref<Task[]>([]),
    records = ref<Record<string, Task>>({});
  const loading = ref(false),
    error = ref(""),
    initialized = ref(false);
  // URL 带筛选参数时以 URL 为准（分享链接场景）；否则从 localStorage 恢复上次状态。
  const filters = ref<FilterState>(
    urlHasFilterParams()
      ? readFiltersFromUrl()
      : (readSavedFilters() ?? readFiltersFromUrl()),
  );
  // 只在首次加载时判断：下面会把本地筛选回写进 URL，之后再问就永远是"带参数的链接"了。
  const sharedLinkFilters = urlHasFilterParams();
  writeFiltersToUrl(filters.value);
  saveFilters(filters.value);
  /**
   * 视图设置的跨客户端同步：网页端与「进入应用」的内嵌窗口是两套 WebView 存储，
   * localStorage 各存各的，只有服务端那份能两边共用。登录后先对账 —— 服务端有就以
   * 服务端为准，没有就把本机的推上去；此后本机一改就回推。
   */
  const viewStateSynced = ref(false);
  let viewTimer: ReturnType<typeof setTimeout> | undefined;
  async function syncViewState() {
    // 带筛选参数的链接是分享场景，以 URL 为准：既不被服务端覆盖，也不回推覆盖别人。
    if (sharedLinkFilters) {
      viewStateSynced.value = true;
      return;
    }
    try {
      const remote = await api.getViewState();
      if (remote?.filters) filters.value = sanitizeFilters(remote.filters);
      else await api.putViewState(sanitizeFilters(filters.value));
    } catch {
      /* 服务不可用或旧版服务端：沿用本机设置，不影响使用 */
    }
    viewStateSynced.value = true;
  }
  function resetViewStateSync() {
    viewStateSynced.value = false;
    clearTimeout(viewTimer);
    viewTimer = undefined;
  }
  function scheduleViewStatePush() {
    clearTimeout(viewTimer);
    viewTimer = setTimeout(() => {
      viewTimer = undefined;
      void api.putViewState(sanitizeFilters(filters.value)).catch(() => {});
    }, 400);
  }
  const page = ref(1),
    // 每页条数沿用上次的选择（见 pageSize.ts）
    pageSize = ref(readPageSize()),
    total = ref(0),
    groups = ref<TaskGroupCount[]>([]);
  const pages = computed(() =>
    Math.max(1, Math.ceil(total.value / pageSize.value)),
  );
  const pending = ref<Record<string, number>>({}),
    batchBusy = ref(false);
  const saving = computed(
    () => batchBusy.value || Object.values(pending.value).some((n) => n > 0),
  );
  const selection = ref<Record<string, Task>>({});
  const selectedIds = computed(() => Object.keys(selection.value));
  const connection = ref<"connecting" | "live" | "reconnecting">("connecting"),
    externalRevision = ref(0);
  const activeFilterCount = computed(
    () =>
      FILTER_KEYS.reduce((n, k) => n + filters.value[k].length, 0) +
      (filters.value.keyword ? 1 : 0),
  );
  const query = computed<TaskPageQuery>(() => ({
    project: filters.value.project.length
      ? [...filters.value.project]
      : undefined,
    type: filters.value.type.length ? [...filters.value.type] : undefined,
    status: filters.value.status.length ? [...filters.value.status] : undefined,
    status_mode: filters.value.status.length
      ? filters.value.status_mode
      : undefined,
    submitter: filters.value.submitter.length
      ? [...filters.value.submitter]
      : undefined,
    priority: filters.value.priority.length
      ? [...filters.value.priority]
      : undefined,
    keyword: filters.value.keyword || undefined,
    sort_by: filters.value.sort_by,
    sort_order: filters.value.sort_order,
    group_by: filters.value.group_by || undefined,
  }));
  const loadedKey = ref("");
  const currentKey = computed(() =>
    JSON.stringify([query.value, page.value, pageSize.value]),
  );
  const isCurrentPage = computed(() => loadedKey.value === currentKey.value);
  let requestId = 0,
    generation = 0;
  let controller: AbortController | undefined,
    timer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;
  const queues = new Map<string, Promise<unknown>>();
  async function refresh(anchorId?: string): Promise<TaskPage | undefined> {
    if (disposed) return;
    if (saving.value) {
      scheduleRefresh();
      return;
    }
    clearTimeout(timer);
    controller?.abort();
    controller = new AbortController();
    const id = ++requestId,
      version = generation;
    loading.value = true;
    error.value = "";
    const requestStarted = performance.now();
    try {
      const result = await api.pageTasks(
        {
          ...query.value,
          page: page.value,
          page_size: pageSize.value,
          anchor_id: anchorId,
        },
        controller.signal,
      );
      if (id !== requestId || version !== generation || disposed) return;
      const renderStarted = performance.now();
      tasks.value = result.items;
      total.value = result.total;
      page.value = result.page;
      pageSize.value = result.page_size;
      groups.value = result.groups;
      result.items.forEach((t) => {
        records.value[t.id] = t;
        if (selection.value[t.id]) selection.value[t.id] = t;
      });
      initialized.value = true;
      loadedKey.value = currentKey.value;
      if (import.meta.env.DEV && import.meta.env.MODE !== "test") {
        await nextTick();
        const end = performance.now();
        performance.measure("pm-page-load", {
          start: requestStarted,
          end,
          detail: { total: result.total, rows: result.items.length },
        });
        performance.measure("pm-dom-update", {
          start: renderStarted,
          end,
          detail: { total: result.total, rows: result.items.length },
        });
        console.debug(
          "[pm-perf] " +
            JSON.stringify({
              total: result.total,
              rows: result.items.length,
              load_ms: Number((end - requestStarted).toFixed(2)),
              dom_ms: Number((end - renderStarted).toFixed(2)),
            }),
        );
      }
      return result;
    } catch (e) {
      if (
        id === requestId &&
        version === generation &&
        !(e instanceof Error && e.name === "AbortError")
      )
        error.value = errorText(e);
    } finally {
      if (id === requestId) loading.value = false;
    }
  }
  function scheduleRefresh(delay = 120) {
    if (disposed) return;
    clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      void refresh();
    }, delay);
  }
  function acceptTask(task: Task) {
    const previous = records.value[task.id];
    const merged = {
      ...previous,
      ...task,
      attachment_count:
        task.attachment_count ?? previous?.attachment_count ?? 0,
    };
    records.value[task.id] = merged;
    const index = tasks.value.findIndex((t) => t.id === task.id);
    if (index >= 0) tasks.value[index] = merged;
    if (selection.value[task.id]) selection.value[task.id] = merged;
  }
  async function updateTask(
    id: string,
    patch: Parameters<typeof api.patchTask>[1],
  ) {
    if (batchBusy.value) throw new Error("批量操作进行中，请稍后重试");
    generation++;
    controller?.abort();
    pending.value[id] = (pending.value[id] ?? 0) + 1;
    const operation = (queues.get(id) ?? Promise.resolve())
      .catch(() => {})
      .then(async () => {
        const task = await api.patchTask(id, patch);
        acceptTask(task);
        return task;
      });
    queues.set(id, operation);
    try {
      return await operation;
    } finally {
      pending.value[id]--;
      if (queues.get(id) === operation) queues.delete(id);
      scheduleRefresh();
    }
  }
  async function removeTask(id: string) {
    if (batchBusy.value) throw new Error("批量操作进行中，请稍后重试");
    generation++;
    controller?.abort();
    await (queues.get(id) ?? Promise.resolve()).catch(() => {});
    await api.deleteTask(id);
    tasks.value = tasks.value.filter((t) => t.id !== id);
    delete records.value[id];
    delete selection.value[id];
    scheduleRefresh();
  }
  /** 手动排序：乐观重排当前页后调 reorder 接口；失败则刷新回服务端顺序 */
  async function moveTask(id: string, prevId: string | null, nextId: string | null) {
    if (batchBusy.value) throw new Error("批量操作进行中，请稍后重试");
    if (prevId === id || nextId === id || (!prevId && !nextId)) return;
    generation++;
    controller?.abort();
    const from = tasks.value.findIndex((t) => t.id === id);
    if (from < 0) return;
    const moved = tasks.value[from];
    const rest = tasks.value.filter((t) => t.id !== id);
    let to = nextId ? rest.findIndex((t) => t.id === nextId) : -1;
    if (to < 0) {
      const p = prevId ? rest.findIndex((t) => t.id === prevId) : -1;
      to = p < 0 ? rest.length : p + 1;
    }
    tasks.value = [...rest.slice(0, to), moved, ...rest.slice(to)];
    pending.value[id] = (pending.value[id] ?? 0) + 1;
    try {
      const task = await api.reorderTask(id, {
        prev_id: prevId ?? undefined,
        next_id: nextId ?? undefined,
      });
      acceptTask(task);
    } catch (e) {
      notify(errorText(e), "error");
      await refresh();
    } finally {
      pending.value[id]--;
      scheduleRefresh();
    }
  }
  /**
   * 切入手动排序前的基线：以当前排序字段/方向重铺全部任务的 position。
   * 成功后由调用方把 sort_by 设为 manual（触发刷新），列表视觉顺序不变。
   */
  async function rebaseManualOrder() {
    if (saving.value) throw new Error("还有更改正在保存，请稍后重试");
    generation++;
    controller?.abort();
    batchBusy.value = true;
    try {
      await api.rebaseOrder({
        sort_by: filters.value.sort_by,
        sort_order: filters.value.sort_order,
      });
    } catch (e) {
      notify(errorText(e), "error");
      throw e;
    } finally {
      batchBusy.value = false;
    }
  }
  function toggleSelection(task: Task, selected = !selection.value[task.id]) {
    if (batchBusy.value || !isCurrentPage.value) return;
    if (!selected) {
      delete selection.value[task.id];
      return;
    }
    if (selectedIds.value.length >= 500) {
      notify("每次最多选择 500 条任务", "info");
      return;
    }
    selection.value[task.id] = task;
  }
  function clearSelection() {
    if (!batchBusy.value) selection.value = {};
  }
  async function applyBatch(request: TaskBatchRequest) {
    if (saving.value) throw new Error("还有更改正在保存，请稍后重试");
    generation++;
    controller?.abort();
    batchBusy.value = true;
    try {
      const result = await api.batchTasks(request);
      result.results.forEach((item) => {
        if (item.error) return;
        if (item.task) acceptTask(item.task);
        else {
          tasks.value = tasks.value.filter((t) => t.id !== item.id);
          delete records.value[item.id];
        }
        delete selection.value[item.id];
      });
      return result;
    } finally {
      batchBusy.value = false;
      await refresh();
    }
  }
  async function setPage(value: number) {
    if (!Number.isFinite(value) || saving.value) return;
    generation++;
    controller?.abort();
    page.value = Math.max(1, Math.min(pages.value, Math.trunc(value)));
    return refresh();
  }
  async function setPageSize(value: number) {
    if (!PAGE_SIZES.includes(value) || saving.value) return;
    generation++;
    pageSize.value = value;
    page.value = 1;
    // 只记用户显式选择的档位，不回写服务端在响应里回显的值
    rememberPageSize(value);
    return refresh();
  }
  async function reveal(id: string) {
    return refresh(id);
  }
  async function adjacentTask(id: string, direction: -1 | 1) {
    let index = tasks.value.findIndex((t) => t.id === id);
    if (index < 0) {
      const located = await reveal(id);
      if (!located?.anchor_found) return;
      index = tasks.value.findIndex((t) => t.id === id);
    }
    const adjacent = tasks.value[index + direction];
    if (adjacent) return adjacent;
    if (
      (direction === -1 && page.value > 1) ||
      (direction === 1 && page.value < pages.value)
    ) {
      const result = await setPage(page.value + direction);
      if (result)
        return direction === -1
          ? result.items[result.items.length - 1]
          : result.items[0];
    }
  }
  function toggleFilter(key: (typeof FILTER_KEYS)[number], value: string) {
    const arr = filters.value[key] as string[],
      index = arr.indexOf(value);
    if (index >= 0) arr.splice(index, 1);
    else arr.push(value);
  }
  function clearFilters() {
    FILTER_KEYS.forEach((k) => {
      filters.value[k] = [];
    });
    filters.value.keyword = "";
    filters.value.status_mode = "include";
  }
  function setProject(project?: string) {
    filters.value.project = project ? [project] : [];
  }
  function applyFilters(value: unknown) {
    filters.value = sanitizeFilters(value);
  }
  function toggleSort(field: FilterState["sort_by"]) {
    if (filters.value.sort_by === field)
      filters.value.sort_order =
        filters.value.sort_order === "asc" ? "desc" : "asc";
    else {
      filters.value.sort_by = field;
      filters.value.sort_order = "desc";
    }
  }
  let previousKeyword = filters.value.keyword,
    fingerprint = filterFingerprint(filters.value);
  watch(
    filters,
    (f) => {
      generation++;
      controller?.abort();
      writeFiltersToUrl(f);
      saveFilters(f);
      if (viewStateSynced.value) scheduleViewStatePush();
      page.value = 1;
      const next = filterFingerprint(f);
      if (next !== fingerprint) {
        selection.value = {};
        fingerprint = next;
      }
      const keywordChanged = previousKeyword !== f.keyword;
      previousKeyword = f.keyword;
      scheduleRefresh(keywordChanged ? 300 : 0);
    },
    { deep: true, flush: "sync" },
  );
  onScopeDispose(() => {
    disposed = true;
    clearTimeout(timer);
    clearTimeout(viewTimer);
    controller?.abort();
  });
  return {
    tasks,
    records,
    loading,
    initialized,
    error,
    filters,
    query,
    syncViewState,
    resetViewStateSync,
    pending,
    saving,
    connection,
    externalRevision,
    activeFilterCount,
    page,
    pageSize,
    pages,
    total,
    groups,
    isCurrentPage,
    selection,
    selectedIds,
    batchBusy,
    refresh,
    scheduleRefresh,
    acceptTask,
    updateTask,
    removeTask,
    moveTask,
    rebaseManualOrder,
    toggleFilter,
    clearFilters,
    setProject,
    toggleSort,
    applyFilters,
    toggleSelection,
    clearSelection,
    applyBatch,
    setPage,
    setPageSize,
    reveal,
    adjacentTask,
  };
});
