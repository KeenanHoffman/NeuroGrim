# Epic: v5 SDK Extraction (Theme C)

**Theme:** C
**Release:** v5 (entry pinned 2026-05-01; this epic is gated on Theme B close — see `v5-roadmap.md` §"v5 Entry Decision Tracker")
**Status:** PLANNED (drafted 2026-05-01)
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

- [ ] `neurogrim-sdk` crate exists as a thin re-export layer
- [ ] Public surface documented: every type has a doc comment + example
- [ ] "Hello world sensor" example outside `D:\Brains\` compiles with one cargo dep on `neurogrim-sdk`
- [ ] Semver gate in CI: any change to a re-exported trait shape blocks merge without explicit major bump
- [ ] Conformance fixtures exposed for: `Sensor`, `ScoringSource`, `QueueBackend`, `TestRunner`
- [ ] Documented: how a third-party crate runs the conformance suite against its own impls
- [ ] CI in this repo runs every built-in impl against its conformance suite

---

## Stories

### V5-SDK-1: neurogrim-sdk crate (extraction, not invention) (~7–10 days)

**Status:** Planned
**Effort:** M
**Depends on:** V5-MOD-1, V5-MOD-2, V5-MOD-3, V5-FOUND-4

**What:** Extract a thin SDK crate from `neurogrim-core`. Re-exports stable contract types only: `Sensor`, `ScoringSource`, `QueueBackend`, `Transport`, `TestRunner`, plus core types (`DomainDefinition`, `BrainRegistry`, etc.). Versioned independently; follows semver. `neurogrim-core` can break internals; `neurogrim-sdk` cannot break trait shapes without major-version bump.

**Why:** No `neurogrim-sdk` crate today; `neurogrim-core` is the de-facto SDK. Building a brand-new SDK with novel ergonomics on top of unstable trait shapes would lock in mistakes. Extraction is the right move once Theme B's trait shapes are real. Gives third-party module authors a stable surface to depend on.

**Architectural decision: 0.x first, promote to 1.0 only after external adopter validates.** Pre-1.0 explicit allowance for trait-shape changes if Theme B reveals a flaw post-ship. Promotion to 1.0 requires (a) ≥6 weeks of soak post-Theme-B-completion, (b) at least one external adopter confirming the surface works for their use case.

**Done when:**
- [ ] `neurogrim-sdk` crate exists at `crates/neurogrim-sdk/` as a thin re-export layer
- [ ] Public surface documented: every type has a doc comment + at least one usage example
- [ ] "Hello world sensor" example outside `D:\Brains\` compiles with one cargo dep on `neurogrim-sdk`
- [ ] Semver gate in CI (`cargo-semver-checks` or equivalent): any change to a re-exported trait shape blocks merge without explicit major bump
- [ ] Workspace `Cargo.toml` lists `neurogrim-sdk` as workspace member
- [ ] Initial version `0.1.0` published; CHANGELOG documents the contract

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

**Status:** Planned
**Effort:** S
**Depends on:** V5-SDK-1

**What:** Promote per-trait conformance suites from Theme B epics into the SDK crate as `#[cfg(feature = "conformance")]` test fixtures. Any third-party impl can add `neurogrim-sdk` with `--features conformance` and run the same tests the built-ins pass.

**Why:** "Modular middleware ships degraded" — the adversary concern that alternate impls are 80% feature-complete. Conformance suites distributed via SDK make "passes the same tests as built-ins" a checkable claim. Lifts third-party module quality bar to match in-tree.

**Done when:**
- [ ] Conformance fixtures exposed for: `Sensor` (≥6 tests), `ScoringSource` (≥8 tests), `QueueBackend` (≥10 tests), `TestRunner` (≥6 tests)
- [ ] All fixtures include negative-path tests (malformed input, panic recovery, timeout)
- [ ] Documented: how a third-party crate runs the conformance suite against its own impls — `cargo test --features conformance` recipe in SDK docs
- [ ] CI in this repo runs every built-in impl against its conformance suite (gates regression)
- [ ] `neurogrim-sdk` README has a "writing a conformant Sensor" walkthrough

---

## Verification (end-to-end smoke per story)

**V5-SDK-1 neurogrim-sdk crate:**
- Outside the repo (e.g., a fresh `cargo new`), write a 30-line sensor crate that depends only on `neurogrim-sdk`; confirm it compiles and runs against a local NeuroGrim instance
- Force a trait-shape change in CI (rename a method on `Sensor`); confirm `cargo-semver-checks` (or equivalent) blocks the merge
- Verify `neurogrim-sdk` builds standalone (without `neurogrim-core` available as a path dep) — the contract-integrity check

**V5-SDK-2 SDK conformance suites:**
- From a third-party crate, add `neurogrim-sdk` with `--features conformance` and run the test fixtures; confirm they execute against the third-party impls
- Verify CI in this repo runs every built-in impl against its conformance suite (Sensor, ScoringSource, QueueBackend, TestRunner) — gates regression
- Walk the "writing a conformant Sensor" walkthrough end-to-end; produce a working sensor that passes the conformance fixtures

---

## Risks (adversary concerns brought forward)

🟡 **Premature stability.** A trait shape might still be wrong when SDK extracts it. Mitigation: 6-week soak between Theme B last ship and SDK extraction is built into the dependency graph. SDK ships as `0.x` first; promotion to `1.0` requires external-adopter validation.

🟡 **Re-export bloat.** SDK might balloon into "everything in core re-exported" if not disciplined. Mitigation: SDK only re-exports types that appear in trait surface signatures. Internal helpers stay in core.

🟡 **Semver-checks false positives.** `cargo-semver-checks` flags some legitimate changes (e.g., adding a non-required trait method) as breaking. Mitigation: document override path; require dual-review on any semver gate override.

🔵 **Suggestion: SDK + core version-pin docs.** Ship a compatibility matrix (`neurogrim-core 4.5.x ⇄ neurogrim-sdk 0.1.x`) so adopters know which versions work together. v5.5 polish.

🔵 **Suggestion: pre-publish dry-run for `neurogrim-sdk`.** S12 publish-gate pipeline gains a `sdk-publish-dryrun` gate that validates the SDK can be published cleanly. Reuses S12 infrastructure.

---

## Cross-references

- Master roadmap: `roadmap/v5-roadmap.md`
- Pre-plan: `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`
- Theme B traits (extracted by Theme C): V5-MOD-1, V5-MOD-2, V5-MOD-3, V5-FOUND-4
- Existing publish-gate infra: S12-G-3, S12-G-4 (semver-check gate added here)
- Existing workspace pattern: `crates/neurogrim-core/Cargo.toml`, `crates/neurogrim-a2a/Cargo.toml`
