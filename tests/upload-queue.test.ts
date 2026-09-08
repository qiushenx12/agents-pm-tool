// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useUploadQueue } from "@/grid-app/components/useUploadQueue";
import { api } from "@/grid-app/api/client";
vi.mock("@/grid-app/api/client", () => ({
  api: { uploadAttachment: vi.fn() },
}));
beforeEach(() => vi.resetAllMocks());
describe("partial attachment retries", () => {
  it("retries failed files on the same task without uploading completed files again", async () => {
    const queue = useUploadQueue();
    const a = new File(["a"], "a.txt"),
      b = new File(["b"], "b.txt");
    queue.add([a, b]);
    vi.mocked(api.uploadAttachment)
      .mockResolvedValueOnce({} as never)
      .mockRejectedValueOnce(new Error("网络中断"))
      .mockResolvedValueOnce({} as never);
    expect(await queue.upload("created-task")).toBe(false);
    expect(queue.items.value.map((i) => i.state)).toEqual(["done", "error"]);
    expect(await queue.upload("created-task")).toBe(true);
    expect(api.uploadAttachment).toHaveBeenCalledTimes(3);
    expect(
      vi
        .mocked(api.uploadAttachment)
        .mock.calls.map((call) => [call[0], call[1].name]),
    ).toEqual([
      ["created-task", "a.txt"],
      ["created-task", "b.txt"],
      ["created-task", "b.txt"],
    ]);
  });
  it("deduplicates added files and prevents concurrent uploads", async () => {
    const queue = useUploadQueue(),
      file = new File(["a"], "a.txt", { lastModified: 1 });
    queue.add([file, file]);
    expect(queue.items.value).toHaveLength(1);
    let finish!: () => void;
    vi.mocked(api.uploadAttachment).mockReturnValue(
      new Promise((resolve) => {
        finish = () => resolve({} as never);
      }),
    );
    const first = queue.upload("1");
    expect(await queue.upload("1")).toBe(false);
    queue.remove(queue.items.value[0].id);
    expect(queue.items.value).toHaveLength(1);
    finish();
    expect(await first).toBe(true);
    expect(api.uploadAttachment).toHaveBeenCalledTimes(1);
  });
});
