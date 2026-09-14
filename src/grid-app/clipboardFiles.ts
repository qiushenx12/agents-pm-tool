export function clipboardFiles(event: ClipboardEvent): File[] {
  const transfer = event.clipboardData;
  if (!transfer) return [];
  const files = Array.from(transfer.files);
  if (files.length) return files;
  return Array.from(transfer.items)
    .filter((item) => item.kind === "file")
    .map((item) => item.getAsFile())
    .filter((file): file is File => file !== null);
}
