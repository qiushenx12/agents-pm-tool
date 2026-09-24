// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createPinia, disposePinia, setActivePinia, type Pinia } from "pinia";
import { api } from "@/grid-app/api/client";
import {
  DEFAULT_PAGE_SIZE,
  PAGE_SIZES,
  readPageSize,
  rememberPageSize,
} from "@/grid-app/stores/pageSize";
import { useTaskStore } from "@/grid-app/stores/taskStore";

vi.mock("@/grid-app/api/client", () => ({ api: { pageTasks: vi.fn() } }));

const KEY = "pm-table-page-size-v1";
let pinia: Pinia;

beforeEach(() => {
  localStorage.clear();
  pinia = createPinia();
  setActivePinia(pinia);
  // 服务端会按 1..=200 校验后原样回显 page_size，refresh() 会用这个回显覆盖本地 ref
  vi.mocked(api.pageTasks).mockImplementation(async (query) => ({
    items: [],
    total: 0,
    page: query?.page ?? 1,
    page_size: query?.page_size ?? DEFAULT_PAGE_SIZE,
    groups: [],
    anchor_found: null,
  }));
});

afterEach(() => {
  disposePinia(pinia);
  localStorage.clear();
});

it("没有存过时用默认档位", () => {
  expect(PAGE_SIZES).toEqual([50, 100, 200]);
  expect(readPageSize()).toBe(DEFAULT_PAGE_SIZE);
});

it("记住显式选择的档位", () => {
  rememberPageSize(200);
  expect(localStorage.getItem(KEY)).toBe("200");
  expect(readPageSize()).toBe(200);
});

it("非法档位既不写入也不覆盖已有选择", () => {
  rememberPageSize(30);
  expect(localStorage.getItem(KEY)).toBeNull();
  expect(readPageSize()).toBe(DEFAULT_PAGE_SIZE);

  rememberPageSize(50);
  rememberPageSize(0);
  rememberPageSize(Number.NaN);
  expect(readPageSize()).toBe(50);
});

it("存储里的值不合法时回落默认档位", () => {
  for (const broken of ["", "abc", "0", "-100", "{}"]) {
    localStorage.setItem(KEY, broken);
    expect(readPageSize(), broken).toBe(DEFAULT_PAGE_SIZE);
  }
});

it("新建 store 时沿用上次的档位", () => {
  rememberPageSize(200);
  expect(useTaskStore(pinia).pageSize).toBe(200);
});

it("切换档位会写盘并把页码拉回第一页", async () => {
  const tasks = useTaskStore(pinia);
  expect(tasks.pageSize).toBe(DEFAULT_PAGE_SIZE);

  await tasks.setPageSize(50);

  expect(tasks.pageSize).toBe(50);
  expect(tasks.page).toBe(1);
  expect(localStorage.getItem(KEY)).toBe("50");
  expect(api.pageTasks).toHaveBeenLastCalledWith(
    expect.objectContaining({ page: 1, page_size: 50 }),
    expect.anything(),
  );
});

it("菜单之外的档位被忽略，也不会写盘", async () => {
  const tasks = useTaskStore(pinia);
  await tasks.setPageSize(30);
  expect(tasks.pageSize).toBe(DEFAULT_PAGE_SIZE);
  expect(localStorage.getItem(KEY)).toBeNull();
});
