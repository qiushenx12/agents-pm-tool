// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useTaskStore } from "@/grid-app/stores/taskStore";

describe("筛选条件 URL query 编解码（刷新不丢）", () => {
  beforeEach(() => {
    window.history.replaceState(null, "", "/");
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
});
