import {
  GROUP_FIELDS,
  SUBMITTERS,
  TASK_STATUSES,
  TASK_TYPES,
  type GroupField,
  type Submitter,
  type TaskListQuery,
  type TaskStatus,
  type TaskType,
} from "@/shared/types";
export interface FilterState {
  project: string[];
  type: TaskType[];
  status: TaskStatus[];
  submitter: Submitter[];
  keyword: string;
  sort_by: NonNullable<TaskListQuery["sort_by"]>;
  sort_order: "asc" | "desc";
  group_by: GroupField | "";
}
export const FILTER_KEYS = ["project", "type", "status", "submitter"] as const;
const SORT_FIELDS = ["created_at", "finished_at", "seq", "updated_at"] as const;
export function sanitizeFilters(value: unknown): FilterState {
  const f =
    value && typeof value === "object"
      ? (value as Record<string, unknown>)
      : {};
  const strings = (key: string) =>
    Array.isArray(f[key])
      ? [
          ...new Set(
            (f[key] as unknown[]).filter(
              (v): v is string => typeof v === "string" && !!v,
            ),
          ),
        ]
      : [];
  return {
    project: strings("project"),
    type: strings("type").filter((v) =>
      TASK_TYPES.includes(v as TaskType),
    ) as TaskType[],
    status: strings("status").filter((v) =>
      TASK_STATUSES.includes(v as TaskStatus),
    ) as TaskStatus[],
    submitter: strings("submitter").filter((v) =>
      SUBMITTERS.includes(v as Submitter),
    ) as Submitter[],
    keyword: typeof f.keyword === "string" ? f.keyword : "",
    sort_by: SORT_FIELDS.includes(f.sort_by as FilterState["sort_by"])
      ? (f.sort_by as FilterState["sort_by"])
      : "created_at",
    sort_order: f.sort_order === "asc" ? "asc" : "desc",
    group_by: GROUP_FIELDS.includes(f.group_by as GroupField)
      ? (f.group_by as GroupField)
      : "",
  };
}
export function filterFingerprint(value: FilterState) {
  const f = sanitizeFilters(value);
  FILTER_KEYS.forEach((key) => (f[key] as string[]).sort());
  return JSON.stringify(f);
}
export function readFiltersFromUrl() {
  const p = new URLSearchParams(window.location.search);
  return sanitizeFilters({
    ...Object.fromEntries(p),
    ...Object.fromEntries(FILTER_KEYS.map((k) => [k, p.getAll(k)])),
  });
}
export function writeFiltersToUrl(f: FilterState) {
  const p = new URLSearchParams();
  FILTER_KEYS.forEach((k) => f[k].forEach((v) => p.append(k, v)));
  if (f.keyword) p.set("keyword", f.keyword);
  if (f.sort_by !== "created_at") p.set("sort_by", f.sort_by);
  if (f.sort_order !== "desc") p.set("sort_order", f.sort_order);
  if (f.group_by) p.set("group_by", f.group_by);
  window.history.replaceState(
    null,
    "",
    window.location.pathname +
      (p.size ? "?" + p.toString() : "") +
      window.location.hash,
  );
}
