/**
 * 每页条数的记忆。
 *
 * 分页状态本身归 taskStore，但「上次选的每页条数」是浏览器偏好：刷新或重开页面后
 * 应该沿用，否则用户选了 200 条/页、一刷新又回到 100。
 *
 * 单独用一个 localStorage key，不并进 `pm-table-view-v1`：那个 key 由 viewStore
 * 整体重写，外部塞进去的字段会在下一次列/密度变更时被抹掉。
 * 可选档位在这里唯一声明，tableStore 校验与表格底部的菜单都取它。
 */
const PAGE_SIZE_KEY = "pm-table-page-size-v1";

/** 允许的每页条数，顺序即菜单顺序。 */
export const PAGE_SIZES: readonly number[] = [50, 100, 200];
export const DEFAULT_PAGE_SIZE = 100;

/** 读回上次选择的每页条数；没有、不是合法档位或存储不可用时回落到默认值。 */
export function readPageSize(): number {
  try {
    const saved = Number(localStorage.getItem(PAGE_SIZE_KEY));
    return PAGE_SIZES.includes(saved) ? saved : DEFAULT_PAGE_SIZE;
  } catch {
    return DEFAULT_PAGE_SIZE;
  }
}

export function rememberPageSize(size: number) {
  if (!PAGE_SIZES.includes(size)) return;
  try {
    localStorage.setItem(PAGE_SIZE_KEY, String(size));
  } catch {
    /* browsing without storage still works */
  }
}
