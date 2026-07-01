---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: Publish Gates — Stage 12

**Stage:** 12
**Release:** v4.0 — "Ship Without Surprise"
**Status:** PLANNED (drafted 2026-04-29)
**Priority:** Foundation — must ship before S13–S15 because every later stage's publishes use this infrastructure
**Goal:** Replace today's "manual operator review + `methodology_drift` test only" pre-publish posture with a structured gate pipeline: fast automated checks, curated Playwright E2E smoke tests for key features, and a manual operator-validation checklist with explicit verification steps per declared feature. NeuroGrim itself becomes the first adopter.

**Depends on:**
- v3.5 (current shipped state)
- Existing pull-based skills `plan-critic`, `dual-review`, `review-loop` — codified into push-based gate definitions, not replaced

**Blocks:**
- S13 (S13's publishes go through the gates this epic establishes)
- S14, S15 transitively

**Master roadmap:** `roadmap/v4-roadmap.md`

---

## Stage 12 Is Done When

- [x] `cargo test --workspace --all-targets` runs in <90s baseline with the two `context_overhead.rs` benchmarks marked `#[ignore]` *(S12-G-1 — shipped in `6e7e6e1`; baseline 218s → 29s warm cache, 96s cold; snapshot at `roadmap/data/test-runtime-baseline.txt`)*
- [x] `neurogrim test` quiet wrapper (carry-over from v3.5.1 backlog) ships with `--keep-last`, `--show-only-new`, `--retry-failed` *(S12-G-2 — shipped in this commit; also `--slow` and `--verbose`)*
- [x] `<brain>/.claude/brain/publish-gates.yaml` schema authored + validated by `neurogrim doctor` *(S12-G-3 — shipped in this commit; schema at `crates/neurogrim-mcp/data/schemas/publish-gates-v1.schema.json`; validator + doctor check 8 in `crates/neurogrim-mcp/src/publish_gates.rs` + `doctor.rs::check_publish_gates`)*
- [x] `neurogrim publish-gate run` CLI ships with `--gate <id>`, `--mode {pre-commit,pre-publish,full}` *(S12-G-4 — shipped in this commit; also `publish-gate ack` sub-command for manual-gate operator acknowledgements; `--mode` filter is heuristic in v1, schema v2 will add explicit per-gate mode tags)*
- [x] Gate-result ledger at `<brain>/.claude/brain/publish-gate-ledger.jsonl` with append-only writer + read helpers *(S12-G-4 — `LedgerEntry` schema with run_id, gate_id, gate_type, mode, started_at, completed_at, status, blocking, operator, exit_code, stdout/stderr truncation; `append_ledger_entries` + `read_most_recent_pending` exported)*
- [x] Playwright foundation: `crates/neurogrim-dashboard/frontend/e2e/`, headless Chromium, total run time enforced <3 minutes *(S12-G-5 — `playwright.config.ts` shipped with `globalTimeout: 180_000`; chromium-only project; sequential workers; webServer block spawns the prebuilt `target/debug/neurogrim.exe` on port 17345)*
- [x] Three smoke specs ship green: `overview-loads.spec.ts`, `federation-page.spec.ts`, `layout-edit.spec.ts` *(S12-G-5 — all three pass in 9.6s wall-clock; targets are `data-testid` markers + accessible button names; `pageerror` + `console.error` listeners catch React #310-class crashes)*
- [x] Manual gate UI: `/brains/:id/publish-gates` page renders pending checklist + per-item verify URL/command *(S12-G-6 — read-only React page at `frontend/src/components/publish-gates/`; backed by `GET /api/brains/:brain_id/publish-gates` joining manifest + ledger; nav link in AppShell; ack still happens via the CLI `ack` sub-command but inline `--interactive` y/N prompt also added to `run` for TTY operators)*
- [x] NeuroGrim's own `publish-gates.yaml` authored; v4.0 itself publishes through the gate pipeline as the first dogfood pass *(S12-G-7 — `.claude/brain/publish-gates.yaml` with 7 gates: doctor-clean, tests-pass, cargo-publish-dryrun, changelog-dated, e2e-smoke, review-changelog, dashboard-renders-locally)*
- [x] 12th explain topic ships: `neurogrim explain publish-gates` *(S12-G-5 — `crates/neurogrim-mcp/data/explain/publish-gates.md`; covers gate types, manifest schema, runner CLI, ledger, mode filter, ack flow, e2e setup, adopter onboarding; methodology_drift `TOPICS` extended)*
- [x] Adopter walkthrough doc: how to set up gates in a fresh adopter Brain *(S12-G-5 — covered in the explain topic's "Adopter onboarding" section, plus the v4.0 publish-process doc covers adopter perspective at the bottom; v4.0 ships with permissive doctor stance — missing manifest = silent — so adopters can roll the pipeline in at their own pace)*
- [x] CHANGELOG documents that v4.0+ NeuroGrim publishes go through `publish-gate run` before tagging *(S12-G-7 — v4.0 [Unreleased] section in CHANGELOG.md declares the requirement under "Changed — v4.0+ NeuroGrim publishes go through `publish-gate run`")*

---

## Stories

### S12-G-1: Slow-benchmark surgery (1 day)

**What:** Mark `crates/neurogrim-cli/tests/context_overhead.rs` and `crates/neurogrim-cli/tests/phase_15_benchmark.rs` integration tests with `#[ignore]` and put them behind `#[cfg(feature = "benchmarks")]`. Add a `benchmarks` feature flag to `neurogrim-cli/Cargo.toml`.

**Why:** Today's 3m38s integration suite is dominated by these two benchmark tests. Marking `#[ignore]` drops the suite to ~45s, which makes "run full tests every publish" a viable default rather than a 4-minute interruption. Benchmarks still run via `cargo test --ignored` or `cargo test --features benchmarks`.

**Done when:**
- [ ] `cargo test --workspace --all-targets 2>&1 | tail -3` shows total <90s
- [ ] `cargo test --features benchmarks --ignored` runs the slow ones; documented in `neurogrim explain publish-gates`
- [ ] Snapshot file `roadmap/data/test-runtime-baseline.txt` records the new baseline

### S12-G-2: `neurogrim test` quiet wrapper (3 days) — ✅ SHIPPED

**What:** New CLI subcommand `neurogrim test` that wraps `cargo test --workspace --all-targets`, suppresses success spam, and appends failures to `<brain>/.claude/brain/test-failures.jsonl`. Reuses the JSONL append pattern from `disposition.rs:48`. Flags: `--keep-last N` (default 500), `--show-only-new`, `--retry-failed`, plus `--slow` (passes `--include-ignored`) and `--verbose` (bypasses the quiet wrapper for parser-debug).

**Why:** Carry-over from v3.5.1 plans. Required by S12-G-4 because the publish-gate runner consumes test results. Without quiet output, agents/operators drown in success noise.

**Done when:**
- [x] CLI subcommand registered in `crates/neurogrim-cli/src/main.rs`
- [x] `crates/neurogrim-cli/src/commands/test.rs` module created (~650 lines, schema documented in module docstring)
- [x] 5+ unit tests cover quiet output, append-mode, retry-failed flow *(actually 10: parser no-failures, parser one-failure-one-binary, parser ANSI-strip, parser stderr-appended-after-stdout ordering, append round-trip, recent-batch read, ledger rotate, rotate no-op, failure-detail-header, binary-id extraction)*
- [x] Documentation in `cli.md` explain topic *(Family 3 row added; `methodology_drift::no_topic_references_unknown_command` known-commands list extended with `"test"`)*

**Status:** Complete as a standalone CLI. Not yet integrated into a publish-gate — S12-G-4 wires it as the `tests-pass` automated gate.

### S12-G-3: Gate definition format (3 days) — ✅ SHIPPED

**What:** New file `<brain>/.claude/brain/publish-gates.yaml` declaring gate IDs, gate-type (`automated | manual | e2e`), description, and per-gate checks/instructions. Schema-versioned. Validated by `neurogrim doctor`.

**Why:** Operators need to know what gates exist for their project; agents need to read them programmatically. Putting them in a versioned YAML file lets us extend without breaking adopters.

**Schema example:**
```yaml
schema_version: "1"
gates:
  - id: tests-pass
    gate_type: automated
    description: All tests green via `neurogrim test`
    blocking: true
    timeout_seconds: 120

  - id: changelog-dated
    gate_type: automated
    description: CHANGELOG's [Unreleased] section converted to date stamp
    blocking: true
    check_command: "grep -E '\\[\\d+\\.\\d+\\.\\d+\\] - 20\\d\\d' CHANGELOG.md"

  - id: dashboard-loads-locally
    gate_type: manual
    description: Operator visits dashboard, verifies new feature renders
    instructions: |
      1. Run `neurogrim ui --allow-mutations`
      2. Navigate to <brain>/<feature-page>
      3. Verify <specific behavior>
    operator_required: true

  - id: e2e-smoke
    gate_type: e2e
    description: Playwright smoke covering Overview, Federation, Layout edit
    blocking: true
    timeout_seconds: 180
```

**Done when:**
- [x] Schema authored with JSON Schema sidecar at `crates/neurogrim-mcp/data/schemas/publish-gates-v1.schema.json` *(Draft-07; closed vocabulary at every level via `additionalProperties: false`; kebab-case `id` pattern; `if/then` rules for `manual → instructions` and `automated → check_command`; timeout bounded 1–3600s)*
- [x] `neurogrim doctor` reports findings for missing/malformed `publish-gates.yaml` *(new check 8: `check_publish_gates`; missing-file = silent advisory posture for v4.0 rollout; YAML syntax error / I/O error = `publish-gates-syntax` Error; schema-validation failure = `publish-gates-schema` Error per issue; duplicate gate IDs = single Error)*
- [x] 8+ unit tests covering parse, validate, edge cases *(actually 22: 17 in `publish_gates::tests` covering parser + schema-validation paths + load round-trip, plus 5 in `doctor::tests` covering missing/clean/malformed/schema-invalid/duplicate-ids)*

**Status:** Complete as a standalone validator + doctor check. Not yet a usable runtime gate — S12-G-4 (`neurogrim publish-gate run`) consumes the typed `PublishGatesConfig` view to execute gates. The schema's typed Rust mirror (`PublishGatesConfig`, `Gate`, `GateType`) is exported from `neurogrim_mcp::publish_gates` and ready for G-4 to import.

### S12-G-4: `neurogrim publish-gate run` CLI (5 days) — ✅ SHIPPED

**What:** New CLI subcommand that reads `publish-gates.yaml`, executes automated gates in declared order, emits per-gate findings to `publish-gate-ledger.jsonl`, surfaces manual gates as a checklist with copy-paste verification steps. Supports `--gate <id>` to run a single gate; `--mode {pre-commit,pre-publish,full}` selects which gates run; exit code reflects pass/fail/pending.

**Why:** This is the load-bearing CLI for S12. Every other story produces inputs to this one. Self-hosting target: NeuroGrim's own publishes go through it.

**Done when:**
- [x] CLI registered + exit code semantics documented (0=pass, 1=fail, 2=pending operator) *(precedence: failed > pending > passed; non-blocking gate failures recorded in ledger but never drive exit code)*
- [x] Ledger writer + reader; ledger entries include gate ID, mode, started_at, completed_at, status, operator (if manual), findings *(`append_ledger_entries` + `read_most_recent_pending`; entries include exit_code, stdout/stderr truncated to 4 KB head + 4 KB tail to keep typical lines under PIPE_BUF)*
- [x] 12+ tests cover happy path, automated failure, manual pending, missing gates, malformed YAML *(actually 28 unit tests: passing/failing/timing-out automated, missing check_command, manual-pending, e2e-deferred, aggregate exit-code precedence (5 cases), select_gates filters (4 modes + unknown-id error), ledger round-trip + read_most_recent_pending behavior (3 cases), truncate_stream (3 cases), ack flow (2 cases), resolve_operator (3 cases))*
- [x] Verbose mode (`-v`) shows command output per gate *(stdout + stderr first 20 lines per gate; trims to truncation envelope)*

**Status:** Complete as a standalone CLI with ack flow. e2e gate type ships as `deferred` until S12-G-5 wires the Playwright harness — adopters can declare e2e gates in their manifest today and they'll be visible in the ledger without driving exit code (deferred is non-blocking by design). Two extension sub-commands reserved for future stories: `publish-gate list` (read ledger) and `publish-gate inspect <gate-id>` (gate detail) — out of scope for v1.

### S12-G-5: Playwright E2E foundation (4 days) — ✅ SHIPPED

**What:** New directory `crates/neurogrim-dashboard/frontend/e2e/` with `playwright.config.ts`. Headless Chromium only (Webkit fallback documented). Total run-time constraint enforced (test files >30s fail the build via custom matcher). Three initial smoke specs: `overview-loads.spec.ts`, `federation-page.spec.ts`, `layout-edit.spec.ts`.

**Why:** v3.5 polish needed exactly these tests (the React #310 federation crash would have been caught by `federation-page.spec.ts`). E2E catches regressions that unit tests miss because they don't render real DOM. Constraint: must stay under 3 minutes total or operators won't run them.

**Done when:**
- [x] `playwright.config.ts` shipped with timeout enforcement *(`globalTimeout: 180_000`; per-test 30s; `expect.timeout: 10_000`; `retries: process.env.CI ? 1 : 0`; `workers: 1` to prevent races on the single webServer instance)*
- [x] 3 smoke specs green against a locally-running dashboard *(verified end-to-end against the v4.0-dev binary serving NeuroGrim's own brain registry; total wall-clock 9.6s of the 180s budget)*
- [x] `neurogrim test --e2e` invokes them *(test.rs Args.e2e flag; diverts entirely from cargo path; `find_dashboard_frontend` walks up looking for both workspace-root and repo-root layouts; `spawn_playwright_inherit` shells out to `npx playwright test` with inherited stdio so the operator sees real-time progress; mirrors playwright's exit code)*
- [x] Documentation in `publish-gates.md` explain topic + frontend README *(also satisfies the top-level "12th explain topic" done-when; methodology_drift TOPICS extended)*
- [ ] CI YAML scaffolding (`/.github/workflows/e2e.yml` if user opts in) — **deliberately skipped in v1**; user is single-adopter with no CI today, the epic explicitly says "if user opts in", and adding YAML adds maintenance for no value. Re-open this when a second adopter or a CI need materializes.

**Status:** Complete. Bonus wire-up: G-4's `execute_e2e_deferred` was replaced by `execute_e2e_playwright` — `e2e` gate types in `publish-gates.yaml` now actually run Playwright through the runner, capture stdout/stderr to the ledger (truncated to 4 KB head + 4 KB tail), and respect `timeout_seconds`. End-to-end smoke against a single-gate manifest passed cleanly with the e2e gate landing as `passed` in the ledger.

### S12-G-6: Manual gate UI surface (3 days) — ✅ SHIPPED

**What:** When `publish-gate run` encounters a manual gate, it prints a numbered checklist + per-item URL or CLI command. Each operator-checked item logs to ledger with `$NEUROGRIM_OPERATOR`. Read-only UI surface in dashboard: `/brains/:id/publish-gates` page.

**Why:** Manual gates can't be automated, but the operator's clicks need recording for audit. UI page makes "what's pending" visible at a glance.

**Done when:**
- [x] CLI prints checklist, accepts y/n input per item *(via `--interactive`; auto-detect via `IsTerminal` when neither `--interactive` nor `--no-interactive` is passed; inline 'y' marks the gate `passed` with operator handle and writes the ledger entry directly; 'n' / blank / unresolvable operator falls through to async pending)*
- [x] Operator handle from `$NEUROGRIM_OPERATOR` env var (matches existing convention) *(`--operator` flag on `run` overrides; falls back to env; missing handle interactively → falls through to pending with a clear warning instead of ack'ing under "unknown")*
- [x] `/brains/:id/publish-gates` page lists pending + completed gates from ledger *(read-only React page at `frontend/src/components/publish-gates/PublishGatesPage.tsx`; renders 4 branches: empty / malformed-manifest banner / gate table with status badges / recent-ledger timeline; backed by new `GET /api/brains/:brain_id/publish-gates` Rust endpoint that joins manifest + ledger; AppShell nav link with `GitMerge` icon)*
- [x] Dashboard test added covering page render + state transitions *(7 vitest cases covering empty state, malformed banner, gate-table render, all 7 status-badge variants, recent-ledger timeline, operator-handle display, fetch-error state; plus 1 Playwright spec — `publish-gates-page.spec.ts` — covering nav-link click + page render + no console errors)*

**Status:** Complete. The dashboard surface is read-only; ack happens via the CLI (`ack` sub-command or `--interactive` flag on `run`). A future story can add ack buttons to the page once the audit-trail discipline for dashboard-side mutations is settled (the same conversation that gated `--allow-mutations` in v3.5).

### S12-G-7: Self-hosting milestone (2 days) — ✅ SHIPPED

**What:** Author NeuroGrim's own `publish-gates.yaml`. First v4.x publish (v4.0 itself) goes through the pipeline manually. Update CHANGELOG to declare gate-required-from-v4.0 forward.

**Why:** No methodology, just dogfood. If our own publishes don't run through the gates, why would adopters trust them?

**Done when:**
- [x] `publish-gates.yaml` declared at NeuroGrim repo root with at least 5 gates *(7 gates at `.claude/brain/publish-gates.yaml` exercising all three gate types: 4 automated — `doctor-clean`, `tests-pass`, `cargo-publish-dryrun`, `changelog-dated`; 1 e2e — `e2e-smoke`; 2 manual — `review-changelog`, `dashboard-renders-locally`. Validates cleanly via `neurogrim doctor`.)*
- [x] v4.0 publish process documented as: develop → run gates → fix → re-run → publish *(`roadmap/v4.0-publish-process.md` — pre-flight, 9-step publish flow, per-gate failure-mode guide, adopter-perspective section)*
- [x] CHANGELOG declares the requirement *(v4.0 [Unreleased] section in CHANGELOG.md, "Changed — v4.0+ NeuroGrim publishes go through `publish-gate run`")*

**Status:** Self-hosting infrastructure complete. The actual v4.0 publish is a separate event (operator-driven, requires real `cargo publish` invocations); this story authored everything required for that event to happen, but does not itself publish v4.0. When the operator runs the pipeline for the v4.0 release, the `[Unreleased]` section in CHANGELOG flips to `[4.0.0] - YYYY-MM-DD` (the `changelog-dated` gate enforces this), the workspace version bumps, and the publish proceeds per the runbook.

---

## Risks (plan-critic concerns brought forward)

🟡 **Playwright on Windows can be flaky.** Headless Chromium fonts, antivirus interference, intermittent timeouts. Mitigation: pin Playwright version; document troubleshooting; provide `--skip-e2e` for emergency publishes (logged to ledger as caveat).

🟡 **Manual gates have a "did the operator actually verify?" trust problem.** A bored operator clicks ✓ on everything. Mitigation: ledger entries are timestamped; CHANGELOG references gate IDs; dual-review skill can re-verify on a sample.

🟡 **Adopter onboarding cost.** Adopter Brains need to author their own `publish-gates.yaml`. Mitigation: ship a template via `neurogrim init --template ...`; document common gate patterns in the explain topic.

🔵 **Suggestion: "gate-coverage" advisory domain (post-S12).** Reads `publish-gate-ledger.jsonl`; emits findings if any declared gate has not run in the last N publishes (likely candidate for v4.4 or backlog).

---

## Cross-references

- Master roadmap: `roadmap/v4-roadmap.md`
- Existing skills: `.claude/skills/plan-critic/`, `.claude/skills/dual-review/`, `.claude/skills/review-loop/`
- Existing sensor: `crates/neurogrim-sensory/src/deploy_readiness.rs`
- Existing test: `crates/neurogrim-cli/tests/methodology_drift.rs`
- Ledger pattern: `crates/neurogrim-cli/src/commands/disposition.rs:48`
