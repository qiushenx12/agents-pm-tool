import type { Attachment } from "@/shared/types";

export type AttachmentKind = "image" | "video" | "file";

export function attachmentKind(attachment: Attachment): AttachmentKind {
  return /^image\//.test(attachment.mime ?? "") ||
    /\.(png|jpe?g|gif|webp)$/i.test(attachment.filename)
    ? "image"
    : /^video\//.test(attachment.mime ?? "") ||
        /\.(mp4|mov)$/i.test(attachment.filename)
      ? "video"
      : "file";
}
