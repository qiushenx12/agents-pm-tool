// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  applyTheme,
  currentTheme,
  reconcileTheme,
  savedLocalTheme,
  type Theme,
  type ThemeTransport,
} from "@/shared/theme";

afterEach(() => {
  document.documentElement.removeAttribute("data-theme");
  localStorage.clear();
  vi.restoreAllMocks();
});

function transport(remote: Theme | null): ThemeTransport & { push: ReturnType<typeof vi.fn> } {
  return {
    fetch: vi.fn(async () => remote),
    push: vi.fn(async (_t: Theme) => {}),
  };
}

describe("reconcileTheme", () => {
  it("服务端有值时应用它并覆盖本机偏好", async () => {
    localStorage.setItem("pm-theme", "light");
    const t = transport("dark");

    const result = await reconcileTheme(t);

    expect(result).toBe("dark");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(localStorage.getItem("pm-theme")).toBe("dark");
    expect(t.push).not.toHaveBeenCalled();
  });

  it("服务端没设置且本机存过时，把本机偏好迁移为全局", async () => {
    localStorage.setItem("pm-theme", "dark");
    const t = transport(null);

    const result = await reconcileTheme(t);

    expect(result).toBe("dark");
    expect(t.push).toHaveBeenCalledWith("dark");
  });

  it("服务端没设置、本机也没存过时维持跟随系统，不上报", async () => {
    const t = transport(null);

    const result = await reconcileTheme(t);

    expect(result).toBe(null);
    expect(t.push).not.toHaveBeenCalled();
    expect(document.documentElement.getAttribute("data-theme")).toBe(null);
  });

  it("迁移上报失败（如未登录）时静默，返回本机值", async () => {
    localStorage.setItem("pm-theme", "light");
    const t = transport(null);
    t.push.mockRejectedValue(new Error("401"));

    const result = await reconcileTheme(t);

    expect(result).toBe("light");
  });

  it("服务不可达时维持现状，不改本机状态", async () => {
    localStorage.setItem("pm-theme", "light");
    const t: ThemeTransport = {
      fetch: vi.fn(async () => {
        throw new Error("网络错误");
      }),
      push: vi.fn(async () => {}),
    };

    const result = await reconcileTheme(t);

    expect(result).toBe(null);
    expect(localStorage.getItem("pm-theme")).toBe("light");
    expect(t.push).not.toHaveBeenCalled();
  });
});

describe("savedLocalTheme / currentTheme 现有行为", () => {
  beforeEach(() => {
    // jsdom 没有 matchMedia，用例里若命中系统分支要补 mock
  });

  it("savedLocalTheme 只认合法值", () => {
    expect(savedLocalTheme()).toBe(null);
    localStorage.setItem("pm-theme", "blue");
    expect(savedLocalTheme()).toBe(null);
    localStorage.setItem("pm-theme", "dark");
    expect(savedLocalTheme()).toBe("dark");
  });

  it("applyTheme 写 DOM 属性与本地缓存", () => {
    applyTheme("dark");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(currentTheme()).toBe("dark");
    applyTheme("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe(null);
    expect(currentTheme()).toBe("light");
  });
});
