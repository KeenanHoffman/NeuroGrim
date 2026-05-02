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

- [ ] tracing spans + `.claude/brain/diagnostics.jsonl` ledger emit on cargo build, neurogrim test, MCP tool dispatch, A2A POST/SSE, scoring pipeline, dashboard route handlers
- [ ] `neurogrim diag report` summarizes top-N slow operations + counts
- [ ] ~~`neurogrim diag synthesize` invokes a bounded-prompt agent that MUST cite measured baseline + target~~ — **deferred 2026-05-02 to V5-FOUND-1.1** (no Rust-side LLM pathway exists today; deferral preserves V5-FOUND-1's L estimate; design carried forward in `.claude/plans/v5-found-1-diagnostic-monitor.md` § V5-FOUND-1.1)
- [ ] cargo-nextest adopted; `.config/nextest.toml` profiles `ci` + `default`
- [ ] sccache (or equivalent) configured in `.cargo/config.toml`
- [ ] Per-test wall-time SLO documented; existing ≥5s tests audited (fixed / `#[ignore]`d / moved to `benches/`)
- [ ] cargo-llvm-cov opt-in build mode produces per-test profile data
- [ ] Symbol→test map persisted at `.claude/brain/test-coverage-map.jsonl`
- [ ] `neurogrim test --select-by-coverage --since HEAD~1` runs strict subset; subset includes ≥1 test verified to cover a single-file change
- [ ] Default `neurogrim test` does NOT incur instrumentation cost
- [ ] `TestRunner` trait + 2 impls (`NextestRunner`, `AgentDrivenRunner`) + 6-test conformance suite

---

## Stories

### V5-FOUND-1: Diagnostic Monitor (instrumentation backbone) (~5–7 days)

**Status:** Planned
**Effort:** L
**Depends on:** S12-G-1 (publish-gates ledger pattern)

**What:** Add tracing spans + a persistent JSONL ledger for common operations: cargo build, test runs, MCP tool invocations, dashboard requests, A2A round-trips, scoring pipeline. Extends the invocation-ledger pattern ([disposition.rs:48](../crates/neurogrim-cli/src/commands/disposition.rs)) with a sibling `.claude/brain/diagnostics.jsonl`. Optional dashboard surfacing.

**Why:** Modularity work needs baselines. Without measurements, "did Theme B regress latency?" becomes a vibe argument. Diagnostics ledger also unlocks the agent-synthesis flow that lets the agent "experience time" from the human's perspective — but only with hard guardrails (see Done When).

**Done when:**
- [ ] tracing spans emit on: cargo invocation, `neurogrim test`, MCP tool dispatch, A2A POST/SSE, scoring pipeline run, dashboard route handlers
- [ ] Diagnostics emitted to `.claude/brain/diagnostics.jsonl` (one event per line)
- [ ] `schema_version` field present; gitignored same as invocation-ledger
- [ ] Privacy floor: no prompts, no tool args, no peer payloads — names + durations only
- [ ] `neurogrim diag report` summarizes top-N slow operations + counts
- [ ] ~~`neurogrim diag synthesize` invokes a bounded-prompt agent; agent output MUST cite measured baseline + target. Prose-only "go faster" recommendations rejected at write time.~~ — **deferred 2026-05-02 to V5-FOUND-1.1** (see Theme A Done-When for rationale)
- [ ] Unit-test coverage for span emission, ledger append, malformed-line skip, privacy filter (≥4 negative paths per v5 conformance discipline)
- [ ] **S15 scoring round-trip baseline captured** in `roadmap/data/v5-scoring-baseline-<date>.json` (must land before V5-MOD-1 begins — V5-MOD-1's 5% perf gate compares against this baseline)

### V5-FOUND-2: nextest adoption + build cache (~3–4 days)

**Status:** Planned
**Effort:** M
**Depends on:** none

**What:** Replace plain libtest harness with cargo-nextest. Add `.config/nextest.toml` with profiles: `ci` (strict, retries=2), `default` (developer-friendly). Configure sccache (or equivalent) build cache. Establish per-test wall-time SLO: ≥1s = investigate, ≥5s = move to bench or `#[ignore]`.

**Why:** The user's "50-test batches" idea, rejected. nextest already does smarter scheduling (CPU/mem budgeting, retry-on-flake) than fixed-size batching. sccache addresses the actual second-largest dev-loop bottleneck (cold builds). SLO discipline keeps test wall-time bounded as suite grows.

**Done when:**
- [ ] `neurogrim test` invokes cargo-nextest, default profile
- [ ] CI uses ci profile with retry-on-flake (`retries = 2`)
- [ ] sccache (or equivalent) configured in `.cargo/config.toml`
- [ ] Test wall-time SLO documented; existing ≥5s tests audited and either fixed, tagged `#[ignore]`, or moved to `benches/`
- [ ] Verified: test suite wall-time on a representative laptop logged in `roadmap/data/v5-test-baseline-<date>.json`
- [ ] Existing test-failure ledger ([test.rs](../crates/neurogrim-cli/src/commands/test.rs)) integrates with nextest output

### V5-FOUND-3: Change-driven test selection (per-test coverage) (~5–7 days)

**Status:** Planned
**Effort:** L
**Depends on:** V5-FOUND-2
**Absorbs:** BACKLOG B-28 (Coverage-aware test selection — v4.x-deferred)

**What:** Add cargo-llvm-cov as opt-in build mode (`neurogrim test --instrument-coverage`). Capture per-test profile data; build a symbol→test map persisted at `.claude/brain/test-coverage-map.jsonl`. Add `neurogrim test --select-by-coverage --since <git-rev>` that runs only tests covering files changed since `<git-rev>`. This is the user's "LSP-brain blast radius" idea, scoped as a test-selection feature in v5; promotion to a Brain domain is v6 successor work (BACKLOG B-44).

**Why:** Rerunning all 1,470 tests on every change is wasteful. Per-test coverage maps changes to test subsets — the equivalent of an LSP "find references" but for test coverage. Map first, score later. v5 ships the map and the selector; v6 may promote to a domain after the map proves predictive.

**Done when:**
- [ ] Coverage build mode produces per-test profile data
- [ ] Symbol→test map persisted as JSONL; `schema_version` stable
- [ ] `neurogrim test --select-by-coverage --since HEAD~1` selects a strict subset for a single-file change AND that subset includes ≥1 test verified to cover the change
- [ ] Documented opt-in; default `neurogrim test` does NOT incur instrumentation cost
- [ ] Stale-map handling: file mtime + git revision keys; map invalidates on any covering-file change
- [ ] BACKLOG B-28 closed with pointer to V5-FOUND-3 on completion

### V5-FOUND-4: TestRunner trait (minimal modular testing surface) (~2–3 days)

**Status:** Planned
**Effort:** S
**Depends on:** V5-FOUND-2, V5-FOUND-3

**What:** Define a small `TestRunner` trait inside `neurogrim-core` with two impls: `NextestRunner` (default), `AgentDrivenRunner` (calls an agent for orchestration — stub initially). Public via SDK in Theme C. Trait surfaces only: `run(&self, selection: &TestSelection) -> TestRunReport`. Resists the urge to make TestRunner a god-object.

**Why:** Smallest modular surface that supports the user's "users create their own testing surface" goal. No god-object risk because the trait deliberately exposes one method. AgentDrivenRunner is a stub today; v5.5 may flesh it out if the pattern proves useful.

**Done when:**
- [ ] `TestRunner` trait + 2 impls land in `neurogrim-core`
- [ ] `neurogrim test --runner=nextest` (default) and `--runner=agent` both dispatch via the trait
- [ ] Conformance suite: any TestRunner impl must pass shared 6-test suite (happy path, empty selection, malformed selection, cancellation, timeout, malformed report output)

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
