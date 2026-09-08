import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { useTaskStore } from "./taskStore";
import {
  filterFingerprint,
  sanitizeFilters,
  type FilterState,
} from "./filters";
export interface SavedView {
  id: string;
  name: string;
  filters: FilterState;
}
const STORAGE_KEY = "pm-saved-views-v1";
export const useSavedViewStore = defineStore("saved-views", () => {
  const tasks = useTaskStore();
  let stored: SavedView[] = [];
  try {
    const value: unknown = JSON.parse(
      localStorage.getItem(STORAGE_KEY) || "[]",
    );
    if (Array.isArray(value)) {
      const ids = new Set<string>(),
        names = new Set<string>();
      stored = value
        .filter((v) => {
          if (
            !v ||
            typeof v.id !== "string" ||
            typeof v.name !== "string" ||
            !v.name.trim() ||
            ids.has(v.id) ||
            names.has(v.name.trim())
          )
            return false;
          ids.add(v.id);
          names.add(v.name.trim());
          return true;
        })
        .map((v) => ({
          id: v.id,
          name: v.name.trim().slice(0, 40),
          filters: sanitizeFilters(v.filters),
        }));
    }
  } catch {
    /* corrupted local preferences can be replaced by a new save */
  }
  const views = ref<SavedView[]>(stored);
  const activeId = ref(
    stored.find(
      (v) => filterFingerprint(v.filters) === filterFingerprint(tasks.filters),
    )?.id ?? "",
  );
  const active = computed(() =>
    views.value.find((v) => v.id === activeId.value),
  );
  const dirty = computed(
    () =>
      !!active.value &&
      filterFingerprint(active.value.filters) !==
        filterFingerprint(tasks.filters),
  );
  function persist(next: SavedView[]) {
    // Report storage failure instead of presenting a save that will be lost on reload.
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    } catch {
      throw new Error("浏览器无法保存方案，请检查本地存储空间或隐私设置");
    }
    views.value = next;
  }
  function checkedName(value: string, except?: string) {
    const name = value.trim();
    if (!name || name.length > 40)
      throw new Error("方案名称请填写 1–40 个字符");
    if (views.value.some((v) => v.id !== except && v.name === name))
      throw new Error("已有同名方案，请使用其他名称");
    return name;
  }
  function save(name: string) {
    const id =
      crypto.randomUUID?.() ??
      "view-" +
        Date.now().toString(36) +
        "-" +
        Math.random().toString(36).slice(2);
    const view = {
      id,
      name: checkedName(name),
      filters: sanitizeFilters(tasks.filters),
    };
    persist([...views.value, view]);
    activeId.value = view.id;
  }
  function update() {
    if (!active.value) return;
    persist(
      views.value.map((v) =>
        v.id === activeId.value
          ? { ...v, filters: sanitizeFilters(tasks.filters) }
          : v,
      ),
    );
  }
  function rename(id: string, name: string) {
    const valid = checkedName(name, id);
    persist(views.value.map((v) => (v.id === id ? { ...v, name: valid } : v)));
  }
  function remove(id: string) {
    persist(views.value.filter((v) => v.id !== id));
    if (activeId.value === id) activeId.value = "";
  }
  function activate(id: string) {
    const view = views.value.find((v) => v.id === id);
    if (!view) {
      activeId.value = "";
      tasks.applyFilters({});
      return;
    }
    tasks.applyFilters(view.filters);
    activeId.value = id;
  }
  function renameProject(oldName: string, newName: string) {
    if (!views.value.some((v) => v.filters.project.includes(oldName))) return;
    persist(
      views.value.map((v) => ({
        ...v,
        filters: {
          ...v.filters,
          project: v.filters.project.map((p) => (p === oldName ? newName : p)),
        },
      })),
    );
  }
  return {
    views,
    activeId,
    active,
    dirty,
    save,
    update,
    rename,
    remove,
    activate,
    renameProject,
  };
});
