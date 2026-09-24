import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import { TASK_ROW_ACTIONS } from "../taskActions";
// 新装用户的默认列：顺序即任务字段的默认排布（操作列固定最后，不在此列表内），
// 全部默认展示。老用户已保存的列顺序/显隐由下面的合并逻辑保留，不受此处调整影响。
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
    key: "submitter",
    label: "提交人",
    icon: "user",
    width: 150,
    visible: true,
  },
  {
    key: "assignee",
    label: "负责人",
    icon: "bot",
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
    visible: true,
  },
  { key: "note", label: "备注", icon: "edit", width: 240, visible: true },
  { key: "id", label: "ID", icon: "text", width: 200, visible: true },
  {
    key: "predecessor_task_ids",
    label: "子任务 ID",
    icon: "git",
    width: 240,
    visible: true,
  },
  {
    key: "unlock_task_ids",
    label: "父级任务 ID",
    icon: "git",
    width: 240,
    visible: true,
  },
];
export const useViewStore = defineStore("view", () => {
  let saved: {
    columns?: { key: string; width: number; visible: boolean }[];
    density?: number;
    frozenColumns?: number;
    collapsed?: boolean;
    projectsOpen?: boolean;
    hiddenActions?: unknown[];
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
      ...previousColumns.map((p) => p.key),
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
  const frozenColumns = ref(
    Number.isInteger(saved.frozenColumns) &&
      saved.frozenColumns! >= 0 &&
      saved.frozenColumns! <= 7
      ? saved.frozenColumns!
      : 1,
  );
  const collapsed = ref(saved.collapsed ?? window.innerWidth < 1100);
  /** 侧栏「Agents PM」模块的展开状态：收起只留模块标题，展开显示项目列表。 */
  const projectsOpen = ref(saved.projectsOpen ?? true);
  /**
   * 操作列里被隐藏的按钮 key。存「隐藏」而不是「显示」：以后新增的行级按钮
   * 对老用户默认可见，无需迁移；新装用户为空数组，即所有按钮都在。
   * 只保留仍然存在的 key，避免版本升级后残留旧按钮名。
   */
  const hiddenActions = ref(
    (Array.isArray(saved.hiddenActions) ? saved.hiddenActions : []).filter(
      (key): key is string =>
        typeof key === "string" &&
        TASK_ROW_ACTIONS.some((action) => action.key === key),
    ),
  );
  // 存储被改坏成「全部隐藏」时按无隐藏处理，保证操作列至少留一个按钮
  if (hiddenActions.value.length >= TASK_ROW_ACTIONS.length)
    hiddenActions.value = [];
  const visibleColumns = computed(() => columns.value.filter((c) => c.visible));
  /** 操作列实际渲染的按钮。 */
  const visibleRowActions = computed(() =>
    TASK_ROW_ACTIONS.filter(
      (action) => !hiddenActions.value.includes(action.key),
    ),
  );
  /** 已隐藏的可以重新显示；可见时必须还有别的按钮在——操作列至少保留一个。 */
  function canToggleRowAction(key: string) {
    return (
      hiddenActions.value.includes(key) || visibleRowActions.value.length > 1
    );
  }
  function toggleRowAction(key: string) {
    if (!canToggleRowAction(key)) return;
    hiddenActions.value = hiddenActions.value.includes(key)
      ? hiddenActions.value.filter((item) => item !== key)
      : [...hiddenActions.value, key];
  }
  function reset() {
    columns.value = DEFAULT_COLUMNS.map((c) => ({ ...c }));
    density.value = 36;
    frozenColumns.value = 1;
    hiddenActions.value = [];
  }
  function moveColumn(key: string, direction: -1 | 1) {
    const index = columns.value.findIndex((c) => c.key === key),
      target = index + direction;
    if (index < 0 || target < 0 || target >= columns.value.length) return;
    const [column] = columns.value.splice(index, 1);
    columns.value.splice(target, 0, column);
  }
  function moveBefore(source: string, target: string) {
    if (source === target) return;
    const index = columns.value.findIndex((c) => c.key === source);
    if (
      index < 0 ||
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
    [columns, density, frozenColumns, collapsed, projectsOpen, hiddenActions],
    () => {
      try {
        localStorage.setItem(
          "pm-table-view-v1",
          JSON.stringify({
            columns: columns.value,
            density: density.value,
            frozenColumns: frozenColumns.value,
            collapsed: collapsed.value,
            projectsOpen: projectsOpen.value,
            hiddenActions: hiddenActions.value,
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
    frozenColumns,
    collapsed,
    projectsOpen,
    hiddenActions,
    visibleRowActions,
    canToggleRowAction,
    toggleRowAction,
    reset,
    moveColumn,
    moveBefore,
  };
});
