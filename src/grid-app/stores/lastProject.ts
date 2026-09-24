/**
 * 新建任务时默认项目的记忆。
 *
 * 与「项目筛选」是两件事：筛选只决定当前列表显示什么，不应该改写用户上次新建时
 * 选中的项目。所以这里只由**用户显式选择项目的动作**写入——弹窗里的项目下拉、
 * 快速新建。之前靠在弹窗里 watch 项目值回写，导致打开弹窗（默认跟随单一项目筛选）
 * 就会把筛选值当成用户选择覆盖掉记忆：取消筛选后新建又跳回旧项目，再在弹窗里
 * 选到跟筛选相同的项目也不产生变更、写不进去，记忆永远纠正不回来。
 */
const LAST_PROJECT_KEY = "pm-create-task-project-v1";
export function readLastCreateProject(): string {
  try {
    return localStorage.getItem(LAST_PROJECT_KEY) ?? "";
  } catch {
    return "";
  }
}
export function rememberCreateProject(name: string) {
  if (!name) return;
  try {
    localStorage.setItem(LAST_PROJECT_KEY, name);
  } catch {
    /* browsing without storage still works */
  }
}
