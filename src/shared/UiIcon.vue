<script setup lang="ts">
withDefaults(defineProps<{ name: string; size?: number }>(), { size: 16 });
const paths: Record<string, string> = {
  grid: "M3 3h18v18H3z M3 9h18 M3 15h18 M9 3v18",
  layers: "m12 3 10 5-10 5L2 8z M2 12l10 5 10-5 M2 16l10 5 10-5",
  chevron: "m8 10 4 4 4-4",
  right: "m9 5 7 7-7 7",
  left: "m15 5-7 7 7 7",
  plus: "M12 5v14 M5 12h14",
  close: "m6 6 12 12 M18 6 6 18",
  search: "M21 21l-5-5 M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0",
  filter: "M3 5h18 M6 12h12 M9 19h6",
  sort: "M8 4v16 m-4-4 4 4 4-4 M14 5h7 M14 10h5 M14 15h3",
  columns: "M3 4h18v16H3z M9 4v16 M15 4v16",
  density: "M4 5h16 M4 12h16 M4 19h16",
  grip: "M9 5h.01 M15 5h.01 M9 12h.01 M15 12h.01 M9 19h.01 M15 19h.01",
  more: "M5 12h.01 M12 12h.01 M19 12h.01",
  check: "m5 12 4 4L19 6",
  circle: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0",
  clock: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0 M12 7v5l3 2",
  review: "M8 3h8v4H8z M8 5H5v16h14V5h-3 M8 13l3 3 5-5",
  folder: "M3 5h6l2 3h10v12H3z",
  git: "M6 3v12 M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M18 9a9 9 0 0 1-9 9",
  settings:
    "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8 M12 2v3 M12 19v3 M2 12h3 M19 12h3 M5 5l2 2 M17 17l2 2 M5 19l2-2 M17 7l2-2",
  moon: "M20 15A9 9 0 0 1 9 4a9 9 0 1 0 11 11",
  sun: "M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0 M12 2v2 M12 20v2 M2 12h2 M20 12h2 M5 5l1 1 M18 18l1 1 M5 19l1-1 M18 6l1-1",
  sidebar: "M3 4h18v16H3z M9 4v16",
  text: "M4 6h16 M4 12h16 M4 18h10",
  tag: "M3 3h8l10 10-8 8L3 11z M7 7h.01",
  user: "M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0 M4 21v-2a8 8 0 0 1 16 0v2",
  bot: "M5 7h14v13H5z M12 3v4 M2 11v5 M22 11v5 M9 12h.01 M15 12h.01 M9 16h6",
  attachment: "m8 13 7-7a3 3 0 0 1 4 4L9 20a5 5 0 0 1-7-7L13 2 M7 14l9-9",
  expand: "M14 3h7v7 M21 3l-9 9 M10 21H3v-7 M3 21l9-9",
  trash: "M3 6h18 M9 6V3h6v3 M5 6l1 15h12l1-15 M10 10v7 M14 10v7",
  copy: "M8 8h13v13H8z M16 8V3H3v13h5",
  upload: "M12 16V3 m-5 5 5-5 5 5 M4 15v6h16v-6",
  download: "M12 3v13 m-5-5 5 5 5-5 M4 17v4h16v-4",
  refresh: "M20 7v5h-5 M4 17v-5h5 M6 6a8 8 0 0 1 14 6 M18 18a8 8 0 0 1-14-6",
  info: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0 M12 11v6 M12 7h.01",
  edit: "m4 16-1 5 5-1L21 7l-4-4z M14 6l4 4",
  up: "m6 14 6-6 6 6",
  down: "m6 10 6 6 6-6",
  minus: "M5 12h14",
  play: "m8 5 11 7-11 7z",
  stop: "M7 7h10v10H7z",
  window: "M4 4h16v16H4z",
  "app-window": "M3 4h18v16H3z M3 9h18 M6.5 6.5h.01 M9.5 6.5h.01",
  globe:
    "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0 M3 12h18 M12 3c2.5 2.7 3.9 5.7 3.9 9c0 3.3-1.4 6.3-3.9 9c-2.5-2.7-3.9-5.7-3.9-9c0-3.3 1.4-6.3 3.9-9z",
  file: "M4 2h10l6 6v14H4z M14 2v6h6",
  video: "M4 5h11v14H4z M15 10l5-3v10l-5-3z",
  flag: "M5 21V4 M5 4c4-2 8 2 13 0v9c-5 2-9-2-13 0",
  locate:
    "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z M12 2v3 M12 19v3 M2 12h3 M19 12h3",
};
</script>
<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.65"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    class="ui-icon"
  >
    <path :d="paths[name] || paths.grid" />
  </svg>
</template>
