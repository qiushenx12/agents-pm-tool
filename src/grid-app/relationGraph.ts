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
  bounds: Rect;
  width: number;
  height: number;
}

export const CARD_WIDTH = 272;
export const CARD_HEIGHT = 106;
const COLUMN_STEP = 340;
const ROW_STEP = 170;
const MARGIN = 32;
const ROUTE_CLEARANCE = 10;
const TURN_COST = 16;
const BEND_RADIUS = 20;

export interface NodePosition { x: number; y: number }

interface Point { x: number; y: number }
interface Port { inner: Point; outer: Point; axis: 0 | 1 }
interface Rect { left: number; right: number; top: number; bottom: number }

function ports(node: RelationNode): Port[] {
  const { x, y } = node;
  const halfWidth = CARD_WIDTH / 2;
  const halfHeight = CARD_HEIGHT / 2;
  return [
    { inner: { x, y: y + halfHeight }, outer: { x: x - ROUTE_CLEARANCE, y: y + halfHeight }, axis: 0 },
    { inner: { x: x + CARD_WIDTH, y: y + halfHeight }, outer: { x: x + CARD_WIDTH + ROUTE_CLEARANCE, y: y + halfHeight }, axis: 0 },
    { inner: { x: x + halfWidth, y }, outer: { x: x + halfWidth, y: y - ROUTE_CLEARANCE }, axis: 1 },
    { inner: { x: x + halfWidth, y: y + CARD_HEIGHT }, outer: { x: x + halfWidth, y: y + CARD_HEIGHT + ROUTE_CLEARANCE }, axis: 1 },
  ];
}

function roundedPath(points: Point[], nodes: RelationNode[]): string {
  const commands = [`M ${points[0].x} ${points[0].y}`];
  for (let index = 1; index < points.length - 1; index++) {
    const before = points[index - 1];
    const corner = points[index];
    const after = points[index + 1];
    const inLength = Math.abs(corner.x - before.x) + Math.abs(corner.y - before.y);
    const outLength = Math.abs(after.x - corner.x) + Math.abs(after.y - corner.y);
    const incoming = { x: Math.sign(corner.x - before.x), y: Math.sign(corner.y - before.y) };
    const outgoing = { x: Math.sign(after.x - corner.x), y: Math.sign(after.y - corner.y) };
    let radius = Math.min(BEND_RADIUS, inLength / 2, outLength / 2);
    const curve = (distance: number) => ({
      start: { x: corner.x - incoming.x * distance, y: corner.y - incoming.y * distance },
      end: { x: corner.x + outgoing.x * distance, y: corner.y + outgoing.y * distance },
    });
    // A rounded corner can cut inside an obstacle even when its straight legs are clear.
    const curveClear = (distance: number) => {
      const { start, end } = curve(distance);
      for (let step = 0; step <= 16; step++) {
        const t = step / 16;
        const x = (1 - t) ** 2 * start.x + 2 * (1 - t) * t * corner.x + t ** 2 * end.x;
        const y = (1 - t) ** 2 * start.y + 2 * (1 - t) * t * corner.y + t ** 2 * end.y;
        if (nodes.some((node) => x > node.x - 2 && x < node.x + CARD_WIDTH + 2 &&
          y > node.y - 2 && y < node.y + CARD_HEIGHT + 2)) return false;
      }
      return true;
    };
    while (radius >= 2 && !curveClear(radius)) radius /= 2;
    if (radius < 2) {
      commands.push(`L ${corner.x} ${corner.y}`);
      continue;
    }
    const { start, end } = curve(radius);
    commands.push(`L ${start.x} ${start.y}`, `Q ${corner.x} ${corner.y} ${end.x} ${end.y}`);
  }
  const last = points[points.length - 1];
  commands.push(`L ${last.x} ${last.y}`);
  return commands.join(" ");
}

/** Route every segment outside all card rectangles, including unrelated cards. */
function routeEdge(source: RelationNode, target: RelationNode, nodes: RelationNode[]): string {
  // For neighboring columns, use the middle of the gap as a shared bend lane.
  // This keeps the fan of direct relationships away from either card edge.
  const sourceRightOfTarget = source.x >= target.x + CARD_WIDTH + ROUTE_CLEARANCE * 2;
  const targetRightOfSource = target.x >= source.x + CARD_WIDTH + ROUTE_CLEARANCE * 2;
  if (sourceRightOfTarget || targetRightOfSource) {
    const from = { x: sourceRightOfTarget ? source.x : source.x + CARD_WIDTH, y: source.y + CARD_HEIGHT / 2 };
    const to = { x: sourceRightOfTarget ? target.x + CARD_WIDTH : target.x, y: target.y + CARD_HEIGHT / 2 };
    const laneX = (from.x + to.x) / 2;
    const direct = from.y === to.y
      ? [from, to]
      : [from, { x: laneX, y: from.y }, { x: laneX, y: to.y }, to];
    const crossesCard = direct.slice(1).some((point, index) => nodes.some((node) => {
      const before = direct[index];
      const clearance = node === source || node === target ? 0 : ROUTE_CLEARANCE;
      const left = node.x - clearance;
      const right = node.x + CARD_WIDTH + clearance;
      const top = node.y - clearance;
      const bottom = node.y + CARD_HEIGHT + clearance;
      return point.y === before.y
        ? point.y > top && point.y < bottom &&
          Math.max(point.x, before.x) > left && Math.min(point.x, before.x) < right
        : point.x > left && point.x < right &&
          Math.max(point.y, before.y) > top && Math.min(point.y, before.y) < bottom;
    }));
    if (!crossesCard) return roundedPath(direct, nodes);
  }
  const obstacles: Rect[] = nodes.map(({ x, y }) => ({
    left: x - ROUTE_CLEARANCE,
    right: x + CARD_WIDTH + ROUTE_CLEARANCE,
    top: y - ROUTE_CLEARANCE,
    bottom: y + CARD_HEIGHT + ROUTE_CLEARANCE,
  }));
  const sources = ports(source);
  const targets = ports(target);
  const xs = [...new Set([...obstacles.flatMap((r) => [r.left, r.right]), ...sources.map((p) => p.outer.x), ...targets.map((p) => p.outer.x)])].sort((a, b) => a - b);
  const ys = [...new Set([...obstacles.flatMap((r) => [r.top, r.bottom]), ...sources.map((p) => p.outer.y), ...targets.map((p) => p.outer.y)])].sort((a, b) => a - b);
  const nx = xs.length;
  const pointCount = nx * ys.length;
  const inside = (x: number, y: number) => obstacles.some((r) => x > r.left && x < r.right && y > r.top && y < r.bottom);
  const valid = Array.from({ length: pointCount }, (_, index) => !inside(xs[index % nx], ys[Math.floor(index / nx)]));
  const indexOf = (point: Point) => ys.indexOf(point.y) * nx + xs.indexOf(point.x);
  const segmentClear = (a: Point, b: Point) => obstacles.every((r) =>
    a.y === b.y
      ? !(a.y > r.top && a.y < r.bottom && Math.max(a.x, b.x) > r.left && Math.min(a.x, b.x) < r.right)
      : !(a.x > r.left && a.x < r.right && Math.max(a.y, b.y) > r.top && Math.min(a.y, b.y) < r.bottom),
  );
  // A state includes the entering axis, so shorter routes with fewer bends win.
  const distance = Array<number>(pointCount * 2).fill(Infinity);
  const previous = Array<number>(pointCount * 2).fill(-1);
  const origin = Array<number>(pointCount * 2).fill(-1);
  const heap: { state: number; cost: number }[] = [];
  const push = (state: number, cost: number) => {
    heap.push({ state, cost });
    for (let i = heap.length - 1; i > 0;) {
      const parent = Math.floor((i - 1) / 2);
      if (heap[parent].cost <= heap[i].cost) break;
      [heap[parent], heap[i]] = [heap[i], heap[parent]];
      i = parent;
    }
  };
  const pop = () => {
    const first = heap[0];
    const last = heap.pop()!;
    if (heap.length) {
      heap[0] = last;
      for (let i = 0;;) {
        const left = i * 2 + 1;
        if (left >= heap.length) break;
        const right = left + 1;
        const child = right < heap.length && heap[right].cost < heap[left].cost ? right : left;
        if (heap[i].cost <= heap[child].cost) break;
        [heap[i], heap[child]] = [heap[child], heap[i]];
        i = child;
      }
    }
    return first;
  };
  const stubClear = (port: Port, own: RelationNode) => nodes.every((node) => node === own || !(
    Math.max(port.inner.x, port.outer.x) > node.x && Math.min(port.inner.x, port.outer.x) < node.x + CARD_WIDTH &&
    Math.max(port.inner.y, port.outer.y) > node.y && Math.min(port.inner.y, port.outer.y) < node.y + CARD_HEIGHT
  ));
  sources.forEach((port, portIndex) => {
    const point = indexOf(port.outer);
    if (!valid[point] || !stubClear(port, source)) return;
    const state = point * 2 + port.axis;
    distance[state] = 0;
    origin[state] = portIndex;
    push(state, 0);
  });
  let goal = -1;
  let goalPort = -1;
  let best = Infinity;
  while (heap.length) {
    const { state, cost } = pop();
    if (cost !== distance[state]) continue;
    if (cost >= best) break;
    const point = Math.floor(state / 2);
    const axis = state % 2;
    const ix = point % nx;
    const iy = Math.floor(point / nx);
    const here = { x: xs[ix], y: ys[iy] };
    targets.forEach((port, portIndex) => {
      if (here.x !== port.outer.x || here.y !== port.outer.y || !stubClear(port, target)) return;
      const total = cost + (axis === port.axis ? 0 : TURN_COST);
      if (total < best) { best = total; goal = state; goalPort = portIndex; }
    });
    for (const [nextX, nextY, nextAxis] of [
      [ix - 1, iy, 0], [ix + 1, iy, 0], [ix, iy - 1, 1], [ix, iy + 1, 1],
    ] as const) {
      if (nextX < 0 || nextX >= nx || nextY < 0 || nextY >= ys.length) continue;
      const nextPoint = nextY * nx + nextX;
      if (!valid[nextPoint]) continue;
      const next = { x: xs[nextX], y: ys[nextY] };
      if (!segmentClear(here, next)) continue;
      const nextState = nextPoint * 2 + nextAxis;
      const nextCost = cost + Math.abs(next.x - here.x) + Math.abs(next.y - here.y) + (axis === nextAxis ? 0 : TURN_COST);
      if (nextCost >= distance[nextState]) continue;
      distance[nextState] = nextCost;
      previous[nextState] = state;
      origin[nextState] = origin[state];
      push(nextState, nextCost);
    }
  }
  if (goal < 0) return "";
  const points: Point[] = [];
  for (let state = goal; state >= 0; state = previous[state]) {
    const point = Math.floor(state / 2);
    points.push({ x: xs[point % nx], y: ys[Math.floor(point / nx)] });
  }
  points.reverse();
  points.unshift(sources[origin[goal]].inner);
  points.push(targets[goalPort].inner);
  // Remove intermediate collinear grid points for a compact SVG path.
  const compact = points.filter((point, index) => index === 0 || index === points.length - 1 ||
    (point.x - points[index - 1].x) * (points[index + 1].y - point.y) !==
    (point.y - points[index - 1].y) * (points[index + 1].x - point.x));
  return roundedPath(compact, nodes);
}

function visibleLinks(tasks: Task[]): { from: string; to: string }[] {
  const byId = new Set(tasks.map((task) => task.id));
  const links = new Map<string, { from: string; to: string }>();
  for (const task of tasks) {
    for (const id of task.predecessor_task_ids ?? []) {
      if (id !== task.id && byId.has(id)) links.set(`${id}\0${task.id}`, { from: id, to: task.id });
    }
    for (const id of task.unlock_task_ids ?? []) {
      if (id !== task.id && byId.has(id)) links.set(`${task.id}\0${id}`, { from: task.id, to: id });
    }
  }
  return [...links.values()];
}

function graphFromNodes(nodes: RelationNode[], edges: RelationEdge[]): RelationGraph {
  if (!nodes.length)
    return { nodes: [], edges: [], bounds: { left: 0, right: 0, top: 0, bottom: 0 }, width: 0, height: 0 };
  const bounds = {
    left: Math.min(...nodes.map((node) => node.x - ROUTE_CLEARANCE)),
    right: Math.max(...nodes.map((node) => node.x + CARD_WIDTH + ROUTE_CLEARANCE)),
    top: Math.min(...nodes.map((node) => node.y - ROUTE_CLEARANCE)),
    bottom: Math.max(...nodes.map((node) => node.y + CARD_HEIGHT + ROUTE_CLEARANCE)),
  };
  return {
    nodes,
    edges,
    bounds,
    width: Math.max(1, ...nodes.map((node) => node.x + CARD_WIDTH + MARGIN)),
    height: Math.max(1, ...nodes.map((node) => node.y + CARD_HEIGHT + MARGIN)),
  };
}

/** Show only the selected task's visible, transitively connected dependency network. */
export function buildRelationGraph(tasks: Task[], focusTaskId: string | null, positions: ReadonlyMap<string, NodePosition> = new Map()): RelationGraph {
  const byId = new Map(tasks.map((task) => [task.id, task]));
  if (!focusTaskId || !byId.has(focusTaskId)) return graphFromNodes([], []);
  const rawEdges = visibleLinks(tasks);
  // Positive layers contain predecessors; negative layers contain unlock targets.
  const neighbors = new Map(tasks.map((task) => [task.id, [] as { id: string; direction: -1 | 1 }[]]));
  for (const { from, to } of rawEdges) {
    neighbors.get(from)?.push({ id: to, direction: -1 });
    neighbors.get(to)?.push({ id: from, direction: 1 });
  }
  const layer = new Map([[focusTaskId, 0]]);
  const queue = [focusTaskId];
  while (queue.length) {
    const current = queue.shift()!;
    for (const { id, direction } of neighbors.get(current) ?? []) {
      if (!layer.has(id)) {
        layer.set(id, layer.get(current)! + direction);
        queue.push(id);
      }
    }
  }
  const visible = tasks.filter((task) => layer.has(task.id));
  const edges = rawEdges.filter(({ from, to }) => layer.has(from) && layer.has(to));
  const columns = new Map<number, Task[]>();
  for (const task of visible) {
    const level = layer.get(task.id)!;
    const column = columns.get(level) ?? [];
    column.push(task);
    columns.set(level, column);
  }
  const minLayer = Math.min(...columns.keys());
  const maxCount = Math.max(...[...columns.values()].map((column) => column.length));
  const fartherCount = (task: Task) => {
    const level = layer.get(task.id)!;
    const farther = level + Math.sign(level);
    return level === 0 ? 0 : (neighbors.get(task.id) ?? []).filter(({ id }) => layer.get(id) === farther).length;
  };
  for (const column of columns.values())
    column.sort((a, b) => fartherCount(a) - fartherCount(b) || a.seq - b.seq || a.id.localeCompare(b.id));
  // Anchor the busiest column, then align adjacent columns with the cards they connect to.
  const anchor = [...columns].sort(([levelA, a], [levelB, b]) =>
    b.length - a.length || Math.abs(levelA) - Math.abs(levelB) || levelA - levelB)[0][0];
  const yById = new Map<string, number>();
  columns.get(anchor)!.forEach((task, index) => yById.set(task.id, MARGIN + index * ROW_STEP));
  const placeColumn = (level: number, towardAnchor: number) => {
    const column = columns.get(level)!;
    const desired = column.map((task, index) => {
      const connected = (neighbors.get(task.id) ?? [])
        .filter(({ id }) => layer.get(id) === towardAnchor && yById.has(id))
        .map(({ id }) => yById.get(id)!);
      const fallback = MARGIN + (maxCount - column.length) * ROW_STEP / 2 + index * ROW_STEP;
      return { task, order: index, y: connected.length ? connected.reduce((a, b) => a + b, 0) / connected.length : fallback };
    }).sort((a, b) => a.y - b.y || a.order - b.order);
    const packed: number[] = [];
    desired.forEach(({ y }, index) => packed.push(index ? Math.max(y, packed[index - 1] + ROW_STEP) : y));
    const shift = desired.reduce((sum, item, index) => sum + item.y - packed[index], 0) / desired.length;
    desired.forEach(({ task }, index) => yById.set(task.id, packed[index] + shift));
  };
  for (let level = anchor - 1; columns.has(level); level--) placeColumn(level, level + 1);
  for (let level = anchor + 1; columns.has(level); level++) placeColumn(level, level - 1);
  const yOffset = MARGIN - Math.min(...yById.values());
  const nodes: RelationNode[] = visible.map((task) => {
    const initial = { x: MARGIN + (layer.get(task.id)! - minLayer) * COLUMN_STEP, y: yById.get(task.id)! + yOffset };
    return { task, ...(positions.get(task.id) ?? initial) };
  });
  const nodesById = new Map(nodes.map((node) => [node.task.id, node]));
  const drawnEdges: RelationEdge[] = edges.map(({ from, to }) => {
    const source = nodesById.get(from)!;
    const target = nodesById.get(to)!;
    return { from, to, path: routeEdge(source, target, nodes) };
  });
  return graphFromNodes(nodes, drawnEdges);
}

/** Arrange every visible related tree whose top-level parent has not been accepted. */
export function buildRelationForest(tasks: Task[], positions: ReadonlyMap<string, NodePosition> = new Map()): RelationGraph {
  const links = visibleLinks(tasks);
  const byId = new Map(tasks.map((task) => [task.id, task]));
  const taskOrder = new Map(tasks.map((task, index) => [task.id, index]));
  const outgoing = new Set(links.map(({ from }) => from));
  const neighbors = new Map(tasks.map((task) => [task.id, new Set<string>()]));
  for (const { from, to } of links) {
    neighbors.get(from)!.add(to);
    neighbors.get(to)!.add(from);
  }
  const visited = new Set<string>();
  const components: { root: Task; graph: RelationGraph }[] = [];
  for (const task of tasks) {
    if (visited.has(task.id) || !neighbors.get(task.id)?.size) continue;
    const ids = new Set<string>();
    const queue = [task.id];
    visited.add(task.id);
    for (let index = 0; index < queue.length; index++) {
      const current = queue[index];
      ids.add(current);
      for (const next of neighbors.get(current) ?? []) {
        if (visited.has(next)) continue;
        visited.add(next);
        queue.push(next);
      }
    }
    const members = [...ids].map((id) => byId.get(id)!)
      .sort((a, b) => taskOrder.get(a.id)! - taskOrder.get(b.id)!);
    const roots = members.filter((item) => !outgoing.has(item.id));
    if (roots.length && roots.every((item) => item.status === "验收通过")) continue;
    const root = (roots.length ? roots : members).sort((a, b) => b.seq - a.seq || b.id.localeCompare(a.id))[0];
    components.push({ root, graph: buildRelationGraph(members, root.id) });
  }
  components.sort((a, b) => b.root.seq - a.root.seq || b.root.id.localeCompare(a.root.id));

  const TREE_GAP = 96;
  const ROW_WIDTH = 1600;
  let rowX = MARGIN;
  let rowY = MARGIN;
  let rowHeight = 0;
  const nodes: RelationNode[] = [];
  const edges: RelationEdge[] = [];
  for (const { graph } of components) {
    const width = graph.bounds.right - graph.bounds.left;
    const height = graph.bounds.bottom - graph.bounds.top;
    if (rowX > MARGIN && rowX + width > ROW_WIDTH) {
      rowX = MARGIN;
      rowY += rowHeight + TREE_GAP;
      rowHeight = 0;
    }
    const offsetX = rowX - graph.bounds.left;
    const offsetY = rowY - graph.bounds.top;
    const treeNodes = graph.nodes.map((node) => ({
      task: node.task,
      ...(positions.get(node.task.id) ?? { x: node.x + offsetX, y: node.y + offsetY }),
    }));
    const byId = new Map(treeNodes.map((node) => [node.task.id, node]));
    edges.push(...graph.edges.map(({ from, to }) => ({
      from,
      to,
      path: routeEdge(byId.get(from)!, byId.get(to)!, treeNodes),
    })));
    nodes.push(...treeNodes);
    rowX += width + TREE_GAP;
    rowHeight = Math.max(rowHeight, height);
  }
  return graphFromNodes(nodes, edges);
}
