let nextId = 0;
const layers: number[] = [];
export function enterLayer() {
  const id = ++nextId;
  layers.push(id);
  return id;
}
export function leaveLayer(id: number) {
  const index = layers.indexOf(id);
  if (index >= 0) layers.splice(index, 1);
}
export function isTopLayer(id: number) {
  return layers[layers.length - 1] === id;
}
export const focusableSelector =
  'button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), a[href], [tabindex="0"]';
export function focusables(el?: HTMLElement | null) {
  return Array.from(
    el?.querySelectorAll<HTMLElement>(focusableSelector) ?? [],
  ).filter((e) => e.getClientRects().length > 0);
}
