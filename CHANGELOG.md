# Changelog

All notable changes to NeuroGrim + the LSP Brains specification live
here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### v5 "Everything is Lego" — staged for v5.0.0 tag (2026-05-04)

*v5 — `Everything is Lego` — finishes the interface-and-implementation pattern at four
high-leverage seams (`ScoringSource`, `Sensor`, `QueueBackend`, `TestRunner`), extracts
a thin SDK so users can build modules outside the core repo, and ships a diagnostics +
test-speed foundation that keeps the dev loop fast as adoption scales. See
[`roadmap/v5-roadmap.md`](roadmap/v5-roadmap.md) for the full theme map and
[`docs/v5-composition-guide.md`](docs/v5-composition-guide.md) for the modularity
recipes.*

**Theme status at v5 ship:**

- **Theme A — Foundation: Diagnostics + Test Speed** — 3/4 epics complete.
  V5-FOUND-1 ✅ (diagnostics ledger + `neurogrim diag report`); V5-FOUND-2 ✅
  (cargo-nextest adoption + per-test wall-time SLO at `docs/test-slo.md`);
  V5-FOUND-3 ⏸ DEFERRED to v5.1/v6 (Windows host coverage-toolchain gap —
  `stable-x86_64-pc-windows-gnu` lacks `profiler_builtins`; Phase 0 partial work
  shipped at commit `39d7295`); V5-FOUND-4 ✅ (TestRunner trait + 4-test
  conformance suite + NextestRunner impl).
- **Theme B — Three Modular Conversions** ✅ COMPLETE 2026-05-02
  (V5-MOD-1/2/3: `ScoringSource` / `Sensor` / `QueueBackend` traits + factories
  + registries + per-trait conformance suites).
- **Theme C — SDK Extraction** ✅ COMPLETE 2026-05-04 (V5-SDK-1 thin re-export
  crate at `crates/neurogrim-sdk/`; V5-SDK-2 conformance suites distributed via
  the `conformance` cargo feature). Trait surfaces always-on; conformance suites
  feature-gated to keep `tokio` out of production binaries that don't run them.
- **Theme D — Coherence + Docs** ✅ COMPLETE 2026-05-04
  (V5-DOC-1 — 756-line composition guide at `docs/v5-composition-guide.md` with
  4 working recipes lifted from in-tree example crates;
  V5-DOC-2 — VISION principle #20 added: *"Pluggability by use, not aspiration."*
  Wording finalized via dual-review T+P. LSP-Brains spec bumped v3.0 → v3.1 with
  new §9.8 trait-surface recommendation.)

**Released crate versions:**

- `neurogrim-core` — `5.0.0`
- `neurogrim-sensory` — `5.0.0`
- `neurogrim-mcp` — `5.0.0`
- `neurogrim-a2a` — `5.0.0`
- `neurogrim-secrets` — `5.0.0`
- `neurogrim-ecosystem` — `5.0.0`
- `neurogrim-dashboard` — `5.0.0`
- `neurogrim-cli` — `5.0.0`
- `neurogrim-sdk` — `0.1.0` (in-tree only; `publish = false` during 0.x soak
  per V5-SDK-1 plan-critic 🔴 fix; promotion to `1.0` requires ≥6 weeks soak
  + ≥1 external adopter validation)

**LSP-Brains specification:** v3.0 → v3.1 (additive — new §9.8 trait surface
recommendation; no conformance claims invalidated).

**Deferrals tracked in [`roadmap/BACKLOG.md`](roadmap/BACKLOG.md):**

- B-28 (coverage-aware test selection) → V5-FOUND-3 deferred to v5.1/v6
- B-37..B-45 (v5.5 + v6 successor pipeline — dashboard widget plugin trait, MCP
  tool plugin loading, dynamic `.so/.dll`, per-domain custom CMDB types,
  agent-card versioning, trajectory model abstraction, per-test coverage as
  Brain domain, diagnostic synthesis as Brain domain)
- B-46 (re-export-aware semver gate when rustdoc supports cross-crate inlining)
- B-47 (sccache for CI release-build paths; deferred from V5-FOUND-2 Fork B)
- B-48 (SLO violations fix queue — encrypted-secrets KDF cost parameterization)
- B-49 (SDK surface-assertion conformance pins)
- B-50 (Sensor walkthrough deduplication via `#![doc = include_str!]`)
- B-51 (`AgentDrivenRunner` real impl — paired with V5-FOUND-1.1 Rust LLM
  client; honors VISION #20 — deferred until real second impl is in scope)
- B-52 (`--runner=` CLI flag dispatch via TestRunnerRegistry; gated on B-51 or
  external-adopter contribution)
- B-53 (terminology-coherence sensory tool + CMDB)

This [Unreleased] block moves to a `## [5.0.0] - <date>` heading when the
operator runs `git tag -a v5.0.0`.

## [4.0.0] - 2026-04-30

*v4.0 — Ship Without Surprise — the structured pre-publish pipeline.
Replaces the v3.x "manual operator review + `methodology_drift` test
only" posture with a declarative gate manifest, a runner CLI that
executes gates in declared order, a per-gate JSONL ledger, a
read-only dashboard surface, and a Playwright E2E foundation.
NeuroGrim itself starts going through this pipeline at v4.0 (see
`roadmap/v4.0-publish-process.md`).*

**Also folded into v4.0:** a session of bus dogfooding migrations
(score-history, services, skill-invocations all backed by SQLite bus
topics) plus the v4.5 TSDB foundation (`neurogrim_core::metrics` +
universal self-instrumentation + Plumbing page). All landed before
the inaugural v4 publish; documented under their own subheadings
below for traceability.

### Added — slow-benchmark surgery (S12-G-1)

Two `crates/neurogrim-cli/tests/context_overhead.rs` benchmarks
(`b10_phase1_four_brain_sweep`, `b10_phase1p5_description_only_measurement`)
moved behind `#[ignore]`. Workspace test runtime drops from ~218s
to ~29s warm cache (96s cold). Run the slow ones with `neurogrim
test --slow` (passes `--include-ignored` to cargo). Snapshot at
`roadmap/data/test-runtime-baseline.txt`.

### Added — `neurogrim test` quiet wrapper (S12-G-2)

New CLI subcommand wrapping `cargo test --workspace --all-targets`.
Suppresses success spam, prints failures inline, appends one JSONL
entry per failure to `<project>/.claude/brain/test-failures.jsonl`,
mirrors cargo's exit code. Flags: `--keep-last N` (rotate older
entries to archive), `--show-only-new` (diff against prior run),
`--retry-failed` (replay only the most recent failure batch),
`--slow` (include `#[ignore]`d tests), `--verbose` (bypass parser),
`--e2e` (S12-G-5 — invoke the Playwright suite instead of cargo).

### Added — `publish-gates.yaml` schema + doctor validation (S12-G-3)

New manifest format at `<brain>/.claude/brain/publish-gates.yaml`
declaring publish gates by `gate_type` (`automated` / `manual` /
`e2e`). Schema-versioned (Draft-07 JSON Schema vendored at
`crates/neurogrim-mcp/data/schemas/publish-gates-v1.schema.json`).
Closed vocabulary: kebab-case `id` pattern, `additionalProperties:
false` at every level, `if/then` rules for type-specific required
fields, timeout bounded 1–3600s. Validated by `neurogrim doctor`
(new check 8); missing manifest is silent during v4.0 rollout
(opt-in posture for adopters); malformed manifest emits one Error
finding per validation issue. Typed Rust view `PublishGatesConfig`
exported from `neurogrim_mcp::publish_gates` for downstream
consumers.

### Added — `neurogrim publish-gate {run,ack}` CLI (S12-G-4)

Load-bearing v4.0 CLI. `run` executes the manifest's gates in
declared order, prints per-gate outcomes, and appends one JSONL
entry per gate to `<brain>/.claude/brain/publish-gate-ledger.jsonl`.
`ack` marks the most recent `pending` entry for a manual gate as
passed by an operator. Exit code precedence: failed > pending >
passed (0 = all blocking passed, 1 = any blocking failed/timed_out/
errored, 2 = any blocking pending and none failed). Non-blocking
gate failures recorded but never drive exit. Mode filter (heuristic
in v1; schema v2 will introduce explicit per-gate mode tags):
pre-commit = automated gates with `timeout_seconds ≤ 30`;
pre-publish = all `blocking: true` gates; full = every gate.
`--gate <id>` overrides mode. Operator handle resolution:
`--operator` flag → `$NEUROGRIM_OPERATOR` env → reject (no
"unknown" fallback per spec §17.6 audit-rationale discipline).
Stdout/stderr captured per gate (truncated to 4 KB head + 4 KB
tail with `…[truncated N bytes]…` marker, keeping typical entries
under PIPE_BUF for `O_APPEND` atomicity).

### Added — Playwright E2E foundation (S12-G-5)

New `crates/neurogrim-dashboard/frontend/e2e/` directory with
`playwright.config.ts` (chromium-only, sequential workers,
`globalTimeout: 180_000` enforcing the 3-minute S12 invariant).
Three smoke specs ship green: `overview-loads.spec.ts`,
`federation-page.spec.ts`, `layout-edit.spec.ts` (canary for the
v3.5 React #310 federation crash class). The webServer block
spawns a fresh `target/debug/neurogrim ui` instance on port 17345
mounted at NeuroGrim's brain registry. `e2e` gate type in
`publish-gates.yaml` runs the suite via `npx playwright test`;
adopters without the dashboard frontend get a clear "use
`automated` instead" error.

### Added — Manual gate UI surface (S12-G-6)

Read-only dashboard page at `/brains/:id/publish-gates` joining the
manifest with the ledger. Renders gate table with status badges
(passed / failed / pending / timed_out / deferred / error / no_runs)
+ recent-activity timeline (last 50 entries). Backed by new
`GET /api/brains/:brain_id/publish-gates` API endpoint.
AppShell nav link with `GitMerge` icon. CLI side: `--interactive`
flag on `run` enables inline y/N prompting for manual gates (auto-
detected via `IsTerminal` when neither `--interactive` nor
`--no-interactive` is passed); 'y' + resolvable operator = inline
ack; everything else falls through to the existing async pending
flow, preserving the CI-friendly path.

### Added — NeuroGrim self-hosting (S12-G-7)

NeuroGrim's own publish pipeline declared at
`.claude/brain/publish-gates.yaml` with 7 gates: `doctor-clean`,
`tests-pass`, `cargo-publish-dryrun`, `changelog-dated`,
`e2e-smoke`, `review-changelog`, `dashboard-renders-locally`. v4.0
itself ships through this pipeline as the first dogfood pass.
Publish process documented at `roadmap/v4.0-publish-process.md`.

### Added — 12th explain topic

`neurogrim explain publish-gates` covers the gate pipeline end to
end: gate types, manifest schema, runner CLI, ledger schema, mode
filter, ack flow, e2e setup, adopter onboarding.

### Changed — v4.0+ NeuroGrim publishes go through `publish-gate run`

Starting with v4.0, every NeuroGrim release is gated on a clean
`neurogrim publish-gate run` from the repo root. The runbook lives
at `roadmap/v4.0-publish-process.md`; the gate manifest at
`.claude/brain/publish-gates.yaml`. Adopters are NOT required to
adopt the same posture — `neurogrim doctor`'s `check_publish_gates`
treats a missing manifest as silent (opt-in during rollout).

### Architectural decisions baked in

These were locked via the 2026-04-29 four-question + follow-up
conversation, documented at the top of each epic file:

- Hard gates default-on (single-adopter reality; `--enforce-autonomy`
  reserved as future escape hatch, not default-off opt-in).
- Schema v2 will add per-gate mode tags; v1 mode filter is
  heuristic only.
- e2e gates are NeuroGrim-internal in v1 (run the bundled Playwright
  suite); adopters with their own browser tooling should use
  `automated` gate type.

### Added — bus dogfooding: score-snapshots SQLite topic

`score-history.json` retired. Score snapshots now publish to the
`_neurogrim/score-snapshots` SQLite-backed bus topic. Writer
(`neurogrim-mcp::context::append_score_history`) opens
`SqliteBackend` directly and auto-migrates from legacy JSON on first
write; reader (`neurogrim-dashboard::logs::read_score_history`)
uses bounded `read_from(start, limit+1)` with delta-window
optimization. Eliminates the read-modify-write of a full JSON array
on every score run — the worst persistence pattern in the prior
codebase. Filesystem watcher recognizes both `.sqlite` and
`.sqlite-wal` paths so SSE notifications fire on the first INSERT,
not just on checkpoint.

### Added — bus dogfooding: skill-invocations hybrid topic

The PostToolUse shell hook (`scripts/record-skill-invocation.sh`)
keeps writing JSONL — bash can't write SQLite without paying ~100ms
cold-start per invocation. JSONL stays canonical; new
`neurogrim_core::skill_invocations::ingest_and_open` lazily catches
SQLite up from JSONL on every read using non-empty-line-count vs
SQLite-row-count as the watermark. Both the dashboard reader and
the `capability-hygiene` sensor switch to SQLite-backed reads —
the sensor now consumes its own `_neurogrim/skill-invocations` bus
topic. 6 new unit tests cover ingest, idempotency, malformed-line
tolerance.

### Added — bus dogfooding: services SQLite topic

`services.jsonl` retired. Service lifecycle events (started /
failed / stopped) now ride the `_neurogrim/services` SQLite topic.
Same shape as the score-snapshots migration. Closes a parallel
persistence layer that lived alongside the bus's queue
infrastructure.

### Added — `neurogrim_core::metrics` time-series store (B-36 iter 1)

Local time-series store at
`<project>/.claude/brain/queues/_neurogrim/metrics.sqlite`. Single
file, WAL mode, schema:
`metric_points(id, metric_name, ts_ms, tags_json, value)` with a
composite index on `(metric_name, ts_ms)`. Pre-declared tag
dimensions (no Prometheus-style freeform labels). Public API:
`MetricsStore`, `MetricsHandle` thread-safe wrapper, typed `Tags`,
`Query` builder with tag filters / time windows / limits, six
aggregations (avg / sum / min / max / count / last) with bucketing,
`list_series()` + `total_points()` + `size_bytes()` for plumbing
introspection, `delete_before()` for retention sweeps. 9 unit tests.

### Added — universal self-instrumentation

Five always-on metric series record dashboard activity into the
TSDB:

- `request_duration_ms{path, status}` — axum middleware on every
  `/api/` request. Path normalizer collapses
  `/api/brains/<id>/domains/<name>` to
  `/api/brains/:id/domains/:name` for bounded cardinality.
- `cache_event{cache, kind=hit|miss|invalidate}` — wired into
  `BrainContextCache::load_or_get` plus the SSE invalidation task.
- `peer_probe_ms{peer, outcome}` — wired into `build_federation`.
- `bus_publish{topic, backend}` — wired into `state.bus.publish()`
  call sites.
- `domain_score{domain}` + `brain_score` + `domain_confidence{domain}`
  — auto-ingested from `_neurogrim/score-snapshots` payloads via a
  bus subscriber spawned at server startup. One-time backfill from
  the topic's persistent storage at first start.

### Added — Plumbing dashboard page (B-36 iter 1 frontend)

New top-level page at `/brains/$brainId/plumbing` (Wrench icon nav).
Header strip with 4 stat cards (TSDB series count, total points +
size, queue topic size, brain ID). Two functional tabs: **Metrics**
(registered series table → click for expanded recent-points view;
cardinality > 50 surfaces as destructive badge) and **Queues** (bus
topic listing). Both auto-refresh every 10s via TanStack Query.
Backend endpoints: `GET /api/brains/:id/plumbing/{overview,
metrics/series, metrics/:name}`. Iteration 2 will add Storage and
Watchers tabs plus operator actions (vacuum, replay, export to
JSONL).

### Added — `read_json_value` + `internal_error` route helpers

Two tiny shared helpers in `routes.rs` collapse the two repeated
patterns: 6-line BOM-stripping JSON read (`read_json_value(&path)`,
2 sites) and 8-line
`(StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": ...})))
.into_response()` (`internal_error(format!(...))`, 9 sites for
"failed to load BrainContext"). Net –41 lines; each call site reads
more directly.

### Added — `with_host_context` / `with_brain_context` async wrappers

8 handlers (4 legacy single-Brain + 4 federation-aware) shared an
identical resolve-load-error scaffolding. Two async-closure wrappers
take a `Arc<BrainContext>` by value (so the closure can hold it
across `.await` without lifetime gymnastics) and return any
`IntoResponse`. Net –20 lines, clearer intent.

### Added — perf baseline + B-33/B-34/B-35 backlog entries

Pre-TSDB dashboard timing baseline at
`roadmap/data/dashboard-perf-baseline-2026-04-30.md` with full
per-page measurements + reproducible Playwright harness. Backlog
entries: B-33 (federation peer-probe parallelization, ~30 min fix
for the 2200ms wall-clock issue), B-34 (stat-validated CMDB cache
to eliminate ~180ms/load of repeat parsing), B-35 (granular
ScoreChanged invalidation), B-36 (TSDB epic with 4-iteration plan).

### Fixed — stream-truncation underflow in `publish-gate run`

`publish_gate.rs::truncate_for_log` would `total - head.len() -
tail.len()` where `total` is byte-length but the truncate
constants are character counts. Multibyte content (Playwright's
stderr can carry UTF-8 box-drawing characters) caused integer
underflow → panic. Fix: compare character counts in the early
return; use `saturating_sub` for the "truncated bytes" message.

### Test totals at v4.0 publish

- Workspace lib: 1046 passing across 7 crates (was 1023 — +9 metrics
  + 8 path-normalization + 6 skill-invocations).
- Frontend (vitest): 283 passing across 28 files.

## [3.5.0] - 2026-04-29

*Bind & Run — per-project random port allocation + dashboard service
lifecycle. Solves the recurring port-conflict pain when running
multiple Brains side-by-side, and lets power users start/stop A2A
services from the dashboard UI on demand instead of always-on.
Plus anchor-based deep links into the Overview page and a
`ports-panel` widget for surfacing the new port allocation.*

### Added — per-project port allocation (`neurogrim-core::ports`)

New module in `neurogrim-core` that picks two ports (dashboard +
a2a) from the IANA dynamic range (49152-65535) on first run,
persists them to `<project>/.claude/brain/ports.json`, and reuses
them on every subsequent invocation. Same atomic-write pattern as
`dashboard-layout.json` (temp file + rename — concurrent readers
never see a partial write). Idempotent via `read_ports → allocate`
fallthrough; ports are sticky across restarts unless the file is
deleted or the operator passes an explicit `--port` (which
deliberately does NOT touch `ports.json`, preserving v3.4-era
bookmarks at `:8420`).

The `try_bind` precheck filters candidates against actual OS bind
feasibility before persisting; deterministic-seeded RNG is exposed
for tests. Failure mode: when every candidate in the range is
already bound, `AllocateError::Exhausted` carries the attempt
count + range bounds for forensics.

### Added — `neurogrim federation rewire` CLI subcommand

Operator-explicit migration tool for parent registries that
hardcode pre-v3.5 child ports. Reads the child's
`<brain_path>/.claude/brain/ports.json::a2a_port` and rewrites
the parent's `config.children[name].a2a_endpoint` +
`agent_card_url` to match. With `--probe-only`, prints the diff
and exits 0 without modifying anything. No silent registry
mutations on parent's start — the operator runs `rewire` once
per affected child after a v3.5 upgrade.

### Added — `--allow-mutations` flag + service start/stop endpoints

The dashboard advertised `--allow-mutations` as planned-for-v3.5 in
its v3.4 doc-comments; this release wires it through. When set,
three new mutation endpoints become reachable (otherwise they
return 403 `code: "mutations-disabled"`):

- `POST /api/brains/:id/peers/:peer/start` — spawns
  `<current_exe> a2a-serve --port <port> --project-root
  <peer_brain_path>` as a child process, sets up a per-service log
  at `<peer_root>/.claude/brain/logs/<peer-name>.log`, and broadcasts
  `ServiceStarting` immediately + `ServiceStarted` (or
  `ServiceFailed`) within ~5 s once the readiness watcher confirms
  the port is bound.
- `POST /api/brains/:id/peers/:peer/stop` — kills the tracked child
  (`tokio::process::Child::kill` → SIGKILL on Unix,
  `TerminateProcess` on Windows). 409 when not running under this
  dashboard. Broadcasts `ServiceStopped`.
- `GET /api/brains/:id/services` — read-only inventory of services
  this dashboard instance currently supervises.

`tokio::process::Child::kill_on_drop` is intentionally NOT set:
spawned services survive a dashboard restart (matches the user
preference for "leave running"). On dashboard restart, services
from a previous session probe as `alive` via the regular
federation probe but the Stop button stays hidden — we don't
adopt PIDs from prior runs in v3.5.

Four new SSE event variants drive the frontend:
`ServiceStarting{peer_name, pid, port}`,
`ServiceStarted{...}`, `ServiceStopped{peer_name, pid}`,
`ServiceFailed{peer_name, reason}`.

### Added — frontend `PeerActions` component

Slotted into the Federation page's `PeerDetailCard` between the
detail grid and the Agent Card excerpt. Visible only when
`mutations_allowed` is true (read from `/api/health` once on
mount) and the peer's transport is `"a2a"`. Optimistic UI via
`useMutation`: clicking Start flips local state to a spinner;
the SSE feed invalidates the federation query so the
StatusBadge refreshes within ~5 s. Errors from synchronous
spawn failures (422 `port-conflict`, 409 `already-running`,
etc.) are surfaced inline with a 6 s auto-clear.

### Added — anchor-based deep links

Each Overview-page widget now renders with
`id="widget-<spec.id>"`. The new helper
`@/lib/anchors.widgetAnchorUrl(brainId, widgetId)` returns
`/brains/<brain>/#widget-<id>` so agents (and humans) can link
deep into a specific widget. On page mount + on every
`hashchange`, `applyHashAnchor` smooth-scrolls the matching
widget into view and applies a 1.5 s pulse highlight via the
new `widget-pulse` CSS animation.

### Added — `ports-panel` widget

New widget type in the dashboard catalog. Self-fetches
`/api/brains/:id/ports` and surfaces:

- Dashboard port + bound/free indicator
- A2A port + bound/free indicator
- Path to `ports.json`, its `created_at`, and the
  `generated_by` software-version stamp

Useful diagnostic for power users running multiple Brains
side-by-side. When `ports.json` is missing on disk, the widget
explains how to allocate via `neurogrim ui` or `neurogrim
a2a-serve`.

### Changed — `--port` becomes optional on `neurogrim ui` + `neurogrim a2a-serve`

Both commands now take `Option<u16>` instead of `u16` with a
hardcoded default. Precedence rule:

1. CLI `--port <n>` explicit → use it, do NOT touch `ports.json`.
2. `ports.json` exists → use the persisted value for the relevant
   role (`dashboard_port` for `ui`, `a2a_port` for `a2a-serve`).
3. Neither → allocate fresh from the dynamic range, persist,
   announce loudly.

The previous defaults (8420 for `ui`, 8421 for `a2a-serve`) are
gone. Existing users with bookmarks at `:8420` keep working with
`neurogrim ui --port 8420`; the explicit-port path is opt-in
preservation, not a silent fallback.

### Bindings + tests

Five new ts-rs bindings exported to `crates/neurogrim-dashboard/
bindings/`: `StartPeerResponse`, `StopPeerResponse`,
`ServiceErrorDto`, `ServiceSnapshot`, `ServicesListResponse`. The
existing `HealthResponse` gained a `mutations_allowed` field.

Test count: 11 new ports tests in `neurogrim-core::ports::tests`,
4 new `federation rewire` tests in
`neurogrim-cli::commands::federation::tests`, 3 services-registry
tests in `neurogrim-dashboard::services::tests`. All passing.

### Migration notes for v3.4 adopters

- **Bookmarks at `:8420`**: pass `--port 8420` to `neurogrim ui`
  to keep them working, OR delete the project's existing
  expectations and let v3.5 allocate fresh.
- **Parent registries with hardcoded `:8421` child endpoints**:
  run `neurogrim federation rewire --child <name>` once per
  child after upgrading. `--probe-only` prints the diff first.
- **Custom dashboard layouts** (v3.4 `dashboard-layout.json`)
  continue to work unchanged. Add a `ports-panel` widget if you
  want the new port-allocation visibility.

## [3.4.0] - 2026-04-29

*The third audience surface — a self-contained HTTP + React
dashboard (`neurogrim ui`) that gives humans a visual companion
to the CLI and MCP server. Multi-Brain navigation, customizable
homepages with a widget system, SSE-driven live updates,
hat-lens picker, dark/light theme. New crate
`neurogrim-dashboard` (the seventh in the workspace).*

### Added — `neurogrim ui` command + `neurogrim-dashboard` crate

The dashboard ships as a single binary: the React frontend is
built into `frontend/dist/` and embedded at compile time via
`rust-embed`. Operators `cargo install neurogrim-cli` and the UI
ships with it — no Node.js required at runtime. Read-only in
v3.4 by design; mutation endpoints are gated behind a
`--allow-mutations` flag planned for v3.5.

#### Five pages

- **Overview** (`/`) — landing page with score gauge, trajectory
  badge, top three strongest signals, top three recommendations,
  and the all-advisory "N/A · observe-only posture" rendering for
  Brains that haven't promoted any domain to weighted yet.
- **Domains** (`/domains`) — sortable table per declared domain
  with weight, raw score, effective score, confidence,
  trajectory, last-updated. Color-coded scores (green/amber/red).
  Click a row to drill in.
- **Domain detail** (`/domains/:name`) — findings table with
  status badges + signed point deltas, Recharts sparkline of
  recent score history, CMDB metadata, and the sensor authoring
  intent block (`_todo_<name>`) when the sensor hasn't been
  written yet.
- **Federation** (`/federation`) — one-hop view of declared
  peers with hand-drawn SVG topology (self + peers, color-coded
  by liveness, dashed edges for read-only siblings) and an
  Agent Card excerpt panel on click. A2A peers are probed at
  `/.well-known/agent-card.json` with a 1.5s timeout;
  subprocess peers stay `unprobed` by design.
- **Skills** (`/skills`) — inventory of every skill under
  `.claude/skills/` paired with invocation-ledger stats. Filter
  chips (alive / dead / new / no-ledger), search by name +
  description, click-to-expand detail. Surfaces a CTA banner
  when the PostToolUse hook hasn't been wired yet so the ledger
  is missing.

#### Live updates over SSE

A `notify::RecommendedWatcher` translates filesystem changes
into typed `DashboardEvent` variants
(`RegistryChanged` / `ScoreChanged{domain}` / `SkillInvoked`)
and broadcasts them over `tokio::sync::broadcast`. The
`/api/events` SSE handler subscribes a fresh receiver per
connection; the React frontend's `useDashboardEvents` hook
parses each event and invalidates only the relevant TanStack
Query keys. End-to-end latency on Windows: ~250 ms.

A small color-coded indicator in the sidebar footer surfaces
connection status (live / connecting / offline / disabled) so
the operator can tell at a glance whether they're getting
real-time updates.

#### Hat-lens picker

Dropdown in the sidebar that lists every hat declared in
`config.hats` plus a synthetic `default` entry. Selecting a hat
adds `?hat=<name>` to every score-aware request so the Brain
output flows through that hat's `domain_multipliers`. Selection
persists in `localStorage`. The picker collapses to a static
"no hats" label when the registry has no hats declared.

#### Theme

Dark/light toggle in the sidebar footer. Persists in
`localStorage`; first-load fallback honors
`prefers-color-scheme`. Tailwind's `darkMode: ["class"]`
strategy applies the `dark` class to `<html>`.

#### Cross-platform browser launch

`neurogrim ui` opens the default browser by default with
hardened detection logic: skips on `CI=true`, on Linux without
`DISPLAY` / `WAYLAND_DISPLAY`, and in headless SSH sessions —
each with a distinct stderr message explaining *why* it
skipped. WSL is detected via `/proc/version` and the URL is
routed through `cmd.exe /c start` to reach the Windows host
browser. Always honors `--no-browser`.

### Added — `neurogrim explain ui`

Tenth bundled methodology topic covering the dashboard surface:
the five pages, `neurogrim ui` flags, browser-launch decisions,
SSE wire details, hat lens, theme, surface comparison
(CLI vs MCP vs dashboard), architecture (rust-embed bundling,
ts-rs bindings, TanStack Router, the seven `/api/*` routes).

`BUNDLED_VERSION` bumped to `v3.4` and every topic file's
version header updated accordingly.

### Added — `neurogrim-dashboard` crate API surface

- `GET /api/health` — server version + registry path
- `GET /api/overview?hat=<name>` — score gauge data + top recs
- `GET /api/domains?hat=<name>` — sortable list view
- `GET /api/domains/:name?hat=<name>` — drill-in detail
- `GET /api/federation` — peers + Agent Card probes
- `GET /api/skills` — inventory + ledger stats
- `GET /api/hats` — picker choices
- `GET /api/events` — SSE stream

Every wire-format type derives `ts_rs::TS` and exports to
`crates/neurogrim-dashboard/bindings/` at `cargo test` time so
the frontend stays type-aligned with the Rust source of truth.

### Tooling — frontend stack

React 18 + TypeScript 5.7 + Vite 5.4 + Tailwind 3.4 + shadcn/ui
copy-paste components + Recharts (gauge + sparkline) +
TanStack Query 5.62 + TanStack Router 1.95. No new npm
dependencies introduced beyond the initial Phase 0 set —
phases 1.3 and 2.1 explicitly avoided installing `react-flow`
and a separate SSE client library, building both surfaces
on existing primitives instead.

### Tooling — supply-chain discipline (interrupt during Phase 0)

- New bundled `dependency-discipline` skill in
  `init-skills/` (152 lines) — captures the discipline
  NeuroGrim's methodology requires before any dep enters
  the project's trust boundary. Federated to all Brain
  copies.
- `cargo xtask sca-check` runs `cargo audit` + `npm audit`
  with severity gating; alias declared in
  `neurogrim/.cargo/config.toml`.
- `audit/dep-accepted-2026-04-28.md` documents the five
  esbuild moderate findings (GHSA-67mh-4wv8-2f99) accepted
  as dev-only.

### Added — multi-Brain navigation (Path 2)

The dashboard learns to route between every Brain reachable
from the host. One bookmark, one server, full federation tree.

- `BrainTree::discover()` walks `config.children` transitively
  from the host registry. Each Brain has a stable kebab-case id
  derived from `meta.project` (or project_root basename); cycle-
  guarded via canonicalized-path visited set; missing child
  registries are recorded as declared entries (declared
  display_name) but not walked further.
- New routes mirror every existing page under `/brains/$brainId`:
  `/api/brains` lists every Brain; `/api/brains/:id/{overview,
  domains, domains/:name, federation, skills, hats,
  dashboard-layout}` are per-Brain scoped. Legacy single-Brain
  routes preserved for backward compatibility.
- Frontend: TanStack Router restructured with a `/brains/$brainId`
  parent layout route. The index `/` redirects to
  `/brains/<self_id>/`. New `BrainSelector` in the AppShell
  sidebar lists every reachable Brain with tree-style indent
  (host → ↳ children → ↳↳ grandchildren). Selecting one
  navigates to that Brain's Overview.
- Page queries gain a `brainId` segment in their queryKey so
  TanStack Query's cache no longer bleeds across Brains.
- Solves the user-flagged problem with all-advisory hosts:
  the ecosystem Brain's "N/A · observe-only" is correct, but
  the *substantive* score data lives in the children. Now the
  operator can navigate from the all-advisory host directly
  into a child's full opinionated dashboard.

### Added — customizable homepage (Phase B slice 1)

The Overview page is no longer hard-coded. Each Brain renders
from a per-Brain widget layout, with posture-aware defaults so
every Brain auto-gets a useful first layout without anyone
authoring JSON.

- Per-Brain layout file at `<brain>/.claude/brain/dashboard-layout.json`.
  Schema: `{ schema_version, brain_id, is_default, widgets: [
  { id, widget_type, size, title, config } ] }`. Sizes are
  `full | half | third | quarter` — list-with-size-hints, not a
  true x/y grid; widgets autoflow.
- 6 initial widget types: `identity`, `score-gauge`,
  `strongest-signals`, `top-recommendations`, `domain-card`
  (single-domain stat with click-through), `markdown-note`
  (free-text card with safe inline rendering of bold/italic/
  code).
- Posture-aware defaults: weighted Brains get the gauge-centric
  layout (identity → gauge / strongest / recs, all third-width).
  All-advisory Brains with declared `child-*` domains get the
  child-card-first layout (identity → observe-only note →
  4 child cards as quarters → strongest + recs as halves) so
  the ecosystem Brain's homepage actually shows substance,
  not "N/A". All-advisory Brains without children fall back
  to the gauge layout (which renders "N/A" honestly).
- "Showing the default layout" banner with a hint about
  `.claude/brain/dashboard-layout.json` so operators know
  custom layouts are a thing they can author.
- Unknown widget types render an `UnknownWidget` placeholder
  instead of breaking the page — forward-compatible if a
  future bundle invents new widget types.
- Layout fetch failure also non-fatal: a hard-coded
  `FALLBACK_WIDGETS` set keeps the page useful even if
  `/api/brains` is misbehaving.

### Added — child-scoring infrastructure (A2A scoring source)

The `scoring_source.type: "a2a"` mechanism (already wired in
v3.3 via `mcp/context.rs::load_a2a_domain` and the
`three_way_brain.rs` integration test) gets first-class
dashboard treatment in v3.4. A Brain can declare a domain
whose raw score IS another Brain's unified score, pulled live
at score time per spec §9 fractal composition. Failure modes
(peer offline, timeout, malformed response) fall through to
`no_file_score` cleanly.

The dashboard's Domains page now renders these as regular
domain rows with the live A2A score; click-through navigates
to the child's full dashboard via Path 2. The all-advisory
ecosystem case becomes substantive: the dashboard shows each
child Brain's score with click-in to drill down.

### Added — soft/hard skill invocation tracking

Caught during user vetting that the invocation ledger
systematically under-counted skill usage by an order of
magnitude — agents follow skills primarily by *reading* the
SKILL.md file via the Read tool, not by invoking the explicit
`Skill` tool that the original PostToolUse hook matched.

- `scripts/record-skill-invocation.sh` rewritten to branch on
  `tool_name`: `Skill` → `subtype: "hard"`, `Read` matching a
  SKILL.md path → `subtype: "soft"`. Path matcher rejects
  nested files (`/.claude/skills/foo/REFERENCE.md`) and
  excluded names (README*, archived, dotfiles).
- Ledger schema bumped from `1` to `2`; existing schema-1
  entries (no `subtype` field) default to `hard` for backward
  compat — the dashboard parser handles both.
- `SkillDto` gains `hard_invocations`, `soft_invocations`,
  `recent_hard_invocations`, `recent_soft_invocations` fields.
  The Skills page renders `5 (2h / 3s)` — total + per-subtype
  breakdown, with tooltip explaining the distinction.
- Each Brain's `.claude/settings.local.json` adds a `Read`
  matcher pointing at the same script (ecosystem, NeuroGrim,
  LSP-Brains, python-starter all updated).

### Added — two-stage federation probe

Federation page peer-status was previously a single Agent Card
fetch with everything-collapsed-to-`unreachable`. The two-stage
probe replaces this with specific outcomes that match the
questions an operator asks.

- **Stage 1 — TCP precheck.** `tokio::net::TcpStream::connect`
  with a 1s timeout. Connection refused → `not-running`. Other
  IO errors → `not-running` (OS-level rejection). Localhost
  timeout → `not-running` (Windows takes seconds to surface
  ConnectionRefused on closed loopback ports due to SYN retry;
  there's no realistic firewall scenario for 127.0.0.1).
  Remote timeout → `unreachable`.
- **Stage 2 — Agent Card fetch.** Only runs when TCP succeeded.
  Failure or timeout → `unhealthy` (process is up but the
  well-known endpoint isn't responding cleanly). Success →
  `alive`.
- **Dual-stack-aware connect.** A separate fix landed during
  user vetting: registries declare endpoints as
  `http://localhost:<port>/...`, and on Windows `localhost`
  resolves to both `::1` and `127.0.0.1`. `tokio::TcpStream::connect`
  takes only the first resolved address; if `::1` sorts first
  and the daemon binds to `127.0.0.1` (the `a2a-serve`
  default), the connect just times out. The probe now calls
  `lookup_host()` and iterates every candidate address until
  one connects, matching curl/browser behavior.

### Added — Radix-based custom Select (UX polish)

Native `<select>` dropdowns on Chromium ignore `option:hover`
styling and use the OS-default highlight (bright blue on
Windows). After several CSS-layer attempts that worked for the
selected state but not the hover state, the BrainSelector and
HatPicker were rewritten on `@radix-ui/react-select` (the
canonical primitive shadcn/ui's Select wraps).

- New dep: `@radix-ui/react-select@^2.2.6` (MIT, WorkOS-
  maintained, 21 internal Radix sub-primitives, no new findings
  in `npm audit` after install).
- New `components/ui/select.tsx` (shadcn-style wrapper),
  reusable for any future dropdowns. Subtle hover highlight
  via `data-[highlighted]:bg-secondary` matches the muted
  dashboard palette.
- New CSS variables `--popover` / `--popover-foreground` for
  the Radix portal panel styling.

### Added — `neurogrim ui` browser-launch hardening

Browser open is now a testable decision pipeline that
distinguishes the *reason* it skipped:

- `--no-browser` always wins (operator intent).
- `CI=true` / `GITHUB_ACTIONS=true` → "CI environment detected".
- Linux without `DISPLAY`/`WAYLAND_DISPLAY` → "no graphical
  session" (or "remote SSH session without DISPLAY" if
  `SSH_CONNECTION` is set).
- WSL detected via `/proc/version` (more reliable than
  `WSL_DISTRO_NAME`); routes through `cmd.exe /c start` so
  the URL opens in the host Windows browser.
- 10 unit tests cover the full decision matrix.

### Changed
- **Workspace `version` 3.3.0 → 3.4.0** across all 7 crates +
  `[workspace.dependencies]`. Frontend `package.json` synced.
- `neurogrim-cli` now depends on `neurogrim-dashboard` (new).
- `BrainContext` relocated from `neurogrim-cli` to
  `neurogrim-mcp` (Phase 0.1) so both the CLI and the
  dashboard server share a single source of truth for
  registry + scoring pipeline loading.
- The Skills page invocation column renders the hard/soft
  split (`5 (2h / 3s)`) instead of the prior raw total. Total
  count is preserved as the bold leading number; the
  parenthetical adds the breakdown.

### Test surface delta
- 73 Rust dashboard tests covering events classification,
  watcher integration, route smoke for all endpoints
  (overview, domains, federation, skills, hats, layout,
  events), the BrainTree discovery walk (host-only,
  direct children, grandchildren, missing registry,
  collision resolution), the skills scanner with YAML
  block-scalar handling and soft/hard subtype split,
  the layout module (default-layout dispatch, file
  read/parse/missing/malformed paths, is_default
  override), the two-stage federation probe (closed
  port → not-running, open-but-unhealthy → unhealthy),
  and the ScoreQuery hat normalization
- 10 ui-cmd tests (browser-launch decision matrix —
  --no-browser / CI / Linux-no-display / SSH / WSL / etc)
- 104 vitest tests across 14 files covering component
  rendering, theme persistence, SSE hook lifecycle,
  hat-picker context wiring, the multi-Brain
  router-helper, and (slice 1's gap that the publish-prep
  cycle is closing) the layout-driven Overview rendering
- 1 explain regression test (`ui_topic_describes_the_five_pages_and_sse`)

### Breaking changes
None. The CLI surface is unchanged; the new `ui` subcommand is
additive. The dashboard ships read-only by design — any
existing automation pointed at the registry is untouched.

### Background — what prompted this release

The CLI gives agents a canonical contract; the MCP server gives
LLMs typed tools. But neither surface is reach-and-glance for
humans wanting charts, sparklines, and a sortable view of every
domain. The v3.4 dashboard closes that audience gap, while
adding the SSE substrate that v3.5+ live mutation flows will
need. The supply-chain interrupt early in the phase strengthened
the npm-dependency policy across the project; that discipline
work was a precondition for ever shipping a frontend.

## [3.3.0] - 2026-04-28

*Agent-authoring substrate. Closes all 10 friction points (F1–F10)
surfaced by the v3.2.2 agent-driven adoption test on the job-hunt
pilot, including a real runtime liveness bug (port 8423 collision
across the federation tree).*

### Added — explain topic
- **`neurogrim explain autonomy`** (F1) — bundled topic covering the
  autonomy block schema: levels, action_types, safety_invariants. The
  v3.2.2 agent had to grep the binary for field names; this closes
  that discoverability gap. Also serves as the worked example for the
  v3.2.2-era schema divergence (the agent invented `autonomy_bias`
  instead of using the canonical `blast_radius` / `reversible` /
  `description` set).

### Added — `doctor` checks
- **`check_autonomy`** (F3) — validates the autonomy block end-to-end:
  declared levels include the four canonical names; `action_types[].default_level`
  references a known level; `safety_invariants[]` entries have
  `rule` + at least one of `minimum_level` / `enforced_level`; both
  set is ambiguous (warn); unknown fields trigger warnings (catches
  invented fields like `autonomy_bias`); `description` recommended
  on action_types + safety_invariants.
- **`check_federation_ports` walks transitively** (F7) — the v3.2.2
  port-uniqueness check only considered direct children. v3.3 walks
  each peer's `brain_path/.claude/brain-registry.json` recursively
  and reports clashes across the entire federation tree. Includes
  cycle-guard via visited-paths set.

### Added — CLI flags
- **`neurogrim agent --prose --all-domains`** (F4) — list every
  declared domain in the prose signals section instead of capping
  at top 3. Auto-expands when the Brain is all-advisory (the
  "strongest signals" framing is misleading when no domain has
  weight > 0). MCP `orient` tool has matching `all_domains: bool`.
- **`neurogrim skill new --stub`** (F2) — produces a minimal-but-routable
  skill: sensible-default frontmatter (no literal `TODO —` strings)
  + a single-paragraph body identifying the file as a stub. Routing
  index has something to match against immediately. Use this when
  the operator's intent is "scaffold stubs, fill bodies later."
- **`neurogrim init --description "<text>"`** (F8) — operator-supplied
  Brain description for `meta.description`. Replaces the generic
  "initialized via `neurogrim init` ..." boilerplate when bespoke
  framing is needed.
- **`neurogrim init --domain-describe "NAME=DESCRIPTION"`** (F10) —
  repeatable. Each entry becomes a `_todo_<name>` field on the
  domain's definition, capturing operator-supplied sensor authoring
  intent for when the sensor is later written.
- **`neurogrim domain new --sensor-intent "<text>"`** (F10) — same
  but for adding domains one-at-a-time after init. MCP `domain_new`
  tool has matching `sensor_intent: Option<String>`.

### Added — automatic transitive port allocation
- **`neurogrim federation register` allocator walks transitively** (F7).
  The v3.2.2-era port-8423 collision (job-hunt allocated to a port
  already used by python-starter — the ecosystem's grandchild) is no
  longer possible. The allocator reads each peer's registry from disk
  and considers the full transitive port set when picking the next
  free slot.

### Added — bundled skill
- **`cli-mode` ships in `init-skills/`** (F5) — the `neurogrim-onboarding`
  skill referenced `cli-mode` but it wasn't bundled, producing a
  Read-fail for any agent following the reference. v3.3 ships it
  in the abstract-project / code-project / mixed templates.

### Changed
- **`init` no longer logs two `.gitignore` updates** (F6) — when
  `--template` is set, the registry-phase awareness-only update is
  skipped because the template's gitignore-snippet covers the same
  entry. Single update operation, single log line.
- **`validate` surfaces autonomy-block counts** — adds `Autonomy: N
  levels, M action_types, K safety_invariants` to the summary so
  operators can see at a glance whether they have safety declarations.
- **Workspace `version` 3.2.2 → 3.3.0** across all 6 crates +
  `[workspace.dependencies]` synchronized.

### Background — what prompted this release

A fresh Claude Code session re-bootstrapped the job-hunt project
end-to-end via the published v3.2.2 CLI (`cargo install neurogrim-cli`).
The agent's adoption-feedback report surfaced six friction points
(F1–F6); the operator-side audit added four more (F7–F10) including
the F7 port collision. Per-finding rationale is in
`audit/job-hunt-rebootstrap-prompt-2026-04-28.md` and the
`adoption-feedback-2026-04-28.md` report at
`D:/job-hunt/archive/adoption-feedback-2026-04-28.md`.

## [3.2.2] - 2026-04-28

*Publish-prep release. No new features; closes the last `cargo publish`
blockers identified in the v3.2.1 audit.*

### Changed
- Workspace `version` bumped from `3.0.0` (stale through v3.1, v3.1.1,
  v3.2, v3.2.1) to `3.2.2`. Inter-workspace deps in
  `[workspace.dependencies]` synchronized.
- **Schema vendoring**: 5 `include_str!` references in `neurogrim-a2a`
  and `neurogrim-sensory` previously pointed at
  `../../../../../LSP-Brains/schemas/*.schema.json` (sibling-repo
  relative), which broke `cargo publish` since LSP-Brains isn't part of
  the published tarball. Schemas are now vendored into each consuming
  crate's `data/schemas/` directory; canonical source remains in the
  LSP-Brains repo, drift caught by existing schema-conformance tests.
  - `neurogrim-a2a/data/schemas/a2a-federated-pattern-v1.schema.json`
  - `neurogrim-sensory/data/schemas/hat-contract-v1.schema.json`
  - `neurogrim-sensory/data/schemas/invocation-ledger-v1.schema.json`
  - `neurogrim-sensory/data/schemas/pattern-aggregation-ledger-v1.schema.json`
  - `neurogrim-sensory/data/schemas/trust-budget-v1.schema.json`

### Added
- Crate-level rustdoc on `neurogrim-core/src/lib.rs` covering each
  public module's role + stability stance.

## [3.2.1] - 2026-04-28

*Closes the two real onboarding gaps from the v3.2 audit: MCP
exposure of the new commands + propagating the bundled
`neurogrim-onboarding` skill to existing federation Brains.*

### Added
- **Four new MCP tools** mirroring the v3.2 CLI commands so agents
  using `neurogrim serve` (the default tool-invocation surface) can
  reach onboarding entry points without bash:
  - `orient` — agent-friendly prose summary (= `agent --prose`)
  - `doctor` — config audit returning structured JSON findings
  - `explain` — bundled methodology primer (8 topics)
  - `domain_new` — domain scaffolder
- `neurogrim-onboarding` skill propagated byte-identical across all
  6 federation copies (canonical bundled source +
  ecosystem/NeuroGrim/LSP-Brains/python-starter/job-hunt).

### Changed
- **Architecture refactor**: shared logic moved from `neurogrim-cli`
  to `neurogrim-mcp` so MCP tools and CLI commands use a single
  source of truth — new modules `mcp::prose`, `mcp::doctor`,
  `mcp::explain`, `mcp::domain`. The 8 `data/explain/*.md` files
  relocated to `neurogrim-mcp/data/explain/` accordingly.
- `neurogrim-cli/src/output/prose.rs`, `commands/doctor.rs`,
  `commands/explain.rs`, `commands/domain.rs` are now thin clap +
  printing wrappers.

## [3.2.0] - 2026-04-28

*Agent Onboarding & Domain Authoring campaign. Three workstreams
(Phase A introspection, Phase B methodology primer, Phase C domain
scaffolder) close the entry-point gap for AI agents on first contact
with a NeuroGrim project.*

### Added — Phase A: Brain introspection
- **`neurogrim agent --prose [--plain]`**: agent-friendly prose
  orientation summary. 8 sections — Brain identity, current state,
  strongest signals, calls to action, available skills, available
  hats, federation peers, footer. All-advisory Brains render
  "Score: N/A (observe-only posture)" rather than a misleading 0/100.
- **`neurogrim doctor [--plain]`**: read-only configuration auditor
  distinct from `validate` (registry-shape only) and `health`/`score`
  (run scoring pipeline). Six check families: schema validate,
  domain-definitions alignment, principle_map alignment, CMDB path
  resolution, culture.yaml presence, federation port uniqueness.
  Exit 0/1/2 by severity; advisory-orphan vs weighted-orphan severity
  split caught NeuroGrim's own pre-existing `_todo_rust-health`
  placeholder as a warn (not error).

### Added — Phase B: Methodology primer
- **`neurogrim explain <topic>`**: 8 bundled topic files
  (methodology, domain, sensor, hat, scoring, federation, cli,
  culture). Loaded via `include_str!` at compile time; ~80 KB binary
  growth. With no topic, lists topics with one-line summaries. With
  `--version`, prints bundle metadata.
- **Bundled `neurogrim-onboarding` skill** in `init-skills/`. Every
  new project from `init --template` gets it automatically; routing
  frontmatter triggers on "what is this Brain", "where do I start",
  "I just entered this project", etc.
- `tests/methodology_drift.rs`: 5 integration checks verifying bundled
  topic files are well-formed (presence, version-header uniformity,
  substantive content, valid command references).
- `NeuroGrim/CLAUDE.md` gains a "Getting Oriented" section pointing
  at the four onboarding commands.

### Added — Phase C: Domain scaffolder
- **`neurogrim domain new <name>`**: scaffolder mirroring `skill new`
  UX. Mutates `brain-registry.json` atomically across 3 sections
  (`domain_weights`, `principle_map`, `domain_definitions`), generates
  a stub CMDB, and optionally scaffolds a Python sensor skeleton at
  `sensory/check_<name>.py`. Idempotent re-registration with `--force`.
- `--type stub|python` (default `stub`); `--type rust` is intentionally
  unsupported (contributor work, not adopter work).
- 7 subprocess integration tests covering stub path, python path,
  force, kebab-case validation, post-mutation `validate` + `agent --prose`.

## [3.1.1] - 2026-04-28

*Init automation. The 50-step manual sibling-Brain onboarding from
v3.1 B'1 collapses to a few CLI commands using bundled templates.*

### Added
- **`neurogrim init --template <kind>`**: full Brain-integration
  scaffolding beyond the registry. Three templates: `abstract-project`
  (no primary code), `code-project` (software project; default
  detection), `mixed`. Generates `culture.yaml`, stub CMDBs,
  bundled skills, PostToolUse hook script, `CLAUDE.md`, `.gitignore`
  extension.
- **`neurogrim skill new <name>`**: scaffolds a project-specific
  `SKILL.md` skeleton. kebab-case validated; idempotent with `--force`.
- **`neurogrim federation register --name <peer> --path <path>`**:
  adds a child Brain to local federation. Auto-allocates the next
  unused A2A port from 8421. `--read-only` for sibling-project peers
  (bumps registry schema_version 2 → 2.1 if needed).
- **Bundled artifacts** at compile time: 3 init templates with
  manifests, 6 general-purpose skills (hats, imagination-mode,
  north-star, rubber-duck, human-comms, write-skill), culture.yaml,
  hook script, narration templates.
- 6 subprocess integration tests covering the full bootstrap flow.

## [3.1.0] - 2026-04-28

*"Activate the Grammar" campaign. Five workstreams (A–E) take the
v3.0 structurally-complete grammar and put it to work.*

### Added — Workstream A: Sensor authoring
- **`rust-health` sensor** at `neurogrim-sensory/src/rust_health.rs`:
  static-only signals — Cargo.toml, Cargo.lock, MSRV, rustfmt,
  clippy config, cargo-deny, `[lints]`, CI integration.
- PostToolUse invocation-ledger hook enabled in all 4 federation
  Brains (was: only ecosystem). 30-day calibration window opens.

### Added — Workstream B: Sibling project as observed peer
- `read_only: true` flag on registry `config.children` entries
  (LSP-Brains schema additive v2 → v2.1, commit 9cb83cf).
- `check_culture_coherence.py` extended to discover the federation
  set dynamically (was: hardcoded 4-path list, commit 6be88da).
- `check_observed_peers.py` ecosystem-level sensor reads sibling
  scores via A2A.
- B'1 Pilot #1: job-hunt registered as the 5th Brain (read-only
  sibling, port 8424).

### Added — Workstream C: Hat-calibrated narration
- **`neurogrim narrate --hat <name>`**: 3–5 lines of
  hat-templated prose. 7 declared hats; deterministic templates
  (no LLM) loaded from bundled TOML.

### Added — Workstream D: Spec dogfooding
- 11 missing Appendix E glossary entries authored in
  `LSP-BRAINS-SPEC.md` (`glossary-freshness` score 37 → 84).

### Added — Workstream E: Federated patterns intelligence
- `cross_peer_co_occurrence` finding kind in
  `neurogrim-sensory/src/federated_patterns.rs`. Detection: ≥2
  distinct anonymized origins emitting semantically-similar feature
  vectors within a 7-day window.

## [3.0.0] - 2026-04-27

*Stable consolidated release. Closes the supply-chain campaign
(E-SC-0..E-SC-10) + the Brains-2.0 self-observability campaign
(E-B2-0..E-B2-8). Both master gates 11 + 12 in `BEFORE-PUBLIC-
RELEASE.md` are 🟢; remaining 🟡 gates are operator-controlled.
`cargo publish` is operator-decision per
`docs/publish-day-runbook.md`.*

The version jump from `0.1.0` (workspace `Cargo.toml` default) to
`3.0.0` reflects methodology maturity across stages S1–S10 + the
two post-S10 master-gate campaigns (supply-chain + self-observability).
The intermediate `3.0.0-rc.1` plan was paused 2026-04-24 to ship
the supply-chain master gate first; that plan's content is folded
into this stable release alongside the Brains-2.0 work that
followed.

### Added — Core implementation
- **Rust workspace** (`neurogrim/crates/*`): `neurogrim-core` (pure
  scoring, zero I/O), `neurogrim-sensory` (12 built-in sensor
  domains), `neurogrim-mcp` (MCP server + client), `neurogrim-a2a`
  (peer protocol), `neurogrim-cli` (binary entry point).
- **12 sensor domains**: `git-health`, `test-health`, `code-quality`,
  `deploy-readiness`, `security-standards`, `coherence`,
  `human-comms`, `secret-refs`, `docker-topology`, `agent-behavior`,
  `skill-coherence`, `capability-hygiene`.
- **Correlation engine** with condition-tree operators
  (comparison + branch) evaluated against domain variables.
- **Unified scoring** with per-domain weights + confidence model +
  floor constraints + non-linear aggregation. Trajectory intelligence
  (velocity / acceleration) from ledger history.
- **Dual tool-invocation modes**: MCP server (`neurogrim serve`,
  ~983 tokens at session start) and CLI-only (0 tokens; opt in via
  the `cli-mode` skill).

### Added — LSP Brains spec v2.5
- 15 normative sections + 7 appendices.
- Covers: Brain architecture, registry schema, CMDB envelope, scoring
  model, correlation engine, MCP + A2A protocols (§13), agent-behavior
  verification (§15), domain promotion path (§15.5).
- Companion `METHODOLOGY-EVOLUTION.md` with 14 discovery-log entries
  tracking how the spec got here.

### Added — Skill + hat system
- **20 plugin-format skills** (`.claude/skills/<name>/SKILL.md` with
  YAML frontmatter). `capability-hygiene` domain scores authoring
  quality against a 1,536-char description+when_to_use budget.
- **7 hats**: adversary, architect, incident-commander, rubber-duck,
  security-auditor, visionary, source-reader.
- **Culture substrate**: 5-value invariant
  (positivity / integrity / honesty / critical-but-kind / respect),
  byte-identical across peer Brains, enforced by `culture-coherence`
  at the ecosystem level.

### Added — Governance infrastructure
- **Axis 4 v1 invocation ledger**: PostToolUse hook captures every
  `Skill` tool invocation (name + timestamp only — privacy by
  design). `capability-hygiene` classifies skills as alive / dead /
  new against a 30-day grace period.
- **Gated domain promotion**: `abv-run promote` / `rollback` /
  `promotion-watch` with append-only `domain-promotion-ledger.jsonl`,
  three rebalance strategies (proportional / explicit / refuse), and
  `ABV_OPERATOR` guard. Stage 10 spec §15.5 normative.
- **Judge-integrity ledger**: red-sample calibration gate with
  triage CLI (`abv-run judge-integrity list | triage`).
- **Red-mode sweeps**: mock-bad-agent generation +
  13-sample / 6-scenario failure-mode library.

### Added — Peer + adoption topology
- **A2A peer protocol** (spec §13): agent card + envelope + task
  client/server. Fractal composition (parent↔child) and dual-brain
  (local↔external) topologies demonstrated across the four-Brain
  ecosystem.
- **Ecosystem Brain** (`.claude/`): six advisory domains
  (spec-impl-alignment, terminology-coherence, protocol-boundary,
  north-star-alignment, ecosystem-trajectory, culture-coherence).
- **Python starter template** (`NeuroGrim-python-starter/`): child
  Brain with 4 advisory domains, demonstrating the adoption pattern.

### Added — Experimental evidence base
- 432-row `comparison-ledger.jsonl` from the 2026-04-22/23
  brain-vs-control experiment (Phases 1-3, plus 22-task held-out
  set). All pre-registered; falsification criteria locked before
  analysis; kill decisions honored. Reports, ledgers, and
  post-mortem at `.claude/experiments/brain-vs-control/`.
- **Evidence + Hypothesis posture** (ROADMAP): longitudinal value
  is the primary hypothesis; single-turn benchmarks are bounded
  instruments. METHODOLOGY-EVOLUTION §14 absorbs this honestly.

### Added — Adoption surface
- `docs/getting-started.md`: ~20-minute path from clone to working
  Brain.
- `examples/hello-brain/`: minimal standalone demo.
- Ecosystem + NeuroGrim + LSP-Brains `LICENSE` files (MIT).
- Release notes + publish-day runbook + prepublish-check script.

### Added — Supply-chain master gate (E-SC-0..E-SC-10, 2026-04-26)

- **Three-layer SCA awareness** across Rust + Python + Node ecosystems:
  Layer 1 mechanical SCA (native-Rust, no scanner-binary shell-outs);
  Layer 2 deep-signal vigilance (7 sub-sensors: typosquat,
  publish-cadence, maintainer-delta, transitive-surface, signature-gap,
  binary-reproducibility, exfil-indicator); Layer 3 agent-assisted
  human review framework (decision ledger + review tickets +
  auto-create bridge + `supply-chain-auditor` hat).
- **Spec normative**: LSP-Brains v2.6 §16 + METH-EV §15 + 2 new schemas
  (decision-ledger-v1, review-ticket-v1) + A2A enum extensions for
  `supply-chain-signal`.
- **Calibration framework**: fixture library + `sca-calibrate` CLI +
  `--check-promotion-ready` gate. v1 calibration:
  pass-with-sample-size-warning across all three layers;
  promotion-not-ready (gaps documented).
- **`prepublish-check.sh`** extended with strict-with-bypass for L2 +
  L3 + LiteLLM-equivalent fresh-OSV-rerun.
- **`publish-day-runbook.md`** documents the supply-chain rollback
  window between tag and publish.
- **Master gate 11** in `BEFORE-PUBLIC-RELEASE.md` 🟢.

### Added — Brains-2.0 self-observability master gate (E-B2-0..E-B2-8, 2026-04-27)

- **E-B2-1 Confidence as first-class envelope field** (spec §3.8) —
  numeric integer 0–100 at protocol; categorical (low/medium/high) at
  UI only.
- **E-B2-2 Self-coherence + domain-calibration ledgers** (spec §17) —
  one ledger per domain family at
  `.claude/brain/<domain>-calibration-ledger.jsonl`.
- **E-B2-3 Hat-as-formal-contract** (spec §5.4.1) — closed-set
  vocabulary + new `hat-contract-v1.schema.json` + per-hat
  frontmatter migration. Static (file audit) at v1; runtime checks
  deferred to v2 (BACKLOG B-23).
- **E-B2-4 Trust-budget primitive** (spec §16.8) — per-Brain
  `trust-budget.toml` declares allowed crates / shell-outs / external
  services. Soft (advisory) at v1; hard gates deferred to v2.
- **E-B2-5 METH-EV §16 multi-round assessment cadence** (METH-EV §16) —
  strict bar → surgical bar → diminishing-returns + Phase 1.5 escape
  hatch. RECOMMENDED for pre-release / epic-close-out contexts.
- **E-B2-6 Operator-calibration domain** (spec §17.12) — extends
  invocation-ledger schema with additive `disposition` field
  (accept/reject/modify; no transcript content). Aggregation-only
  export.
- **E-B2-7 Federated patterns A2A** (spec §16.6.1) — new
  `federated-pattern` A2A message type + `pattern-aggregation-ledger.jsonl`.
  Bidirectional opt-in posture; closed-set numeric-only feature
  vector; recursion guard at wire + source level; per-peer rate
  limit; aggregation-only export.
- **E-B2-8 Dogfooding + spec v3.0 stability marker** — all 4 Brains
  declare the 4 new advisory domains at weight 0.0; CMDBs present +
  schema-valid; cross-Brain federated-pattern integration test
  compiles + passes; hat-contract migration applied to LSP-Brains
  (2 hats: spec-editor, rubber-duck) + python-starter (2 hats:
  adopter, rubber-duck) extending NeuroGrim + ecosystem (8 hats each);
  `prepublish-check.sh` extended with strict gate-12 checks
  (CMDB-presence + advisory-weight invariant + cross-Brain integration).
- **Spec promoted v2.6 → v3.0** progressively (v2.7→v2.12 → v3.0
  stability marker). v3.0 = additive over v2.x; deprecation track
  deferred to v4.0 (no symbols deprecated, removed, or withdrawn).
- **Charter Amendment 2026-04-27** reframes the ≥30-day self-coherence
  + ≥50 operator-calibration record metrics from "before v3.0" to
  post-publish observation feeding a v3.1 calibration-report gate
  (mirrors gate-11 supply-chain "pass-with-sample-size-warning"
  precedent). See `audit/BRAINS-2-0-CHARTER.md` Charter Amendment +
  `audit/BRAINS-2-0-RETROSPECTIVE-2026-04-27.md`.
- **Master gate 12** in `BEFORE-PUBLIC-RELEASE.md` 🟢.

### Changed
- **Workspace version** `0.1.0` (default) → `3.0.0` final (intra-workspace
  dep pins also bumped from `3.0.0-rc.1` to `3.0.0`).
- **Spec header** v2.12 → v3.0 (stability marker; `Status: Active` →
  `Status: Stable v3.0`).
- **Top-level pre-release status** in `BEFORE-PUBLIC-RELEASE.md` 🔴 → 🟢
  (both master gates closed; remaining 🟡 gates are operator-controlled).

### Calibration window
- v3.0 ships the structural surface for the seven Brains-2.0 primitives
  without 30-day self-coherence + 50 operator-calibration records (per
  Charter Amendment 2026-04-27). The post-publish observation window
  feeds a v3.1 calibration-report gate. v3.0.x bug-fix releases may
  flow during the window without re-opening the master gate; v3.1.0
  ships when the calibration-report gate closes.

### Known open gates (operator-controlled)
See `BEFORE-PUBLIC-RELEASE.md` for the full status; short form:
- 🟡 Legal / trademark formal clearance.
- 🟡 Per-crate README + `cargo package --list` inspection.
- 🟡 CONTRIBUTING + per-crate rustdoc.
- 🟡 CI matrix enablement.
- ⚪ **PyPI publish — no current plan.** The Python SDK is
  dogfood-only per the 2026-04-24 Python SDK reframe. BACKLOG B-20
  tracks the dormant roadmap item; source install via `pip install
  -e sdk-python/` is the supported path for adopters who need
  Python. See [`docs/sdk.md`](docs/sdk.md) for the canonical Rust
  SDK story.

### Known deferred to post-publish
- **S5-TP-3** (team outside LaaS adopts the framework): re-framed as
  a post-publication milestone rather than a release blocker. v3.0.0
  ships the adoption surface; adopter-found is a separate track.
- **S10-DP-4** (agent-behavior weight flip 0.0 → 0.05): operator-
  gated on calibration + red-mode audit. Mechanism complete; flip
  ships when the operator runs the audit.
- **S7-ABV-6** worked-example first real-credential run: illustrative
  `+18` delta documented; ships with that caveat.
- **B-14 through B-19** (CANDIDATE BACKLOG items): dispatch rule
  generalization, content freshness, L2 synthesis, rubric
  sensitivity, longitudinal artifacts — all tracked, none committed.

### Known not in this release
- Python SDK on PyPI (gate 7; package-name reserved but not published).
- S6-DB-6 Python SDK A2A helper (stretch-only).
- Any claim that single-turn experiments prove the Brain's
  longitudinal value (see METH-EV §14 on instrument bounds).

---

## Release-note links

- Full release notes for this version: `docs/release-notes/v3.0.0.md`.
- Publish-day runbook: `docs/publish-day-runbook.md`.
- Pre-publish status tracker: `BEFORE-PUBLIC-RELEASE.md`.
- Spec changelog (per-version normative diff): `D:/Brains/LSP-Brains/spec/LSP-BRAINS-SPEC.md` § Changelog.
- Methodology evolution log (per-insight discovery history): `D:/Brains/LSP-Brains/spec/METHODOLOGY-EVOLUTION.md`.
