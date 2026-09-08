import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "path";

const host = process.env.TAURI_DEV_HOST;
// Keep dependency prebundling aligned with the supported WebView/browser baseline.
const browserTarget =
  process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari16";

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  optimizeDeps: { esbuildOptions: { target: browserTarget } },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  server: {
    port: 5173,
    strictPort: false,
    host: host || "127.0.0.1",
    hmr: host ? { protocol: "ws", host, port: 5174 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
    // 开发模式下把 /api 代理到 axum 服务（默认 17890）
    proxy: {
      "/api": {
        target: process.env.PM_API_TARGET || "http://127.0.0.1:17890",
        changeOrigin: true,
      },
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: browserTarget,
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rollupOptions: {
      input: {
        index: path.resolve(__dirname, "index.html"),
        config: path.resolve(__dirname, "config.html"),
      },
      output: {
        manualChunks: {
          vue: ["vue", "pinia"],
          tauri: ["@tauri-apps/api", "@tauri-apps/plugin-opener"],
        },
      },
    },
  },
});
