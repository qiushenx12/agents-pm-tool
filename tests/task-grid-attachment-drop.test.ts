// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createApp, h, nextTick, type App } from "vue";
import { createPinia, disposePinia, type Pinia } from "pinia";
import TaskGrid from "@/grid-app/components/TaskGrid.vue";
import { api } from "@/grid-app/api/client";
import { useTaskStore } from "@/grid-app/stores/taskStore";
import type { Attachment, Task } from "@/shared/types";

vi.mock("@/grid-app/api/client", () => ({
  api: {
    pageTasks: vi.fn(),
    uploadAttachment: vi.fn(),
    listAttachments: vi.fn(),
    attachmentUrl: vi.fn((id: string) => `/api/web/attachments/${id}`),
  },
}));

let app: App, pinia: Pinia, host: HTMLElement;

beforeEach(() => vi.resetAllMocks());
afterEach(() => {
  app?.unmount();
  disposePinia(pinia);
  host?.remove();
  vi.restoreAllMocks();
});

function fileDragEvent(type: string, files: File[]) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, "dataTransfer", {
    value: { files, types: ["Files"], dropEffect: "none" },
  });
  return event;
}

it("uploads files dropped directly on a task attachment cell", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  const task: Task = {
    id: "task-attachment",
    seq: 1,
    project: "测试项目",
    type: "优化",
    status: "未开始",
    description: "直接拖入附件",
    note: "",
    submitter: "用户",
    created_at: "",
    finished_at: null,
    updated_at: "",
    position: 1,
    attachment_count: 0,
  };
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [task],
    total: 1,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  });
  vi.mocked(api.uploadAttachment).mockResolvedValue({
    id: "attachment-1",
    task_id: task.id,
    filename: "验收截图.png",
    stored_path: "stored.png",
    mime: "image/png",
    size: 3,
    created_at: "",
  } satisfies Attachment);
  vi.mocked(api.listAttachments).mockResolvedValue([]);

  const tasks = useTaskStore(pinia);
  await tasks.refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);

  const attachmentCell = host.querySelector<HTMLElement>(
    '[data-task-id="task-attachment"][data-column="attachments"]',
  )!;
  const file = new File(["png"], "验收截图.png", { type: "image/png" });
  const dragover = fileDragEvent("dragover", [file]);
  attachmentCell.dispatchEvent(dragover);
  await nextTick();
  expect(dragover.defaultPrevented).toBe(true);
  expect(attachmentCell.classList.contains("attachment-drop-target")).toBe(true);

  attachmentCell.dispatchEvent(fileDragEvent("drop", [file]));
  await vi.waitFor(() => {
    expect(api.uploadAttachment).toHaveBeenCalledWith(task.id, file);
    expect(tasks.records[task.id].attachment_count).toBe(1);
  });
  expect(attachmentCell.textContent).toContain("1");
  expect(attachmentCell.classList.contains("attachment-uploading")).toBe(false);
});

it("shows image thumbnails and icons for video and other attachments", async () => {
  window.history.replaceState(null, "", "/");
  localStorage.clear();
  pinia = createPinia();
  const task: Task = {
    id: "task-preview",
    seq: 2,
    project: "测试项目",
    type: "优化",
    status: "未开始",
    description: "附件预览",
    note: "",
    submitter: "用户",
    created_at: "",
    finished_at: null,
    updated_at: "",
    position: 2,
    attachment_count: 3,
  };
  const attachments: Attachment[] = [
    {
      id: "image-1",
      task_id: task.id,
      filename: "界面.png",
      stored_path: "image.png",
      mime: "image/png",
      size: 3,
      created_at: "",
    },
    {
      id: "video-1",
      task_id: task.id,
      filename: "操作.mp4",
      stored_path: "video.mp4",
      mime: "video/mp4",
      size: 3,
      created_at: "",
    },
    {
      id: "file-1",
      task_id: task.id,
      filename: "说明.pdf",
      stored_path: "file.pdf",
      mime: "application/pdf",
      size: 3,
      created_at: "",
    },
  ];
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [task],
    total: 1,
    page: 1,
    page_size: 100,
    groups: [],
    anchor_found: null,
  });
  vi.mocked(api.listAttachments).mockResolvedValue(attachments);

  const tasks = useTaskStore(pinia);
  await tasks.refresh();
  host = document.createElement("div");
  document.body.append(host);
  app = createApp({ render: () => h(TaskGrid) });
  app.use(pinia);
  app.mount(host);

  await vi.waitFor(() => {
    expect(api.listAttachments).toHaveBeenCalledWith(task.id);
    expect(host.querySelectorAll(".attachment-preview")).toHaveLength(3);
  });
  expect(
    host
      .querySelector<HTMLImageElement>(".attachment-preview-image img")
      ?.getAttribute("src"),
  ).toBe("/api/web/attachments/image-1");
  expect(
    host.querySelector(".attachment-preview-video .ui-icon"),
  ).not.toBeNull();
  expect(host.querySelector(".attachment-preview-file .ui-icon")).not.toBeNull();

  host.querySelector<HTMLButtonElement>(".attachment-preview-image")!.click();
  await nextTick();
  expect(
    document.body
      .querySelector<HTMLImageElement>(".media-preview img")
      ?.getAttribute("src"),
  ).toBe("/api/web/attachments/image-1");
  document.body.querySelector<HTMLButtonElement>('[aria-label="关闭"]')!.click();
  await nextTick();

  host.querySelector<HTMLButtonElement>(".attachment-preview-video")!.click();
  await nextTick();
  expect(
    document.body
      .querySelector<HTMLVideoElement>(".media-preview video")
      ?.getAttribute("src"),
  ).toBe("/api/web/attachments/video-1");
});
