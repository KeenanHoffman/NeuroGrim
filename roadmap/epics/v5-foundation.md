---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: v5 Foundation — Diagnostics + Test Speed (Theme A)

**Theme:** A
**Release:** v5 (entry **pinned 2026-05-01**; concurrent with in-flight v4.x S15/S16 work per operator pin — see `v5-roadmap.md` §"v5 Entry Decision Tracker")
**Status:** PLANNED (drafted 2026-05-01)
**Priority:** Foundation — must ship before Theme B because modular-conversion work needs measurements
**Goal:** Land tracing-based diagnostics, cargo-nextest adoption, sccache, per-test coverage as opt-in build mode, and a minimal `TestRunner` trait. After Theme A: dev loop is fast, agent can synthesize bottlenecks with measured baselines + targets, and we have data to validate Theme B's modularity claims.

**Depends on:**
- S12-G-1 (publish-gates ledger pattern — extending JSONL ledger conventions)
- S15 (Command Post UI shipped — v5 entry pin)

**Blocks:**
- Theme B (modular conversions need diagnostics + fast tests)

**Master roadmap:** `roadmap/v5-roadmap.md`
**Pre-plan source:** `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`

> **✅ ENTRY PINNED 2026-05-01 — Theme A active, V5-FOUND-1 begins concurrently with in-flight v4.x (S15 Command Post UI / S16 Plumbing).** The operator waived the original pre-plan default ("concurrent v4.x + v5 work is not pursued") via the third re-evaluation trigger in `v5-roadmap.md` §"v5 Entry Decision Tracker". V5-FOUND-1 is the safest concurrent starter because it adds tracing instrumentation (additive — does not modify scoring or UI surfaces). The "S15 scoring round-trip baseline" Done-When item now references the **current main-branch state at baseline-capture time** (not post-S15-ship state); V5-MOD-1's 5%-perf gate inherits this revised reference point. If S15 ships scoring-path-affecting changes before V5-MOD-1 runs, the baseline must be re-captured at that point.

---

## Theme A Is Done When

- [x] tracing spans + `.claude/brain/diagnostics.jsonl` ledger emit on cargo build, neurogrim test, MCP tool dispatch, A2A POST/SSE, scoring pipeline, dashboard route handlers (V5-FOUND-1 Phase 3, 2026-05-02)
- [x] `neurogrim diag report` summarizes top-N slow operations + counts (V5-FOUND-1 Phase 4, 2026-05-02)
- [ ] ~~`neurogrim diag synthesize` invokes a bounded-prompt agent that MUST cite measured baseline + target~~ — **deferred 2026-05-02 to V5-FOUND-1.1** (no Rust-side LLM pathway exists today; deferral preserves V5-FOUND-1's L estimate; design carried forward in `.claude/plans/v5-found-1-diagnostic-monitor.md` § V5-FOUND-1.1)
- [x] cargo-nextest adopted; `.config/nextest.toml` profiles `ci` + `default` (V5-FOUND-2 Phase 1+2, 2026-05-03)
- [ ] ~~sccache (or equivalent) configured in `.cargo/config.toml`~~ — **deferred 2026-05-03 to v5.5 BACKLOG B-47** per V5-FOUND-2 Fork B (sccache + CARGO_INCREMENTAL=0 conflict; `Swatinem/rust-cache` already covers CI cold builds)
- [x] Per-test wall-time SLO documented; existing ≥5s tests audited (fixed / `#[ignore]`d / moved to `benches/`) (V5-FOUND-2 Phase 4, 2026-05-03 — `docs/test-slo.md`; tag-only at v5.0; 9 violations + 1 investigate; fixes queued to v5.5 BACKLOG B-48)
- [ ] cargo-llvm-cov opt-in build mode produces per-test profile data
- [ ] Symbol→test map persisted at `.claude/brain/test-coverage-map.jsonl`
- [ ] `neurogrim test --select-by-coverage --since HEAD~1` runs strict subset; subset includes ≥1 test verified to cover a single-file change
- [ ] Default `neurogrim test` does NOT incur instrumentation cost
- [ ] `TestRunner` trait + 2 impls (`NextestRunner`, `AgentDrivenRunner`) + 6-test conformance suite

---

## Stories

### V5-FOUND-1: Diagnostic Monitor (instrumentation backbone) (~5–7 days)

**Status:** **Complete (2026-05-02)** — 5 phases shipped (Phase 0 tracing-init centralization, Phase 1 ledger writer + schema, Phase 2 tracing Layer, Phase 3 instrumentation across 5 surfaces, Phase 4 `neurogrim diag report` CLI, Phase 5 V5-MOD-1 baseline capture). `synthesize` deferred to **V5-FOUND-1.1** — design carry-forward in `.claude/plans/v5-found-1-diagnostic-monitor.md` § V5-FOUND-1.1.
**Effort:** L (actual: ~6 days, within estimate)
**Depends on:** S12-G-1 (publish-gates ledger pattern)

**What:** Add tracing spans + a persistent JSONL ledger for common operations: cargo build, test runs, MCP tool invocations, dashboard requests, A2A round-trips, scoring pipeline. Extends the invocation-ledger pattern ([disposition.rs:48](../crates/neurogrim-cli/src/commands/disposition.rs)) with a sibling `.claude/brain/diagnostics.jsonl`. Optional dashboard surfacing.

**Why:** Modularity work needs baselines. Without measurements, "did Theme B regress latency?" becomes a vibe argument. Diagnostics ledger also unlocks the agent-synthesis flow that lets the agent "experience time" from the human's perspective — but only with hard guardrails (see Done When).

**Done when:**
- [x] tracing spans emit on: cargo invocation, `neurogrim test`, MCP tool dispatch, A2A POST/SSE, scoring pipeline run, dashboard route handlers (Phase 3 — `score.pipeline.run`, `test.run` + child `cargo.invoke`, `mcp.sensory` per-server, `a2a.post`, `a2a.sse`, `dashboard.route`)
- [x] Diagnostics emitted to `.claude/brain/diagnostics.jsonl` (one event per line) (Phase 1 writer; Phase 2 Layer)
- [x] `schema_version` field present; gitignored same as invocation-ledger (Phase 1 — `diagnostics-ledger-v1.schema.json`; `.gitignore` updated)
- [x] Privacy floor: no prompts, no tool args, no peer payloads — names + durations only (Phase 1 `FORBIDDEN_EXTRAS_KEYS` + per-kind allowed-list; Phase 2 `FieldVisitor` drops `record_debug`)
- [x] `neurogrim diag report` summarizes top-N slow operations + counts (Phase 4 — supports `--json`, `--kind`, `--since`, `--name`, `--top`)
- [ ] ~~`neurogrim diag synthesize` invokes a bounded-prompt agent; agent output MUST cite measured baseline + target. Prose-only "go faster" recommendations rejected at write time.~~ — **deferred 2026-05-02 to V5-FOUND-1.1** (see Theme A Done-When for rationale)
- [x] Unit-test coverage for span emission, ledger append, malformed-line skip, privacy filter (≥4 negative paths per v5 conformance discipline) (32 new tests across `diagnostics_ledger`, `diagnostics_layer`, `diag` cmd; concurrent-writers test caught a real Windows-atomicity bug)
- [x] **S15 scoring round-trip baseline captured** in `roadmap/data/v5-scoring-baseline-2026-05-02.json` (must land before V5-MOD-1 begins — V5-MOD-1's 5% perf gate compares against this baseline). Captured pre-S15-ship per the operator pin (2026-05-01); recapture-trigger files listed in the baseline JSON. Distribution: `p50_ms=16, p95_ms=18, p99_ms=18, max_ms=18` over 30 measured runs (debug build, NeuroGrim's own registry, 19 domains). V5-MOD-1's perf-gate ceiling: `p95_ms ≤ 19`.

### V5-FOUND-1.1: Diagnostic Synthesis (deferred follow-on, ~2–3 days)

**Status:** Planned (deferred 2026-05-02 from V5-FOUND-1 per plan-critic finding)
**Effort:** S–M
**Depends on:** V5-FOUND-1 (ledger writer + Layer + report shipped — done)
**Trigger to start:** operator demand for agent-driven synthesis OR Theme B's V5-MOD-1 needs it for perf-regression triage.

**What:** Implement `neurogrim diag synthesize` — agent-driven analysis of the diagnostics ledger with structural guardrails against prose-only output. The subcommand surface is already reserved in `commands/diag.rs` as a stub; this story implements the underlying LLM call + validator.

**Why deferred:** Plan-critic found that no Rust-side LLM client exists today — no `anthropic` crate, no runtime `reqwest`, `neurogrim-secrets` is for credentials at rest, and `claude-proxy` is for containerized agents with scope tokens. Building that pathway is +2–3 days of dep-discipline-sensitive work that decouples cleanly from V5-FOUND-1's core value (instrumentation + baseline + report). V5-FOUND-1 ships `report` only; the diagnostics ledger and `report` together still deliver V5-MOD-1's baseline-capture need.

**Open architectural decision** (to be revisited at V5-FOUND-1.1 start, not now):
- Add `reqwest` runtime dep + hand-roll Anthropic POST, OR
- Wire through `claude-proxy` (requires running proxy + scope-token plumbing), OR
- Adopt a Rust Anthropic SDK if a maintained one exists by trigger time.

**Done when:**
- [ ] `neurogrim diag synthesize` invokes a bounded-prompt agent; agent output MUST cite measured baseline + target.
- [ ] Output validator requires `{baseline_name, baseline_value_ms, target_value_ms, recommended_actions[]}` with each action carrying `{measurement_to_verify, threshold}`.
- [ ] `target_value_ms ≥ baseline_value_ms` rejected at write time (a "go faster" recommendation must propose a faster target).
- [ ] Prose-only output rejected at write time.
- [ ] Synthesis row to ledger uses `kind=diag_synthesis` (numeric/closed-set extras only); textual rationale to sibling `.claude/brain/diag-synthesis-history.jsonl`.

**Cross-references:**
- Plan: `.claude/plans/v5-found-1-diagnostic-monitor.md` § V5-FOUND-1.1 (carry-forward design)
- Reserved subcommand stub: `neurogrim/crates/neurogrim-cli/src/commands/diag.rs` (DiagCmd::Synthesize)

### V5-FOUND-2: nextest adoption + build cache (~3–4 days) — **COMPLETE 2026-05-03**

**Status:** **Complete (2026-05-03)** — 4 phase commits + Phase 0 prerequisite landed (60eb3b6, 52356f0, 6bc386f, 2078dfd). Plan-critic absorbed; 6 forks pinned; both 🔴 blockers (Fork B sccache → B3 deferral, Fork C C1 → stdout parser) resolved cleanly.
**Effort:** M (actual: ~1 day, well under estimate — the plan-critic-driven scope tightening + Fork B/C deferrals saved most of the budget)
**Depends on:** none

**What:** Replace plain libtest harness with cargo-nextest. Add `.config/nextest.toml` with profiles: `ci` (strict, retries=2), `default` (developer-friendly). ~~Configure sccache (or equivalent) build cache.~~ **Build cache deferred to v5.5 (B-47)** — `sccache` + `CARGO_INCREMENTAL=0` interaction (`mozilla/sccache#236`) makes its dev-loop benefit unreachable while `Swatinem/rust-cache` already covers CI cold builds. Establish per-test wall-time SLO: ≥5s = investigate, ≥10s = tag `#[ignore]` (audit-only at v5.0, fixes to v5.5).

**Why:** The user's "50-test batches" idea, rejected. nextest already does smarter scheduling (CPU/mem budgeting, retry-on-flake) than fixed-size batching. sccache evaluation found it net-negative for the dev-loop pattern; deferred. SLO discipline keeps test wall-time bounded as suite grows.

**Done when:**
- [x] `neurogrim test` invokes cargo-nextest, default profile (Phase 1; `--profile default` is the wrapper default; `--profile ci` opt-in)
- [x] CI uses ci profile with retry-on-flake (`retries = 2`) (Phase 5; plus `flaky-result = "fail"` so passes-on-retry STILL red-light the run, closing the false-negative concern)
- [ ] ~~sccache (or equivalent) configured in `.cargo/config.toml`~~ — **deferred to v5.5 BACKLOG B-47** per Fork B revision (sccache + CARGO_INCREMENTAL=0 conflict; `Swatinem/rust-cache` already covers CI). Phase 3 documentation only.
- [x] Test wall-time SLO documented; existing ≥5s tests audited and either fixed, tagged `#[ignore]`, or moved to `benches/` (Phase 4 — tag-only at v5.0; 9 violations + 1 investigate; `docs/test-slo.md` documents the audit; fixes queued to v5.5 BACKLOG B-48)
- [x] Verified: test suite wall-time on a representative laptop logged in `roadmap/data/v5-test-baseline-2026-05-03.json` (Phase 0 — pre-nextest baseline: warm p50=95s, p95=98s on the V5-FOUND-2 host)
- [x] Existing test-failure ledger ([test.rs](../crates/neurogrim-cli/src/commands/test.rs)) integrates with nextest output (Phase 1 — new `parse_nextest_output()` parser; smoke-tested against live nextest 0.9.133 output; failures + panic detail correctly extracted; ledger appends correctly; `--retry-failed` re-runs by name via libtest-compat `--exact`)

**V5-FOUND-2 retrospective (2026-05-03):**

- **Wall-time outcome on the secrets crate:** post-nextest + post-SLO-tagging, `cargo nextest run -p neurogrim-secrets --profile default` runs 32 tests in **0.371s wall-time** (was ~50s under cargo test with all 41 tests including the 9 Argon2id KDF tests). The 9 ignored tests can still be exercised explicitly via `neurogrim test --slow`.
- **Wall-time outcome on the full workspace:** with SLO tags applied, the full-workspace warm wall-time was not re-benchmarked at close-out (3-run benchmark would have taken ~5–10 min and the data would have been confounded by an unrelated pre-existing test failure in `commands::init_scaffold::tests::scaffold_full_writes_expected_files`). Recapture deferred — V5-FOUND-3 picks up this comparison alongside its own per-test selection benchmarks.
- **Pre-existing test failure noted but not fixed:** `commands::init_scaffold::tests::scaffold_full_writes_expected_files` fails on this host (last seen 2026-04-30 in the failure ledger; reproduced 2026-05-03 during the Phase 4 audit). Out of scope for V5-FOUND-2; flagged here for the next operator pass.
- **Plan deviation: stdout parser instead of JUnit XML parser.** Live nextest output exposes everything the wrapper needs; adding a `quick-xml` dep would have been ceremony. JUnit XML is still emitted (Phase 2 profiles configure it) and is what Phase 5 uploads as a CI artifact — the wrapper just doesn't parse it.
- **Plan deviation: stricter SLO threshold than originally drafted.** Original draft said "≥1s investigate, ≥5s `#[ignore]` or move to benches"; Fork D1 pin tightened the violation threshold to 10s (so fewer tests get `#[ignore]`'d at v5.0). The 1s threshold would have tagged dozens of legitimate integration tests; 10s targets only the genuinely-slow security-critical KDF tests.
- **What surprised the implementation:** the parser unit-test fixtures used a fabricated `--- STDERR: <binary> <test> ---` block-marker format that doesn't match real nextest output (`stdout ───` / `stderr ───` / `────────────`). Live smoke test caught it; both the parser and the fixtures are now real-format.
- **What's NOT done that the plan called for:** Phase 3 build cache (intentional — Fork B revision deferred to v5.5 B-47).

### V5-FOUND-3: Change-driven test selection (per-test coverage) (~5–7 days)

**Status:** **⏸ DEFERRED 2026-05-03 to v5.1/v6** — Windows host coverage-toolchain gap. Phase 0 partial work shipped (commit `39d7295`); remainder pending operator-environment decision (install VS Build Tools / install `xwin` / pick up at v5.1 / push to v6). See § V5-FOUND-3 deferral note below.
**Effort:** L (scope unchanged on re-entry; toolchain prereq added)
**Depends on:** V5-FOUND-2 ✅, **+ working `cargo-llvm-cov` toolchain on the build host** (NEW — discovered 2026-05-03)
**Absorbs:** BACKLOG B-28 (Coverage-aware test selection — v4.x-deferred) — *deferral propagates to B-28 (re-deferred 2026-05-03); flips back when V5-FOUND-3 unblocks*

**What:** Add cargo-llvm-cov as opt-in build mode (`neurogrim test --instrument-coverage`). Capture **per-binary** profile data (per-test deferred to v6 / BACKLOG B-44 — see deferral note for plan-critic rationale); build a binary→source-files map persisted at `.claude/brain/test-coverage-map.jsonl`. Add `neurogrim test --select-by-coverage --since <git-rev>` that runs only the test binaries covering files changed since `<git-rev>`. This is the user's "LSP-brain blast radius" idea, scoped as a test-selection feature in v5; promotion to a Brain domain is v6 successor work (BACKLOG B-44).

**Why:** Rerunning all 1,670 tests on every change is wasteful. Coverage-driven binary selection narrows the run to the 1–3 binaries that exercise the changed files — the equivalent of an LSP "find references" but for test coverage. Map first, score later. v5 ships the map and the selector; v6 may promote to a domain after the map proves predictive.

**Done when:**
- [ ] Coverage build mode produces per-binary profile data
- [ ] Binary→source-files map persisted as JSONL; `schema_version` stable
- [ ] `neurogrim test --select-by-coverage --since HEAD~1` selects a strict subset for a single-file change AND that subset includes ≥1 binary verified to cover the change
- [ ] Documented opt-in; default `neurogrim test` does NOT incur instrumentation cost
- [ ] Stale-map handling: file mtime + blake3 hybrid keys (mtime fast-trigger; blake3 confirms content actually changed); map invalidates on any covering-file change
- [ ] Mutual-exclusion guards on `--instrument-coverage` / `--retry-failed` / `--select-by-coverage` (sysexits.h `EX_USAGE = 64` for invalid combos)
- [ ] BACKLOG B-28 closed with pointer to V5-FOUND-3 on completion

#### V5-FOUND-3 deferral note (2026-05-03)

**What shipped (Phase 0 partial):**
- Phase 0a: `llvm-tools-preview` rustup component installed (both GNU + MSVC toolchains).
- Phase 0c: `build_cargo_args(args, retry_names)` extracted from `commands::test::run` as a single source of truth for cargo-nextest argv. **Bundled bug fix:** `neurogrim test --retry-failed --slow` was silently dropping `--include-ignored` because the retry branch hardcoded `-- --exact <names>` with no slot for libtest flags. After V5-FOUND-2 Phase 4 tagged 9 encrypted-secrets tests `#[ignore]`, anyone retrying a failed slow run would have lost those 9 tests from the replay. 6 new unit tests, all passing, including the regression `build_cargo_args_retry_and_slow_propagates_include_ignored`. Commit `39d7295`.

**What blocked (Phase 0b smoke):**
- Goal of Phase 0b: confirm `cargo llvm-cov nextest --no-report -p neurogrim-secrets` produces one `.profdata` per binary on this host (the foundation Phase 1+ would build on).
- `stable-x86_64-pc-windows-gnu` (active default toolchain): `error[E0463]: can't find crate for 'profiler_builtins'`. The `.rlib` is not part of rustup's `rust-std` distribution for this triple. `-C instrument-coverage` cannot compile.
- `stable-x86_64-pc-windows-msvc` (alternate toolchain): `libprofiler_builtins-*.rlib` present in sysroot, but `link.exe` not on PATH. No Visual Studio Build Tools installed on the host (`C:\Program Files (x86)\Microsoft Visual Studio\` contains only `Shared\`). `rust-lld.exe` and `lld-link.exe` are bundled with the toolchain but cannot link MSVC targets without the Microsoft CRT and Windows SDK `.lib` files.

**Re-entry triggers (any one is sufficient):**
1. Operator installs Visual Studio Build Tools (3–7 GB) — official, supported, unblocks immediately.
2. Operator installs `xwin` (`cargo install xwin && xwin splat --output ~/.xwin`, ~300–500 MB) and configures `[target.x86_64-pc-windows-msvc]` linker + link-args in `.cargo/config.toml`. Lighter, reversible.
3. v5.1 plans pick up the epic with the toolchain prereq documented up-front.
4. v6 horizon takes it if v5.1 doesn't.

**Plan record:** [`.claude/plans/v5-found-3-coverage-selection.md`](../../.claude/plans/v5-found-3-coverage-selection.md) — v2 plan with all 3 plan-critic 🔴 blockers absorbed (Fork B per-test→binary-level, mutual-exclusion guards added, exit-code semantics specified); 1 🟡→🟢 (mtime→mtime+blake3 hybrid); operator-pinned 6 forks (A1/B1'/C1/D2/E1/F1). Preserved verbatim for re-entry.

### V5-FOUND-4: TestRunner trait (minimal modular testing surface) (~2–3 days)

**Status:** **✅ COMPLETE 2026-05-04** — 6 phase commits (`cf6e0cf` Phase 0, `985b7e6` Phase 1, `7060bf3` Phase 2, `ae5604c` Phase 3, `8b98599` Phase 4, plus this Phase 5 close-out). Effort actual: ~1 day, well under the 2–3d S estimate. Plan-critic absorbed two methodology blockers pre-implementation (AgentDrivenRunner stub fails proposed-#20 reshape rule; Fork D1 silent-green-CI hazard) by deferring AgentDrivenRunner + the `--runner=` CLI flag to v5.5 BACKLOG (B-51, B-52).
**Effort:** S (actual: ~1 day)
**Depends on:** V5-FOUND-2 ✅; V5-FOUND-3 ⏸ DEFERRED (**soft** — `TestSelection` is `#[non_exhaustive]`; v5.1 adds `ByCoverage(...)` variant non-breakingly when V5-FOUND-3 unblocks. Trait surface itself is shape-stable across coverage-selection's eventual landing. See `.claude/plans/v5-found-4-test-runner-trait.md` § "Soft-dependency on V5-FOUND-3" for the operator decision rule.)

**What:** ~~Define a small `TestRunner` trait inside `neurogrim-core` with two impls: `NextestRunner` (default), `AgentDrivenRunner` (calls an agent for orchestration — stub initially).~~ **REVISED 2026-05-04:** ships the trait + types + 4-test conformance suite in `neurogrim-core::test_runner` + a single concrete impl (`NextestRunner` in `neurogrim-cli/src/commands/test_runner_impls/nextest.rs`). The wrapper at `commands::test::run` dispatches via `Box<dyn TestRunner>` internally — no `--runner=` clap flag at v5.0 (only one runner exists; flag deferred to v5.5 B-52). AgentDrivenRunner deferred to v5.5 B-51 alongside the agent-orchestration work that would make it real; deferral honors VISION proposed-#20 ("pluggability is justified by use, not aspiration") by avoiding aspirational stub-as-second-impl. Trait surface: `async fn run(&self, selection: &TestSelection) -> Result<TestRunReport>` — single method.

**Why:** Smallest modular surface that supports adopters' "users create their own testing surface" goal. Trait extraction at v5.0 clears the v5-roadmap §A reshape rule via clause **(iii)** — leaving NextestRunner concrete was the blocker for V5-SDK-2's promised conformance-suite re-export (deliverable 2). Phase 4 lifted V5-SDK-2 from ◐ PARTIAL → ✅ COMPLETE. Theme C is now ✅ COMPLETE.

**Done when:**
- [x] `TestRunner` trait + types + 4-test conformance suite land in `neurogrim-core` (Phase 1, commit `985b7e6`)
- [x] `NextestRunner` impl + factory ship in `neurogrim-cli` (Phase 2, commit `7060bf3`); the wrapper at `commands::test::run` dispatches via `Box<dyn TestRunner>` (Phase 3, commit `ae5604c`)
- [x] Conformance suite: 4 cross-cutting tests (factory_name_non_empty, factory_name_stable_across_calls, factory_build_repeatable, run_with_malformed_selection_returns_ok_or_err_no_panic). Plan v2 reduced from 6 — dropped `run_with_empty_selection_completes` (undefined cargo behavior with `--exact` + zero names) and `run_is_concurrent_safe` (cargo lockfile contention deadlocks 5-parallel runs). Object-safety + Send guaranteed at compile time via `_object_safety_check_test_runner` + `_send_check_test_run_report` guards in `crate::test_runner` instead.
- [ ] ~~`neurogrim test --runner=nextest` (default) and `--runner=agent` both dispatch via the trait~~ — **deferred to v5.5 (BACKLOG B-52)**. v5.0 has only one runner; adding the clap flag with one value would be ceremony without value. The trait dispatch is internal — wrapper constructs `Box<dyn TestRunner>` from the in-tree `NextestRunner`.

**V5-FOUND-4 retrospective (2026-05-04):**

- **Plan record:** [`.claude/plans/v5-found-4-test-runner-trait.md`](../../.claude/plans/v5-found-4-test-runner-trait.md) — v2 plan, two plan-critic rounds (technical + methodology in parallel), 3 🔴 blockers absorbed (AgentDrivenRunner-as-stub fails reshape rule; Fork D1 silent-green-CI hazard; `run_with_empty_selection_completes` test undefined behavior + `run_is_concurrent_safe` cargo lockfile contention). 7 🟡 concerns absorbed: tokio dev-dep restoration, rustdoc intra-doc-link gating, CI profile fix, dropped Fork E (surface-assertion), `cargo tree` package-cwd discipline, `runner_name()` removed from trait, span ownership specified, byte-identical-ledger verification.
- **Forks pinned (5 — down from 7):** A1 (TestSelection variants All/Names/Packages) / B1 (build_cargo_args stays in commands::test) / C1 (parse_nextest_output stays in commands::test) / F1 (SDK trait re-export always-on; only conformance module gated) / G1' (4-test conformance suite). Forks D + E dropped after methodology absorption.
- **Outcome:** trait extraction with one real impl, no aspirational stub. The wrapper's dispatch through `Box<dyn TestRunner>` validates trait integration; existing 24 unit tests pass byte-identically post-Phase-3 (no operator-facing regression). `neurogrim_sdk::TestRunner` joins the 4 always-on V5-MOD-1/2/3 trait surfaces; `neurogrim_sdk::test_runner_conformance` joins the 3 gated conformance modules.
- **What's NOT done that the original V5-FOUND-4 epic called for:** AgentDrivenRunner (deferred to v5.5 BACKLOG B-51 alongside agent-orchestration work + Rust LLM client landing); `--runner=` CLI flag dispatch (deferred to v5.5 BACKLOG B-52 once a second runner exists). Both deferrals are honesty-floor compliant — neither failed silently; both are concretely tracked.

---

## Verification (end-to-end smoke per story)

**V5-FOUND-1 Diagnostic Monitor:**
- Run a real `cargo test --workspace` under `neurogrim test`; observe diagnostics emitted to `.claude/brain/diagnostics.jsonl`
- Run `neurogrim diag report`; confirm top-N slow operations + counts surface
- ~~Run `neurogrim diag synthesize`; confirm agent output cites measured baseline + target (rejected if not)~~ — **deferred to V5-FOUND-1.1**; in V5-FOUND-1, the `synthesize` subcommand is registered as a stub that returns a "not yet implemented; see V5-FOUND-1.1" error (reserves the name)
- Capture S15 scoring round-trip baseline JSON for V5-MOD-1 perf-gate comparison

**V5-FOUND-2 nextest + sccache:**
- Compare wall-time against `roadmap/data/v5-test-baseline-<date>.json`
- Confirm `retries=2` in CI profile triggers on flake injection
- Confirm sccache cache hit on warm rebuild (sccache stats post-run)

**V5-FOUND-3 Change-driven test selection:**
- Touch a single file in `neurogrim-core`; run `neurogrim test --select-by-coverage --since HEAD~1`; confirm subset is strict (less than full workspace) AND includes ≥1 test verified to cover the changed file
- Confirm default `neurogrim test` (without `--instrument-coverage`) does NOT incur instrumentation cost (compare wall-time)

**V5-FOUND-4 TestRunner trait:**
- Run `neurogrim test --runner=nextest` and `neurogrim test --runner=agent` against the same fixture; both dispatch via the trait and produce a `TestRunReport`
- Run conformance suite against `NextestRunner`; confirm 6/6 tests pass

---

## Risks (adversary concerns brought forward)

🟡 **Tracing instrumentation overhead.** Span macros add some cost even when subscriber is disabled. Mitigation: V5-FOUND-1 acceptance criterion requires "production default disabled"; benchmark span overhead in 12-test suite.

🟡 **Coverage build mode 10–30% slower.** Mitigation: opt-in only; default `neurogrim test` is unchanged. Documented in `neurogrim explain test`.

🟡 **Symbol→test map staleness.** Mitigation: file-mtime + git-revision keys; map invalidates on any covering-file change. CI re-builds the map on schedule.

🟡 **Agent-synthesize flow drift.** Risk: agents narrate slowness instead of measuring it. Mitigation: prompt template requires baseline + target citations; ledger writer rejects entries without them.

🔵 **Suggestion: "diag-readiness" advisory domain (post-Theme D).** Reads diagnostics.jsonl; emits findings if any common operation has zero events in the last N runs (instrumentation regression). Cheap once Theme A ships. Likely v5.5 candidate.

🔵 **Suggestion: integrate test wall-time SLO into publish-gate `tests-pass`.** S12-G-4's gate runs tests; could fail when wall-time SLO is violated. v5.5 polish, not v5 scope.

---

## Cross-references

- Master roadmap: `roadmap/v5-roadmap.md`
- Pre-plan: `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`
- Existing ledger pattern: [disposition.rs:48](../crates/neurogrim-cli/src/commands/disposition.rs)
- Existing test command: [test.rs](../crates/neurogrim-cli/src/commands/test.rs)
- Existing port allocator: [ports.rs](../crates/neurogrim-core/src/ports.rs)
- Workspace tracing-subscriber dep: [Cargo.toml:54-55](../Cargo.toml)
- Absorbs: BACKLOG B-28
