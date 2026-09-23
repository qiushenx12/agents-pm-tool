import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
export const DEFAULT_COLUMNS = [
  {
    key: "description",
    label: "任务描述",
    icon: "text",
    width: 380,
    visible: true,
  },
  { key: "project", label: "项目", icon: "folder", width: 156, visible: true },
  { key: "type", label: "任务类型", icon: "tag", width: 120, visible: true },
  {
    key: "priority",
    label: "优先级",
    icon: "flag",
    width: 96,
    visible: true,
  },
  {
    key: "status",
    label: "当前状态",
    icon: "circle",
    width: 138,
    visible: true,
  },
  {
    key: "predecessor_task_ids",
    label: "前置任务 ID",
    icon: "git",
    width: 240,
    visible: true,
  },
  {
    key: "unlock_task_ids",
    label: "解锁任务 ID",
    icon: "git",
    width: 240,
    visible: true,
  },
  {
    key: "submitter",
    label: "提交人",
    icon: "user",
    width: 150,
    visible: true,
  },
  {
    key: "attachments",
    label: "附件",
    icon: "attachment",
    width: 90,
    visible: true,
  },
  {
    key: "created_at",
    label: "创建时间",
    icon: "clock",
    width: 172,
    visible: true,
  },
  {
    key: "finished_at",
    label: "完成时间",
    icon: "clock",
    width: 172,
    visible: false,
  },
  { key: "id", label: "ID", icon: "text", width: 200, visible: false },
  { key: "note", label: "备注", icon: "edit", width: 240, visible: true },
];
export const useViewStore = defineStore("view", () => {
  let saved: {
    columns?: { key: string; width: number; visible: boolean }[];
    density?: number;
    collapsed?: boolean;
    projectsOpen?: boolean;
  } = {};
  try {
    saved = JSON.parse(localStorage.getItem("pm-table-view-v1") || "{}") ?? {};
  } catch {
    /* reset malformed preferences */
  }
  const previousColumns = Array.isArray(saved.columns)
    ? saved.columns.filter((p) => p && typeof p.key === "string")
    : [];
  const order = [
    ...new Set([
      "description",
      ...previousColumns
        .map((p) => p.key)
        .filter((key) => key !== "description"),
      ...DEFAULT_COLUMNS.map((c) => c.key),
    ]),
  ];
  const columns = ref(
    order
      .flatMap((key) => DEFAULT_COLUMNS.filter((c) => c.key === key))
      .map((c) => {
        const previous = previousColumns.find((p) => p.key === c.key);
        return {
          ...c,
          width:
            previous && Number.isFinite(previous.width)
              ? Math.max(
                  c.key === "description" ? 260 : 80,
                  Math.min(800, previous.width),
                )
              : c.width,
          visible:
            c.key === "description" ||
            (typeof previous?.visible === "boolean"
              ? previous.visible
              : c.visible),
        };
      }),
  );
  const density = ref(
    [32, 36, 44].includes(saved.density ?? 0) ? saved.density! : 36,
  );
  const collapsed = ref(saved.collapsed ?? window.innerWidth < 1100);
  /** 侧栏「Agents PM」模块的展开状态：收起只留模块标题，展开显示项目列表。 */
  const projectsOpen = ref(saved.projectsOpen ?? true);
  const visibleColumns = computed(() => columns.value.filter((c) => c.visible));
  function reset() {
    columns.value = DEFAULT_COLUMNS.map((c) => ({ ...c }));
    density.value = 36;
  }
  function moveColumn(key: string, direction: -1 | 1) {
    const index = columns.value.findIndex((c) => c.key === key),
      target = index + direction;
    if (index <= 0 || target <= 0 || target >= columns.value.length) return;
    const [column] = columns.value.splice(index, 1);
    columns.value.splice(target, 0, column);
  }
  function moveBefore(source: string, target: string) {
    if (
      source === target ||
      source === "description" ||
      target === "description"
    )
      return;
    const index = columns.value.findIndex((c) => c.key === source);
    if (
      index < 1 ||
      (target !== "actions" && !columns.value.some((c) => c.key === target))
    )
      return;
    const [column] = columns.value.splice(index, 1);
    if (target === "actions") {
      columns.value.push(column);
      return;
    }
    columns.value.splice(
      columns.value.findIndex((c) => c.key === target),
      0,
      column,
    );
  }
  watch(
    [columns, density, collapsed, projectsOpen],
    () => {
      try {
        localStorage.setItem(
          "pm-table-view-v1",
          JSON.stringify({
            columns: columns.value,
            density: density.value,
            collapsed: collapsed.value,
            projectsOpen: projectsOpen.value,
          }),
        );
      } catch {
        /* browsing without storage still works */
      }
    },
    { deep: true },
  );
  return {
    columns,
    visibleColumns,
    density,
    collapsed,
    projectsOpen,
    reset,
    moveColumn,
    moveBefore,
  };
});
