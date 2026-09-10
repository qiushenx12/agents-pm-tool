// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { createApp, nextTick } from "vue";
import AuthScreen from "@/grid-app/components/users/AuthScreen.vue";
import { api } from "@/grid-app/api/client";

vi.mock("@/grid-app/api/client", () => ({
  api: {
    hostLogin: vi.fn(),
    login: vi.fn(),
    register: vi.fn(),
  },
}));

let host: HTMLDivElement | undefined;

afterEach(() => {
  host?.remove();
  host = undefined;
  vi.clearAllMocks();
});

function mount() {
  host = document.createElement("div");
  document.body.append(host);
  const app = createApp(AuthScreen);
  app.mount(host);
  return app;
}

describe("authentication screen", () => {
  it("offers loopback host login and emits the authenticated user", async () => {
    const user = {
      id: "host",
      username: "主机",
      role: "super_admin" as const,
      created_at: "",
      disabled: false,
      is_host: true,
    };
    vi.mocked(api.hostLogin).mockResolvedValue({ user });
    const emitted: unknown[] = [];
    host = document.createElement("div");
    document.body.append(host);
    const wrapper = createApp(AuthScreen, {
      onAuthenticated: (value: unknown) => emitted.push(value),
    });
    wrapper.mount(host);
    host.querySelector<HTMLButtonElement>(".auth-host-button")!.click();
    await vi.waitFor(() => expect(api.hostLogin).toHaveBeenCalledOnce());
    await nextTick();
    expect(emitted).toEqual([user]);
    wrapper.unmount();
  });

  it("registers with the entered credentials", async () => {
    const user = {
      id: "u1", username: "alice", role: "user" as const, created_at: "", disabled: false, is_host: false,
    };
    vi.mocked(api.register).mockResolvedValue({ user });
    const app = mount();
    const tabs = host!.querySelectorAll<HTMLButtonElement>(".auth-tabs button");
    tabs[1].click();
    const inputs = host!.querySelectorAll<HTMLInputElement>(".auth-form input");
    inputs[0].value = "alice";
    inputs[0].dispatchEvent(new Event("input"));
    inputs[1].value = "password-123";
    inputs[1].dispatchEvent(new Event("input"));
    await nextTick();
    host!.querySelector<HTMLFormElement>(".auth-form")!.dispatchEvent(new Event("submit"));
    await vi.waitFor(() =>
      expect(api.register).toHaveBeenCalledWith({ username: "alice", password: "password-123" }),
    );
    app.unmount();
  });
});
