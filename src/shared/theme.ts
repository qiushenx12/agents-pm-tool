// 双主题：light/dark，localStorage 持久化，默认跟随系统
const KEY = "pm-theme";

export type Theme = "light" | "dark";

export function currentTheme(): Theme {
  const saved = localStorage.getItem(KEY);
  if (saved === "light" || saved === "dark") return saved;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function applyTheme(t: Theme) {
  if (t === "dark") {
    document.documentElement.setAttribute("data-theme", "dark");
  } else {
    document.documentElement.removeAttribute("data-theme");
  }
  localStorage.setItem(KEY, t);
}

export function toggleTheme(): Theme {
  const next: Theme = currentTheme() === "dark" ? "light" : "dark";
  applyTheme(next);
  return next;
}

export function initTheme() {
  applyTheme(currentTheme());
}

/** 本机保存过的主题偏好；没存过（跟随系统）返回 null */
export function savedLocalTheme(): Theme | null {
  const saved = localStorage.getItem(KEY);
  return saved === "light" || saved === "dark" ? saved : null;
}

/** 全局主题的远端存取：网页端走 HTTP，设置窗口走 Tauri 命令 */
export interface ThemeTransport {
  /** 拉全局主题；null = 服务端从未设置过 */
  fetch(): Promise<Theme | null>;
  /** 上报全局主题 */
  push(t: Theme): Promise<void>;
}

/**
 * 与全局主题对账（设置界面与表格界面共用一份）：
 * - 服务端有值 → 应用它，各界面统一；
 * - 服务端没有 → 把本机已保存的偏好上报（老用户无感迁移），
 *   本机也没存过则维持跟随系统。
 * 网络失败静默维持现状，下次再试。
 */
export async function reconcileTheme(t: ThemeTransport): Promise<Theme | null> {
  let remote: Theme | null = null;
  try {
    remote = await t.fetch();
  } catch {
    return null;
  }
  if (remote === "light" || remote === "dark") {
    applyTheme(remote);
    return remote;
  }
  const local = savedLocalTheme();
  if (local) {
    try {
      await t.push(local);
    } catch {
      /* 未登录或网络问题：下次有机会再迁移 */
    }
    return local;
  }
  return null;
}
