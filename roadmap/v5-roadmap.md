# v5 Roadmap — "Everything is Lego" (adversary-reviewed)

**Span:** Themes A–D (Foundation, Modular Conversions, SDK Extraction, Coherence) — stage numbers assigned at backlog-merge time
**Approved on:** 2026-05-01 via three strategic decisions (stage shape: open-ended; adversary trim: partial with successor pipeline; v4/v5 timing: pinned 2026-05-01 — same-day operator pin overrode the default "decide-later" stance — see §"v5 Entry Decision Tracker")
**Posture:** **adversary hat** worn throughout — adversarial review baked into every theme; trimmed items pre-scheduled in the v5.5/v6 successor pipeline (BACKLOG B-37..B-45) rather than rejected.
**Pre-plan source:** `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`

> **Why themes rather than pre-counted stages:** the user chose "open-ended staging — let stages be sized by epics, not pre-counted." Each theme groups 1–4 epics that share dependencies; themes are sequential. How many themes map to how many stages is a backlog-merge decision; v5 entry pinned 2026-05-01 (concurrent with in-flight v4.x S15/S16 work) — see §"v5 Entry Decision Tracker" for the revised reference points the V5-MOD-1 perf gate now uses.

---

## North-star reframe — from "everything in core" to "core + impl-able interfaces"

Through v4.x, NeuroGrim core has accumulated trait shapes that the spec calls for (`QueueBackend`, `Transport`, `SecretBackend`) plus several string-dispatch surfaces that should be traits (`ScoringSource` registry, sensory tools registry). v5 finishes the interface-and-implementation pattern at three high-leverage seams, extracts a thin SDK so users can build modules outside the core repo, and ships a diagnostic + test-speed foundation that keeps the dev loop fast as adoption scales.

v5 flips the surface orientation:
- **Core defines the shape; impls ship as crates.** Three traits become the "swap me" surface: `ScoringSource` (string dispatch becomes factory), `Sensor` (compile-time-only catalogue becomes pluggable), `QueueBackend` (already trait, factory plumbing missing).
- **SDK is extraction, not invention.** No new ergonomics. `neurogrim-sdk` re-exports the stable contract types from core with semver discipline.
- **Tests get fast, then they get smart.** nextest + sccache (Theme A) before per-test coverage selection (still Theme A, but later) before any of Theme B's modular conversions.

Four themes make this work, in this order:
1. **Foundation: Diagnostics + Test Speed (Theme A).** Modularity work needs measurements. Diagnostics first.
2. **Three Modular Conversions (Theme B).** Scoring source + sensory + queue. Not "everything."
3. **SDK Extraction (Theme C).** Thin re-export crate with semver gate.
4. **Coherence + Docs (Theme D).** Composition guide + VISION/spec alignment.

---

## Theme / epic table

| Theme | Title | Stories | Effort | Strict deps |
|-------|-------|---------|--------|-------------|
| **A** | **Foundation: Diagnostics + Test Speed** — 3/4 epics complete (V5-FOUND-1 ✅ 2026-05-02; V5-FOUND-2 ✅ 2026-05-03; V5-FOUND-3 ⏸ DEFERRED 2026-05-03 to v5.1/v6 — Windows coverage-toolchain gap, Phase 0 partial shipped at commit `39d7295`; V5-FOUND-4 ✅ COMPLETE 2026-05-04) | V5-FOUND-1..4 | ~3–5 weeks | S15 ships |
| **B** | **Three Modular Conversions** ✅ COMPLETE 2026-05-02 | V5-MOD-1..3 | ~4–6 weeks | Theme A |
| **C** | **SDK Extraction** ✅ COMPLETE 2026-05-04 (V5-SDK-1 ✅ COMPLETE 2026-05-03; V5-SDK-2 ✅ COMPLETE 2026-05-04 — TestRunner conformance suite closure via V5-FOUND-4 lifted V5-SDK-2 from PARTIAL to COMPLETE) | V5-SDK-1..2 | ~2–3 weeks | Theme B |
| **D** | **Coherence + Docs** | V5-DOC-1..2 | ~2 weeks | Theme C |

Theme order is firm; intra-theme epic order is firm. The pre-plan default was "concurrent v4.x + v5 work is not pursued" because Stage 15 changes UI surfaces v5 might re-touch — the operator **explicitly waived this default on 2026-05-01** to begin V5-FOUND-1 (Diagnostic Monitor) concurrently with in-flight S15 / S16 work. V5-FOUND-1 is the safest concurrent epic because it adds tracing instrumentation (additive — does not modify scoring or UI surfaces). Theme B (modular conversions) remains gated on Theme A close + a re-check of the concurrent-work risk before V5-MOD-1's perf-gate runs.

---

## Conventions used in this roadmap

These deliberate choices differ from v4-era patterns; readers should not flag them as drift:

- **"Theme X Is Done When" rather than "Stage X Is Done When"** — intentional. Per the user decision (2026-05-01) to use open-ended staging, themes group epics but stage numbers are not pre-counted. When v5 epics merge into formal stages, the theme-level Done When folds into a per-stage Done When.
- **Epic IDs use the `V5-` prefix** (e.g., `V5-FOUND-1`, `V5-MOD-1`) rather than the existing `S<N>-<TAG>-<n>` pattern. The `V5-` prefix is provisional — when the backlog merges these into formal stages, the actual stage prefix prepends (e.g., `S<N>-V5-FOUND-1`) following the project's existing ID convention.
- **Cross-references use forward-slash relative paths** (`../crates/neurogrim-core/...`) within NeuroGrim and absolute forward-slash paths (`D:/Brains/LSP-Brains/...`) for cross-Brain references.
- **No pre-counted stage entries in `ROADMAP.md`** — v5 appears as a "v5 pre-plan" callout only. Stage rows are added when individual themes pin to stage numbers at backlog-merge time.

---

## Adversary findings — these gate the themes

### A. "Everything becomes an interface" — reshape

The codebase already has well-placed traits: `QueueBackend` ([queue_backend.rs:69](../crates/neurogrim-core/src/queue_backend.rs)), `Transport` ([transport.rs:56](../crates/neurogrim-a2a/src/transport.rs)), `SecretBackend` ([backend.rs:79](../crates/neurogrim-secrets/src/backend.rs)). It also has hardcoded surfaces that *should* be traits — scoring source dispatch is a string match in [registry.rs:135–157](../crates/neurogrim-core/src/registry.rs), sensory tools are compile-time only ([sensory/lib.rs](../crates/neurogrim-sensory/src/lib.rs)). And it has surfaces where converting to a trait would be Java-style ceremony with no in-flight customer requesting it: per-domain custom types, agent-card versioning, trajectory model abstraction.

**Reshape rule:** A seam becomes a trait only when (i) ≥2 plausible alternate impls already exist or are in scope, OR (ii) an external user has asked for it, OR (iii) leaving it concrete is provably blocking adoption. v5 lands **3 conversions** (scoring source, sensory plugin, queue factory) — not "everything." Items that fail the rule today but might pass it later are tracked in BACKLOG B-37..B-45, not deleted.

### B. "50-test batches" — wrong primitive

NeuroGrim today runs `cargo test --workspace --all-targets` with default libtest parallelism. Per-test wall time is already ≤5s on the happy path; slow benches (60–90s) are correctly `#[ignore]`-marked. The actual bottlenecks (inferred from absence of any tooling addressing them):

- **No `cargo-nextest`** — slower startup, no per-test retry-on-flake, no automatic CPU/mem budgeting (nextest already does what 50-batch is trying to invent)
- **No build cache** — no sccache; minimal `.cargo/config.toml`
- **No incremental selection** — every change reruns 1,470 tests; no "run only tests covering changed symbol"

**Reshape rule:** v5 adopts cargo-nextest (subsumes the 50-batch idea), adds sccache, ships per-test-coverage-driven selection. Fixed batch size is **permanently rejected** — it would mask flake-on-ordering and cap parallelism artificially at peak machines. Per-test coverage absorbs the v4.x-deferred BACKLOG B-28.

### C. "Diagnostic agentic flow" — approve, with guardrails

Risk: agents narrate slowness instead of measuring it; recommendations drift toward "go faster" without a concrete A/B criterion. Mitigation baked into V5-FOUND-1 — the agent reads only **structured diagnostics output** (JSONL, like the existing invocation-ledger pattern at [disposition.rs:48](../crates/neurogrim-cli/src/commands/disposition.rs)), and any output it produces must cite the measured baseline plus a measurable target. No prose-only recommendations make it into the diagnostics ledger.

### D. "Per-test coverage → LSP-brain blast radius" — approve as test-selection feature, not as a domain (in v5)

LLVM source-based coverage adds 10–30% test runtime overhead. Acceptable as **opt-in** instrumented mode for `neurogrim test --select-by-coverage --since <git-rev>`. Storing the symbol→test map is fine for v5; declaring a Brain *domain* to score it is premature in v5 — pushed to v6 successor pipeline (BACKLOG B-44). Map first, score later.

### E. "Modular middleware ships degraded" — counter with conformance suites

Risk: every alternate impl is 80% feature-complete, sum of features available across "any combination" is less than the union of any one. Counter — each new trait ships with a **shared conformance test suite** that any impl must pass (already the pattern in [queue_backend.rs](../crates/neurogrim-core/src/queue_backend.rs) for its two backends — generalize this).

### F. SDK is extraction, not invention

There is no `neurogrim-sdk` crate today; `neurogrim-core` is the de-facto SDK. Building a brand-new SDK with novel ergonomics on top of unstable trait shapes would lock in mistakes. v5 SDK is a **thin re-export layer** of the (now-stable) trait shapes from Theme B, with semver discipline. Distinct from core — core can break internals, SDK cannot break trait shapes without major-version bump.

---

## Theme A — Foundation: Diagnostics + Test Speed

**Theme:** Modularity work needs measurements. The dev loop must not melt during multi-theme v5 work.

**Goal:** Land tracing-based diagnostics, cargo-nextest adoption, sccache, per-test coverage as opt-in build mode, and a minimal `TestRunner` trait. After Theme A: dev loop is fast, agent can synthesize bottlenecks with measured baselines + targets, and we have data to validate Theme B's modularity claims.

**Architectural anchors (already exist):**
- **Invocation ledger pattern** ([disposition.rs:48](../crates/neurogrim-cli/src/commands/disposition.rs)) — JSONL append-only, schema_version, gitignored. Diagnostics ledger reuses the same writer signature.
- **Test-failure ledger** ([test.rs](../crates/neurogrim-cli/src/commands/test.rs)) — already structured per-failure entries; nextest integration is additive.
- **Port allocator** ([ports.rs](../crates/neurogrim-core/src/ports.rs)) — IANA dynamic-range allocation persisted to `.claude/brain/ports.json`. Removes port-collision concern for batch parallelism.
- **`tracing-subscriber`** is already a workspace dep (181 occurrences across crates); spans + emit are additive.

**Stories:** V5-FOUND-1..4 (see `epics/v5-foundation.md`).

**Adversary concerns:**
- 🟡 **Tracing instrumentation overhead.** Mitigation: span macros wrap existing functions with zero-cost when subscriber is disabled; production default disabled.
- 🟡 **Coverage build mode 10–30% slower.** Mitigation: opt-in only; default `neurogrim test` is unchanged.
- 🟡 **Symbol→test map staleness.** Mitigation: file-mtime + git-revision keys; map invalidates on any covering-file change.

---

## Theme B — Three Modular Conversions

**Theme:** Convert the three highest-leverage seams to trait+factory pattern. Each ships a conformance suite.

**Goal:** `ScoringSource` becomes `Box<dyn ScoringSource>` with factory registry; `Sensor` trait converts the existing sensors with cargo-feature-gate discovery (dynamic loading deferred to v5.5); `QueueBackend` factory replaces `BackendHandle` enum.

**Architectural anchors (already exist):**
- **`QueueBackend` trait** at [queue_backend.rs:69](../crates/neurogrim-core/src/queue_backend.rs) — already shape-correct; needs factory plumbing.
- **Per-topic config pattern** in `queue-config.yaml` — extends to user-registered backend types.
- **Inventory-based registries** — Rust's `inventory` crate or static linker tables; precedent in `metrics.rs` for series declarations.

**Stories:** V5-MOD-1..3 (see `epics/v5-modular-conversions.md`).

**Adversary concerns:**
- 🟡 **Inverse coupling smell.** `neurogrim-sensory` currently depends on `neurogrim-cli` (reverse of ideal). Theme B fixes this as part of V5-MOD-2.
- 🟡 **Conformance-suite coverage gaps.** Each trait's conformance must include negative-path tests (impl returns malformed CMDB; impl panics; impl times out). Otherwise "passes conformance" is too weak.
- 🔴 **BLOCKING — performance regression risk.** Dyn dispatch on hot scoring path could regress latency. V5-MOD-1 acceptance criterion: scoring round-trip latency unchanged within 5% of S15 baseline. If it fails, revisit dispatch pattern (generic-bounded vs dyn).

---

## Theme C — SDK Extraction

**Status:** IN PROGRESS — V5-SDK-1 **COMPLETE** 2026-05-03 (commits `f27eed1` Phase 0, `ed014d0` Iter 1, `1a3fcda` Phase 3, `343fc68` Phase 4, `<this commit>` Phase 5). V5-SDK-2 planned (scope reduced — V5-SDK-1 absorbed conformance re-exports per Fork C1).

**Theme:** Stabilize the contract surface. Thin re-export crate with semver gate.

**Goal:** `neurogrim-sdk` exists as a thin re-export layer of stable contract types. Versioned independently from `neurogrim-core` — core can break internals; SDK cannot break trait shapes without major-version bump. Conformance suites are `#[cfg(feature = "conformance")]` test fixtures distributed via the SDK.

**Architectural anchors:**
- **Type re-export pattern** — already used informally; SDK formalizes it with `pub use` discipline. ✅ shipped at V5-SDK-1.
- **CI semver checks** — ~~`cargo-semver-checks` crate exists; integrates as a publish gate~~. **Re-classified at V5-SDK-1 Phase 4 (2026-05-03):** `cargo-semver-checks` is structurally blind to pure re-export crates (rust#94338, blocked upstream). Switched to compile-test gate (`crates/neurogrim-sdk/tests/sdk_surface_assertion.rs`) which pins every re-exported trait method's signature mechanically. See `roadmap/BACKLOG.md` § B-46 for the upstream-tooling tracker.

**Stories:** V5-SDK-1..2 (see `epics/v5-sdk.md`).

**Adversary concerns:**
- 🟡 ~~**Premature stability.** A trait shape might still be wrong when SDK extracts it.~~ — **DEFANGED 2026-05-03** by shipping V5-SDK-1 at `0.1.0` with `publish = false`. The SDK is in-tree only during 0.x soak; explicit allowance for trait-shape changes via minor bumps. Promotion to 1.0 requires ≥6 weeks soak (earliest 2026-06-13) + at least one external-adopter validation.
- 🔵 ~~**Suggestion: ship SDK with a `0.x` version line first.**~~ ✅ adopted at V5-SDK-1.

---

## Theme D — Coherence + Docs

**Theme:** Composition guide written from shipped reality, plus VISION/spec alignment.

**Goal:** `docs/v5-composition-guide.md` documents real recipes; LSP-Brains spec §9 + §F reflect SDK trait shapes; VISION.md gains principle #20 ("Pluggability is justified by use, not aspiration"); `culture-coherence` domain still passes (byte-identity preserved across all four `.claude/culture.yaml` copies).

**Stories:** V5-DOC-1..2 (see `epics/v5-coherence.md`).

**Adversary concerns:**
- 🟡 **Doc rot.** Composition guide must include working code samples; CI builds them. Otherwise the guide drifts.
- 🟡 **Principle #20 inflation.** "Pluggability is justified by use" risks becoming a slogan. Dual-review skill (T+P) gates the principle's wording before merge.

---

## Cross-cutting concerns + methodology fit

### Principle alignment (cited from VISION.md)

| Principle | Bearing on v5 |
|-----------|---------------|
| #1 Declarations over dashboards | SDK plugin discovery is declarative (cargo features, factory registration) — not code edits |
| #6 Fractal by design | Each modular conversion preserves fractal composition; A2A child Brains can run their own combinations |
| #8 Absorption over invention | SDK is **extraction** (V5-SDK-1), not invention. Critical principle for the adversary lens. |
| #13 Domains are single-concern; coherence is the association cortex | New "diagnostic synthesis" / "blast-radius" Brain domains pushed to v6 — not invented in v5 |
| #16 Right protocol for the role — MCP for tools, A2A for peers | v5 must not blur this. Sensors stay MCP; child Brains stay A2A |
| #17 Culture is the substrate of communication | Any v5 culture update mirrored byte-identical across all four Brains |
| Proposed #20 Pluggability is justified by use, not aspiration | New principle proposed in V5-DOC-2; gates the v5.5/v6 successor pipelines (BACKLOG B-37..B-45) |

### Methodology fit

The four themes preserve LSP Brains methodology core invariants:
- **Honesty over plausibility:** diagnostics ledger surfaces real durations; agent recommendations cite measurements, not vibes.
- **Cumulative project awareness:** every theme adds an append-only ledger or extends one (V5-FOUND-1 extends the diagnostics surface; V5-FOUND-3 adds a coverage-map ledger; V5-MOD-* keep using existing CMDB schemas).
- **Cultural substrate:** culture.yaml stays read-only across themes. No v5 culture additions currently proposed; if one emerges during implementation, V5-DOC-2 covers byte-identical mirroring across all four copies per CLAUDE.md culture-changes-propagation rule.
- **Fractal composition:** SDK enables external Brains to ship custom modules without forking; the fractal-composition pattern (§9) extends without protocol changes.

### Backward compatibility commitments

- **CLI remains canonical.** Every new CLI sub-command (`neurogrim diag report`, `neurogrim diag synthesize`, `neurogrim test --select-by-coverage`) maps to documented flags; no existing flag removed.
- **JSONL files remain editable.** Diagnostics ledger, coverage-map ledger, all stay text-readable.
- **Existing scoring sources, sensors, queue backends keep working.** Theme B refactors dispatch; concrete types are preserved.
- **`neurogrim-sdk 0.x` may iterate.** Pre-1.0 explicit allowance for trait-shape changes.

### Decisions explicitly deferred (BACKLOG additions)

- **Dashboard widget plugin trait** — BACKLOG B-37 (v5.5 successor)
- **MCP tool plugin loading (dynamic)** — B-38 (v5.5 successor)
- **Transport runtime selection** — B-39 (v5.5 successor)
- **Dynamic .so/.dll plugin loading** — B-40 (v5.5 successor)
- **Per-domain custom CMDB types** — B-41 (v6 horizon)
- **Agent-card versioning trait** — B-42 (v6 horizon)
- **Trajectory model abstraction** — B-43 (v6 horizon)
- **Per-test coverage as Brain domain** — B-44 (v6 horizon)
- **Diagnostic synthesis as Brain domain** — B-45 (v6 horizon)

Each entry has explicit triggers per the user's "partial trim with successor pipeline" decision (2026-05-01).

---

## v5 Entry Decision Tracker

**Question:** When does v5 work begin?

**Locked decision (2026-05-01, revised same-day):** **pinned 2026-05-01.** v5 Theme A (V5-FOUND-1 Diagnostic Monitor) begins **concurrently** with in-flight v4.x work (S15 Command Post UI / S16 Plumbing). The original pre-plan stance was "concurrent v4.x + v5 work is not pursued by default"; the operator has explicitly waived that default — v5 entry was pinned by the third re-evaluation trigger ("Operator explicitly asks to pin a v5 entry date") on the same day the pre-plan was approved.

**Trigger that fired:** Operator explicit pin (2026-05-01).

**Re-evaluation triggers (kept on file for future stage transitions):**
- Stage 15 (S15-C-1..9) ships and v4.3 publishes through the gate pipeline.
- S13/S14 status crystallizes (no longer mid-flight; either complete or post-mortem'd).
- Operator explicitly asks to pin a v5 entry date. ← **fired 2026-05-01**

**Concurrent-work implications recorded at pin time:**
- **V5-MOD-1 performance gate** — the original adversary BLOCKING concern referenced "S15 baseline." Because v5 begins pre-S15-ship, the baseline V5-FOUND-1 captures will be against the current main branch state (pre-S15-ship), not post-S15-ship state. V5-FOUND-1's done-when criterion ("S15 scoring round-trip baseline captured in `roadmap/data/v5-scoring-baseline-<date>.json`") and V5-MOD-1's perf-gate ("scoring round-trip latency unchanged within 5% of S15 baseline") both inherit this revised reference point. If S15 ships UI changes that materially alter the scoring path before V5-MOD-1 runs, V5-FOUND-1 may need to re-capture the baseline at that point.
- **Stage-numbering** — v5 epics still use the `V5-` prefix until backlog-merge time. Stage rows in `ROADMAP.md` are added when individual themes pin to stage numbers; the open-ended-staging decision still holds.

**Decision owner:** project maintainer (single-operator today). Ecosystem-Brain agents may surface re-evaluation prompts via the diagnostics ledger if they observe Stage 15 closure or any of the other triggers.

**Where this decision lives** (single-source-of-truth: this section):
- This section (canonical statement; update here when triggers fire).
- `ROADMAP.md` "v5 pre-plan" callout (cross-reference).
- Each v5 epic file's `Release:` line and (for `v5-foundation.md`) gate banner.
- Pre-plan source: `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md` §9.

---

## Adversary verdict (final)

🔴 **Blocking concerns (1):**
- V5-MOD-1's dyn-dispatch latency must be within 5% of S15 baseline. Unverified until Theme B Story 1 ships. **If it regresses, revisit dispatch pattern (generic-bounded vs `dyn`) before continuing Theme B.**

🟡 **Major concerns (3):**
- "Everything becomes an interface" was rejected at the planning stage; if scope creep returns it during implementation, the v5.5/v6 successor pipeline is the explicit landing zone, not the v5 epics.
- Conformance suites must include negative-path tests; "passes happy path only" is too weak a guarantee for third-party impls.
- Theme C SDK extraction depends on Theme B trait shapes being stable; a 6-week soak between Theme B last ship and SDK extraction is built into the dependency graph.

🔵 **Suggestions (cross-cutting):**
- **Build the diagnostics ledger as the canonical "what's slow" log.** Future investigations benefit from a single source of truth across builds, tests, MCP, A2A, dashboard.
- **Ship `neurogrim-sdk` as `0.x` first.** Promote to 1.0 only after one external adopter validates.
- **Document the intentional non-goals** in each theme's epic file so adopters know what NOT to expect.

🟢 **Strengths:**
- Sequencing matches dependency graph (diagnostics → modularity → SDK → docs); no parallel paths needed.
- Each theme is self-contained value (Theme A alone is shippable as a dev-loop improvement); release-frequency stays high.
- All four themes reuse existing architectural anchors (invocation-ledger, port allocator, queue-config) rather than reinventing.
- Decisions made via AskUserQuestion (stage shape, adversary trim, v4/v5 timing) reduce ambiguity through the rest of v5.
- Adversarial review baked into each theme rather than appended.

---

## Per-theme epic files (depth)

- `roadmap/epics/v5-foundation.md` — V5-FOUND-1..4 (Theme A)
- `roadmap/epics/v5-modular-conversions.md` — V5-MOD-1..3 (Theme B)
- `roadmap/epics/v5-sdk.md` — V5-SDK-1..2 (Theme C)
- `roadmap/epics/v5-coherence.md` — V5-DOC-1..2 (Theme D)

Each epic file follows the existing convention from `S6-..S15-` epic files. Read them before starting the theme; revise them as work reveals reality.

---

## What this roadmap is NOT

- **Not pre-stage-numbered.** Stage assignment happens at backlog-merge time; could be 1, 2, 3, or 4 stages.
- **Not pre-committed to start date.** v5 entry pins when S13/S14 status crystallizes.
- **Not "everything is an interface."** Three seams, not many. The adversary trim is intentional and the v5.5/v6 successor pipeline (B-37..B-45) catches the rest with explicit triggers.
- **Not a substitute for plan-critic before implementation.** Each theme's epic file should get its own plan-critic pass when work begins.
