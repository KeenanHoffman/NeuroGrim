# V5-MOD-1 ScoringSource Trait + Factory Registry — Implementation Plan

**Epic:** `roadmap/epics/v5-modular-conversions.md` § V5-MOD-1
**Effort estimate (epic):** M, ~7–10 days
**v5 entry:** pinned 2026-05-01; Theme A closed (V5-FOUND-1 shipped 2026-05-02)
**Methodology:** plan-critic before implementation per `v5-roadmap.md` final note
**Baseline ceiling for the perf-gate:** `p95_ms ≤ 19` (from `roadmap/data/v5-scoring-baseline-2026-05-02.json`)

## Context

V5-MOD-1 is the highest-leverage seam in v5: convert the string-dispatch in scoring-source loading to a trait-based factory pattern. Today, every new scoring-source type requires forking `neurogrim-mcp` to add a match arm. After V5-MOD-1, third-party crates can register their own scoring sources without touching core.

The 🔴 BLOCKING adversary concern from the v5 master roadmap is performance: dyn-dispatch on the hot scoring path could regress latency. The V5-FOUND-1 baseline gives a concrete ceiling — `p95_ms ≤ 19` — that V5-MOD-1's implementation must respect. If the gate fails, the plan falls back to generic-bounded dispatch with a small enum for built-ins (decision recorded in epic when V5-MOD-1 ships).

## File-anchor corrections (vs. the epic file)

The V5-MOD-1 epic story names two anchors that don't match current code:

| Epic says | Reality |
|---|---|
| "registry.rs:135–157 string-dispatch" | Lines 135–157 of `registry.rs` are the **`ScoringSource` config struct** (serde-deserialized; not a dispatch). The actual string-dispatch lives in `neurogrim-mcp/src/context.rs:218` (function `load_cmdb_data`). |
| "matches on `source_type` ∈ {jsonl, a2a, file}" | Actual source-type names: `cmdb`, `a2a`, `function`. There is no `jsonl` or `file` source type today. |

**Confirmed dispatch sites (3 total):**
- `neurogrim-mcp/src/context.rs:218` — `load_cmdb_data`: full match on `cmdb` / `a2a` / `function` / unknown.
- `neurogrim-mcp/src/server.rs:75` — `load_cmdb_from_disk`: partial; only handles `cmdb` (duplicates context.rs's `cmdb` arm; converging through V5-MOD-1 is a goal).
- `neurogrim-mcp/src/doctor.rs:155` — validation check (`source_type != "cmdb"` → skip).

The plan below uses the correct anchors. The epic file should be edited at V5-MOD-1 close-out to fix the stale references.

## Architectural anchors (extending, not inventing)

| Anchor | What we reuse |
|---|---|
| `neurogrim-core::queue_backend::QueueBackend` (already a trait) | Trait shape — `async fn` + `&self`. Already proven in production with two impls (`JsonlBackend`, `SqliteBackend`). V5-MOD-1's trait shape mirrors this. |
| `inventory` crate (used elsewhere in workspace? — check at planning end) | Static factory registration if available; otherwise `static FACTORIES: &[FactoryEntry]` table. |
| V5-FOUND-1 diagnostics ledger + `score.pipeline.run` span | Perf-gate measurement instrument. `neurogrim diag report --kind scoring --json` produces the comparison numbers. |
| `disposition.rs` discipline | Closed-set enum names; `additionalProperties: false` analog (factories registered must declare a known `source_type` string). |

## Naming decision — RESOLVED 2026-05-02: Option (A) + accept semver-major bump

**Decision:** rename `pub struct ScoringSource` → `pub struct ScoringSourceConfig`. The new trait takes the obvious name `ScoringSource`. **Operator pin (2026-05-02):** accept the semver-major breaking change for `neurogrim-core` (the crate is published to crates.io; downstream consumers will fail loudly at compile time and update). Clean break, clean naming, no back-compat alias debt. The "we're at v5 anyway, this IS the v5 boundary" framing applies.

**Fork-decision history (kept on file for future reference):**

| Option | Pros | Cons | Status |
|---|---|---|---|
| **(A) Rename config struct to `ScoringSourceConfig`** | Trait keeps the obvious name. Reads cleanly. | Touches every site that names the config type (~6+ files); semver-major for `neurogrim-core`. | **CHOSEN** |
| (B) Keep config as `ScoringSource`; trait becomes `ScoringSourceImpl` | Smaller diff; no breaking change. | "Impl" suffix is a Java-ism; reads awkwardly. | Rejected — naming clarity > diff size. |
| (C) Both named `ScoringSource`, in different modules | Smallest diff. | Two types with the same name in the same crate — confusing. | Rejected — readability cost too high. |
| (D) Defer to Phase 7 (working name `ScoringSourceImpl`, decide at close-out) | Implementation can start immediately. | Rename then ripples through tests + V5-SDK example, more churn. | Rejected — picking now is cheaper. |

**Phase 0 implications (Option A):**
- Step 1: rename across all 5 Rust call sites + lib.rs rustdoc (per Subagent 3 audit).
- Step 4 (NEW): bump `neurogrim-core`'s package version to mark the breaking change. Workspace shares `version.workspace = true` for most crates, so the bump is at the workspace root.
- Phase 7 close-out: add CHANGELOG entry (or equivalent — check whether NeuroGrim has a CHANGELOG.md or relies on commit-message changelog convention).

## Phases (incremental delivery)

Each phase ships independently. Iteration boundaries are explicit.

### Phase 0 — Naming + dependency promotion + audits (Day 1, ~0.5 day) — PREREQUISITE

**Goal:** Resolve the naming collision (per fork-decision), promote `async_trait` to a workspace dep, and audit for hidden references before any other work begins.

**Steps:**
1. **Naming-collision rename** (Option A confirmed by operator pin 2026-05-02). Rename `pub struct ScoringSource` → `pub struct ScoringSourceConfig` across all 5 Rust call sites: `registry.rs:115`, `registry.rs:135`, `lib.rs:10` (rustdoc), `governance.rs:547` (test fixture import), `context.rs:290` (parameter type). Single atomic commit, no behavior change.
2. **Promote `async_trait` to `workspace.dependencies`.** Plan-critic Subagent 1 confirmed: `async_trait` is already a direct dep of `neurogrim-a2a` (Cargo.toml line 44) but NOT in `workspace.dependencies`. V5-MOD-1's trait lands in `neurogrim-core`, so the macro will be reused — promote it to the workspace level and reference via `async-trait = { workspace = true }` from both `neurogrim-a2a` (existing usage) and `neurogrim-core` (new usage). One-line `Cargo.toml` workspace edit + per-crate switches.
3. **Schemas-directory audit** for any `ScoringSource` mention. Run `grep -r ScoringSource neurogrim/schemas/ neurogrim/crates/*/data/schemas/` and `grep -r ScoringSource D:/Brains/LSP-Brains/spec/`. If found, list each occurrence + decide whether the rename ripples there. (Plan-critic Subagent 3 noted: `METHODOLOGY-EVOLUTION.md` line 1115-1120 names `ScoringSource` in spec prose — flag for Phase 7 sync.)
4. **Bump `neurogrim-core` to a major version** to mark the breaking change. The workspace likely shares `version.workspace = true` for most crates (check `Cargo.toml` workspace.package); if so, the bump is at the workspace root. If `neurogrim-core` has a private version line, bump that. Target: `4.x → 5.0.0` (matches the v5 epic boundary). The bump is part of the Phase 0 commit so the rename + version-bump land atomically — downstream consumers see "neurogrim-core 5.0.0 renamed `ScoringSource` to `ScoringSourceConfig`" as a single self-explanatory upgrade event.
5. **NOT** adding the `inventory` crate. Per plan-critic Subagent 2: `inventory` is not in the workspace, and the project's `dependency-discipline` skill enforces a 4-point pre-flight before any new dep lands. The hand-rolled approach (Phase 2) is the same 40 lines of code with zero supply-chain review burden — promoted to default.

**Ship criterion:** rename diff lands cleanly (cargo test green, no behavior change); `async_trait` is `workspace = true`; schemas/spec audit results captured in the commit message or this plan's appendix; `neurogrim-core` version reflects the breaking change.

### Phase 1 — Define the trait (Day 1–2, ~1 day)

**Goal:** Define `pub trait ScoringSource` in a new `neurogrim-core/src/scoring_source.rs` module. No dispatch wired yet — just the contract.

**Files (new):**
- `neurogrim/crates/neurogrim-core/src/scoring_source.rs`

**Trait shape (sketch; subject to plan-critic refinement):**
```rust
/// V5-MOD-1: pluggable contract for loading a domain's scoring data.
/// Replaces the string-dispatch at neurogrim-mcp/src/context.rs:218.
///
/// Implementations are object-safe (`Box<dyn ScoringSource>`) and
/// registered via the factory registry below. Built-in impls
/// (`CmdbSource`, `A2aSource`, `FunctionSource`) preserve the v4
/// behavior verbatim; the contract is identical, only the dispatch
/// mechanism changes.
#[async_trait::async_trait]
pub trait ScoringSource: Send + Sync {
    /// Stable wire-name (matches `ScoringSourceConfig::source_type`).
    /// Used by the factory registry for dispatch.
    fn source_type_name(&self) -> &'static str;

    /// Load this domain's scoring data. Returns None if the source
    /// is unreachable / missing — caller falls through to
    /// `no_file_score` semantics. NEVER panics; errors are logged
    /// at warn and surfaced as None (matches v4 behavior).
    async fn load(
        &self,
        domain_key: &str,
        config: &ScoringSourceConfig,
        project_root: &Path,
    ) -> Option<CmdbData>;
}

/// Factory: produces a ScoringSource impl for a given config.
/// Built-in factories registered statically; third-party crates
/// register their own via the `inventory` mechanism (or static
/// table fallback).
pub trait ScoringSourceFactory: Send + Sync {
    fn source_type_name(&self) -> &'static str;
    fn build(&self) -> Box<dyn ScoringSource>;
}
```

**Tests (Phase 1 — trait definition only):**
- Compile-only test: trait is object-safe (`fn _check(_: Box<dyn ScoringSource>) {}`).
- The trait's contract is documented; no behavior to test yet.

**Ship criterion:** `cargo test --workspace` green; new module has rustdoc with examples.

### Phase 2 — Built-in factories + hand-rolled registration (Day 2–4, ~2 days)

**Goal:** Implement `CmdbSource`, `A2aSource`, `FunctionSource` as `ScoringSource` impls. Wire registration via a hand-rolled `HashMap<&'static str, Box<dyn ScoringSourceFactory>>` populated at app startup. **Per plan-critic: NOT using the `inventory` crate** — the workspace has no existing static-registration substrate and `dependency-discipline` would gate the new dep with a 4-point pre-flight. The hand-rolled path is the same 40 lines of code with zero supply-chain review burden, and it's explicit (registration happens visibly in `main.rs`) rather than magical.

**Registration shape (sketch):**
```rust
// neurogrim-core/src/scoring_source.rs (or scoring_source_registry.rs)
use std::collections::HashMap;
use std::sync::OnceLock;

pub struct ScoringSourceRegistry {
    factories: HashMap<&'static str, Box<dyn ScoringSourceFactory>>,
}

impl ScoringSourceRegistry {
    pub fn with_built_ins() -> Self { /* register cmdb, a2a, function */ }
    pub fn register(&mut self, factory: Box<dyn ScoringSourceFactory>) { ... }
    pub fn get(&self, source_type: &str) -> Option<&dyn ScoringSourceFactory> { ... }
}

// Singleton for the dispatch path (initialized at first access).
static GLOBAL: OnceLock<ScoringSourceRegistry> = OnceLock::new();
pub fn global_registry() -> &'static ScoringSourceRegistry {
    GLOBAL.get_or_init(ScoringSourceRegistry::with_built_ins)
}
```

Third-party crates register their own factory at startup by calling a mutable variant of the registry from `main.rs` (or via a public init API the consuming binary calls). The init-call requirement is explicit; if the v5.5 demand for plugins-without-init-call emerges, a `inventory`-based v2 is a clean follow-on (BACKLOG B-37/B-40 vicinity).

**Files (new):**
- `neurogrim/crates/neurogrim-core/src/scoring_sources/mod.rs`
- `neurogrim/crates/neurogrim-core/src/scoring_sources/cmdb.rs`
- `neurogrim/crates/neurogrim-core/src/scoring_sources/a2a.rs`
- `neurogrim/crates/neurogrim-core/src/scoring_sources/function.rs`
- `neurogrim/crates/neurogrim-core/src/scoring_source_registry.rs` (or in scoring_source.rs)

**Migration strategy — verbatim semantics:**
- `CmdbSource::load` is line-for-line equivalent to the `"cmdb"` arm of `load_cmdb_data` (context.rs:218–254). Includes BOM stripping, score-field/updated_at-field defaults, confidence override.
- `A2aSource::load` is line-for-line equivalent to the `"a2a"` arm + `load_a2a_domain` helper (context.rs:256–270 + 288–...).
- `FunctionSource::load` is the no-op for now (matches the existing `"function"` arm). Documented as "implementation-specific scoring functions handled elsewhere in the pipeline; this factory exists so the source-type is known but produces no `CmdbData`."

**Tests (≥4 negative paths per epic done-when):**
- `CmdbSource::load`: happy path; missing file → None; malformed JSON → None; missing required field → None; BOM-prefixed file → handled.
- `A2aSource::load`: happy path; bad URL → None; unreachable peer → None; version mismatch → None.
- `FunctionSource::load`: always returns None (no-op verified).
- Registry lookup: known `source_type` → returns factory; unknown → None.
- **`Box<dyn ScoringSource>` early-validation test** (per plan-critic 🔵 suggestion): a small `#[tokio::test]` that exercises `Box<dyn ScoringSource>` end-to-end via `async_trait` boxing — catches any future-boxing wart before the conformance suite generalizes the pattern in Phase 5.

**Ship criterion:** all built-in factories pass their tests; registry can produce a `Box<dyn ScoringSource>` from any of the three known names; the early `Box<dyn>` validation test is green.

### Phase 3 — Convert the dispatch sites (Day 4–6, ~2 days)

**Goal:** Replace the three string-match sites with factory-based dispatch. Behavior must be bit-identical to v4.

**Files (modified)** — exhaustive list per plan-critic Subagent 3 audit (don't lose track of the duplicate dispatch sites):

- **`neurogrim/crates/neurogrim-mcp/src/context.rs:218`** — `load_cmdb_data` becomes a loop over domains, each calling `factory_registry.get(source_type).build().load(...)`. This is the primary dispatch site; the match arm on `cmdb`/`a2a`/`function`/unknown is fully replaced.
- **`neurogrim/crates/neurogrim-mcp/src/server.rs:75`** — `load_cmdb_from_disk` is a duplicate dispatch (`cmdb`-only branch that mirrors context.rs). V5-MOD-1 converges these — the method either shrinks to a single call into context.rs's helper or is removed entirely. **Easy to miss** in execution; explicitly flagged here.
- **`neurogrim/crates/neurogrim-mcp/src/doctor.rs:155`** — validation check (`source_type != "cmdb"` → skip). Becomes "registered factory exists for this source_type"; doctor calls `factory_registry.get(name).is_some()` instead. The check's intent shifts from "is this source_type one we understand" to "is a factory registered for this source_type" — same semantics, factory-aware.
- **`neurogrim-core/src/lib.rs`** — re-exports for the new module + (post-fork-decision) any `pub use` aliases for the `ScoringSource` rename.

**Risk: a tiny per-domain perf cost.** Each domain's score load now routes through a `Box<dyn>` dispatch instead of a direct match arm. The diagnostics-ledger baseline (p95=18 ms) is end-to-end including 19 domains; the per-domain overhead is amortized.

**Tests (Phase 3):**
- All existing tests in `context.rs`, `server.rs`, `doctor.rs` must still pass — that's the regression bar.
- New integration test: register a fake `Mock` factory, decorate a domain to use `source_type: "mock"`, run scoring, observe the mock factory was invoked. Proves the dispatch path actually goes through factories.

**Ship criterion:** `cargo test --workspace --all-targets -- --test-threads=1` green; manual smoke `neurogrim score` produces identical output to pre-V5-MOD-1.

### Phase 4 — Perf-gate verification (Day 6–7, ~0.5 day)

**Goal:** Re-run the V5-FOUND-1 baseline capture protocol against the V5-MOD-1 implementation; verify p95_ms ≤ 19.

**Steps:**
1. Build with the V5-MOD-1 changes (debug profile, same as baseline).
2. Run `NEUROGRIM_DIAG=1 neurogrim score` — 5 warmup, ledger cleared, 30 measured (same protocol as the baseline JSON).
3. Run `neurogrim diag report --kind scoring --json`.
4. Compare against `roadmap/data/v5-scoring-baseline-2026-05-02.json` ceiling: `p95_ms ≤ 19`.
5. **If pass:** record the result in a sibling `v5-mod-1-perf-result-2026-05-<dd>.json` under `roadmap/data/`; mark the perf-gate done-when. Decision recorded in epic: "dyn-dispatch acceptable; generic-bounded fallback not invoked."
6. **If fail:** halt Theme B per the adversary BLOCKING gate. Pivot to generic-bounded + small enum dispatch (Phase 4-fallback in this plan). The fallback design is sketched in §"Fallback design" below.

**Ship criterion:** perf-gate result documented; if pass, V5-MOD-1 cleared for Phase 5.

### Phase 5 — Conformance suite (Day 7–8, ~1.5 days)

**Goal:** Generalize the per-factory tests into a reusable conformance suite that any third-party `ScoringSource` impl must pass.

**Files (new):**
- `neurogrim/crates/neurogrim-core/src/scoring_source_conformance.rs` (or `tests/scoring_source_conformance.rs`)

**Test count target:** ≥8 tests covering happy path + ≥4 negative paths (per epic Done-When):
1. Happy path: factory builds, source loads, returns CmdbData.
2. Malformed config: source_type unknown → registry returns None (caller logs warn, falls through).
3. Unreachable endpoint (a2a-shaped test): source returns None.
4. Schema violation (cmdb-shaped test): malformed JSON returns None.
5. Factory panic safety: a panicking factory's `build()` is caught at registry boundary; returns None or surfaces as a tracing::error.
6. Concurrent loads: 100 parallel `load` calls don't deadlock or interleave (proves `Send + Sync`).
7. Idempotency: repeated `load` calls return equivalent results.
8. Privacy: ScoringSource impls don't leak source-config secrets into log output (visual review + grep test).

**Ship criterion:** suite passes on all three built-in factories; documented as the contract any third-party impl must pass.

### Phase 6 — Out-of-tree example crate (Day 8–10, ~1.5 days)

**Goal:** Ship `examples/scoring-source-prom/` — a third-party crate that registers a `Prom` scoring source (reads from a Prometheus endpoint). Proves the factory pattern unblocks third-party plugins without forking core.

**Files (new):**
- `neurogrim/examples/scoring-source-prom/Cargo.toml`
- `neurogrim/examples/scoring-source-prom/src/lib.rs`
- `neurogrim/examples/scoring-source-prom/README.md`

**Behavior:** the example HTTP-fetches a Prometheus query endpoint, parses the result, returns a `CmdbData`. Failure modes (unreachable, malformed) match the built-in `A2aSource` discipline.

**Integration test:** in a fixture project, register the Prom source via a registry that includes `source_type: "prom"`, run scoring, verify the Prom factory was consulted.

**Ship criterion:** example crate compiles + integrates via `cargo build -p scoring-source-prom`; integration test passes.

### Phase 7 — Epic close-out + LSP-Brains spec sync (Day 10, ~0.5 day)

- Update `v5-modular-conversions.md`: mark V5-MOD-1 status Complete; check off all Done-When items; correct the file-anchor errors in the epic prose (the `registry.rs:135-157` and `{jsonl, a2a, file}` references are stale).
- Cross-reference the perf-gate result JSON.
- Note the architectural decision (dyn-dispatch held; or generic-bounded fallback chosen + rationale).
- **LSP-Brains spec sync**: per plan-critic Subagent 3, `D:/Brains/LSP-Brains/spec/METHODOLOGY-EVOLUTION.md` line 1115-1120 names `ScoringSource` in spec prose. Either (a) update the spec to match the post-rename naming + add a "this name moved" note, or (b) add a cross-reference comment in this V5-MOD-1 close-out commit acknowledging the impl-vs-spec drift and queuing a follow-up spec PR. Plan favors (a) — the spec edit is small (rename + 2-sentence note) and avoids drift.
- **V5-SDK coordination cross-reference**: Theme C's V5-SDK epic re-exports stable contract types including `ScoringSource`. Add a note to `roadmap/epics/v5-sdk.md` or this close-out commit that V5-SDK should re-export the *trait* (not the config struct, post-rename) — important for SDK consumers, who care about behavior not config. Per plan-critic 🔵 #2.

## Files inventory

### New
- `neurogrim/crates/neurogrim-core/src/scoring_source.rs` (Phase 1: trait + registry)
- `neurogrim/crates/neurogrim-core/src/scoring_sources/{mod.rs,cmdb.rs,a2a.rs,function.rs}` (Phase 2)
- `neurogrim/crates/neurogrim-core/src/scoring_source_conformance.rs` (Phase 5; or `tests/`)
- `roadmap/data/v5-mod-1-perf-result-2026-05-<dd>.json` (Phase 4 output)
- `neurogrim/examples/scoring-source-prom/{Cargo.toml,src/lib.rs,README.md}` (Phase 6)

### Modified
- `neurogrim/Cargo.toml` (Phase 0: promote `async_trait` to `workspace.dependencies`)
- `neurogrim/crates/neurogrim-a2a/Cargo.toml` (Phase 0: switch to `async-trait = { workspace = true }`)
- `neurogrim/crates/neurogrim-core/Cargo.toml` (Phase 0: add `async-trait = { workspace = true }`)
- `neurogrim/crates/neurogrim-core/src/registry.rs` (Phase 0 if Option A: rename ScoringSource → ScoringSourceConfig)
- `neurogrim/crates/neurogrim-core/src/governance.rs:547` (Phase 0 if Option A: import-name update in test fixture)
- `neurogrim/crates/neurogrim-core/src/lib.rs` (Phase 0 if Option A: rustdoc + module-level mention; Phase 1: re-export new module)
- `neurogrim/crates/neurogrim-mcp/src/context.rs` (Phase 0 if Option A: parameter type at line 290; Phase 3: replace the match in `load_cmdb_data` at line 218)
- `neurogrim/crates/neurogrim-mcp/src/server.rs` (Phase 3: fold `load_cmdb_from_disk`'s cmdb branch into unified factory path; line 75)
- `neurogrim/crates/neurogrim-mcp/src/doctor.rs` (Phase 3: validation check at line 155)
- `roadmap/epics/v5-modular-conversions.md` (Phase 7: status → Complete; file-anchor corrections)
- `roadmap/epics/v5-sdk.md` (Phase 7: V5-SDK re-export coordination note for the trait/config split)
- `D:/Brains/LSP-Brains/spec/METHODOLOGY-EVOLUTION.md` (Phase 7: spec-prose sync at line 1115-1120 if Option A renamed the struct)

## Risks (from epic + new ones surfaced by this plan)

🔴 **BLOCKING — performance regression risk.** Inherited from epic. Mitigation: V5-FOUND-1 baseline + Phase 4 perf-gate.

🟡 **Name collision (`ScoringSource` struct vs trait)** — new finding from this plan. Mitigation: fork-decision before Phase 1 (Options A/B/C above).

🟡 **Two duplicate dispatch sites** (context.rs vs server.rs) — V5-MOD-1 is the natural moment to converge them. Mitigation: Phase 3 explicitly handles both, with `server.rs::load_cmdb_from_disk` either delegating to context.rs's helper or being removed.

🟡 **Factory-registration mechanism — `inventory` deferred.** Plan-critic surfaced that `inventory` is not in the workspace and the project enforces a 4-point dep-discipline pre-flight on new crates. Mitigation: Phase 2 uses a hand-rolled `HashMap<&'static str, Box<dyn ScoringSourceFactory>>` registered at startup — same 40 lines, zero supply-chain review burden. `inventory` reserved for v5.5 if a "register without explicit init call" demand emerges from real adopters.

🟡 **Async trait + Box<dyn> overhead — empirically validated as feasible.** Plan-critic Subagent 1 confirmed: workspace MSRV is 1.75 (RPITIT available as alternative), but `async_trait` is the established convention (3 existing async-trait usages, including `Transport` which is dispatched via `Box<dyn>` in production at `neurogrim-cli/src/commands/queue.rs:2`). The boxing overhead is a known quantity that the V5-FOUND-1 baseline (`p95_ms ≤ 19`) already absorbs implicitly. The Phase 0 `async_trait` workspace promotion is a one-line edit; no architectural rework needed.

🟡 **Public-API breaking change (Option A rename).** Plan-critic Subagent 3 confirmed `neurogrim-core` is published to crates.io. Option A (rename `ScoringSource` → `ScoringSourceConfig`) is a semver-MAJOR breaking change for any downstream crate importing the type. **This is the open fork-decision**: (a) accept the major bump and document in CHANGELOG; (b) ship a `pub use registry::ScoringSourceConfig as ScoringSource` back-compat alias for one minor release, then drop in the next; (c) avoid the rename entirely by naming the trait `ScoringSourceImpl` (Option B from §"Naming decision"). Plan-critic noted no clear winner — needs operator pin.

🟡 **LSP-Brains spec drift hazard.** Plan-critic Subagent 3 found `D:/Brains/LSP-Brains/spec/METHODOLOGY-EVOLUTION.md:1115-1120` names `ScoringSource` in spec prose. Mitigation: Phase 7 close-out includes an explicit spec-sync step (rename + 2-sentence note in the spec, or a queued follow-up if the spec PR can't be batched in).

🔵 **Fallback design — generic-bounded with small enum.** If the perf-gate fails, swap `Box<dyn ScoringSource>` for `enum BuiltinScoringSource { Cmdb, A2a, Function }` with `Box<dyn ScoringSource>` only for third-party impls. Built-ins skip dyn-dispatch; third parties pay it. Likely 80% of the modularity benefit at 20% of the perf cost.

🔵 **Suggestion — a `--list-scoring-sources` CLI flag** (forwarded from epic risks). Operator visibility into which factories are registered. Phase 6 nice-to-have, but defer to v5.5 polish if running long.

## Iteration boundaries

| Iter | Phases | Shippable? | Rough duration |
|---|---|---|---|
| 0 | Phase 0 (rename + dep probe) | Yes — refactor only, no behavior change | ~0.5 day |
| 1 | Phase 1 (trait def) | Yes — new module, no dispatch yet | ~1 day |
| 2 | Phase 2 (built-in factories) | Yes — factories independently usable | ~2 days |
| 3 | Phase 3 (dispatch conversion) | Yes — semantics unchanged, dispatch routed through trait | ~2 days |
| 4 | Phase 4 (perf-gate) | Yes — gate result documented | ~0.5 day |
| 5 | Phase 5 (conformance suite) | Yes — third-party-impl contract documented | ~1.5 days |
| 6 | Phase 6 (out-of-tree example) | Yes — proves modularity | ~1.5 days |
| 7 | Phase 7 (epic close-out) | Yes — epic Complete | ~0.5 day |

Total: ~9.5 days. Within epic M estimate (7–10 days).

## Verification (end-to-end, run after Iter 7)

1. `cargo test --workspace --all-targets -- --test-threads=1` green.
2. `neurogrim score` against NeuroGrim's own registry produces identical AgentOutput to pre-V5-MOD-1 (smoke).
3. `NEUROGRIM_DIAG=1 neurogrim score` (5+30 protocol) → `neurogrim diag report --kind scoring --json` → p95_ms ≤ 19 (the perf-gate ceiling).
4. `cargo build -p scoring-source-prom` succeeds; integration test confirms it's registered.
5. Conformance suite passes against all three built-in factories + the Prom example.
6. `neurogrim doctor` does not regress — domains with non-`cmdb` source_types still validate.

## What this plan does NOT do

- Does **not** implement V5-MOD-2 (Sensor trait) or V5-MOD-3 (QueueBackend factory) — those are sequential follow-on stories in Theme B.
- Does **not** add dynamic plugin loading (cdylib/libloading) — deferred to v5.5 BACKLOG B-40.
- Does **not** add a `--list-scoring-sources` CLI flag — v5.5 polish.
- Does **not** restructure the existing `ScoringSourceConfig` schema — only renames it (if Option A chosen).
- Does **not** touch LSP-Brains spec §9 — the conversion is internal; spec impl alignment may be a Phase 7 nicety if any cross-references shift.

## Cross-references

- Epic: `roadmap/epics/v5-modular-conversions.md` § V5-MOD-1
- Master roadmap: `roadmap/v5-roadmap.md` (§Adversary verdict — 🔴 perf gate)
- V5-FOUND-1 baseline: `roadmap/data/v5-scoring-baseline-2026-05-02.json` (ceiling: p95_ms ≤ 19)
- Existing dispatch (Phase 3 target): `neurogrim/crates/neurogrim-mcp/src/context.rs:218` (`load_cmdb_data`)
- Existing config struct: `neurogrim/crates/neurogrim-core/src/registry.rs:134-157` (`ScoringSource` — proposed rename to `ScoringSourceConfig`)
- Pattern reference: `neurogrim/crates/neurogrim-core/src/queue_backend.rs:69` (`QueueBackend` trait — already production-tested)
