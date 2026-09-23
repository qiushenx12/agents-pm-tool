import { expect, it } from "vitest";
import { buildRelationGraph } from "@/grid-app/relationGraph";
import type { Task } from "@/shared/types";

function task(id: string, predecessors: string[] = [], unlocked: string[] = []): Task {
  return {
    id,
    seq: Number(id),
    project: "测试项目",
    type: "新增需求",
    description: `任务 ${id}`,
    note: "",
    status: "未开始",
    priority: "中",
    submitter: "用户",
    created_at: "2026-09-23 10:00:00",
    finished_at: null,
    updated_at: "2026-09-23 10:00:00",
    position: Number(id),
    predecessor_task_ids: predecessors,
    unlock_task_ids: unlocked,
  };
}

it("deduplicates reciprocal dependencies, layers a chain and hides isolated tasks by default", () => {
  const graph = buildRelationGraph(
    [task("1", [], ["2"]), task("2", ["1"], ["3"]), task("3", ["2"]), task("4")],
    false,
  );
  expect(graph.edges.map((edge) => [edge.from, edge.to])).toEqual([
    ["1", "2"],
    ["2", "3"],
  ]);
  expect(graph.nodes.map((node) => node.task.id)).toEqual(["1", "2", "3"]);
  expect(graph.nodes.map((node) => node.x)).toEqual([32, 372, 712]);
  expect(graph.isolatedCount).toBe(1);
  expect(buildRelationGraph([task("1"), task("2")], true).nodes).toHaveLength(2);
});

it("does not expose edges whose endpoint is outside the visible task set", () => {
  const graph = buildRelationGraph([task("1", ["hidden"], ["2"]), task("2")], false);
  expect(graph.edges.map((edge) => [edge.from, edge.to])).toEqual([["1", "2"]]);
  expect(graph.nodes.every((node) => node.x >= 0 && node.y >= 0)).toBe(true);
});

it("puts the upstream task left of its children and centers it across their rows", () => {
  const graph = buildRelationGraph(
    [task("4", ["1"]), task("3", ["1"]), task("2", ["1"]), task("1", [], ["2", "3", "4"])],
    false,
  );
  const nodes = new Map(graph.nodes.map((node) => [node.task.id, node]));
  const parent = nodes.get("1")!;
  const children = [nodes.get("2")!, nodes.get("3")!, nodes.get("4")!];
  expect(children.every((child) => parent.x < child.x)).toBe(true);
  expect(parent.y).toBe((children[0].y + children[2].y) / 2);
  expect(graph.edges.every((edge) => edge.from === "1" && edge.to !== "1")).toBe(true);
});
