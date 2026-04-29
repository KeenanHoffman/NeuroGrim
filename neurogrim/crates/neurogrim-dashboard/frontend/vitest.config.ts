/// <reference types="vitest" />
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";

/**
 * Vitest config — separate from `vite.config.ts` so the test runner
 * doesn't pull in the production proxy/build settings. Kept minimal:
 * jsdom for DOM, the same `@/` and `@bindings/` path aliases as the
 * dev/prod builds, and `setup.ts` for once-per-suite matchers.
 */
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
      "@bindings": fileURLToPath(new URL("../bindings", import.meta.url)),
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
  },
});
