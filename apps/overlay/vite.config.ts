import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri uses a fixed dev port via TAURI_DEV_HOST/TAURI_DEV_PORT env vars when
// available; default to 5173 for plain `npm run dev`.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 5174 }
      : undefined,
    watch: {
      // Don't watch the Rust source.
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "es2022",
    rollupOptions: {
      input: {
        main: "index.html",
        settings: "settings.html",
      },
    },
  },
});
