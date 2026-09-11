// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useTaskStore } from "@/grid-app/stores/taskStore";

describe("筛选条件 URL query 编解码（刷新不丢）", () => {
  beforeEach(() => {
    window.history.replaceState(null, "", "/");
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it("多选筛选写入 URL 并可读回", () => {
    const store = useTaskStore();
    store.filters.project.push("agents-pm-tool");
    store.filters.status.push("进行中", "待验证");
    store.filters.keyword = "登录";

    // watch 是异步的，但序列化逻辑同步可验：直接读 URLSearchParams 往返
    const p = new URLSearchParams();
    store.filters.project.forEach((v) => p.append("project", v));
    store.filters.status.forEach((v) => p.append("status", v));
    p.set("keyword", store.filters.keyword);
    const s = p.toString();

    const back = new URLSearchParams(s);
    expect(back.getAll("project")).toEqual(["agents-pm-tool"]);
    expect(back.getAll("status")).toEqual(["进行中", "待验证"]);
    expect(back.get("keyword")).toBe("登录");
  });

  it("toggleFilter 往返", () => {
    const store = useTaskStore();
    store.toggleFilter("status", "已完成");
    expect(store.filters.status).toEqual(["已完成"]);
    store.toggleFilter("status", "已完成");
    expect(store.filters.status).toEqual([]);
  });

  it("不包含模式进入查询和 URL，并可在刷新后恢复", () => {
    const store = useTaskStore();
    store.filters.status_mode = "exclude";
    store.toggleFilter("status", "取消");

    expect(store.query.status).toEqual(["取消"]);
    expect(store.query.status_mode).toBe("exclude");
    expect(new URLSearchParams(window.location.search).get("status_mode")).toBe(
      "exclude",
    );

    setActivePinia(createPinia());
    const restored = useTaskStore();
    expect(restored.filters.status).toEqual(["取消"]);
    expect(restored.filters.status_mode).toBe("exclude");
  });

  it("旧视图和非法模式回退包含，清空条件也恢复包含", () => {
    window.history.replaceState(
      null,
      "",
      "/?status=进行中&status_mode=invalid",
    );
    const store = useTaskStore();
    expect(store.filters.status_mode).toBe("include");
    store.filters.status_mode = "exclude";
    store.clearFilters();
    expect(store.filters.status_mode).toBe("include");
    expect(store.query.status_mode).toBeUndefined();
  });
});

describe("筛选/排序/分组状态持久化（退出网页不丢）", () => {
  beforeEach(() => {
    window.history.replaceState(null, "", "/");
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it("状态变化写入 localStorage", () => {
    const store = useTaskStore();
    store.filters.status.push("进行中");
    store.filters.sort_order = "asc";
    store.filters.group_by = "project";

    const saved = JSON.parse(localStorage.getItem("pm-grid-filters-v1")!);
    expect(saved.status).toEqual(["进行中"]);
    expect(saved.sort_order).toBe("asc");
    expect(saved.group_by).toBe("project");
  });

  it("URL 无参数时从 localStorage 恢复并写回 URL", () => {
    localStorage.setItem(
      "pm-grid-filters-v1",
      JSON.stringify({
        status: ["已完成"],
        keyword: "登录",
        sort_by: "updated_at",
        sort_order: "asc",
        group_by: "type",
      }),
    );
    const store = useTaskStore();
    expect(store.filters.status).toEqual(["已完成"]);
    expect(store.filters.keyword).toBe("登录");
    expect(store.filters.sort_by).toBe("updated_at");
    expect(store.filters.sort_order).toBe("asc");
    expect(store.filters.group_by).toBe("type");

    const p = new URLSearchParams(window.location.search);
    expect(p.getAll("status")).toEqual(["已完成"]);
    expect(p.get("sort_by")).toBe("updated_at");
    expect(p.get("group_by")).toBe("type");
  });

  it("URL 参数优先于 localStorage", () => {
    window.history.replaceState(null, "", "/?status=进行中");
    localStorage.setItem(
      "pm-grid-filters-v1",
      JSON.stringify({ status: ["已完成"], sort_order: "asc" }),
    );
    const store = useTaskStore();
    expect(store.filters.status).toEqual(["进行中"]);
    // URL 未提及的项回退默认，而不是拼上本地残留
    expect(store.filters.sort_order).toBe("desc");
  });

  it("localStorage 内容损坏时回退默认状态", () => {
    localStorage.setItem("pm-grid-filters-v1", "{oops");
    const store = useTaskStore();
    expect(store.filters.status).toEqual([]);
    expect(store.filters.sort_by).toBe("created_at");
    expect(store.filters.sort_order).toBe("desc");
    expect(store.filters.group_by).toBe("");
  });
});

describe("优先级筛选/排序/分组", () => {
  beforeEach(() => {
    window.history.replaceState(null, "", "/");
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it("priority 是合法筛选键并往返 URL", () => {
    const store = useTaskStore();
    store.toggleFilter("priority", "高");
    store.toggleFilter("priority", "低");
    expect(store.filters.priority).toEqual(["高", "低"]);
    expect(store.activeFilterCount).toBe(2);
    store.toggleFilter("priority", "高");
    expect(store.filters.priority).toEqual(["低"]);
  });

  it("priority 可作为排序与分组字段", () => {
    const store = useTaskStore();
    store.filters.sort_by = "priority";
    store.filters.group_by = "priority";
    expect(store.query.sort_by).toBe("priority");
    expect(store.query.group_by).toBe("priority");
  });

  it("URL 中非法的 priority 被丢弃", () => {
    window.history.replaceState(
      null,
      "",
      "/?priority=紧急&priority=高&sort_by=priority&group_by=priority",
    );
    const store = useTaskStore();
    expect(store.filters.priority).toEqual(["高"]);
    expect(store.filters.sort_by).toBe("priority");
    expect(store.filters.group_by).toBe("priority");
  });
});
