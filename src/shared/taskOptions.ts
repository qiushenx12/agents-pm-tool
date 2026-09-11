import { PRIORITIES, TASK_STATUSES, TASK_TYPES, SUBMITTERS } from "./types";
export const statusTones: Record<string, string> = {
  未开始: "red",
  进行中: "orange",
  待验证: "blue",
  已完成: "green",
  验收未通过: "red",
  验收通过: "teal",
  取消: "gray",
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
// 优先级颜色对齐状态色：高=红（未开始）、中=黄（进行中）、低=绿（已完成）
export const priorityTones: Record<string, string> = {
  高: "red",
  中: "orange",
  低: "green",
};
export const priorityOptions = PRIORITIES.map((value) => ({
  value,
  tone: priorityTones[value],
}));
