import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "@/grid-app/api/client";
import type { Project } from "@/shared/types";
import { SUBMITTERS, TASK_STATUSES, TASK_TYPES } from "@/shared/types";

export const useMetaStore = defineStore("meta", () => {
  const projects = ref<Project[]>([]);
  const taskTypes = TASK_TYPES;
  const taskStatuses = TASK_STATUSES;
  const submitters = SUBMITTERS;

  async function refresh() {
    projects.value = await api.listProjects();
  }

  function projectColor(name: string): string {
    return projects.value.find((p) => p.name === name)?.color ?? "#007AFF";
  }

  return { projects, taskTypes, taskStatuses, submitters, refresh, projectColor };
});
