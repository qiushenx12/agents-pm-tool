import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "@/grid-app/api/client";
import type { Project } from "@/shared/types";
import { PRIORITIES, SUBMITTERS, TASK_STATUSES, TASK_TYPES } from "@/shared/types";

export const useMetaStore = defineStore("meta", () => {
  const projects = ref<Project[]>([]);
  const error = ref("");
  /** 提交人筛选的用户名候选（可见项目内有任务归属的未停用账号） */
  const submitterNames = ref<string[]>([]);
  let requestId = 0;
  const taskTypes = TASK_TYPES;
  const taskStatuses = TASK_STATUSES;
  const submitters = SUBMITTERS;
  const priorities = PRIORITIES;

  async function refresh() {
    const id = ++requestId;
    try {
      const [projectList, names] = await Promise.all([
        api.listProjects(),
        api.listSubmitterNames(),
      ]);
      if (id === requestId) {
        projects.value = projectList;
        submitterNames.value = names;
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
    submitterNames,
    priorities,
    refresh,
    projectColor,
  };
});
