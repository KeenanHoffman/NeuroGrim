# V5-FOUND-1 Diagnostic Monitor — Implementation Plan

**Epic:** `roadmap/epics/v5-foundation.md` § V5-FOUND-1
**Effort estimate (epic):** L, ~5–7 days
**v5 entry:** pinned 2026-05-01; Theme A active
**Methodology:** plan-critic before implementation per `v5-roadmap.md` final note

## Context

The v5 master roadmap requires a measured baseline before Theme B's modular conversions land — V5-MOD-1's adversary BLOCKING gate ("scoring round-trip latency unchanged within 5% of S15 baseline") cannot be evaluated without instrumentation that doesn't exist today. V5-FOUND-1 is the work that creates that instrumentation: a tracing-based diagnostics ledger plus two CLI commands (`neurogrim diag report` for human reading, `neurogrim diag synthesize` for agent synthesis with hard guardrails against prose-only output).

Because v5 entry was pinned pre-S15-ship (see gate-lift commit), the "S15 baseline" V5-FOUND-1 captures references the **current main-branch state at baseline-capture time**, not a post-S15-ship state. If S15 ships scoring-path-affecting changes before V5-MOD-1 runs, the baseline must be re-captured.

## Architectural anchors (extending, not inventing)

| Anchor | What we reuse |
|---|---|
| [crates/neurogrim-cli/src/commands/disposition.rs](neurogrim/crates/neurogrim-cli/src/commands/disposition.rs) | Append-only writer pattern: `OpenOptions::create(true).append(true)`, `\n`-terminated JSON, POSIX `O_APPEND` atomicity reasoning, recursion-guard precedent, **explicit privacy contract** (no free-text fields). |
| [crates/neurogrim-sensory/data/schemas/invocation-ledger-v1.schema.json](neurogrim/crates/neurogrim-sensory/data/schemas/invocation-ledger-v1.schema.json) | Schema location convention: `data/schemas/<name>-v1.schema.json`. `additionalProperties: false` + closed-set enums as the structural privacy floor. |
| Workspace `tracing-subscriber` (181 occurrences) | Already a dep — we add a `Layer`, not a new dep. |
| Per-command `tracing_subscriber::fmt().try_init()` (a2a_discover.rs:15, a2a_invoke.rs:53, a2a_serve.rs:175) | **Plan-critic finding: incompatible with bolting on a Layer.** `try_init()` installs a global subscriber; subsequent calls silent-fail; you cannot add a Layer afterwards. **Phase 0 (centralized init) is the prerequisite that makes the rest of this plan work.** |
| `.gitignore` `.claude/brain/invocation-ledger.jsonl` etc. | Diagnostics ledger gets identical gitignore treatment — per-deployment, never committed. |
| `roadmap/data/b09-bench-<date>.json`, `b10-phase1-<date>.json` (existing pattern) | Baseline JSON files go in `roadmap/data/` with dated filenames. |

**Critical methodological discipline carried over from disposition.rs:**

- `additionalProperties: false` on the schema — entries with unknown fields fail validation.
- Privacy contract: NO free-text fields anywhere in the schema. Names + durations + counts + IDs only.
- Closed-set enums for any taxonomic field (event_kind, etc.).
- Append-only writer; corrections come as new rows, never mutations.

## Phases (incremental delivery)

Each phase is independently shippable and unblocks the next. Iteration boundaries are explicit so we can ship after each.

### Phase 0 — Centralize tracing-subscriber init (Day 1, ~0.5–1 day) — **PREREQUISITE**

**Why this exists:** plan-critic surfaced that the codebase's current per-command `tracing_subscriber::fmt().try_init()` pattern is **incompatible** with bolting on a custom `Layer`. `try_init()` installs a global subscriber; you cannot attach a Layer afterwards. Phase 1's writer can stand alone, but Phase 2's Layer requires a single centralized init that builds the full Registry chain. This Phase 0 makes that prerequisite explicit and self-contained — without it, Phase 2 silently does nothing.

**Files (new):**
- `neurogrim/crates/neurogrim-cli/src/tracing_init.rs` — defines `setup_tracing(opts: TracingOpts)` that builds `Registry::default().with(fmt_layer).with(...)` and calls `.init()` exactly once. `TracingOpts` carries `enable_diag: bool` (off by default; set when `NEUROGRIM_DIAG=1` or `--diag` is passed).

**Files (modified):**
- `neurogrim/crates/neurogrim-cli/src/main.rs` — call `setup_tracing(opts)` once near the top of `main()`, before subcommand dispatch.
- `neurogrim/crates/neurogrim-cli/src/commands/a2a_discover.rs` — remove the `tracing_subscriber::fmt()...try_init()` block at line 15.
- `neurogrim/crates/neurogrim-cli/src/commands/a2a_invoke.rs` — remove the equivalent at line 53.
- `neurogrim/crates/neurogrim-cli/src/commands/a2a_serve.rs` — remove the equivalent at line 175.
- Any other site `grep -r "tracing_subscriber::fmt" crates/neurogrim-cli/src/` reveals.

**Behavioral guarantee:** the centralized init produces *the same fmt output* the per-command sites used to produce when `NEUROGRIM_DIAG` is unset — i.e., this refactor is a **no-op at the user-visible level** until Phase 2 attaches the diag Layer.

**Tests:**
- `cargo test --workspace --all-targets` green after the refactor (no behavioral regression).
- Manual smoke: `RUST_LOG=info neurogrim a2a-discover ...` still emits the same info-level lines as before.
- A new unit test in `tracing_init.rs` confirms `setup_tracing` is idempotent (calling twice does not double-install or panic — uses `try_init` internally with a `OnceCell` guard).

**Ship criterion:** `cargo test` green; manual smoke confirms equivalent log output; no per-command `tracing_subscriber::fmt().try_init()` calls remain in the CLI crate.

**Rollback:** the refactor is mechanical — restore the three `try_init()` calls and delete `tracing_init.rs` if anything goes sideways. No data migration involved.

### Phase 1 — Schema + Ledger Writer (Day 1–2, ~1 day)

**Goal:** Define the diagnostics event schema and a writer that any subsystem can emit through. No tracing yet — direct emit API only.

**Files (new):**
- `neurogrim/crates/neurogrim-cli/data/schemas/diagnostics-ledger-v1.schema.json`
- `neurogrim/crates/neurogrim-cli/src/lib/diagnostics_ledger.rs` (or `.../neurogrim-core/src/diagnostics.rs` — TBD by where the writer is reused)

**Schema sketch (closed-set, additionalProperties: false):**
```json
{
  "schema_version": 1,
  "event_id": "uuid-v4",
  "ts_start": "ISO8601",
  "duration_ms": 12345,
  "kind": "build|test|mcp_dispatch|a2a_post|a2a_sse|scoring|dashboard_route",
  "name": "<closed string per kind, e.g. score_pipeline.run>",
  "outcome": "ok|err|timeout|cancelled",
  "depth": 0,
  "parent_event_id": "uuid-v4 | null",
  "extras": { "<closed-per-kind keys, all numeric or enum>": "..." }
}
```

**Privacy floor (structural, like disposition.rs):**
- No `prompt`, `args`, `payload`, `text`, `body`, `note`, `comment`, `reason` keys anywhere in the schema or any extras object.
- The schema validator (and writer) reject unknown keys.
- `extras` for each kind is a closed set documented in the schema (e.g., `mcp_dispatch.extras = {tool_name, success}` only).

**Writer API:**
```rust
pub fn emit(entry: &DiagnosticsEntry, project_root: &Path) -> Result<()>
```
- Opens `<project_root>/.claude/brain/diagnostics.jsonl` with `OpenOptions::create(true).append(true)`.
- Single `\n`-terminated JSON line per call.
- Same atomicity reasoning as disposition.rs (≤PIPE_BUF on POSIX, FILE_APPEND_DATA on Windows).

**Tests (≥4 negative paths, per epic done-when):**
1. Happy path: emit one entry, read back the line, parse, equality check.
2. Privacy filter: writer rejects an entry whose `extras` contains a forbidden key.
3. Schema-version mismatch: reader rejects a ledger row with an unknown `schema_version`.
4. Malformed line skip: reader skips a corrupted line and continues.
5. Concurrent writers: two threads each emit 100 entries; resulting ledger has 200 well-formed lines, no interleaving.
6. Large entry near PIPE_BUF: ensure entry stays under bound.

**.gitignore:** add `.claude/brain/diagnostics.jsonl` to NeuroGrim's `.gitignore` (same treatment as invocation-ledger).

**Ship criterion:** unit tests green; `cargo test --workspace --all-targets` green; one manually-emitted entry visible in the ledger file.

### Phase 2 — Tracing Layer (Day 3–4, ~1.5 days)

**Goal:** A `tracing-subscriber` `Layer` that turns spans into ledger entries, opt-in via env var. **Depends on Phase 0** (centralized init).

**File (new):**
- `neurogrim/crates/neurogrim-cli/src/lib/diagnostics_layer.rs` (or core)

**Files (modified):**
- `neurogrim/crates/neurogrim-cli/src/tracing_init.rs` (Phase 0 file) — extend `setup_tracing` to attach the diag Layer when `opts.enable_diag` is true.

**Behavior:**
- Implements `tracing_subscriber::Layer`.
- On span enter: record start timestamp + parent ID.
- On span close: compute `duration_ms`, compose `DiagnosticsEntry`, emit via Phase-1 writer.
- Span name → `(kind, name)` mapping is a closed table (e.g., span name `score.pipeline.run` maps to `kind=scoring, name=score_pipeline.run`); unknown names are dropped.
- Outcome captured from a span field `outcome` set by the instrumented code.
- Production default disabled — Layer attaches in `setup_tracing` only when `opts.enable_diag` is true (set from `NEUROGRIM_DIAG=1` env var or top-level `--diag` flag). Zero-cost when off (the Layer is not in the Subscriber chain at all).

**Tests:**
- Layer attached, span enters/exits, ledger line appears.
- Layer detached, span enters/exits, no ledger line.
- Span depth tracked correctly across nested spans.
- Span with unmapped name dropped silently (warn-level log).

**Ship criterion:** above tests green; manual smoke: `NEUROGRIM_DIAG=1 neurogrim score --project-root <test fixture>` produces ≥1 entry of `kind=scoring`.

### Phase 3 — Span Instrumentation (Day 4–6, ~2 days)

**Goal:** Instrument the surfaces named in the epic done-when. **Scoring pipeline first** — V5-MOD-1's perf-gate baseline depends on it. File anchors corrected per plan-critic findings.

**Order of attack** (each adds ~1–4 spans, additive only):

1. **Scoring pipeline** — `neurogrim-mcp/src/server.rs:132` `async fn run_scoring(...)`. *Plan-critic correction: this is the natural single-span boundary; it transitively calls into `neurogrim-core` but the end-to-end run is in `neurogrim-mcp`, not `neurogrim-core/registry.rs`.* Span: `score.pipeline.run` with extras `{domains_count, score, confidence}` (numeric only, no text).

2. **`neurogrim test`** — `neurogrim-cli/src/commands/test.rs:132` `pub async fn run(args: Args)`. Span: `test.run` with extras `{test_count, fail_count, ignored_count}` wraps the whole command. **Cargo invocation is folded into this surface** — it's a child span `cargo.invoke` (extras `{cmd, exit_code}`) wrapping the `Command::new("cargo")...output()` call at `test.rs:188` (the same subprocess boundary). Folding eliminates the spurious "6th surface" in the original plan; cargo timing is captured as a child of the test span. Integrates with the existing test-failure ledger additively.

3. **MCP tool dispatch** — `neurogrim-mcp/src/client.rs` `async fn invoke_single_server(name, config, ...)` (around line 41). *Plan-critic clarification: scoped to per-server granularity for V5-FOUND-1.* Span: `mcp.sensory.<server_name>` wraps the per-server connection (clean boundary at line 69) and the per-tool loop within (lines 78–114). Extras: `{server_name, tool_count, fail_count}`. **Per-tool granularity (`mcp.tool.<tool_name>`) deferred to v5.5** — V5-MOD-1's perf-gate doesn't depend on per-tool timing, and per-server is sufficient for diagnostic-ledger purposes. **Tool args + responses NOT captured** — privacy floor.

4. **A2A POST/SSE** — `neurogrim-a2a/src/server.rs:149–200` (axum router). Spans installed as **axum middleware** (the `Next` hook, line ~35), so all routes get instrumentation without per-handler edits. Spans: `a2a.post`, `a2a.sse_event`. Extras: `{peer_id_hash, status_code}`. **Payload NOT captured.**

5. **Dashboard route handlers** — `neurogrim-dashboard/src/routes.rs:118` `with_host_context()` is the convergence point through which all dashboard scoring operations flow. Span: `dashboard.route` attached either as global axum middleware (preferred) or by wrapping `with_host_context`. Extras: `{route_name, status_code}`. **Request body NOT captured.**

**Per surface:** add 1 unit test that exercises the span path and confirms an entry of the expected `kind` + extras lands in the ledger.

**Ship criterion (this phase):** all five surfaces emit (cargo timing is a child of #2, not a sixth); integration test runs `neurogrim test --diag` end-to-end and observes events of `{scoring, test, cargo, mcp_dispatch, a2a_post|sse, dashboard_route}` kinds.

### Phase 4 — CLI Command: `neurogrim diag report` only (Day 6, ~1 day)

> **Fork decision recorded 2026-05-02:** `neurogrim diag synthesize` is **deferred to V5-FOUND-1.1** (a discrete follow-on epic). V5-FOUND-1 ships `report` only. Reasoning: plan-critic surfaced that no Rust-side LLM pathway exists today (no `anthropic` crate, no runtime `reqwest`, `neurogrim-secrets` is for storing operator credentials at rest only, `claude-proxy` is for containerized agents with pre-issued scope tokens). Building that pathway is +2–3 days of dep-discipline-sensitive work that decouples cleanly from V5-FOUND-1's core value (instrumentation + baseline + operator-readable report). The diagnostics ledger plus `diag report` together still deliver V5-MOD-1's baseline-capture need; `synthesize` is the agent-experience layer that can land independently.

**Goal:** `neurogrim diag report` — human-readable summary of the diagnostics ledger.

**Files (new):**
- `neurogrim/crates/neurogrim-cli/src/commands/diag.rs`

**File (modify):**
- `neurogrim/crates/neurogrim-cli/src/commands/mod.rs` — add `pub mod diag;`
- `neurogrim/crates/neurogrim-cli/src/main.rs` — wire `Diag(diag::Args)` subcommand into the top-level enum.

**`neurogrim diag report` behavior:**
- Reads `.claude/brain/diagnostics.jsonl`.
- Computes top-N (default 10) slow operations by `name`, with `count`, `p50_ms`, `p95_ms`, `p99_ms`, `max_ms`.
- Default output: a small table. JSON output via `--json`.
- Filters: `--kind <enum>`, `--since <iso8601>`, `--name <prefix>`.

**Tests:**
- `diag report` parses fixtures and computes correct percentiles.
- `diag report --json` produces well-formed JSON.
- Empty ledger handled gracefully (prints "no events"; non-zero exit only on file error).
- Malformed line skipping observed in real-world `tail`-style usage (mid-line truncation).

**Ship criterion:** `diag report` works against a fixture ledger and against a real ledger captured in Phase 5 baseline runs.

**Subcommand surface placeholder for V5-FOUND-1.1:** `neurogrim diag synthesize` is reserved as a subcommand name; the dispatcher in `commands/diag.rs` includes it as a stub that returns a `not yet implemented; see V5-FOUND-1.1` error. Reserving the name now prevents future name collision with sibling commands and signals intent in `--help` output.

### Phase 5 — Baseline Capture + V5-FOUND-1 Close (Day 6–7, ~0.5–1 day)

**Goal:** Capture the V5-MOD-1 reference baseline; close the epic.

**Steps:**
1. Run a representative scoring round-trip with `NEUROGRIM_DIAG=1`: `neurogrim score --project-root <a fixture project> --json`.
2. Run it 30+ times (warm runs) to get a stable distribution.
3. Extract `kind=scoring, name=score_pipeline.run` events from the ledger.
4. Compute p50/p95/p99/max and persist to `roadmap/data/v5-scoring-baseline-2026-05-<dd>.json` with metadata: hardware label, git SHA at capture time, run count, distribution summary, **explicit note that this is pre-S15-ship**.
5. Cross-reference the baseline file from `v5-foundation.md` V5-FOUND-1 done-when checkbox and `v5-modular-conversions.md` V5-MOD-1 perf-gate.
6. Mark V5-FOUND-1 status: `Planned → Complete`. Update epic file done-when checkboxes.

**Ship criterion:** baseline JSON exists; epic file marked Complete; integration test green.

## Files inventory

### New
- `neurogrim/crates/neurogrim-cli/src/tracing_init.rs` (Phase 0)
- `neurogrim/crates/neurogrim-cli/data/schemas/diagnostics-ledger-v1.schema.json` (Phase 1)
- `neurogrim/crates/neurogrim-cli/src/lib/diagnostics_ledger.rs` (Phase 1; core or cli — TBD by where the writer is reused; defaulting to cli for v1)
- `neurogrim/crates/neurogrim-cli/src/lib/diagnostics_layer.rs` (Phase 2)
- `neurogrim/crates/neurogrim-cli/src/commands/diag.rs` (Phase 4)
- `roadmap/data/v5-scoring-baseline-2026-05-<dd>.json` (Phase 5 output)

### Modified
- `neurogrim/crates/neurogrim-cli/src/main.rs` (Phase 0: call `setup_tracing()` once; Phase 4: wire `Diag` subcommand)
- `neurogrim/crates/neurogrim-cli/src/commands/a2a_discover.rs` (Phase 0: remove per-command `try_init()` at line 15)
- `neurogrim/crates/neurogrim-cli/src/commands/a2a_invoke.rs` (Phase 0: remove at line 53)
- `neurogrim/crates/neurogrim-cli/src/commands/a2a_serve.rs` (Phase 0: remove at line 175)
- `neurogrim/crates/neurogrim-cli/src/commands/mod.rs` (Phase 4: register `diag`)
- `neurogrim/crates/neurogrim-cli/src/lib.rs` (re-export new modules if needed)
- `neurogrim/crates/neurogrim-mcp/src/server.rs` (Phase 3 step 1: span on `run_scoring()` at line 132 — corrected from `neurogrim-core/registry.rs` per plan-critic)
- `neurogrim/crates/neurogrim-cli/src/commands/test.rs` (Phase 3 step 2: parent span at line 132 + child `cargo.invoke` span at line 188)
- `neurogrim/crates/neurogrim-mcp/src/client.rs` (Phase 3 step 3: per-server span on `invoke_single_server` ~line 41–114)
- `neurogrim/crates/neurogrim-a2a/src/server.rs` (Phase 3 step 4: axum middleware spans on POST/SSE)
- `neurogrim/crates/neurogrim-dashboard/src/routes.rs` (Phase 3 step 5: middleware OR wrap `with_host_context` at line 118)
- `NeuroGrim/.gitignore` (Phase 1: add `.claude/brain/diagnostics.jsonl` and (if synthesize ships) `.claude/brain/diag-synthesis-history.jsonl`)
- `roadmap/epics/v5-foundation.md` (Phase 5: status → Complete, done-when checkboxes)

**Phase 4 may add files/deps** depending on fork decision (see next section).

## Risks (from epic + new ones surfaced by this plan)

🟡 **Tracing instrumentation overhead.** Mitigation: opt-in Layer; production default disabled. Phase 2 ships a benchmark comparing `cargo bench` with and without the Layer.

🟡 **Pre-S15-ship baseline staleness.** Per gate-lift commit: if S15 ships scoring-path-affecting changes before V5-MOD-1 runs, the Phase-5 baseline must be re-captured. **New mitigation:** the baseline JSON includes `recapture_required_if_changes_to: [list of files/symbols]`; V5-MOD-1's perf-gate logic checks the list and fails-loud if any are dirty since baseline.

🟡 **Schema evolution.** v1 schema is closed-set; future `kind`s require a v2 schema bump. Mitigation: schema_version = 1 hardcoded; readers reject unknowns; `kind` enum is documented in the schema with the bump procedure.

🟡 **Agent-synthesize drift.** Mitigation: validator rejects prose-only output at write time. **New mitigation:** validator ALSO rejects target_value_ms ≥ baseline_value_ms (a "go faster" recommendation must propose a faster target, not a slower or equal one).

🔵 **Suggestion forwarded to v5.5:** "diag-readiness" advisory domain (epic risk #5) — reads diagnostics.jsonl, emits findings if any common operation has zero events in the last N runs (instrumentation regression). Cheap once Theme A ships. NOT in V5-FOUND-1 scope.

## Iteration boundaries (so we can ship pieces, not one big lump)

| Iter | Phases | Shippable? | Rough duration |
|------|--------|------------|----------------|
| 0 | Phase 0 (centralize tracing init) | Yes — refactor only, no behavior change | ~0.5–1 day |
| 1 | Phase 1 (schema + writer + tests) | Yes — writer is independently usable | ~1 day |
| 2 | Phase 2 (Layer) + Phase 3 step 1 (scoring) | Yes — V5-MOD-1 baseline source ready | ~2 days |
| 3 | Phase 3 steps 2–5 (test+cargo, MCP per-server, A2A, dashboard) | Yes — full ledger coverage | ~1.5 days |
| 4 | Phase 4 (`diag report` only — `synthesize` deferred to V5-FOUND-1.1) | Yes — operator-visible reports | ~1 day |
| 5 | Phase 5 (baseline capture + close) | Yes — V5-FOUND-1 closed | ~0.5 day |

Total: ~6.5 days, within epic L estimate (5–7 days).

## Verification (end-to-end, run after Iter 5)

1. `cargo test --workspace --all-targets` green.
2. `NEUROGRIM_DIAG=1 neurogrim test --workspace`: ledger has events of all six kinds (scoring, test, mcp_dispatch, a2a_post|sse, dashboard_route, cargo).
3. `neurogrim diag report --kind scoring --since 1h`: shows at least one entry of `name=score_pipeline.run` with realistic durations.
4. `neurogrim diag synthesize` against a fixture ledger: writes a synthesis row with baseline+target; rejects a mocked prose-only response with a clear error.
5. Baseline JSON at `roadmap/data/v5-scoring-baseline-2026-05-<dd>.json` exists with metadata.
6. `neurogrim doctor`: no new warnings introduced.
7. Privacy floor: grep the ledger for any forbidden key (`prompt|args|payload|body|note`); zero hits.

## What this plan does NOT do

- Does **not** ship `neurogrim diag synthesize` — deferred to **V5-FOUND-1.1** (a discrete follow-on; needs a Rust-side LLM client which doesn't exist today; tracked in epic file).
- Does **not** ship `--instrument-coverage` build mode — that's V5-FOUND-3.
- Does **not** define the `TestRunner` trait — that's V5-FOUND-4.
- Does **not** promote diagnostic synthesis to a Brain domain — that's BACKLOG B-45 (v6 horizon).
- Does **not** add a "diag-readiness" Brain domain — forwarded as v5.5 suggestion.
- Does **not** touch Theme B (modular conversions) concerns; this is foundation only.
- Does **not** ship cargo-nextest adoption (V5-FOUND-2) or sccache config — those are independent.
- Does **not** add `reqwest`, `anthropic`, or any LLM-client dependency — see V5-FOUND-1.1.

## V5-FOUND-1.1 — Diagnostic Synthesis (deferred follow-on)

**Status:** Planned (deferred 2026-05-02 from V5-FOUND-1)
**Effort estimate:** S–M (~2–3 days)
**Depends on:** V5-FOUND-1 (ledger + report must exist)
**Trigger to start:** operator demand for agent-driven synthesis OR Theme B's V5-MOD-1 needs it for perf-regression triage.

**What:** Implement `neurogrim diag synthesize` — agent-driven analysis of the diagnostics ledger with structural guardrails against prose-only output.

**Open architectural decision** (to be revisited at V5-FOUND-1.1 start, not now):
- Add `reqwest` runtime dep + hand-roll Anthropic POST, OR
- Wire through `claude-proxy` (requires running proxy + scope-token plumbing), OR
- Adopt a Rust Anthropic SDK if a maintained one exists by trigger time.

**Carried-forward design** (preserved from V5-FOUND-1 plan-critic discipline):
- Output validator requires `{baseline_name, baseline_value_ms, target_value_ms, recommended_actions[]}` with each action carrying `{measurement_to_verify, threshold}`.
- `target_value_ms ≥ baseline_value_ms` rejected at write time (a "go faster" recommendation must propose a faster target).
- Prose-only output rejected at write time.
- Synthesis row to ledger uses `kind=diag_synthesis` (numeric/closed-set extras only); textual rationale to sibling `.claude/brain/diag-synthesis-history.jsonl`.

**Cross-references:**
- Epic file: needs an entry added under `roadmap/epics/v5-foundation.md` § V5-FOUND-1.1 at V5-FOUND-1 close-out (Phase 5).
- Done-when item carried over: "neurogrim diag synthesize invokes a bounded-prompt agent that MUST cite measured baseline + target".

## Cross-references

- Epic: `roadmap/epics/v5-foundation.md` § V5-FOUND-1
- Master roadmap: `roadmap/v5-roadmap.md` (entry pinned 2026-05-01)
- Pre-plan source: `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`
- Pattern-source files: `crates/neurogrim-cli/src/commands/disposition.rs`, `crates/neurogrim-sensory/data/schemas/invocation-ledger-v1.schema.json`
- Downstream consumer: `roadmap/epics/v5-modular-conversions.md` § V5-MOD-1 (5% perf-gate)
