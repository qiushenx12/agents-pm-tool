// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createApp, nextTick, type App } from "vue";
import { createPinia, disposePinia } from "pinia";
import UserManagementDialog from "@/grid-app/components/users/UserManagementDialog.vue";
import { api } from "@/grid-app/api/client";

vi.mock("@/grid-app/api/client", () => ({
  api: {
    listUsers: vi.fn(),
    listProjects: vi.fn(),
    listSubmitterNames: vi.fn().mockResolvedValue([]),
    getUserPermissions: vi.fn(),
    putUserPermissions: vi.fn(),
    patchUser: vi.fn(),
    deleteUser: vi.fn(),
  },
}));

let root: HTMLDivElement | undefined;
let pinia: ReturnType<typeof createPinia> | undefined;
let app: App | undefined;

afterEach(() => {
  app?.unmount();
  if (pinia) disposePinia(pinia);
  root?.remove();
  document.body.querySelectorAll(".dialog-mask").forEach((element) => element.remove());
  vi.clearAllMocks();
});

it("edits an ordinary user's project access", async () => {
  const ordinary = {
    id: "u1", username: "alice", role: "user" as const, created_at: "2026-09-10", disabled: false, is_host: false,
  };
  const hostUser = {
    id: "host", username: "主机", role: "super_admin" as const, created_at: "", disabled: false, is_host: true,
  };
  vi.mocked(api.listUsers).mockResolvedValue([ordinary]);
  vi.mocked(api.listProjects).mockResolvedValue([
    { name: "demo", color: "#3370ff", sort_order: 0, local_path: "", git_url: "", created_at: "" },
  ]);
  vi.mocked(api.getUserPermissions).mockResolvedValue({ user: ordinary, permissions: [] });
  vi.mocked(api.putUserPermissions).mockImplementation(async (_, permissions) => ({
    user: ordinary, permissions,
  }));

  root = document.createElement("div");
  document.body.append(root);
  pinia = createPinia();
  app = createApp(UserManagementDialog, { currentUser: hostUser });
  app.use(pinia);
  app.mount(root);
  await vi.waitFor(() => expect(document.body.querySelector(".user-row")).not.toBeNull());
  document.body.querySelector<HTMLButtonElement>(".user-row")!.click();
  await vi.waitFor(() => expect(api.getUserPermissions).toHaveBeenCalledWith("u1"));
  await vi.waitFor(() =>
    expect(document.body.querySelector<HTMLButtonElement>(".permission-save")?.disabled).toBe(false),
  );
  const access = document.body.querySelector<HTMLInputElement>(".project-permission-title input")!;
  access.click();
  await nextTick();
  document.body.querySelector<HTMLButtonElement>(".permission-save")!.click();
  await vi.waitFor(() => expect(api.putUserPermissions).toHaveBeenCalled());
  expect(vi.mocked(api.putUserPermissions).mock.calls[0][1]).toContainEqual({
    project: "demo", field: "project_access", allowed_values: null,
  });
});
