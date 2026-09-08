import { ref, shallowRef } from "vue";
export const notices = ref<
  { id: number; text: string; kind: "success" | "error" | "info" }[]
>([]);
let noticeId = 0;
export function notify(
  text: string,
  kind: "success" | "error" | "info" = "success",
) {
  const id = ++noticeId;
  notices.value.push({ id, text, kind });
  setTimeout(() => dismissNotice(id), kind === "error" ? 8000 : 3500);
}
export function dismissNotice(id: number) {
  notices.value = notices.value.filter((n) => n.id !== id);
}
export const confirmation = shallowRef<{
  title: string;
  text: string;
  label: string;
  danger: boolean;
  resolve: (value: boolean) => void;
} | null>(null);
export function askConfirm(
  title: string,
  text: string,
  label = "确认",
  danger = false,
): Promise<boolean> {
  if (confirmation.value) return Promise.resolve(false);
  return new Promise((resolve) => {
    confirmation.value = { title, text, label, danger, resolve };
  });
}
export function settleConfirm(value: boolean) {
  const pending = confirmation.value;
  confirmation.value = null;
  pending?.resolve(value);
}
export function errorText(e: unknown) {
  return e instanceof Error ? e.message : String(e);
}
export async function copyText(value: string) {
  try {
    await navigator.clipboard.writeText(value);
    notify("已复制");
  } catch {
    notify("复制失败，请手动选择复制", "error");
  }
}
