/**
 * 主机“工作区设置”的运行环境分流：
 * 系统浏览器继续使用 Web 弹窗；Tauri 应用内切换回桌面设置窗口。
 */
export async function openWorkspaceSettings(
  desktop: boolean,
  openDialog: () => void,
  openDesktopSettings: () => Promise<unknown>,
) {
  if (desktop) {
    await openDesktopSettings();
  } else {
    openDialog();
  }
}
