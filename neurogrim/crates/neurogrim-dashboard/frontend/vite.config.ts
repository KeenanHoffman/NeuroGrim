import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// During `npm run dev`, Vite serves the React app on port 5173 and
// proxies /api/* to the Rust dashboard server at port 8420 — so the
// dev experience uses a single origin. In production the embedded
// `dist/` is served by Rust at port 8420 directly; no proxy needed.
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@bindings": path.resolve(__dirname, "../bindings"),
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8420",
        changeOrigin: false,
      },
    },
  },
  build: {
    outDir: "dist",
    sourcemap: false,
    chunkSizeWarningLimit: 1024,
  },
});
