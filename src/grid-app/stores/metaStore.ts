import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "@/grid-app/api/client";
import type { Project } from "@/shared/types";
import { SUBMITTERS, TASK_STATUSES, TASK_TYPES } from "@/shared/types";

export const useMetaStore = defineStore("meta", () => {
  const projects = ref<Project[]>([]);
  const error = ref("");
  let requestId = 0;
  const taskTypes = TASK_TYPES;
  const taskStatuses = TASK_STATUSES;
  const submitters = SUBMITTERS;

  async function refresh() {
    const id = ++requestId;
    try {
      const result = await api.listProjects();
      if (id === requestId) {
        projects.value = result;
        error.value = "";
      }
    } catch (e) {
      if (id === requestId)
        error.value = e instanceof Error ? e.message : String(e);
    }
  }

  function projectColor(name: string): string {
    return projects.value.find((p) => p.name === name)?.color ?? "#007AFF";
  }

  return {
    projects,
    error,
    taskTypes,
    taskStatuses,
    submitters,
    refresh,
    projectColor,
  };
});
