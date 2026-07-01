---
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# NeuroGrim dashboard frontend

React + TypeScript + Vite + Tailwind + shadcn/ui. Embedded into the
`neurogrim` binary at compile time via RustEmbed (see
`../src/routes.rs`).

## Build

```bash
npm install
npm run build      # produces frontend/dist/
cd ../../.. && cargo build --bin neurogrim    # embeds dist/ into binary
```

The frontend AND the binary must both be rebuilt after a UI
change — RustEmbed snapshots `frontend/dist/` at the binary's
compile time. `npm run build` alone won't surface UI changes
in the running dashboard.

## Unit tests (Vitest)

```bash
npm test           # one-shot
npm run test:watch # watch mode
```

Unit tests cover component logic; the DOM is jsdom (faked).

## E2E tests (Playwright — v4.0 S12-G-5)

End-to-end tests run a real headless Chromium against a real
`neurogrim ui` instance. The `webServer` block in
`playwright.config.ts` spawns the Rust binary on port 17345 (avoiding
the 8420 production port) for the duration of the test run.

### One-time setup

```bash
npm install
# Recommended on Windows when C: is under disk pressure: redirect
# the Chromium browser cache to D: (or any non-C: drive). Default
# location is %USERPROFILE%\AppData\Local\ms-playwright (Windows)
# or ~/.cache/ms-playwright (Unix).
PLAYWRIGHT_BROWSERS_PATH=/d/playwright-browsers npx playwright install chromium
```

The `chromium` install is ~150 MB; full Playwright + supporting
deps land another ~50 MB in `node_modules/`.

### Running

```bash
# Build the binary first — RustEmbed snapshots frontend/dist/ at
# compile time, so a stale binary serves a stale UI.
cd ../../.. && npm --prefix crates/neurogrim-dashboard/frontend run build
cd ../../.. && cargo build --bin neurogrim

# Then run the full suite:
cd crates/neurogrim-dashboard/frontend
PLAYWRIGHT_BROWSERS_PATH=/d/playwright-browsers npm run e2e

# Or via the wrapper (does not rebuild for you):
neurogrim test --e2e
```

Total wall-clock ceiling is enforced at 180s via
`globalTimeout` — the S12 epic invariant of "operators won't run
them if they're slower than 3 minutes." If the suite hits the
ceiling, either the dashboard is hung or specs grew too ambitious.

The HTML report from the last run lands at
`frontend/playwright-report/`. Open with `npm run e2e:report`.

### Specs (v1)

Three smoke specs in `frontend/e2e/` mirror the S12-G-5 manifest:

- `overview-loads.spec.ts` — index → /brains/$id/ redirect resolves;
  AppShell renders with at least the federation nav link; no console
  errors.
- `federation-page.spec.ts` — federation route renders the topology
  marker without errors. This is the canary for the React #310
  crash class that v3.5 polish surfaced.
- `layout-edit.spec.ts` — Customize button toggles edit mode (the
  `data-testid="edit-mode-on"` toolbar appears); does NOT save
  (covered by the unit suite + manual checklist at S12-G-6).

### Adopting in a publish-gate manifest

Once Playwright is installed, declare an e2e gate in
`<brain>/.claude/brain/publish-gates.yaml`:

```yaml
- id: e2e-smoke
  gate_type: e2e
  description: Playwright smoke specs
  blocking: true
  timeout_seconds: 240
```

`neurogrim publish-gate run` will spawn Playwright the same way
`neurogrim test --e2e` does, capture stdout/stderr, and append a
ledger entry with the outcome.

## Reports & artifacts (gitignored)

- `playwright-report/` — last run's HTML report
- `test-results/` — per-test trace + screenshots (only on failure)

Both are listed in the repo root `.gitignore`.
