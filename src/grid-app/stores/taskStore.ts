import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import { api } from "@/grid-app/api/client";
import type {
  Submitter,
  Task,
  TaskListQuery,
  TaskStatus,
  TaskType,
} from "@/shared/types";

export interface FilterState {
  project: string[];
  type: TaskType[];
  status: TaskStatus[];
  submitter: Submitter[];
  keyword: string;
  sort_by: NonNullable<TaskListQuery["sort_by"]>;
  sort_order: NonNullable<TaskListQuery["sort_order"]>;
}

const FILTER_KEYS = ["project", "type", "status", "submitter"] as const;

function readFiltersFromUrl(): FilterState {
  const p = new URLSearchParams(window.location.search);
  const list = (k: string) => p.getAll(k).filter(Boolean);
  return {
    project: list("project"),
    type: list("type") as TaskType[],
    status: list("status") as TaskStatus[],
    submitter: list("submitter") as Submitter[],
    keyword: p.get("keyword") ?? "",
    sort_by: (p.get("sort_by") as FilterState["sort_by"]) || "created_at",
    sort_order: (p.get("sort_order") as FilterState["sort_order"]) || "desc",
  };
}

function writeFiltersToUrl(f: FilterState) {
  const p = new URLSearchParams();
  for (const k of FILTER_KEYS) f[k].forEach((v) => p.append(k, v));
  if (f.keyword) p.set("keyword", f.keyword);
  if (f.sort_by !== "created_at") p.set("sort_by", f.sort_by);
  if (f.sort_order !== "desc") p.set("sort_order", f.sort_order);
  const s = p.toString();
  const url = s ? `?${s}` : window.location.pathname;
  window.history.replaceState(null, "", url);
}

export const useTaskStore = defineStore("tasks", () => {
  const tasks = ref<Task[]>([]);
  const loading = ref(false);
  const error = ref("");
  const filters = ref<FilterState>(readFiltersFromUrl());

  const query = computed<TaskListQuery>(() => ({
    project: filters.value.project.length ? filters.value.project : undefined,
    type: filters.value.type.length ? filters.value.type : undefined,
    status: filters.value.status.length ? filters.value.status : undefined,
    submitter: filters.value.submitter.length ? filters.value.submitter : undefined,
    keyword: filters.value.keyword || undefined,
    sort_by: filters.value.sort_by,
    sort_order: filters.value.sort_order,
  }));

  async function refresh() {
    loading.value = true;
    error.value = "";
    try {
      tasks.value = await api.listTasks(query.value);
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  function toggleFilter(key: (typeof FILTER_KEYS)[number], value: string) {
    const arr = filters.value[key] as string[];
    const i = arr.indexOf(value);
    if (i >= 0) arr.splice(i, 1);
    else arr.push(value);
  }

  function toggleSort(field: FilterState["sort_by"]) {
    if (filters.value.sort_by === field) {
      filters.value.sort_order = filters.value.sort_order === "asc" ? "desc" : "asc";
    } else {
      filters.value.sort_by = field;
      filters.value.sort_order = "desc";
    }
  }

  watch(
    filters,
    (f) => {
      writeFiltersToUrl(f);
      void refresh();
    },
    { deep: true },
  );

  return { tasks, loading, error, filters, refresh, toggleFilter, toggleSort };
});
