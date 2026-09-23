import type { Task } from "@/shared/types";

export interface RelationNode {
  task: Task;
  x: number;
  y: number;
}

export interface RelationEdge {
  from: string;
  to: string;
  path: string;
}

export interface RelationGraph {
  nodes: RelationNode[];
  edges: RelationEdge[];
  isolatedCount: number;
  width: number;
  height: number;
}

const CARD_WIDTH = 272;
const CARD_HEIGHT = 106;
const COLUMN_STEP = 340;
const ROW_STEP = 140;
const MARGIN = 32;

/** Only the visible task set participates; the API may hide related tasks in other projects. */
export function buildRelationGraph(tasks: Task[], showIsolated: boolean): RelationGraph {
  const byId = new Map(tasks.map((task) => [task.id, task]));
  const links = new Map<string, { from: string; to: string }>();
  for (const task of tasks) {
    for (const id of task.predecessor_task_ids ?? []) {
      if (id !== task.id && byId.has(id)) links.set(`${id}\0${task.id}`, { from: id, to: task.id });
    }
    for (const id of task.unlock_task_ids ?? []) {
      if (id !== task.id && byId.has(id)) links.set(`${task.id}\0${id}`, { from: task.id, to: id });
    }
  }
  const rawEdges = [...links.values()];
  const related = new Set(rawEdges.flatMap(({ from, to }) => [from, to]));
  const isolatedCount = tasks.length - related.size;
  const visible = tasks.filter((task) => showIsolated || related.has(task.id));
  const visibleIds = new Set(visible.map((task) => task.id));
  const edges = rawEdges.filter(({ from, to }) => visibleIds.has(from) && visibleIds.has(to));

  const depth = new Map(visible.map((task) => [task.id, 0]));
  const degree = new Map(visible.map((task) => [task.id, 0]));
  const outgoing = new Map(visible.map((task) => [task.id, [] as string[]]));
  for (const { from, to } of edges) {
    degree.set(to, (degree.get(to) ?? 0) + 1);
    outgoing.get(from)?.push(to);
  }
  const pending = visible.filter((task) => degree.get(task.id) === 0).map((task) => task.id);
  while (pending.length) {
    const from = pending.shift()!;
    for (const to of outgoing.get(from) ?? []) {
      depth.set(to, Math.max(depth.get(to) ?? 0, (depth.get(from) ?? 0) + 1));
      degree.set(to, (degree.get(to) ?? 1) - 1);
      if (degree.get(to) === 0) pending.push(to);
    }
  }
  // The database rejects cycles. If older data contains one, keep it renderable.
  const columns = new Map<number, Task[]>();
  for (const task of visible) {
    const level = depth.get(task.id) ?? 0;
    const column = columns.get(level) ?? [];
    column.push(task);
    columns.set(level, column);
  }
  const contentHeight = Math.max(
    0,
    ...[...columns.values()].map((column) => (column.length - 1) * ROW_STEP + CARD_HEIGHT),
  );
  const nodes: RelationNode[] = [];
  for (const [level, column] of [...columns].sort(([a], [b]) => a - b)) {
    column.sort((a, b) => a.seq - b.seq || a.id.localeCompare(b.id));
    const columnHeight = (column.length - 1) * ROW_STEP + CARD_HEIGHT;
    const columnTop = MARGIN + (contentHeight - columnHeight) / 2;
    column.forEach((task, index) => {
      nodes.push({ task, x: MARGIN + level * COLUMN_STEP, y: columnTop + index * ROW_STEP });
    });
  }
  const positions = new Map(nodes.map((node) => [node.task.id, node]));
  const drawnEdges: RelationEdge[] = edges.map(({ from, to }) => {
    const source = positions.get(from)!;
    const target = positions.get(to)!;
    const x1 = source.x + CARD_WIDTH;
    const x2 = target.x;
    const y1 = source.y + CARD_HEIGHT / 2;
    const y2 = target.y + CARD_HEIGHT / 2;
    const bend = Math.max(35, (x2 - x1) / 2);
    return { from, to, path: `M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}` };
  });
  return {
    nodes,
    edges: drawnEdges,
    isolatedCount,
    width: Math.max(0, ...nodes.map((node) => node.x + CARD_WIDTH + MARGIN)),
    height: Math.max(0, ...nodes.map((node) => node.y + CARD_HEIGHT + MARGIN)),
  };
}
