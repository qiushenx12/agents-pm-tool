/**
 * 页头折叠（「任务管理」标题块 + 视图标签栏）的判定规则。
 *
 * 只有一处驱动：指针停在**表格上方区域**时的滚轮。任务表自身的滚动不参与 ——
 * 位置判定和方向判定混在一起会出现"我在往回滚、页头却收起来"的矛盾。
 *
 * 收起是渐进式的：滚轮位移按固定比例折算成高度，滚几格收几成，不是一格到位；
 * 向上滚原路退回，回到 0 就完全展开。
 */

/** 滚轮每滚这么多像素，页头收起 1px（一格鼠标滚轮约 100px ≈ 收掉三分之一）。 */
export const WHEEL_PIXELS_PER_COLLAPSE_PIXEL = 3;

/** 行模式滚轮（deltaMode === 1）每行折算的像素。 */
export const WHEEL_LINE_HEIGHT = 16;

/** 折叠总行程：走完这么多滚轮像素，页头从全展开走到全收起。 */
export function collapseTravel(natural: number): number {
  if (!Number.isFinite(natural) || natural <= 0) return 0;
  return natural * WHEEL_PIXELS_PER_COLLAPSE_PIXEL;
}

/** 把 WheelEvent 的位移折算成像素（deltaMode：0 像素 / 1 行 / 2 页）。 */
export function normalizeWheelDelta(
  deltaY: number,
  deltaMode: number,
  pageHeight: number,
): number {
  if (!Number.isFinite(deltaY) || deltaY === 0) return 0;
  if (deltaMode === 1) return deltaY * WHEEL_LINE_HEIGHT;
  if (deltaMode === 2) {
    return Number.isFinite(pageHeight) && pageHeight > 0
      ? deltaY * pageHeight
      : 0;
  }
  return deltaY;
}

/** 在现有偏移上叠加滚轮位移，夹在 0..折叠总行程之间。 */
export function resolveCollapseOffset(
  offset: number,
  wheelPixels: number,
  natural: number,
): number {
  const travel = collapseTravel(natural);
  if (travel <= 0) return 0;
  const current = Number.isFinite(offset) ? offset : 0;
  const step = Number.isFinite(wheelPixels) ? wheelPixels : 0;
  return Math.min(travel, Math.max(0, current + step));
}

/** 折叠进度 0..1，用来同步页头底部的留白。 */
export function collapseProgress(offset: number, natural: number): number {
  const travel = collapseTravel(natural);
  if (travel <= 0) return 0;
  const current = Math.min(
    travel,
    Math.max(0, Number.isFinite(offset) ? offset : 0),
  );
  return current / travel;
}

/** 偏移对应的页头高度：0 是完整高度，走到总行程则收到 0。 */
export function collapseHeight(offset: number, natural: number): number {
  if (!Number.isFinite(natural) || natural <= 0) return 0;
  return natural * (1 - collapseProgress(offset, natural));
}

/** 是否已完全收起（再往下滚也不会更矮）。 */
export function isFullyCollapsed(offset: number, natural: number): boolean {
  return collapseTravel(natural) > 0 && collapseProgress(offset, natural) >= 1;
}
