import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { resolve } from "node:path";

const host = process.env.TAURI_DEV_HOST;

// Multi-page app: one HTML entry per window (settings, widget, stats, menu).
// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],

  // Prevent Vite from obscuring Rust errors and pin the dev server for Tauri.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },

  build: {
    target: "esnext",
    rollupOptions: {
      input: {
        index: resolve(__dirname, "index.html"),
        settings: resolve(__dirname, "settings.html"),
        widget: resolve(__dirname, "widget.html"),
        stats: resolve(__dirname, "stats.html"),
        news: resolve(__dirname, "news.html"),
        menu: resolve(__dirname, "menu.html"),
      },
    },
  },
});
