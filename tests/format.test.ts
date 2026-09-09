import { describe, expect, it } from "vitest";
import { formatDateTime, TASK_STATUSES, TASK_TYPES } from "@/shared/types";

describe("时间渲染（规划：yyyy/mm/dd hh:mm:ss）", () => {
  it("横杠转斜杠", () => {
    expect(formatDateTime("2026-09-02 10:05:34")).toBe("2026/09/02 10:05:34");
  });
  it("空值", () => {
    expect(formatDateTime(null)).toBe("");
    expect(formatDateTime("")).toBe("");
  });
});

describe("枚举常量与后端对齐快照", () => {
  it("任务类型", () => {
    expect(TASK_TYPES).toEqual(["新增需求", "优化", "BUG"]);
  });
  it("七态", () => {
    expect(TASK_STATUSES).toEqual([
      "未开始",
      "进行中",
      "待验证",
      "已完成",
      "验收未通过",
      "验收通过",
      "取消",
    ]);
  });
});
