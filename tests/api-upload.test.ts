// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { api } from "@/grid-app/api/client";
afterEach(() => vi.unstubAllGlobals());
it("lets the browser generate a multipart boundary for attachments", async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ id: "attachment" }),
    });
  vi.stubGlobal("fetch", fetchMock);
  const file = new File(["test"], "test.txt");
  await api.uploadAttachment("task", file);
  const init = fetchMock.mock.calls[0][1];
  expect(init.body).toBeInstanceOf(FormData);
  expect(init.headers?.["Content-Type"]).toBeUndefined();
  expect(init.body.get("file").name).toBe("test.txt");
});
