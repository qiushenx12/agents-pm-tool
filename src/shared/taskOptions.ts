import { TASK_STATUSES, TASK_TYPES, SUBMITTERS } from "./types";
export const statusTones: Record<string, string> = {
  未开始: "gray",
  进行中: "blue",
  待验证: "orange",
  已完成: "green",
  验收未通过: "red",
  验收通过: "teal",
};
export const statusOptions = TASK_STATUSES.map((value) => ({
  value,
  tone: statusTones[value],
}));
export const typeOptions = TASK_TYPES.map((value) => ({
  value,
  tone: value === "BUG" ? "red" : value === "优化" ? "purple" : "blue",
}));
export const submitterOptions = SUBMITTERS.map((value) => ({
  value,
  tone: value === "Agent" ? "purple" : "gray",
}));
