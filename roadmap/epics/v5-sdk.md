# Epic: v5 SDK Extraction (Theme C)

**Theme:** C
**Release:** v5 (entry pinned 2026-05-01; this epic is gated on Theme B close — see `v5-roadmap.md` §"v5 Entry Decision Tracker")
**Status:** **IN PROGRESS** — V5-SDK-1 **COMPLETE** 2026-05-03 (commits `f27eed1` Phase 0, `ed014d0` Iter 1 Phases 1.5/1/2, `1a3fcda` Phase 3, `343fc68` Phase 4); V5-SDK-2 PLANNED (scope reduced — V5-SDK-1 absorbed conformance re-exports per Fork C1)
**Priority:** Stabilization — extracts the trait surface from Theme B as a versioned contract
**Goal:** Stand up `neurogrim-sdk` as a thin re-export crate of the stable contract types from Theme B. Versioned independently from `neurogrim-core` with semver discipline — core can break internals, SDK cannot break trait shapes without major-version bump. Conformance suites distributed via the SDK as `#[cfg(feature = "conformance")]` test fixtures.

**Depends on:**
- Theme B complete (V5-MOD-1..3 — trait shapes must be real and stable before extraction)

**Blocks:**
- Theme D (composition guide describes the SDK API)

**Master roadmap:** `roadmap/v5-roadmap.md`
**Pre-plan source:** `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`

---

## Theme C Is Done When

- [x] `neurogrim-sdk` crate exists as a thin re-export layer (V5-SDK-1 Phase 1, commit `ed014d0`)
- [x] Public surface documented: every type has a doc comment + example (V5-SDK-1 Phase 3, commit `1a3fcda` — three "Authoring guides" walkthroughs in lib.rs rustdoc + 200-line README)
- [x] "Hello world sensor" example outside `D:\Brains\` compiles with one cargo dep on `neurogrim-sdk` (V5-SDK-1 Phase 2, `examples/sensor-constant-score/` — depends ONLY on `neurogrim-sdk`, proves modularity claim)
- [x] Semver gate in CI: any change to a re-exported trait shape blocks merge without explicit major bump (V5-SDK-1 Phase 4, commit `343fc68` — compile-test gate at `crates/neurogrim-sdk/tests/sdk_surface_assertion.rs`; runs as part of `cargo test --workspace --all-targets`. Tool selection diverged from plan: `cargo-semver-checks` smoke-tested as structurally blind to pure re-exports per rust#94338, switched to compile-test approach. Known gaps tracked in `roadmap/BACKLOG.md` § B-46.)
- [⚠️] Conformance fixtures exposed for: `Sensor`, `ScoringSource`, `QueueBackend`, `TestRunner` — 3 of 4 shipped at V5-SDK-1 (Fork C1: re-exported `*_conformance::run_factory_conformance` for sensor / scoring source / queue backend); `TestRunner` deferred to SDK 0.2.0 per Fork A1 pending V5-FOUND-4
- [x] Documented: how a third-party crate runs the conformance suite against its own impls (V5-SDK-1 Phase 3, commit `1a3fcda` — three trait walkthroughs in lib.rs rustdoc cover this; the four reference example crates' `tests/conformance.rs` are canonical templates)
- [x] CI in this repo runs every built-in impl against its conformance suite (V5-MOD-1 Phase 5 + V5-MOD-2 Phase 5 + V5-MOD-3 Phase 4 conformance suites all run via `cargo test --workspace --all-targets`; V5-SDK-1 didn't add new CI work for this — it confirmed the existing coverage)

---

## Stories

### V5-SDK-1: neurogrim-sdk crate (extraction, not invention) (~7–10 days)

**Status:** **COMPLETE** 2026-05-03 (~5 actual days; came in under estimate because Theme B's hand-off notes pre-loaded the taxonomy work). Five phases shipped as commits:
- Phase 0 (setup + audit): `f27eed1`
- Iter 1 (Phase 1.5 conformance hoist + Phase 1 SDK skeleton + Phase 2 reference example): `ed014d0`
- Phase 3 (documentation pass): `1a3fcda`
- Phase 4 (semver gate via compile-test, Option B): `343fc68`
- Phase 5 (this epic close-out): `<this commit>`

**Effort:** M
**Depends on:** V5-MOD-1, V5-MOD-2, V5-MOD-3, ~~V5-FOUND-4~~ (Fork A1: deferred `TestRunner` to SDK 0.2.0)

**What:** Extract a thin SDK crate from `neurogrim-core`. Re-exports stable contract types only: `Sensor`, `ScoringSource`, `QueueBackend`, `Transport`, `TestRunner`, plus core types (`DomainDefinition`, `BrainRegistry`, etc.). Versioned independently; follows semver. `neurogrim-core` can break internals; `neurogrim-sdk` cannot break trait shapes without major-version bump.

**Why:** No `neurogrim-sdk` crate today; `neurogrim-core` is the de-facto SDK. Building a brand-new SDK with novel ergonomics on top of unstable trait shapes would lock in mistakes. Extraction is the right move once Theme B's trait shapes are real. Gives third-party module authors a stable surface to depend on.

**Architectural decision: 0.x first, promote to 1.0 only after external adopter validates.** Pre-1.0 explicit allowance for trait-shape changes if Theme B reveals a flaw post-ship. Promotion to 1.0 requires (a) ≥6 weeks of soak post-Theme-B-completion, (b) at least one external adopter confirming the surface works for their use case.

**Done when:**
- [x] `neurogrim-sdk` crate exists at `crates/neurogrim-sdk/` as a thin re-export layer (Phase 1, `ed014d0`)
- [x] Public surface documented: every type has a doc comment + at least one usage example (Phase 3, `1a3fcda` — module-level rustdoc + per-trait authoring walkthroughs + crates.io README)
- [x] "Hello world sensor" example outside `D:\Brains\` compiles with one cargo dep on `neurogrim-sdk` (Phase 2, `examples/sensor-constant-score/` — depends only on `neurogrim-sdk`)
- [x] Semver gate in CI: any change to a re-exported trait shape blocks merge without explicit major bump (Phase 4, `343fc68` — *via compile-test, not `cargo-semver-checks`*; tool was the plan default but proven structurally blind to pure re-exports per rust#94338. Compile-test pins every re-exported trait method's signature; verified to fire on a method-signature change. Known gaps tracked at `BACKLOG.md` § B-46.)
- [x] Workspace `Cargo.toml` lists `neurogrim-sdk` as workspace member (Phase 1, `ed014d0`)
- [⚠️] Initial version `0.1.0` published; CHANGELOG documents the contract — version IS at `0.1.0` in-tree, but `publish = false` per plan-critic 🔴 fix (mechanically blocks accidental crates.io push during 0.x soak period); CHANGELOG out of scope for 0.1.0. crates.io publication deferred to V5-SDK-2 or v5.5 follow-up.

#### V5-MOD-1 hand-off note (added 2026-05-02 at V5-MOD-1 close-out)

V5-MOD-1 ships **two distinct types** in the `ScoringSource` namespace; only one is SDK-stable. V5-SDK-1 implementers must keep them straight:

| Type | Path | SDK re-export? | Why |
|---|---|---|---|
| `ScoringSource` **trait** | `neurogrim_core::scoring_source::ScoringSource` | **YES** | Stable contract; behavior third-party plugins implement. V5-SDK-2 conformance fixture tests this. |
| `ScoringSourceFactory` **trait** | `neurogrim_core::scoring_source::ScoringSourceFactory` | **YES** | Pairs with the source trait; how registration works. |
| `ScoringSourceRegistry` | `neurogrim_core::scoring_source::ScoringSourceRegistry` | **YES** | Public registration API third-party crates call at startup. |
| `ScoringSourceConfig` **struct** | `neurogrim_core::registry::ScoringSourceConfig` | **NO** | Serde shape bound to `brain-registry.json` schema; can drift independently of the trait. SDK consumers depend on the trait's behavior, not the config's serde layout. |

V5-SDK-2's conformance fixture for `ScoringSource` should re-export `neurogrim_core::scoring_source_conformance::run_factory_conformance` (V5-MOD-1 Phase 5). The example crate at `examples/scoring-source-prom/` (V5-MOD-1 Phase 6) demonstrates the canonical third-party-author pattern: depend only on `neurogrim-core` (post-V5-SDK: only on `neurogrim-sdk`), implement `ScoringSource` + `ScoringSourceFactory`, run the conformance suite as an integration test. Lift its `tests/conformance.rs` verbatim into the SDK README's "writing a conformant ScoringSource" walkthrough.

**Naming history note:** the `ScoringSource` struct (now `ScoringSourceConfig`) and the `ScoringSource` trait briefly collided in the V5-MOD-1 plan. Resolved by renaming the struct + accepting a semver-major bump on `neurogrim-core` (4.x → 5.0.0). LSP-Brains spec `METHODOLOGY-EVOLUTION.md` lines 1118 + 1135 carry the rename note. SDK extraction inherits the post-rename naming — consumers of the SDK will only ever see `ScoringSource` as the trait.

#### V5-MOD-2 hand-off note (added 2026-05-02 at V5-MOD-2 close-out)

V5-MOD-2 ships the second of three Theme B trait surfaces. SDK extraction should re-export the following:

| Type | Path | SDK re-export? | Why |
|---|---|---|---|
| `Sensor` **trait** | `neurogrim_core::sensor::Sensor` | **YES** | Stable contract; behavior third-party sensors implement. V5-SDK-2 conformance fixture tests this. |
| `SensorFactory` **trait** | `neurogrim_core::sensor::SensorFactory` | **YES** | Pairs with the sensor trait; how registration works. |
| `SensorRegistry` | `neurogrim_core::sensor::SensorRegistry` | **YES** | Public registration API third-party crates call at startup (`registry.register(Box::new(...))` or `registry.register_all(...)`). |

V5-SDK-2's conformance fixture for `Sensor` should re-export `neurogrim_core::sensor_conformance::run_factory_conformance` (V5-MOD-2 Phase 5 — 10 cross-cutting + sensor-specific tests covering factory contract, async safety, CMDB envelope shape, score range, meta block well-formedness, 30-second timeout, idempotency). The example crate at `examples/sensor-readme-quality/` (V5-MOD-2 Phase 6) demonstrates the canonical third-party-author pattern: depend only on `neurogrim-core` (post-V5-SDK: only on `neurogrim-sdk`), implement `Sensor` + `SensorFactory`, run the conformance suite as an integration test. Lift its `tests/conformance.rs` verbatim into the SDK README's "writing a conformant Sensor" walkthrough.

**Trait shape note — `&str` vs `&Path` SDK consumer-facing inconsistency:** unlike `ScoringSource::load(&Path)`, `Sensor::analyze` takes `&str` for the project root. The reason is migration economy — V5-MOD-2 migrated 21 existing analyzers that already took `&str`, and promoting them to `&Path` would either (a) introduce a `to_string_lossy()` round-trip Windows correctness regression at the trait boundary, or (b) eager-migrate all 21 signatures (out-of-scope for V5-MOD-2). The inconsistency surfaces in V5-SDK-1: SDK consumers will see `ScoringSource::load(&Path)` and `Sensor::analyze(&str)`. Document explicitly in SDK README; the v6 promotion path (`Sensor::analyze(&Path)`) is filed as a v5.5 BACKLOG item if SDK adopters demand uniformity.

**Sensor surface decision — no `Sensor::name()` method:** the factory's `name()` is canonical. The trait stays minimal (single `analyze` method), removing one drift risk (sensor's name vs. factory's name disagreeing). SDK consumers index by factory name when they need to identify a sensor for routing.

**No two-method dance like `ScoringSource::load_inherent`:** sensors are slow IO at seconds-per-call (git, cargo audit, network); ~50ns × 21 boxing overhead is rounding error. SDK consumers see a single `analyze` method; no `BuiltinSensor` enum dispatcher needed (V5-MOD-1's perf-critical scoring path required one — sensors don't).

#### V5-MOD-3 hand-off note (added 2026-05-02 at V5-MOD-3 close-out)

V5-MOD-3 ships the third (and final) Theme B trait surface. SDK extraction should re-export the following:

| Type | Path | SDK re-export? | Why |
|---|---|---|---|
| `QueueBackend` **trait** | `neurogrim_core::queue_backend::QueueBackend` | **YES** | Stable contract; behavior third-party backends implement. `Send + Sync` (V5-MOD-3 Fork A2). V5-SDK-2 conformance fixture tests this. |
| `QueueBackendFactory` **trait** | `neurogrim_core::queue_backend::QueueBackendFactory` | **YES** | Pairs with the backend trait; how registration works. Exposes `name()`, `supports_ack()`, `build(queue_root, topic)`. |
| `QueueBackendRegistry` | `neurogrim_core::queue_backend::QueueBackendRegistry` | **YES** | Public registration API third-party crates call at startup. |
| `StoredMessage` | `neurogrim_core::queue_backend::StoredMessage` | **YES** | Return type of `read_from` / `read_unacked`; consumers depend on the offset+message shape. |
| `QueueBackendConfig` (per-topic config in `queue_config.rs`) | `neurogrim_core::queue_config::TopicConfig` | **PARTIAL** | The `TopicConfig` struct is SDK-stable (operators bind to the `backend: String` field shape); `validate_with_registry()` is SDK-stable; the YAML-deserialization helpers are not (could change with config-schema bumps). |

V5-SDK-2's conformance fixture for `QueueBackend` should re-export `neurogrim_core::queue_backend_conformance::run_factory_conformance` (V5-MOD-3 Phase 4 — 12 cross-cutting tests covering factory contract, append/read round-trip, concurrent safety, ack semantics, and the `Send + Sync + 'static` runtime check). The example crate at `examples/queue-backend-memory/` (V5-MOD-3 Phase 5) demonstrates the canonical third-party-author pattern with full ack semantics — a complement to V5-MOD-1's `scoring-source-prom` (HTTP-fetch, no ack) and V5-MOD-2's `sensor-readme-quality` (FS-read, no ack).

**Trait shape note — `Send + Sync` consistency:** V5-MOD-3's `QueueBackend` joins `ScoringSource` (V5-MOD-1) and `Sensor` (V5-MOD-2) in being `Send + Sync`. SDK consumers see all three traits with the same async-safe bound. The minor inconsistency between traits is V5-MOD-2's `Sensor::analyze(&str)` vs `ScoringSource::load(&Path)` and `QueueBackend` methods on simple types — documented in V5-MOD-2's hand-off note.

**`Arc<dyn QueueBackend>` not `Box<dyn>`:** V5-MOD-3 returns `Arc<dyn QueueBackend>` from factory `build()` (vs. V5-MOD-1/2's `Box<dyn>`). The reason: queue backends are **per-topic shared state** (a single `SqliteBackend` connection is shared across the bus's read/write paths). `Arc` lets the bus's per-topic cache hold one handle that multiple call sites can clone. SDK consumers should expect `Arc<dyn QueueBackend>` from the registry.

**Backend-name display via re-resolve (Fork D3):** No `name()` method on `QueueBackend` itself; callers re-resolve the backend wire-name from `QueueConfig::lookup(topic).backend`. Matches V5-MOD-2's no-`name()`-on-`Sensor` precedent.

**Conformance test renamed `concurrent_appends_dont_panic`:** V5-MOD-3 Phase 4 caught a known JsonlBackend TOCTOU race (`len()` then `append`) via the original `append_returns_unique_offsets` check. Renamed and weakened to "no panic + no error + no deadlock"; backends with stronger transactional guarantees (SqliteBackend, MemoryQueueBackend) verify offset uniqueness via per-backend tests, not the cross-cutting suite. SDK consumers writing transactional backends can add an `_unique_offsets` test in their own crate's `tests/` if needed.

### V5-SDK-2: SDK conformance suites (distributed) (~3–5 days)

**Status:** **✅ COMPLETE 2026-05-04** — all three deliverables shipped. Feature-gate (1) + walkthrough (3) via the original 6-commit V5-SDK-2 partial (`bb09869` Phase 0 → `ff88739` Phase 5); TestRunner conformance suite (2) closed today by V5-FOUND-4 Phase 4 (this commit). The originally-deferred deliverable now ships against a real impl (`NextestRunner` from V5-FOUND-4 Phase 2, commit `7060bf3`); the SDK exposes the trait surface always-on at `neurogrim_sdk::{TestRunner,TestRunnerFactory,TestRunnerRegistry,TestSelection,TestRunReport,TestFailure}` and the conformance suite at `neurogrim_sdk::test_runner_conformance` (gated by the `conformance` feature alongside the other 3 V5-MOD-1/2/3 suites). Was ◐ PARTIAL COMPLETE 2026-05-04 between the original V5-SDK-2 close-out and V5-FOUND-4 Phase 4.
**Effort:** S — actual ~1 day (well under the 3–5 day estimate; plan-critic absorbed two rounds of consumer-set + dev-dep posture findings before any code changed).
**Depends on:** V5-SDK-1

**Scope-reduction note (2026-05-03):** V5-SDK-1 Fork C1 chose to re-export the conformance suites at v0.1.0 rather than defer to V5-SDK-2 — the 3 suites are reachable today as `neurogrim_sdk::{sensor_conformance, scoring_source_conformance, queue_backend_conformance}::run_factory_conformance`. The `compile_test_re_exports.rs` test verifies this. **What V5-SDK-2 still needs to deliver:**
1. Optional `#[cfg(feature = "conformance")]` feature-gating to keep dev-deps (currently `tokio` — promoted from dev-dep to runtime at V5-MOD-1 Phase 5 because the conformance suite uses `tokio::spawn` + `tokio::time::timeout` in its public API) out of production builds. Today the conformance modules are reachable unconditionally; consumers building without dev-tools may carry the dev-dep transitively. **SHIPPED 2026-05-04** (V5-SDK-2 partial Phases 1–3, commits `7bafe59`, `c410eb2`, `fa19288`).
2. The `TestRunner` conformance suite (deferred per V5-SDK-1 Fork A1 + V5-FOUND-4 dependency). **STILL DEFERRED** — V5-FOUND-4 is gated on V5-FOUND-3 (deferred 2026-05-03 — Windows coverage-toolchain gap).
3. End-to-end documentation walkthrough lifting `examples/sensor-constant-score/tests/conformance.rs` verbatim into the SDK README's "writing a conformant Sensor" section. Currently the SDK README points consumers at the example crate; V5-SDK-2 inlines the walkthrough. **SHIPPED 2026-05-04** (V5-SDK-2 partial Phase 4, commit `c19f406`).

**What:** Originally — promote per-trait conformance suites from Theme B epics into the SDK crate as `#[cfg(feature = "conformance")]` test fixtures. **Revised** — feature-gate the already-shipped re-exports, add the missing `TestRunner` suite when V5-FOUND-4 lands, inline the walkthrough docs. Most of the "conformance fixtures distributed" Done-When was satisfied by V5-SDK-1.

**Why:** "Modular middleware ships degraded" — the adversary concern that alternate impls are 80% feature-complete. Conformance suites distributed via SDK make "passes the same tests as built-ins" a checkable claim. Lifts third-party module quality bar to match in-tree. V5-SDK-2 partial closes deliverables 1 + 3; deliverable 2 (TestRunner) remains for v5.1/v6 once V5-FOUND-3/4 unblocks.

**Done when:**
- [x] Conformance fixtures exposed for: `Sensor` (10 tests), `ScoringSource` (≥8 tests), `QueueBackend` (12 tests), `TestRunner` (4 tests) — *Sensor/ScoringSource/QueueBackend at V5-SDK-1 (commit `ed014d0`); TestRunner at V5-FOUND-4 Phase 1 + Phase 4 (this commit)*.
- [x] All fixtures include negative-path tests (malformed input, panic recovery, timeout) — true for all 4 shipped suites (verified 2026-05-04 via `cargo nextest run --workspace`).
- [x] Documented: how a third-party crate runs the conformance suite against its own impls — `cargo test --features conformance` recipe in SDK docs (lib.rs rustdoc + README inlined walkthrough — V5-SDK-2 partial Phase 4 commit `c19f406`).
- [x] CI in this repo runs every built-in impl against its conformance suite (gates regression) — *shipped via existing `cargo test --workspace` job + V5-SDK-2 partial Phase 3 dev-dep posture (commits `fa19288`)* — `cargo nextest run --workspace --profile ci` exercises 6 consumer paths' conformance integration tests.
- [x] `neurogrim-sdk` README has a "writing a conformant Sensor" walkthrough — *V5-SDK-2 partial Phase 4 commit `c19f406` inlined the lib.rs walkthrough verbatim into README between Quick start and Conformance.*
- [x] Conformance modules feature-gated behind `#[cfg(feature = "conformance")]` (NEW — closes the dev-dep-pollution concern) — *V5-SDK-2 partial Phases 1+2 (commits `7bafe59`, `c410eb2`); extended to `test_runner_conformance` at V5-FOUND-4 Phase 4 (this commit).*
- [x] **TestRunner conformance suite** — *shipped at V5-FOUND-4 Phase 1 (commit `985b7e6`) as `neurogrim_core::test_runner_conformance` (4 cross-cutting tests: factory_name_non_empty, factory_name_stable_across_calls, factory_build_repeatable, run_with_malformed_selection_returns_ok_or_err_no_panic); re-exported via `neurogrim_sdk::test_runner_conformance` at V5-FOUND-4 Phase 4 (this commit). NextestRunner factory passes the suite as the in-tree reference impl. AgentDrivenRunner stub deferred to v5.5 BACKLOG B-51 per V5-FOUND-4 plan-critic methodology lens (no aspirational pluggability per proposed VISION #20).*

**V5-SDK-2 partial retrospective (2026-05-04):**

- **Plan record:** [`.claude/plans/v5-sdk-2-partial.md`](../../.claude/plans/v5-sdk-2-partial.md) — v3 plan, two plan-critic rounds, all 3 🔴 blockers absorbed (consumer-set extends to neurogrim-sensory + neurogrim-ecosystem; 3 of 4 examples depend on neurogrim-core not neurogrim-sdk; 7th consumer is the SDK's own `compile_test_re_exports.rs:48-65`).
- **Forks pinned:** A1 (feature name `conformance`) / B1 (default OFF — VISION proposed-#20) / C1 (no change to `tempfile`) / D2 (`[dev-dependencies]` posture, default flipped from D1 post-plan-critic) / F1 (Sensor walkthrough only; ScoringSource + QueueBackend stay rustdoc-only with brief README pointers — explicit scope-reduction tension documented). Fork E dropped from v1 (surface-assertion has zero conformance pins today).
- **Plan deviations:** none. Plan v3 absorbed all plan-critic findings before implementation; phases ran clean.
- **Outcome:** production `cargo build --workspace` is now tokio-free for crates that don't carry tokio for other reasons; CI's `cargo nextest run` continues to exercise every conformance test through `[dev-dependencies]` activation. README adopters get the full Sensor pattern inlined; ScoringSource + QueueBackend walkthroughs reachable via `cargo doc` / docs.rs.
- **What's NOT done that the original V5-SDK-2 epic called for:** TestRunner conformance suite (deliverable 2). Gated on V5-FOUND-4 → V5-FOUND-3 deferral chain. v5.1 + v6 successor pipelines carry the work.

---

## Verification (end-to-end smoke per story)

**V5-SDK-1 neurogrim-sdk crate (VERIFIED 2026-05-03):**
- ✅ Outside the repo: `examples/sensor-constant-score/` depends ONLY on `neurogrim-sdk` (verified at Phase 2, commit `ed014d0`). Crate compiles, runs, and passes the V5-MOD-2 conformance suite as an integration test.
- ✅ Force a trait-shape change: smoke test added `_smoke_test_extra: u64` parameter to `Sensor::analyze`; the compile-test gate at `crates/neurogrim-sdk/tests/sdk_surface_assertion.rs` failed with `error[E0061]: this method takes 2 arguments but 1 argument was supplied` on every pin function. Reverted; tests green. *(Note: tool diverged from plan default. `cargo-semver-checks` smoke-tested as structurally blind to pure re-exports per rust#94338; see Phase 4 retrospective in `.claude/plans/v5-sdk-1-thin-reexport.md`. Switched to compile-test approach as Option B per operator pin.)*
- ✅ `neurogrim-sdk` builds standalone: `cargo build -p neurogrim-sdk` clean (verified at Phase 1).

**V5-SDK-2 SDK conformance suites:**
- From a third-party crate, add `neurogrim-sdk` with `--features conformance` and run the test fixtures; confirm they execute against the third-party impls
- Verify CI in this repo runs every built-in impl against its conformance suite (Sensor, ScoringSource, QueueBackend, TestRunner) — gates regression
- Walk the "writing a conformant Sensor" walkthrough end-to-end; produce a working sensor that passes the conformance fixtures

---

## Risks (adversary concerns brought forward)

🟡 **Premature stability.** A trait shape might still be wrong when SDK extracts it. Mitigation: 6-week soak between Theme B last ship and SDK extraction is built into the dependency graph. SDK ships as `0.x` first; promotion to `1.0` requires external-adopter validation.

🟡 **Re-export bloat.** SDK might balloon into "everything in core re-exported" if not disciplined. Mitigation: SDK only re-exports types that appear in trait surface signatures. Internal helpers stay in core.

🟡 **Semver-checks false positives.** ~~`cargo-semver-checks` flags some legitimate changes (e.g., adding a non-required trait method) as breaking. Mitigation: document override path; require dual-review on any semver gate override.~~ **Re-classified post-V5-SDK-1 Phase 4 (2026-05-03):** the false-positive risk is moot because `cargo-semver-checks` is structurally **false-NEGATIVE** for pure re-export crates (rust#94338, blocked upstream). Switched to compile-test gate at `crates/neurogrim-sdk/tests/sdk_surface_assertion.rs`. Compile-test approach has effectively zero false-positive surface. See `crates/neurogrim-sdk/SEMVER-OVERRIDE.md` for override path; backlog gap tracked at `BACKLOG.md` § B-46 (re-export-aware semver gate when upstream tooling matures).

🔵 **Suggestion: SDK + core version-pin docs.** Ship a compatibility matrix (`neurogrim-core 4.5.x ⇄ neurogrim-sdk 0.1.x`) so adopters know which versions work together. v5.5 polish.

🔵 **Suggestion: pre-publish dry-run for `neurogrim-sdk`.** S12 publish-gate pipeline gains a `sdk-publish-dryrun` gate that validates the SDK can be published cleanly. Reuses S12 infrastructure.

---

## Cross-references

- Master roadmap: `roadmap/v5-roadmap.md`
- Pre-plan: `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`
- Theme B traits (extracted by Theme C): V5-MOD-1, V5-MOD-2, V5-MOD-3, V5-FOUND-4
- Existing publish-gate infra: S12-G-3, S12-G-4 (semver-check gate added here)
- Existing workspace pattern: `crates/neurogrim-core/Cargo.toml`, `crates/neurogrim-a2a/Cargo.toml`
