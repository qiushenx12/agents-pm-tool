import { expect, it, vi } from "vitest";
import { openWorkspaceSettings } from "@/grid-app/settingsNavigation";

it("系统浏览器打开设置弹窗，不调用桌面窗口切换", async () => {
  const openDialog = vi.fn();
  const openDesktop = vi.fn();

  await openWorkspaceSettings(false, openDialog, openDesktop);

  expect(openDialog).toHaveBeenCalledOnce();
  expect(openDesktop).not.toHaveBeenCalled();
});

it("应用内切换到桌面设置窗口，不再打开网页弹窗", async () => {
  const openDialog = vi.fn();
  const openDesktop = vi.fn().mockResolvedValue(undefined);

  await openWorkspaceSettings(true, openDialog, openDesktop);

  expect(openDesktop).toHaveBeenCalledOnce();
  expect(openDialog).not.toHaveBeenCalled();
});
