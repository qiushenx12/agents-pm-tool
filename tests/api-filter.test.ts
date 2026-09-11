import { afterEach, expect, it, vi } from "vitest";
import { api } from "@/grid-app/api/client";

afterEach(() => vi.unstubAllGlobals());

it("sends the status exclusion mode to the page endpoint", async () => {
  const fetchMock = vi.fn().mockResolvedValue({
    ok: true,
    status: 200,
    json: async () => ({
      items: [],
      total: 0,
      page: 1,
      page_size: 100,
      groups: [],
      anchor_found: null,
    }),
  });
  vi.stubGlobal("fetch", fetchMock);

  await api.pageTasks({ status: ["取消"], status_mode: "exclude" });

  const [path] = fetchMock.mock.calls[0] as [string, RequestInit];
  const url = new URL(path, "http://localhost");
  expect(url.pathname).toBe("/api/web/tasks/page");
  expect(url.searchParams.getAll("status")).toEqual(["取消"]);
  expect(url.searchParams.get("status_mode")).toBe("exclude");
});
