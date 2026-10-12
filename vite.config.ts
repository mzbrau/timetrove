import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // Pre-bundle so cold start does not discover dpi/menu mid-load and force a
  // full-page reload that can leave a Tauri webview blank (black main window).
  optimizeDeps: {
    include: [
      "react",
      "react-dom",
      "react/jsx-runtime",
      "react/jsx-dev-runtime",
      "@tauri-apps/api/app",
      "@tauri-apps/api/core",
      "@tauri-apps/api/dpi",
      "@tauri-apps/api/event",
      "@tauri-apps/api/menu",
      "@tauri-apps/api/webviewWindow",
    ],
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
