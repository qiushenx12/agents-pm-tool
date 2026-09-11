import { expect, it } from "vitest";
import {
  WHEEL_LINE_HEIGHT,
  WHEEL_PIXELS_PER_COLLAPSE_PIXEL,
  collapseHeight,
  collapseProgress,
  collapseTravel,
  isFullyCollapsed,
  normalizeWheelDelta,
  resolveCollapseOffset,
} from "@/grid-app/headerCollapse";

const NATURAL = 120;
const TRAVEL = NATURAL * WHEEL_PIXELS_PER_COLLAPSE_PIXEL;
/** 一格鼠标滚轮的位移（Chrome/Windows 约 100px）。 */
const NOTCH = 100;

it("折叠行程与页头自然高度成正比", () => {
  expect(collapseTravel(NATURAL)).toBe(TRAVEL);
  expect(collapseTravel(0)).toBe(0);
  expect(collapseTravel(-10)).toBe(0);
  expect(collapseTravel(Number.NaN)).toBe(0);
});

it("滚轮位移按 deltaMode 折算成像素", () => {
  expect(normalizeWheelDelta(100, 0, 800)).toBe(100);
  expect(normalizeWheelDelta(3, 1, 800)).toBe(3 * WHEEL_LINE_HEIGHT);
  expect(normalizeWheelDelta(1, 2, 800)).toBe(800);
  // 页模式拿不到视口高度时不能瞎猜。
  expect(normalizeWheelDelta(1, 2, 0)).toBe(0);
  expect(normalizeWheelDelta(0, 0, 800)).toBe(0);
  expect(normalizeWheelDelta(Number.NaN, 0, 800)).toBe(0);
});

it("偏移逐格累加，夹在 0..总行程之间", () => {
  let offset = 0;
  offset = resolveCollapseOffset(offset, NOTCH, NATURAL);
  expect(offset).toBe(NOTCH);
  offset = resolveCollapseOffset(offset, NOTCH, NATURAL);
  expect(offset).toBe(2 * NOTCH);
  // 收到底后继续往下滚不会越过总行程。
  expect(resolveCollapseOffset(TRAVEL, NOTCH, NATURAL)).toBe(TRAVEL);
  // 全展开后继续往上滚不会变成负数。
  expect(resolveCollapseOffset(0, -NOTCH, NATURAL)).toBe(0);
  expect(resolveCollapseOffset(Number.NaN, Number.NaN, NATURAL)).toBe(0);
});

it("高度随进度线性下降，一格只收掉一部分", () => {
  expect(collapseHeight(0, NATURAL)).toBe(NATURAL);
  expect(collapseHeight(TRAVEL, NATURAL)).toBe(0);
  expect(collapseHeight(TRAVEL / 2, NATURAL)).toBe(NATURAL / 2);
  // 关键：一格远不足以收完，这就是"逐步收起"。
  const afterOneNotch = collapseHeight(NOTCH, NATURAL);
  expect(afterOneNotch).toBeGreaterThan(0);
  expect(afterOneNotch).toBeLessThan(NATURAL);
  expect(collapseProgress(NOTCH, NATURAL)).toBeCloseTo(NOTCH / TRAVEL, 6);
});

it("收到底的判定与高度一致", () => {
  expect(isFullyCollapsed(0, NATURAL)).toBe(false);
  expect(isFullyCollapsed(TRAVEL - 1, NATURAL)).toBe(false);
  expect(isFullyCollapsed(TRAVEL, NATURAL)).toBe(true);
  expect(isFullyCollapsed(TRAVEL + 50, NATURAL)).toBe(true);
  // 未量到高度时不算收起，避免首帧闪一下。
  expect(isFullyCollapsed(0, 0)).toBe(false);
});

it("拿不到自然高度时一律回到展开态", () => {
  expect(collapseHeight(NOTCH, 0)).toBe(0);
  expect(collapseProgress(NOTCH, 0)).toBe(0);
  expect(resolveCollapseOffset(NOTCH, NOTCH, 0)).toBe(0);
});
