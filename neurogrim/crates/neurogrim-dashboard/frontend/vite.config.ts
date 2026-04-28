import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";

// During `npm run dev`, Vite serves the React app on port 5173 and
// proxies /api/* to the Rust dashboard server at port 8420 — so the
// dev experience uses a single origin. In production the embedded
// `dist/` is served by Rust at port 8420 directly; no proxy needed.
//
// `node:url` + `import.meta.url` is the pure-ESM alternative to
// `__dirname` (which only works under CommonJS). Avoids needing
// @types/node in our devDependencies.
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
      "@bindings": fileURLToPath(new URL("../bindings", import.meta.url)),
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
