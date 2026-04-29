/**
 * Playwright configuration for the NeuroGrim dashboard E2E suite
 * (S12-G-5, v4.0). The suite is the third gate-type in the v4.0
 * publish pipeline: automated (cargo / shell) + manual (operator
 * checklist) + e2e (this).
 *
 * ## Discipline
 *
 * - **Total wall-clock ceiling 3 minutes** (S12 epic constraint —
 *   "or operators won't run them"). Enforced via `globalTimeout`
 *   below. Per-test timeout 30s; if any single spec needs more, it's
 *   too big and should be split.
 * - **Chromium only** in v1. Webkit/Firefox parity is documented as
 *   future work; the marginal regression coverage isn't worth the CI
 *   cost for a single-adopter product today.
 * - **Sequential workers** (`workers: 1`). The `webServer` spawns one
 *   neurogrim binary on a fixed port; parallel workers would race on
 *   that single instance.
 * - **Built-binary-only**: this config does NOT use the Vite dev
 *   server. The Rust binary embeds the React bundle via RustEmbed at
 *   build time, so the production-fidelity test is "spawn the
 *   embedded binary." Operators must run `npm run build` (frontend)
 *   then `cargo build --bin neurogrim` (binary) before invoking
 *   playwright. README documents the sequence.
 * - **Non-default port** (17345). Avoids clashing with the production
 *   dashboard at 8420 or the auto-allocated dynamic-range ports a
 *   running adopter Brain might use.
 *
 * ## Browser cache location
 *
 * Playwright defaults to `~/.cache/ms-playwright` (Linux/macOS) or
 * `%USERPROFILE%\AppData\Local\ms-playwright` (Windows). On Windows
 * with C: drive pressure, set `PLAYWRIGHT_BROWSERS_PATH` to a D:
 * location BEFORE running `playwright install chromium`. README has
 * a step-by-step.
 */
import { defineConfig, devices } from "@playwright/test";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const PORT = Number(process.env.NEUROGRIM_E2E_PORT ?? 17345);
const BASE_URL = `http://127.0.0.1:${PORT}`;

// Resolve to absolute paths at config-load time. Windows cmd parses
// forward-slash relative paths in the executable position as command
// names (e.g. `../foo` becomes "command '..' with args '/foo'"),
// which fails noisily. Absolute paths sidestep the cmd path-parsing
// entirely and work identically on Unix.
const WORKSPACE_ROOT = path.resolve(__dirname, "..", "..", "..");
const BINARY_PATH = path.join(
  WORKSPACE_ROOT,
  "target",
  "debug",
  process.platform === "win32" ? "neurogrim.exe" : "neurogrim",
);
// NeuroGrim's own brain registry — used for E2E because it has real
// federation peers, real CMDBs, real dashboard layout. Future
// stories (or G-7 self-hosting) can ship a fixture brain instead
// for full hermeticity; v1 trades that for "tests against real
// content that adopters actually see." NOTE: the brain-registry
// lives one level UP from the workspace root, at <repo-root>/.claude/.
const REGISTRY_PATH = path.resolve(WORKSPACE_ROOT, "..", ".claude", "brain-registry.json");

export default defineConfig({
  testDir: "./e2e",
  // Full suite must complete in 3 minutes — S12 epic invariant.
  // If this fires, either the dashboard is hung or specs grew too
  // ambitious; either way it's a real signal.
  globalTimeout: 180_000,
  // Per-test cap. 30s catches the common Playwright failure modes
  // (hung navigation, missing selector) without dragging out the
  // whole suite when one spec wedges.
  timeout: 30_000,
  // `expect` timeouts default to 5s, which is fine for typical
  // assertions but tight for "wait for the dashboard to fetch the
  // brain registry on first paint." Bump to 10s.
  expect: { timeout: 10_000 },
  // CI gets one retry to absorb genuine flakiness; locally we surface
  // failures immediately.
  retries: process.env.CI ? 1 : 0,
  // Single worker — webServer spawns one binary instance.
  workers: 1,
  // Surface the failing spec on the terminal AND emit an HTML report
  // for after-the-fact inspection. Both go under
  // `frontend/playwright-report/` (gitignored).
  reporter: [
    ["list"],
    ["html", { open: "never" }],
  ],
  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "off",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    // Spawn the prebuilt binary serving NeuroGrim's brain registry.
    // `--no-browser` prevents the auto-launched window. Loopback bind.
    // Mutations enabled because the layout-edit spec exercises the
    // edit toolbar.
    command: `"${BINARY_PATH}" ui --port ${PORT} --bind 127.0.0.1 --no-browser --allow-mutations --registry "${REGISTRY_PATH}"`,
    url: `${BASE_URL}/api/health`,
    timeout: 60_000,
    // Don't reuse — we want a clean instance each suite run so any
    // mutation a previous run left behind (e.g. layout-edit if it
    // ever started saving) doesn't leak into the next.
    reuseExistingServer: false,
    stdout: "pipe",
    stderr: "pipe",
  },
});
