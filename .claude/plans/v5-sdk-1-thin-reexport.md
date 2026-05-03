# V5-SDK-1 Thin Re-export Crate — Implementation Plan

**Epic:** `roadmap/epics/v5-sdk.md` § V5-SDK-1
**Effort estimate (epic):** M, ~7–10 days
**Status:** drafted 2026-05-02; ships at `0.1.0` per epic's "0.x first" architectural decision
**Methodology:** plan-critic before implementation per `v5-roadmap.md` final note
**Substrate:** Theme B closed 2026-05-02 (V5-MOD-1 + V5-MOD-2 + V5-MOD-3 all complete) — three trait-surface stories that V5-SDK extracts

## Context

V5-SDK-1 stands up `neurogrim-sdk` as a **thin re-export crate** of the stable contract types from Theme B + adjacent stable traits (`Transport` from A2A, `SecretBackend` from `neurogrim-secrets`). Versioned independently from `neurogrim-core` with semver discipline — core can break internals; the SDK cannot break trait shapes without a major-version bump.

The crate itself is small: ~50–150 lines of `pub use` + module-level rustdoc + a `workspace_version()` helper. The substantial work is **NOT the code** but:

1. The re-export *taxonomy* — what's stable vs. what stays internal (the V5-MOD-1/2/3 hand-off notes already documented this).
2. The **semver gate** in CI — `cargo-semver-checks` (or equivalent) blocking merges that change a re-exported trait shape without an explicit major bump.
3. The **hello-world example** that compiles outside the NeuroGrim workspace with one `cargo` dep on `neurogrim-sdk`. This is what proves the modularity claim end-to-end.
4. The **documentation pass** — every re-exported type has a rustdoc comment + at least one example (V5-SDK-1 epic Done-When).

## File-anchor corrections (vs. the epic file)

The V5-SDK-1 epic story names these traits as Theme C re-exports: `Sensor`, `ScoringSource`, `QueueBackend`, `Transport`, `TestRunner`. Recon at V5-SDK-1 plan time:

| Epic says | Reality |
|---|---|
| `TestRunner` | Does not yet exist. V5-FOUND-4 (`TestRunner` trait + 2 impls) is unshipped — Theme A is only ~25% done (V5-FOUND-1 closed; V5-FOUND-2/3/4 unstarted). |
| (implicit) all 5 | `Sensor` ✓, `ScoringSource` ✓, `QueueBackend` ✓ (Theme B closed); `Transport` ✓ at `neurogrim-a2a/src/transport.rs:56` (`Send + Sync`, trait-based, dispatched via `Box<dyn>` in production); `TestRunner` ✗. |
| (not mentioned) `SecretBackend` | At `neurogrim-secrets/src/backend.rs:79`, `Send + Sync`, trait-based. v4.2 S14 secrets epic. **Candidate addition** to V5-SDK-1's surface — see Fork D. |

**Plan default — defer `TestRunner` to V5-SDK 0.2.0 (post-V5-FOUND-4)** rather than block V5-SDK-1 on the unshipped V5-FOUND-4 dependency. SDK is 0.x — explicit allowance for trait-shape changes; adding `TestRunner` in 0.2.0 is consistent with the epic's "0.x first, promote to 1.0 only after external adopter validates" decision. Fork B captures this.

## Recon-confirmed re-export inventory

| Type | Path | SDK-stable? | Source epic |
|---|---|---|---|
| `ScoringSource` trait | `neurogrim_core::scoring_source::ScoringSource` | YES | V5-MOD-1 |
| `ScoringSourceFactory` trait | `neurogrim_core::scoring_source::ScoringSourceFactory` | YES | V5-MOD-1 |
| `ScoringSourceRegistry` | `neurogrim_core::scoring_source::ScoringSourceRegistry` | YES | V5-MOD-1 |
| `run_factory_conformance` (sources) | `neurogrim_core::scoring_source_conformance::run_factory_conformance` | YES | V5-MOD-1 Phase 5 |
| `ConformanceReport` + `TestResult` (sources) | `neurogrim_core::scoring_source_conformance::*` | YES | V5-MOD-1 Phase 5 |
| `Sensor` trait | `neurogrim_core::sensor::Sensor` | YES | V5-MOD-2 |
| `SensorFactory` trait | `neurogrim_core::sensor::SensorFactory` | YES | V5-MOD-2 |
| `SensorRegistry` | `neurogrim_core::sensor::SensorRegistry` | YES | V5-MOD-2 |
| `run_factory_conformance` (sensors) | `neurogrim_core::sensor_conformance::run_factory_conformance` | YES | V5-MOD-2 Phase 5 |
| `ConformanceReport` + `TestResult` (sensors) | `neurogrim_core::sensor_conformance::*` | YES | V5-MOD-2 Phase 5 |
| `QueueBackend` trait | `neurogrim_core::queue_backend::QueueBackend` | YES | V5-MOD-3 |
| `QueueBackendFactory` trait | `neurogrim_core::queue_backend::QueueBackendFactory` | YES | V5-MOD-3 |
| `QueueBackendRegistry` | `neurogrim_core::queue_backend::QueueBackendRegistry` | YES | V5-MOD-3 |
| `StoredMessage` | `neurogrim_core::queue_backend::StoredMessage` | YES | V5-MOD-3 |
| `built_in_factories` (queue) | `neurogrim_core::queue_backend::built_in_factories` | YES | V5-MOD-3 |
| `run_factory_conformance` (queue) | `neurogrim_core::queue_backend_conformance::run_factory_conformance` | YES | V5-MOD-3 Phase 4 |
| `Transport` trait | `neurogrim_a2a::transport::Transport` | YES | v3.x A2A |
| `SecretBackend` trait | `neurogrim_secrets::backend::SecretBackend` | YES (Fork D) | v4.2 S14 |
| `BrainRegistry` | `neurogrim_core::registry::BrainRegistry` | YES | v3.x core |
| `DomainDefinition` | `neurogrim_core::registry::DomainDefinition` | YES | v3.x core |
| `AgentOutput` | `neurogrim_core::agent_output::AgentOutput` | YES | v3.x core |
| `QueueMessage` | `neurogrim_core::queue::QueueMessage` | YES | v4.1 S13 |
| `Priority` | `neurogrim_core::queue::Priority` | YES | v4.1 S13 |
| **NOT re-exported** | | | |
| `ScoringSourceConfig` struct | `neurogrim_core::registry::ScoringSourceConfig` | NO | bound to `brain-registry.json` schema; per V5-MOD-1 hand-off note |
| `TopicConfig`, `TopicConfigYaml` | `neurogrim_core::queue_config::*` | PARTIAL | `TopicConfig` shape stable; YAML helpers internal |
| `BrainRegistry` parsing helpers | `neurogrim_core::registry::*` | NO | implementation-specific |
| `JsonlBackend`, `SqliteBackend` impls | `neurogrim_core::queue_backend::{JsonlBackend, SqliteBackend}` | NO | impls aren't part of the contract; consumers should depend on the trait |
| `built_in_factories` for sensors | `neurogrim_sensory::built_in_factories` | NO | lives in sensory crate, not core; SDK consumers building their own binaries decide what to register |

## Architectural anchors (extending, not inventing)

| Anchor | What we reuse |
|---|---|
| Theme B trait + factory + registry pattern | The substrate V5-SDK-1 re-exports. No new contracts; pure re-export. |
| Existing workspace conventions (`workspace.dependencies`, `version.workspace = true`) | Cargo.toml conventions for the SDK crate |
| `neurogrim-core/data/schemas/cmdb-envelope-v1.schema.json` (vendored copy + drift-check via `xtask schema-drift-check`) | Pattern for any schemas the SDK might want to re-expose (none in v0.1.0; deferred) |
| V5-MOD-1/2/3 hand-off notes in `roadmap/epics/v5-sdk.md` | Pre-written taxonomy decisions: what re-exports + what stays internal |

## Naming decision — RESOLVED at planning (not a fork)

`neurogrim-sdk` is the obvious crate name; it matches the workspace's `neurogrim-*` convention and the epic's stated name. No collision with existing crates.

Initial version `0.1.0`. Per the epic: "0.x first; promotion to 1.0 requires (a) ≥6 weeks of soak post-Theme-B-completion, (b) at least one external adopter confirming the surface works for their use case." Theme B closed 2026-05-02; the earliest 1.0 promotion would be ~2026-06-13.

## Phases (incremental delivery)

### Phase 0 — Setup + audit (Day 1, ~0.5 day) — PREREQUISITE

**Goal:** Re-confirm the re-export taxonomy from the V5-MOD-1/2/3 hand-off notes; lock the v0.1.0 surface; pin operator decisions.

**Steps:**
1. **Re-verify all 5 trait paths exist** as documented in the inventory table above.
2. **Audit `Cargo.toml` deps cycle.** `neurogrim-sdk` will depend on `neurogrim-core`, `neurogrim-a2a`, `neurogrim-secrets` (per Fork D). None of those should depend on `neurogrim-sdk` — verify with `cargo tree` after Phase 1 lands.
3. **Pin fork decisions** — see "Forks" section below.
4. **Confirm semver-check tool choice** — Fork E.

**Ship criterion:** plan-internal; no code changes. The v0.1.0 surface is locked.

### Phase 1 — SDK crate skeleton + re-exports (Day 1–2, ~1 day)

**Goal:** Create `crates/neurogrim-sdk/` with `Cargo.toml`, `src/lib.rs`, and the full re-export surface. Unit tests verify each re-exported type compiles + is reachable via the SDK path.

**Files (new):**
- `crates/neurogrim-sdk/Cargo.toml`
- `crates/neurogrim-sdk/src/lib.rs`

**Files (modified):**
- `Cargo.toml` (workspace) — add `crates/neurogrim-sdk` to `members`

**Crate shape (sketch):**
```rust
// crates/neurogrim-sdk/src/lib.rs

//! # neurogrim-sdk
//!
//! Stable contract surface for third-party crates that extend
//! NeuroGrim. Versioned independently from neurogrim-core …
//!
//! ## What's here
//! - [`ScoringSource`] + factory + registry (V5-MOD-1)
//! - [`Sensor`] + factory + registry (V5-MOD-2)
//! - [`QueueBackend`] + factory + registry (V5-MOD-3)
//! - [`Transport`] (A2A peer protocol)
//! - [`SecretBackend`] (encrypted-secrets backend)
//! - 3 conformance suites: `scoring_source_conformance`,
//!   `sensor_conformance`, `queue_backend_conformance`
//! - Core types: [`BrainRegistry`], [`DomainDefinition`],
//!   [`AgentOutput`], [`QueueMessage`], [`Priority`]
//!
//! ## Stability
//! `0.x` — minor bumps are allowed to break trait shapes if Theme B
//! reveals a flaw post-ship. `1.0` requires ≥6 weeks of soak +
//! external-adopter validation. See V5-SDK epic for promotion
//! criteria.

pub use neurogrim_core::scoring_source::{
    ScoringSource, ScoringSourceFactory, ScoringSourceRegistry,
};
pub use neurogrim_core::sensor::{Sensor, SensorFactory, SensorRegistry};
pub use neurogrim_core::queue_backend::{
    built_in_factories as queue_built_in_factories, QueueBackend,
    QueueBackendFactory, QueueBackendRegistry, StoredMessage,
};
pub use neurogrim_a2a::transport::Transport;
pub use neurogrim_secrets::backend::SecretBackend;

// Conformance suites — re-export as nested modules so consumers
// write `neurogrim_sdk::sensor_conformance::run_factory_conformance(...)`.
pub mod scoring_source_conformance {
    pub use neurogrim_core::scoring_source_conformance::*;
}
pub mod sensor_conformance {
    pub use neurogrim_core::sensor_conformance::*;
}
pub mod queue_backend_conformance {
    pub use neurogrim_core::queue_backend_conformance::*;
}

// Core types reachable via flat re-exports.
pub use neurogrim_core::agent_output::AgentOutput;
pub use neurogrim_core::queue::{Priority, QueueMessage};
pub use neurogrim_core::registry::{BrainRegistry, DomainDefinition};

// V5-SDK-1 plan-critic 🟡 fix (2026-05-02): `workspace_compat_version()`
// helper dropped (YAGNI + drift hazard). Adopters reach the SDK
// crate's own version via `env!("CARGO_PKG_VERSION")` if they need
// it; cargo-version pins capture the compat in Cargo.toml directly,
// not in a hand-maintained literal.
```

**Cargo.toml shape:**
```toml
[package]
name = "neurogrim-sdk"
version = "0.1.0"
edition.workspace = true
authors.workspace = true
license.workspace = true
rust-version.workspace = true
description = "Stable contract surface for NeuroGrim plugin authors. Re-exports the V5-MOD-1/2/3 trait + factory + registry types + conformance suites + core types."
# V5-SDK-1 plan-critic 🔴 fix (2026-05-02): `publish = false`
# explicitly so `cargo publish` is mechanically blocked until the
# operator approves crates.io publication separately. Prevents
# accidental publication during V5-SDK-1's 0.x soak period; flip
# to `true` (or remove) when publication is approved (V5-SDK-2+).
publish = false
keywords = ["neurogrim", "sdk", "plugin", "lsp-brains"]
categories = ["development-tools"]

[dependencies]
neurogrim-core = { workspace = true }
neurogrim-a2a = { workspace = true }
neurogrim-secrets = { workspace = true }
```

**Tests (Phase 1 — compile-only):**
- `compile_test_each_re_export.rs` — for each re-exported trait, write a stanza like `let _: Box<dyn neurogrim_sdk::Sensor> = unimplemented!();` to verify the path resolves. ~20 lines total.
- `compile_test_conformance_suites_reachable.rs` — call each `run_factory_conformance` via the SDK path with `&dyn` of a stub factory; assert the function signature compiles.

**Ship criterion:** `cargo build -p neurogrim-sdk` clean; compile-tests pass; no new lint warnings.

### Phase 2 — Hello-world example (Day 2–3, ~1 day)

**Goal:** Ship a reference example that **compiles outside the NeuroGrim workspace** with one cargo dep on `neurogrim-sdk`. This is the modularity claim end-to-end.

**Files (new):**
- `examples/sensor-constant-score/Cargo.toml`
- `examples/sensor-constant-score/src/lib.rs`
- `examples/sensor-constant-score/tests/compiles_standalone.rs`
- `examples/sensor-constant-score/README.md`

**Behavior:** A toy `ConstantScoreSensor` that always reports `score: 42`. The example crate's Cargo.toml uses `neurogrim-sdk = "0.1"` (path-pinned in-tree for development; documented as crates.io for true third-party use). Tests verify the conformance suite passes against `ConstantScoreSensorFactory`.

**Why this example pattern:** simpler than the existing V5-MOD-1/2/3 examples (no HTTP, no FS, no concurrency); proves the SDK surface is reachable for the most basic case. Adopters who want richer patterns reference the existing examples.

**Ship criterion:** `cargo build -p sensor-constant-score` clean; conformance test green.

### Phase 3 — Documentation pass (Day 3–4, ~1 day)

**Goal:** Every re-exported type in the SDK has a doc comment + at least one usage example (epic Done-When).

**Approach:** the underlying `neurogrim-core` traits already have rustdoc (we wrote them in V5-MOD-1/2/3). The SDK re-exports inherit that doc. **What needs adding** is:

1. **SDK-level rustdoc** at `crates/neurogrim-sdk/src/lib.rs`:
   - Module-level introduction
   - Stability statement (0.x semantics)
   - "How to write a conformant `Sensor`" walkthrough
   - "How to write a conformant `ScoringSource`" walkthrough
   - "How to write a conformant `QueueBackend`" walkthrough
   - Each lifts the corresponding example crate's README
2. **Per-section module rustdoc** for each conformance submodule explaining when to use it.
3. **`README.md`** at `crates/neurogrim-sdk/README.md` — published with the crate to crates.io. Mirrors the lib.rs intro + adds the badge row, MSRV, links.

**Ship criterion:** `cargo doc -p neurogrim-sdk --no-deps` produces clean rustdoc with zero warnings; manually verify each re-exported type's doc shows up in the rendered output.

### Phase 4 — Semver gate (Day 4–6, ~2 days)

**Goal:** Wire `cargo-semver-checks` into CI to block merges that change a re-exported trait shape without an explicit major-version bump.

**Steps:**
1. **Tool selection (Fork E):** plan default `cargo-semver-checks` — most-maintained Rust semver tool, mature integration with `cargo`. Alternative: `rustsemverver` (less active).
2. **CI integration:** add a job to the workspace's CI pipeline (likely GitHub Actions) that runs `cargo install cargo-semver-checks` + `cargo semver-checks check-release -p neurogrim-sdk` on every PR.
3. **Baseline establishment:** publish `0.1.0` to crates.io (or use a fixture) so `semver-checks` has a baseline to compare against.
4. **Smoke test:** rename a method on a re-exported trait in a feature branch; confirm CI fails. Revert.
5. **Override path documentation:** the SDK epic notes "document override path; require dual-review on any semver gate override." Add a `SEMVER-OVERRIDE.md` documenting how to legitimately bump.

**Risk:** publishing to crates.io is a v5.5 commitment. Alternative: use a Git-based baseline (`semver-checks` supports `--baseline-rev <git-sha>`) so we don't need crates.io publication for the gate to work. **Plan default:** Git-based baseline; crates.io publication deferred to V5-SDK-2 or later.

**Ship criterion:** PR-blocking semver gate active in CI; smoke-test PR proves it works (gate fails on a fake breaking change, succeeds when reverted).

### Phase 5 — Epic close-out (Day 6, ~0.5 day)

- Update `v5-sdk.md`: mark V5-SDK-1 status COMPLETE; check off all Done-When items; add commit references.
- Update `v5-modular-conversions.md`: cross-reference V5-SDK-1 completion (Theme B → Theme C handshake).
- Update `roadmap/v5-roadmap.md`: Theme C entry status.
- LSP-Brains spec: SDK is implementation-specific (NeuroGrim-only — not spec'd); no spec sync needed.

## Files inventory

### New
- `crates/neurogrim-sdk/Cargo.toml`
- `crates/neurogrim-sdk/src/lib.rs`
- `crates/neurogrim-sdk/README.md`
- `crates/neurogrim-sdk/tests/compile_test_each_re_export.rs`
- `crates/neurogrim-sdk/tests/compile_test_conformance_suites_reachable.rs`
- `examples/sensor-constant-score/Cargo.toml`
- `examples/sensor-constant-score/src/lib.rs`
- `examples/sensor-constant-score/tests/conformance.rs`
- `examples/sensor-constant-score/README.md`
- `crates/neurogrim-sdk/SEMVER-OVERRIDE.md` (Phase 4)
- (CI config) — `.github/workflows/semver-check.yml` or wherever the workspace's CI lives

### Modified
- `Cargo.toml` (workspace) — add `crates/neurogrim-sdk` + `examples/sensor-constant-score` to `members`
- `roadmap/epics/v5-sdk.md` (Phase 5: status → Complete; commit references)
- `roadmap/epics/v5-modular-conversions.md` (Phase 5: cross-reference V5-SDK-1)
- `roadmap/v5-roadmap.md` (Phase 5: Theme C entry status)

## Forks — RESOLVED 2026-05-02 (operator pin: all 6 plan defaults)

| Fork | Resolution | Rationale (recap) |
|---|---|---|
| **A** | **A1 — Defer `TestRunner` to SDK 0.2.0** | V5-FOUND-4 unshipped; deferral is purely additive minor bump per plan-critic verification |
| **B** | **B1 — INCLUDE `SecretBackend`** | Already trait-based + `Send + Sync`; pure additive re-export; strengthens modularity claim |
| **C** | **C1 — Re-export conformance suites at V5-SDK-1** | One-line per module; consumers get verifiable contracts immediately |
| **D** | **D1 — DON'T re-export `JsonlBackend`/`SqliteBackend` impls** | SDK = contract crate; impls reach via direct `neurogrim-core` dep |
| **E** | **E1 — `cargo-semver-checks`** | Most-maintained Rust semver tool; `--baseline-rev` Git-only (no crates.io publication required) |
| **F** | **F1 — Hoist `ConformanceReport` + `TestResult` to shared module NOW** | ~1 hr cost; eliminates consumer-side type-mismatch before SDK 0.1.0 ships |

**Plus 4 plan-critic findings absorbed (non-fork fixes):**
- `publish = false` (🔴 BLOCKER): mechanically blocks accidental crates.io push during 0.x soak
- Example renamed `sdk-hello-world` → `sensor-constant-score`: matches V5-MOD-1/2/3 naming convention
- `workspace_compat_version()` helper dropped: YAGNI + drift hazard
- CI exists at `.github/workflows/ci.yml`: Phase 4 appends a job (not bootstrap)

## Forks — pre-pin debate (kept on file for traceability)

### Fork A — `TestRunner` inclusion in V5-SDK-1

V5-FOUND-4 (`TestRunner` trait + 2 impls) is **unshipped**. Theme A is only ~25% done — V5-FOUND-1 closed; V5-FOUND-2/3/4 unstarted.

| Option | What | Cost |
|---|---|---|
| **A1 — Defer to SDK 0.2.0** | V5-SDK-1 ships at v0.1.0 with the 5 traits that ARE ready (ScoringSource + Sensor + QueueBackend + Transport + SecretBackend). When V5-FOUND-4 ships later, V5-SDK 0.2.0 adds `TestRunner` as a minor bump. | Cleanest scope. SDK ships now with what's stable; consumers use it; TestRunner adds incrementally. |
| **A2 — Block V5-SDK-1 on V5-FOUND-4** | First do V5-FOUND-2/3/4 (Theme A completion, ~5–8 days), then V5-SDK-1. | Single 1.0-ready surface. But: V5-FOUND-4's deps (V5-FOUND-2/3) aren't even started. Significant Theme A work first. |
| **A3 — Stub `TestRunner` in SDK 0.1.0** | Re-export a placeholder `TestRunner` trait declared inside `neurogrim-sdk`, NOT `neurogrim-core`. Replace with the real V5-FOUND-4 trait when it ships. | Avoids 0.x → 0.x trait-shape break, but stub trait is dead-code until V5-FOUND-4 lands; misleading SDK documentation. |

**Plan default: A1 — Defer to SDK 0.2.0.** Reasons:
- The SDK's stated stability posture (`0.x first; trait-shape changes allowed in minor bumps if Theme B reveals a flaw`) explicitly accommodates incremental surface growth.
- Shipping V5-SDK-1 today gives Theme B-derived value to adopters NOW, instead of waiting on the unrelated Theme A completion.
- Adding `TestRunner` later is a clean minor bump (additive, no breaking changes to existing surface).

### Fork B — `SecretBackend` inclusion in V5-SDK-1

`SecretBackend` exists at `neurogrim-secrets/src/backend.rs:79` (v4.2 S14). It's `Send + Sync`, trait-based, dispatched via `Box<dyn>` in production. **Not Theme B**, but it IS a stable contract trait that third-party adopters might want.

| Option | What |
|---|---|
| **B1 — INCLUDE in v0.1.0** | Add `pub use neurogrim_secrets::backend::SecretBackend` to the SDK. Consumers who want to write a third-party secret backend (e.g., AWS KMS, HashiCorp Vault) get the contract from the SDK. |
| **B2 — DEFER to SDK 0.3.0** | Keep V5-SDK-1 strictly Theme B + Transport. SecretBackend joins the SDK after V5-SDK-1's first soak period. |

**Plan default: B1 — INCLUDE.** Reasons:
- Already trait-based; no migration needed. Pure additive re-export.
- Consistent with the V5-SDK epic's stated scope ("stable contract types"); SecretBackend qualifies.
- Strengthens the V5-SDK-1 modularity claim (5 trait surfaces, not 4) without expanding implementation work.

### Fork C — Conformance suite re-exports at V5-SDK-1 vs. defer to V5-SDK-2

The V5-SDK-2 epic explicitly handles "SDK conformance suites distributed." But each Theme B story's hand-off note documents that the conformance suites are SDK-stable and should be re-exported.

| Option | What |
|---|---|
| **C1 — Re-export at V5-SDK-1** | `pub mod sensor_conformance` etc. in v0.1.0. Consumers writing a third-party sensor copy the example crate's `tests/conformance.rs` and immediately have a verifiable contract. |
| **C2 — Wait for V5-SDK-2** | V5-SDK-2 (~3-5 days) wraps the conformance suites with feature gates (`--features conformance`) so the dev-deps don't pollute production builds. Cleaner long-term. |

**Plan default: C1 — Re-export at V5-SDK-1.** Reasons:
- The conformance suites already exist as public modules in `neurogrim-core`; re-exporting is one-line per module.
- The "feature-gating to avoid dev-dep pollution" concern (C2's pitch) is small — `tempfile` is the only dev-dep transitive, and it's already pulled by `neurogrim-core`'s test code.
- Adopters get the contract check immediately; V5-SDK-2 can refactor to feature-gating later as a non-breaking change.

### Fork D — `SqliteBackend` / `JsonlBackend` impls re-exported?

Built-in queue backend impls. Used by adopters who write their own binaries that compose NeuroGrim — they need to construct backends directly without going through the registry.

| Option | What |
|---|---|
| **D1 — DON'T re-export; consumers depend on `neurogrim-core` directly for impls** | Cleanest contract surface. Keeps SDK to traits + factories + types. |
| **D2 — Re-export the impls** | Consumers don't need to add `neurogrim-core` as a separate dep. Bloats SDK surface. |

**Plan default: D1 — DON'T re-export impls.** Reasons:
- The SDK is the **contract** crate; impls are implementation. Adopters who need impls add `neurogrim-core` as a direct dep.
- Aligns with V5-MOD-2's `built_in_factories()` deliberately living in `neurogrim-sensory`, not `neurogrim-core` — built-in impls are not the SDK surface.
- If many adopters complain, V5-SDK 0.2.0 can add a `built_ins` module behind a feature flag.

### Fork F — Hoist `ConformanceReport` + `TestResult` to shared module (NEW from plan-critic 🟡)

V5-MOD-1, V5-MOD-2, V5-MOD-3 each ship a `ConformanceReport` + `TestResult` that are **structurally identical** (same fields, same constructors). The 3 modules' rustdocs each carry a TODO acknowledging the duplication: *"A future v5.5 refactor could hoist both into a shared `crate::conformance` module."*

**Plan-critic 🟡 finding:** Consumers writing both a sensor + a queue backend import `ConformanceReport` from two SDK module paths. They are **different nominal types** to the compiler — even though structurally identical, you cannot cross-assign or merge them. Compile errors of the form *"expected `sensor_conformance::ConformanceReport`, found `queue_backend_conformance::ConformanceReport`"* will hit any consumer who tries to write a generic helper across both.

| Option | What | Cost |
|---|---|---|
| **F1 — Hoist now (in V5-SDK-1)** | Add Phase 1.5 (~1 hour): create `neurogrim_core::conformance::{TestResult, ConformanceReport}`. The 3 existing suite modules re-export the shared types. SDK adds `pub mod conformance` that re-exports those. | Touches 3 existing core files (each loses ~40 lines + adds 2-line re-export). One-time refactor cost; eliminates consumer-side type-mismatch hazard before SDK ships. |
| **F2 — Defer to v5.5** | Honor the existing TODO; ship V5-SDK-1 with the duplication. SDK consumers paper over the type mismatch in their own glue code. v5.5 cleanup hoists later, breaking 0.x SDK consumers' nominal-type assumptions (semver-major bump for the SDK at that point). | Smaller V5-SDK-1 scope. Risk: by the time v5.5 lands, consumers may have built code that hard-imports the duplicated types — semver-major bump for an architectural cleanup. |

**Plan default: F1 — Hoist now.** Reasons:
- Cheaper before SDK 0.1.0 ships than after (no consumers locked in to the duplicated nominal types yet).
- The architectural improvement is real; the existing TODOs explicitly named v5.5 as the target, but doing it AT SDK extraction (when the surface is being defined for outside eyes) is the canonical "do it once, do it right" moment.
- Adds ~1 hour to V5-SDK-1's effort budget; total stays well within the M estimate.

**Implementation sketch:**
```rust
// crates/neurogrim-core/src/conformance.rs (new)
//! Shared `ConformanceReport` + `TestResult` for V5-MOD-1/2/3
//! conformance suites (and any future suites). Single source of
//! truth so consumers writing multiple plugin types share one
//! type per conformance concept.
pub struct TestResult { pub name: &'static str, pub passed: bool, pub detail: Option<String> }
pub struct ConformanceReport { pub results: Vec<TestResult> }
// + impls (pass, fail, all_passed, passed_count, total, failures, add)
```

Then each existing suite module:
```rust
// crates/neurogrim-core/src/scoring_source_conformance.rs
pub use crate::conformance::{ConformanceReport, TestResult};
// (delete the per-suite duplicate definitions)
```

Plus the SDK:
```rust
// crates/neurogrim-sdk/src/lib.rs
pub mod conformance {
    pub use neurogrim_core::conformance::*;
}
```

### Fork E — Semver gate tool

| Option | Tool | Notes |
|---|---|---|
| **E1 — `cargo-semver-checks`** | `obi1kenobi/cargo-semver-checks` | Most-maintained, GitHub Actions integration, supports `--baseline-rev <git-sha>` (no crates.io required). Plan default. |
| **E2 — `rustsemverver`** | `rust-lang/rust-semverver` | Older, less active maintenance. |
| **E3 — Hand-rolled diff via `cargo public-api`** | DIY using public-API extractor | More work; less battle-tested. |

**Plan default: E1 — `cargo-semver-checks`.** Reasons:
- Active maintenance (last release 2024-Q4); known integrations (GitHub Actions, GitLab CI).
- Supports Git-based baselines (`--baseline-rev`), so we don't need to publish to crates.io for the gate to work.
- No extra workspace dep; runs as a CI tool.

## Risks (from epic + new ones surfaced by this plan)

🟡 **Premature stability if SDK extracts too early.** The V5-SDK epic's risk note: "A trait shape might still be wrong when SDK extracts it." Mitigation built into the plan: ship at `0.x`, explicit allowance for trait-shape changes, 6-week soak before 1.0.

🟡 **Re-export bloat.** SDK might balloon into "everything in core re-exported" if not disciplined. Mitigation: the per-fork "what to re-export" decisions are conservative defaults (D1 in particular keeps impls out).

🟡 **Semver-checks false positives.** Some legitimate changes (adding a non-required trait method with a default impl) get flagged as breaking. Mitigation: `SEMVER-OVERRIDE.md` documents the override path; require dual-review.

🟡 **Hello-world example dependency drift.** The example uses `neurogrim-sdk = "0.1"` in its template Cargo.toml, but the in-workspace test uses `path = "../../crates/neurogrim-sdk"`. Drift between "what we test" and "what adopters get" is a real risk. Mitigation: documented two Cargo.toml templates in the example README; a manual-test step verifies the crates.io shape works on a fresh-tempdir build.

🟢 **Theme B substrate** — already proven. V5-SDK-1 is mostly mechanical re-exports of types that already have their own conformance.

🔵 **Suggestion — start tracking SDK adoption metrics.** Once V5-SDK-1 ships at 0.1.0, the V5-SDK epic's "external adopter validation" criterion needs evidence-of-use. v5.5 polish: add an `[adopters]` section to `crates/neurogrim-sdk/README.md` for opt-in tracking.

## Iteration boundaries

| Iter | Phases | Shippable? | Rough duration |
|---|---|---|---|
| 0 | Phase 0 (audit) | Yes — plan-only, no code change | ~0.5 day |
| 1 | Phase 1 + 2 (skeleton + hello-world) | Yes — SDK exists at 0.1.0; example proves modularity | ~2 days |
| 2 | Phase 3 (docs pass) | Yes — every type documented | ~1 day |
| 3 | Phase 4 (semver gate) | Yes — gate active in CI | ~2 days |
| 4 | Phase 5 (close-out) | Yes — Theme C V5-SDK-1 marked complete | ~0.5 day |

Total: ~6 days. Within epic M estimate (7–10 days; plan came in slightly under because Theme B's hand-off notes pre-loaded the taxonomy work).

## Verification (end-to-end, after Iter 4)

1. `cargo build -p neurogrim-sdk` clean.
2. `cargo test -p neurogrim-sdk` green (compile-test re-exports work).
3. `cargo build -p sensor-constant-score` clean; conformance test green.
4. `cargo doc -p neurogrim-sdk --no-deps` zero warnings.
5. CI smoke: rename a method on `ScoringSource`; semver-checks fails the PR; revert; CI passes.
6. Manual: `cargo new fresh-test-crate; cd fresh-test-crate; cargo add neurogrim-sdk --path D:/Brains/NeuroGrim/neurogrim/crates/neurogrim-sdk; write a 30-line conformant sensor; cargo test`. Confirms the modularity claim from a clean slate.

## What this plan does NOT do

- Does **not** publish `neurogrim-sdk` to crates.io (V5-SDK-1 ships at 0.x; publication is a separate operator decision).
- Does **not** add `TestRunner` (Fork A1 deferral; SDK 0.2.0).
- Does **not** ship V5-SDK-2 (conformance suite feature-gating + walkthrough docs — a separate ~3-5 day epic).
- Does **not** add SDK-level new traits — pure re-export crate.
- Does **not** modify any Theme B trait shape. The traits are stable as-shipped 2026-05-02.

## Cross-references

- Epic: `roadmap/epics/v5-sdk.md` § V5-SDK-1
- V5-MOD-1 hand-off note: `v5-sdk.md` § V5-SDK-1 (added 2026-05-02 at V5-MOD-1 close)
- V5-MOD-2 hand-off note: `v5-sdk.md` § V5-SDK-1 (added 2026-05-02 at V5-MOD-2 close)
- V5-MOD-3 hand-off note: `v5-sdk.md` § V5-SDK-1 (added 2026-05-02 at V5-MOD-3 close)
- Theme B traits: `crates/neurogrim-core/src/{scoring_source,sensor,queue_backend}.rs`
- A2A trait: `crates/neurogrim-a2a/src/transport.rs:56`
- Secrets trait: `crates/neurogrim-secrets/src/backend.rs:79`
- V5-FOUND-4 (TestRunner): unshipped; tracked in `roadmap/epics/v5-foundation.md` § V5-FOUND-4
