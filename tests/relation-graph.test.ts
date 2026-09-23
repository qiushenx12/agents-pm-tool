import { expect, it } from "vitest";
import { buildRelationForest, buildRelationGraph, CARD_HEIGHT, CARD_WIDTH, type RelationGraph } from "@/grid-app/relationGraph";
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

function expectPathsOutsideCards(graph: RelationGraph) {
  for (const edge of graph.edges) {
    expect(edge.path, `${edge.from} → ${edge.to} has no route`).not.toBe("");
    const tokens = edge.path.match(/[MLQ]|-?\d+(?:\.\d+)?/g)!;
    let current = { x: 0, y: 0 };
    let commands = 0;
    for (let index = 0; index < tokens.length;) {
      const command = tokens[index++];
      const point = { x: Number(tokens[index++]), y: Number(tokens[index++]) };
      commands++;
      if (command === "M") { current = point; continue; }
      if (command === "Q") {
        const end = { x: Number(tokens[index++]), y: Number(tokens[index++]) };
        for (let step = 0; step <= 32; step++) {
          const t = step / 32;
          const x = (1 - t) ** 2 * current.x + 2 * (1 - t) * t * point.x + t ** 2 * end.x;
          const y = (1 - t) ** 2 * current.y + 2 * (1 - t) * t * point.y + t ** 2 * end.y;
          for (const node of graph.nodes)
            expect(x > node.x && x < node.x + CARD_WIDTH && y > node.y && y < node.y + CARD_HEIGHT,
              `${edge.from} → ${edge.to} curves through ${node.task.id}`).toBe(false);
        }
        current = end;
        continue;
      }
      expect(command).toBe("L");
      expect(point.x === current.x || point.y === current.y).toBe(true);
      for (const node of graph.nodes) {
        const crosses = point.y === current.y
          ? point.y > node.y && point.y < node.y + CARD_HEIGHT &&
            Math.max(point.x, current.x) > node.x && Math.min(point.x, current.x) < node.x + CARD_WIDTH
          : point.x > node.x && point.x < node.x + CARD_WIDTH &&
            Math.max(point.y, current.y) > node.y && Math.min(point.y, current.y) < node.y + CARD_HEIGHT;
        expect(crosses, `${edge.from} → ${edge.to} crosses ${node.task.id}`).toBe(false);
      }
      current = point;
    }
    expect(commands).toBeGreaterThan(1);
  }
}

it("deduplicates reciprocal dependencies, layers a chain and hides isolated tasks by default", () => {
  const graph = buildRelationGraph(
    [task("1", [], ["2"]), task("2", ["1"], ["3"]), task("3", ["2"]), task("4")],
    "1",
  );
  expect(graph.edges.map((edge) => [edge.from, edge.to])).toEqual([
    ["1", "2"],
    ["2", "3"],
  ]);
  expect(graph.nodes.map((node) => node.task.id)).toEqual(["1", "2", "3"]);
  // 父级任务（depth 最大）在左，子任务（depth 最小）在右
  expect(graph.nodes.map((node) => node.x)).toEqual([712, 372, 32]);
  expect(buildRelationGraph([task("1"), task("2")], null).nodes).toHaveLength(0);
});

it("does not expose edges whose endpoint is outside the visible task set", () => {
  const graph = buildRelationGraph([task("1", ["hidden"], ["2"]), task("2")], "1");
  expect(graph.edges.map((edge) => [edge.from, edge.to])).toEqual([["1", "2"]]);
  expect(graph.nodes.every((node) => node.x >= 0 && node.y >= 0)).toBe(true);
});

it("puts the main task left of its predecessors and centers it across their rows", () => {
  const graph = buildRelationGraph(
    [task("4", ["1"]), task("3", ["1"]), task("2", ["1"]), task("1", [], ["2", "3", "4"])],
    "1",
  );
  const nodes = new Map(graph.nodes.map((node) => [node.task.id, node]));
  const parent = nodes.get("1")!;
  const children = [nodes.get("2")!, nodes.get("3")!, nodes.get("4")!];
  // 父级任务（depth 最大）在左，子任务（depth 最小）在右
  expect(children.every((child) => parent.x > child.x)).toBe(true);
  expect(parent.y).toBe((children[0].y + children[2].y) / 2);
  expect(graph.edges.every((edge) => edge.from === "1" && edge.to !== "1")).toBe(true);
});

it("shows only the connected component of the requested task", () => {
  const graph = buildRelationGraph(
    [task("1", [], ["2"]), task("2", ["1"]), task("3", [], ["4"]), task("4", ["3"])],
    "2",
  );
  expect(graph.nodes.map((node) => node.task.id)).toEqual(["1", "2"]);
  expect(graph.edges.map((edge) => [edge.from, edge.to])).toEqual([["1", "2"]]);
});

it("shows every active related tree, hides accepted roots, and keeps accepted trees available by focus", () => {
  const tasks: Task[] = [
    task("1", [], ["2"]),
    task("2", ["1"]),
    task("3", [], ["4"]),
    task("4", ["3"]),
    task("5", [], ["6"]),
    { ...task("6", ["5"]), status: "验收通过" },
    task("7"),
  ];
  const forest = buildRelationForest(tasks);
  expect(forest.nodes.map((node) => node.task.id).sort()).toEqual(["1", "2", "3", "4"]);
  expect(forest.edges).toHaveLength(2);
  expectPathsOutsideCards(forest);
  expect(buildRelationGraph(tasks, "6").nodes.map((node) => node.task.id).sort()).toEqual(["5", "6"]);
});

it("keeps a shared tree visible until all of its top-level parents are accepted", () => {
  const shared = task("1", [], ["2", "3"]);
  const accepted: Task = { ...task("2", ["1"]), status: "验收通过" };
  const active: Task = task("3", ["1"]);
  expect(buildRelationForest([shared, accepted, active]).nodes).toHaveLength(3);
  expect(buildRelationForest([shared, accepted, { ...active, status: "验收通过" }]).nodes).toHaveLength(0);
});

it("packs multiple trees into rows without overlapping cards", () => {
  const tasks = Array.from({ length: 6 }, (_, index) => {
    const parent = String(index * 2 + 2);
    const child = String(index * 2 + 1);
    return [task(child, [], [parent]), task(parent, [child])];
  }).flat();
  const forest = buildRelationForest(tasks);
  expect(forest.nodes).toHaveLength(12);
  expect(forest.edges).toHaveLength(6);
  for (const [index, left] of forest.nodes.entries())
    for (const right of forest.nodes.slice(index + 1))
      expect(left.x + CARD_WIDTH <= right.x || right.x + CARD_WIDTH <= left.x ||
        left.y + CARD_HEIGHT <= right.y || right.y + CARD_HEIGHT <= left.y).toBe(true);
  expect(new Set(forest.nodes.map((node) => node.y)).size).toBeGreaterThan(1);
  expectPathsOutsideCards(forest);
});

it("routes the long 23334 → 22001 link around the unrelated 23326 card", () => {
  const left = "202609231220010000";
  const middle = "202609231233260000";
  const top = "202609231233340000";
  const right = "202609231233420000";
  const bottom = "202609231237160000";
  const tasks = [
    task(left, [top, right, middle]),
    task(middle, [bottom], [left]),
    task(top, [], [left]),
    task(right, [], [left]),
    task(bottom, [], [middle]),
  ];
  const positions = new Map([
    [left, { x: 32, y: 245 }],
    [middle, { x: 372, y: 245 }],
    [top, { x: 712, y: 32 }],
    [right, { x: 712, y: 245 }],
    [bottom, { x: 712, y: 455 }],
  ]);
  const graph = buildRelationGraph(tasks, middle, positions);
  expect(graph.edges).toHaveLength(4);
  expect(graph.edges.find((edge) => edge.from === top && edge.to === left)).toBeDefined();
  expect(graph.edges.find((edge) => edge.from === top && edge.to === left)?.path).toContain("Q");
  expectPathsOutsideCards(graph);
  const moved = buildRelationGraph(tasks, middle, new Map([...positions, [middle, { x: 372, y: 430 }]]));
  expectPathsOutsideCards(moved);
  expect(moved.edges.find((edge) => edge.from === middle && edge.to === left)?.path)
    .not.toBe(graph.edges.find((edge) => edge.from === middle && edge.to === left)?.path);
});

it("places all direct predecessors in one column and aligns the next layer with its parent", () => {
  const left = "202609231220010000";
  const middle = "202609231233260000";
  const top = "202609231233340000";
  const center = "202609231233420000";
  const far = "202609231237160000";
  const graph = buildRelationGraph([
    task(left, [top, center, middle]),
    task(middle, [far], [left]),
    task(top, [], [left]),
    task(center, [], [left]),
    task(far, [], [middle]),
  ], left);
  const nodes = new Map(graph.nodes.map((node) => [node.task.id, node]));
  expect(nodes.get(left)?.x).toBe(32);
  expect(nodes.get(top)?.x).toBe(372);
  expect(nodes.get(center)?.x).toBe(372);
  expect(nodes.get(middle)?.x).toBe(372);
  expect(nodes.get(far)?.x).toBe(712);
  expect(nodes.get(top)!.y).toBeLessThan(nodes.get(center)!.y);
  expect(nodes.get(center)!.y).toBeLessThan(nodes.get(middle)!.y);
  expect(nodes.get(left)?.y).toBe(nodes.get(center)?.y);
  expect(nodes.get(far)?.y).toBe(nodes.get(middle)?.y);
  expectPathsOutsideCards(graph);
  const turn = graph.edges.find((edge) => edge.from === top && edge.to === left)?.path;
  // The bend belongs in the center of the 68 px gap, not beside the focus card.
  expect(turn).toMatch(/Q 338 /);
});

it("does not draw a same-row shortcut through the card between its endpoints", () => {
  const graph = buildRelationGraph(
    [task("1", [], ["2", "3"]), task("2", ["1"], ["3"]), task("3", ["1", "2"])],
    "2",
    new Map([["1", { x: 712, y: 100 }], ["2", { x: 372, y: 100 }], ["3", { x: 32, y: 100 }]]),
  );
  expect(graph.edges).toHaveLength(3);
  expectPathsOutsideCards(graph);
});
