# V5-SDK-2 partial — feature-gate conformance + inline walkthrough

**Epic:** [`roadmap/epics/v5-sdk.md`](../../roadmap/epics/v5-sdk.md) § V5-SDK-2 · **Theme C** · Effort S (1–2 days; reduced from S 3–5d after V5-SDK-1 absorbed conformance re-exports per Fork C1; effort revised slightly upward post-plan-critic to absorb the wider consumer-set fix) · Depends on V5-SDK-1 ✅ · **Excludes** V5-SDK-2 deliverable #2 (`TestRunner` conformance suite — gated on V5-FOUND-4, which is gated on V5-FOUND-3 deferral resolution)
**Status:** drafted 2026-05-03; **plan-critic v1 REVISED 2026-05-03** (2 🔴 blockers absorbed: consumer-set expanded beyond examples to include `neurogrim-sensory` + `neurogrim-ecosystem`, and split per-example Cargo.toml edit recipe by the actual SDK-vs-core dep choice; 5 🟡 technical concerns absorbed: tokio dev-dep restoration, rustdoc intra-doc-link gating, CI profile fix, dropped Fork E since surface-assertion has zero conformance pins today, `cargo tree` package-cwd discipline; 3 🟡 methodology concerns absorbed: README inlined Cargo.toml MUST show `features = ["conformance"]`, V5-SDK-2 marked **partial-complete** at close-out, epic prose `tempfile`→`tokio` drift fix); **plan-critic v2 REVISED 2026-05-03** (1 more 🔴 blocker absorbed: 7th consumer is `neurogrim-sdk/tests/compile_test_re_exports.rs:48-65` which uses the gated re-exports via `use neurogrim_sdk::*;` — Phase 2 now self-references the SDK with `features = ["conformance"]` in its own `[dev-dependencies]`); fork decisions pending operator pin.

## Context

V5-SDK-1 closed 2026-05-03 (commits `f27eed1` Phase 0, `ed014d0` Iter 1, `1a3fcda` Phase 3, `343fc68` Phase 4, `74d4fd4` Phase 5). Per Fork C1, V5-SDK-1 chose to **re-export the three V5-MOD-1/2/3 conformance suites at v0.1.0** rather than defer the work to V5-SDK-2. The three suites are reachable today as `neurogrim_sdk::{sensor_conformance, scoring_source_conformance, queue_backend_conformance}::run_factory_conformance`.

That leaves V5-SDK-2 with three remaining deliverables:

1. **Feature-gate the conformance modules** behind `#[cfg(feature = "conformance")]` so consumers building production binaries don't carry `tokio` (the suite uses `tokio::spawn` + `tokio::time::timeout`) transitively. Currently in `neurogrim-core/Cargo.toml`, `tokio` was promoted from `[dev-dependencies]` to `[dependencies]` at V5-MOD-1 Phase 5 specifically because `scoring_source_conformance` uses it in its public API (note: the V5-SDK-2 epic prose at `roadmap/epics/v5-sdk.md:122` says "currently `tempfile`" — that wording is outdated; this partial fixes it).
2. **`TestRunner` conformance suite** — depends on V5-FOUND-4, which is gated on V5-FOUND-3 (deferred 2026-05-03 to v5.1/v6 — Windows coverage-toolchain gap; see `roadmap/epics/v5-foundation.md` § "V5-FOUND-3 deferral note"). **Out of scope for this partial.**
3. **Inline the writing-a-conformant-Sensor walkthrough** verbatim into the SDK README. Today the README has a "Quick start" + a pointer to `examples/sensor-constant-score`; the lib.rs rustdoc has a detailed walkthrough at lines 55–150. V5-SDK-2 inlines the walkthrough into the README so adopters reading the SDK on crates.io get the full pattern without needing the source repo.

Deliverables (1) + (3) ship in this partial. (2) waits on V5-FOUND-4 → V5-FOUND-3 unblock.

## Architectural anchors (extending, not inventing)

- **Optional-dep + feature pattern** already present in `neurogrim-core/Cargo.toml`: `rusqlite = { ..., optional = true }` + `[features] sqlite = ["dep:rusqlite"]`. The new `conformance` feature follows the same shape — extends a shipped pattern, no new ergonomics invented.
- **Three conformance modules** (`crates/neurogrim-core/src/{sensor,scoring_source,queue_backend}_conformance.rs`) + the shared `conformance.rs` types are already four self-contained source files. Gating them is `#[cfg(feature = "conformance")] pub mod foo;` in `lib.rs` — no module-internal restructuring needed.
- **SDK re-exports** at `crates/neurogrim-sdk/src/lib.rs:360–389` (the four `pub mod` blocks for the conformance modules) — gating is one `#[cfg(feature = "conformance")]` per re-export module. The shared types `pub mod conformance { pub use neurogrim_core::conformance::*; }` go behind the same gate.
- **README structure** at `crates/neurogrim-sdk/README.md` already has a "Quick start" section (lines 56–105) + a "Conformance" section (lines 107–138). The walkthrough goes between them as a new "Writing a conformant Sensor" section.
- **Principle alignment.** Feature-gating conformance suites is **VISION proposed-#20 in action** (`v5-roadmap.md:174`: "Pluggability is justified by use, not aspiration") — the four in-tree example crates + two workspace crates (`neurogrim-sensory`, `neurogrim-ecosystem`) ARE the real use that justifies the conformance pluggability. Pre-1.0, this earns its place. Also advances **VISION #8** (absorption over invention — extending the optional-dep pattern, not inventing one).

## Recon-confirmed state — full consumer set

`Grep` of the workspace (2026-05-03) found **seven** call sites for `neurogrim_core::*_conformance` or `neurogrim_sdk::*_conformance` (post plan-critic v2):

| Consumer | File | Imports from | Cargo.toml dep posture |
|---|---|---|---|
| `neurogrim-sensory` | `crates/neurogrim-sensory/tests/sensor_conformance.rs:37` | `neurogrim_core::sensor_conformance` | `[dependencies] neurogrim-core = { workspace = true, features = ["sqlite"] }`; no test-side feature posture today |
| `neurogrim-ecosystem` | `crates/neurogrim-ecosystem/src/scoring_source.rs:241` (inside `#[tokio::test]`) | `neurogrim_core::scoring_source_conformance` | `[dependencies] neurogrim-core = { workspace = true }`; uses conformance only in inline `#[cfg(test)]` mod |
| **`neurogrim-sdk` (self-test)** | `crates/neurogrim-sdk/tests/compile_test_re_exports.rs:48–65` | `neurogrim_sdk::{conformance, scoring_source_conformance, sensor_conformance, queue_backend_conformance}::ConformanceReport` (via `use neurogrim_sdk::*;` glob at line 9) | `[dev-dependencies]` block has no self-reference today; will need one (Phase 2) |
| `sensor-constant-score` | `examples/sensor-constant-score/tests/conformance.rs:11` | `neurogrim_sdk::sensor_conformance` | `[dependencies] neurogrim-sdk = { path = ... }` (only example using the SDK directly) |
| `sensor-readme-quality` | `examples/sensor-readme-quality/tests/conformance.rs:30` | `neurogrim_core::sensor_conformance` | `[dependencies] neurogrim-core = { workspace = true }` |
| `queue-backend-memory` | `examples/queue-backend-memory/tests/conformance.rs:10` | `neurogrim_core::queue_backend_conformance` | `[dependencies] neurogrim-core = { workspace = true }` |
| `scoring-source-prom` | `examples/scoring-source-prom/tests/conformance.rs:26` | `neurogrim_core::scoring_source_conformance` | `[dependencies] neurogrim-core = { workspace = true }` |

**Seven** distinct consumer-side updates needed (six in Phase 3 across consumer crates; one in Phase 2 self-reference inside the SDK's own Cargo.toml). Plan-critic v1 missed two consumers (`neurogrim-sensory`, `neurogrim-ecosystem`); plan-critic v2 caught a third (`neurogrim-sdk` self-test, `compile_test_re_exports.rs`'s `conformance_types_unified_across_suites` fn at lines 48–65). The original draft only updated the 4 example crates and used a single edit recipe assuming all 4 depend on `neurogrim-sdk` — **only `sensor-constant-score` does**. Other recon facts:

- `tempfile = "3"` is in `neurogrim-core`'s `[dev-dependencies]` (line 79) — already correctly dev-only, NOT a leakage source. The actual leakage is `tokio` (line 44).
- `neurogrim-core` itself uses `#[tokio::test]` in test modules of `scoring_source.rs::tests` and `*_conformance.rs::tests` — these need `tokio` macros at test-build time even when the *runtime* tokio is gated. So Phase 1 must restore `tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }` to `[dev-dependencies]` (alongside the new optional `[dependencies]` line).
- CI runs `cargo nextest run --profile ci` (not `--profile default`). Verification commands must match.
- `lib.rs` rustdoc has intra-doc-links to the conformance modules at lines 22–46. After Phase 2 wraps each `pub mod` in `#[cfg(feature = "conformance")]`, building rustdoc *without* the feature emits "unresolved link" warnings. Either build rustdoc with `--features conformance` or guard the doc strings with `#[cfg(feature = "conformance")]` on a doc attribute. **Use `cargo doc --features conformance` for verification** (simpler).
- `crates/neurogrim-sdk/tests/sdk_surface_assertion.rs` (V5-SDK-1's compile-test pin file) **does NOT currently pin any conformance functions** — it pins the five trait surfaces (Sensor, ScoringSource, QueueBackend, Transport, SecretBackend). Therefore there is no surface-assertion change needed in this partial. Adding conformance pins to that file is a separate v5.5 follow-up (BACKLOG addition recommended in the close-out commit; tracked alongside B-46).

## Phases

### Phase 0 — plan + plan-critic + fork pins (this revision)

Plan v1 written; plan-critic v1 returned (technical = REVISE; methodology = PROCEED WITH CAUTION); plan v2 absorbs all findings. Plan-critic v2 spawn: **technical lens only** (re-verify the consumer-set fix is complete and the `cargo build --tests --no-default-features` regression is caught). Operator pinning awaited.

### Phase 1 — feature-gate conformance in `neurogrim-core`

1. In `neurogrim-core/Cargo.toml`:
   - Move runtime tokio: `tokio = { workspace = true }` (line 44, in `[dependencies]`) → `tokio = { workspace = true, optional = true }`.
   - Add to `[features]` (after the existing `sqlite` line): `conformance = ["dep:tokio"]`.
   - **Add to `[dev-dependencies]` (NEW — restores test-time `#[tokio::test]` capability):** `tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }`. Without this, `#[tokio::test]` in `scoring_source.rs::tests` and the three `*_conformance.rs::tests` modules fail to compile under `cargo build --tests --no-default-features`. The historical comment at lines 67–74 already explained this posture; that posture is restored now.
2. In `neurogrim-core/src/lib.rs`, gate the four conformance modules:
   ```rust
   #[cfg(feature = "conformance")] pub mod conformance;
   #[cfg(feature = "conformance")] pub mod scoring_source_conformance;
   #[cfg(feature = "conformance")] pub mod sensor_conformance;
   #[cfg(feature = "conformance")] pub mod queue_backend_conformance;
   ```
3. Verify (NEW set per plan-critic technical agent S1):
   - `cargo build -p neurogrim-core --no-default-features` → succeeds; tokio absent from package-scope dep tree.
   - `cargo build -p neurogrim-core --tests --no-default-features` → succeeds; restored dev-dep tokio gives `#[tokio::test]` macro coverage.
   - `cargo build -p neurogrim-core --features conformance` → succeeds; conformance modules visible.
   - `cargo test -p neurogrim-core --features conformance --tests` → all unit + integration tests in `neurogrim-core` pass under the feature.
   - `cd crates/neurogrim-core && cargo tree --no-default-features | grep tokio` → returns nothing (workspace-feature unification gotcha avoided by `cd`'ing into the package; **plan-critic technical agent C5**).

### Phase 2 — feature-gate in `neurogrim-sdk`

1. In `neurogrim-sdk/Cargo.toml`:
   - Add to a new `[features]` section: `conformance = ["neurogrim-core/conformance"]`.
   - **Add to `[dev-dependencies]` (NEW — closes the 7th-consumer gap caught by plan-critic v2):**
     ```toml
     # Self-reference with conformance ON so the SDK's own integration
     # tests can use the gated re-exports. Specifically,
     # `tests/compile_test_re_exports.rs:48-65` (the
     # `conformance_types_unified_across_suites` fn — Fork F1
     # nominal-identity assertion from V5-SDK-1 Phase 1.5) references
     # `conformance::ConformanceReport`,
     # `scoring_source_conformance::ConformanceReport`,
     # `sensor_conformance::ConformanceReport`, and
     # `queue_backend_conformance::ConformanceReport`. After Phase 2
     # gates the four `pub mod *_conformance` blocks, those references
     # become unresolved unless the conformance feature is active for
     # the test build. Cargo's feature unification across `[dev-deps]`
     # makes activating it via self-reference the cleanest path —
     # `cargo test -p neurogrim-sdk` always builds with conformance on.
     neurogrim-sdk = { path = ".", features = ["conformance"] }
     ```
2. In `neurogrim-sdk/src/lib.rs:360–389`, wrap the four conformance re-export blocks in `#[cfg(feature = "conformance")]`:
   ```rust
   #[cfg(feature = "conformance")] pub mod scoring_source_conformance { ... }
   #[cfg(feature = "conformance")] pub mod sensor_conformance { ... }
   #[cfg(feature = "conformance")] pub mod queue_backend_conformance { ... }
   #[cfg(feature = "conformance")] pub mod conformance { ... }
   ```
3. Update lib.rs rustdoc:
   - At "## Conformance suites" section (line 31), add: "Available behind the `conformance` feature: `neurogrim-sdk = { version = "0.1", features = ["conformance"] }`."
   - The intra-doc links at lines 22–29 + 39–46 (`[\`scoring_source_conformance\`]`, etc.) — leave them; verification uses `cargo doc --features conformance`.
4. Verify:
   - `cargo build -p neurogrim-sdk --no-default-features` → succeeds; tokio absent.
   - `cargo build -p neurogrim-sdk --features conformance` → succeeds; tokio present.
   - `cargo doc -p neurogrim-sdk --features conformance` → no rustdoc warnings about unresolved intra-doc links.
   - `cd crates/neurogrim-sdk && cargo tree --no-default-features | grep tokio` → returns nothing.
   - **`cargo nextest run -p neurogrim-sdk --profile ci` → all integration tests pass, including the 5 in `tests/compile_test_re_exports.rs` (theme-B traits object-safe, adjacent traits reachable, registries constructible, queue built-in factories, conformance types unified). The 5th — `conformance_types_unified_across_suites` — is the one the dev-dep self-reference makes compile.**

(Per plan-critic technical agent C2: the existing `tests/sdk_surface_assertion.rs` does not pin conformance functions, so no edits to that file are needed in this partial. Adding conformance pins is a separate v5.5 follow-up.)

### Phase 3 — update workspace + example consumers (six files)

Six per-crate Cargo.toml edits. **Each consumer chooses `[dev-dependencies]` posture (Fork D2 default)** so production builds remain feature-clean. Plan-critic technical agent's B1+B2 findings drive this exact list:

1. **`crates/neurogrim-sensory/Cargo.toml`** — add to `[dev-dependencies]`:
   ```toml
   neurogrim-core = { workspace = true, features = ["conformance", "sqlite"] }
   ```
   (`sqlite` carried over to match the existing `[dependencies]` posture.)
2. **`crates/neurogrim-ecosystem/Cargo.toml`** — add to `[dev-dependencies]`:
   ```toml
   neurogrim-core = { workspace = true, features = ["conformance"] }
   ```
3. **`examples/sensor-constant-score/Cargo.toml`** — change line 18 from `[dependencies]` to also appear in `[dev-dependencies]`:
   ```toml
   [dev-dependencies]
   neurogrim-sdk = { path = "../../crates/neurogrim-sdk", features = ["conformance"] }
   ```
   (Cargo unifies feature spec across the two entries; the production `[dependencies]` line stays minimal.)
4. **`examples/sensor-readme-quality/Cargo.toml`** — add to `[dev-dependencies]`:
   ```toml
   neurogrim-core = { workspace = true, features = ["conformance"] }
   ```
5. **`examples/queue-backend-memory/Cargo.toml`** — add to `[dev-dependencies]`:
   ```toml
   neurogrim-core = { workspace = true, features = ["conformance"] }
   ```
6. **`examples/scoring-source-prom/Cargo.toml`** — add to `[dev-dependencies]`:
   ```toml
   neurogrim-core = { workspace = true, features = ["conformance"] }
   ```

7. **CI verification** (`.github/workflows/ci.yml`): the existing `cargo nextest run --profile ci` already runs all workspace test targets including the six consumers' tests. Because each consumer now enables `neurogrim-core/conformance` via `[dev-dependencies]`, the workspace-feature-unification rule means the conformance feature is on for the test build of every workspace member that activates dev-deps. Phase 3 step 4 confirms this.
8. Verify (per plan-critic technical agent C3 — match CI's actual profile):
   - `cargo nextest run --workspace --profile ci --color never` → all tests pass; record test count.
   - Compare test count with pre-Phase-1 baseline: the count must be equal (no test silently skipped).
   - Spot-check a single conformance test runs visibly under nextest output (e.g., `sensor_constant_score::constant_score_factory_passes_full_conformance_suite`).
9. Verify production-clean:
   - `cargo build --workspace --no-default-features` → does NOT compile any conformance module path. (Quick smoke that `[dev-dependencies]` posture isolates the gate properly.)

### Phase 4 — inline walkthrough into SDK README

1. Lift the current rustdoc walkthrough at `neurogrim-sdk/src/lib.rs:55–150` ("Writing a conformant Sensor (V5-MOD-2)") **verbatim** into `neurogrim-sdk/README.md`, formatted as a top-level section between "## Quick start" (line 56) and "## Conformance" (line 107). Section heading: "## Writing a conformant Sensor".
2. **CRITICAL (plan-critic methodology agent C1):** the inlined `Cargo.toml` block in the new README section MUST show `features = ["conformance"]` so adopters who copy-paste don't silently miss the conformance suite. Match exactly:
   ```toml
   [dependencies]
   neurogrim-sdk = "0.1"
   async-trait = "0.1"
   anyhow = "1"
   serde_json = "1"
   chrono = { version = "0.4", features = ["serde"] }

   [dev-dependencies]
   tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
   tempfile = "3"
   neurogrim-sdk = { version = "0.1", features = ["conformance"] }
   ```
   Without this, the README walkthrough teaches adopters to write conformance tests against an SDK that doesn't expose the conformance modules — they'd hit `error[E0432]: unresolved import`. Defensive against the "Modular middleware ships degraded" concern.
3. Lib.rs version stays — it's the canonical rustdoc that surfaces in `cargo doc`. The README version is for crates.io's surface (which doesn't render rustdoc). Two copies acceptable for now; v5.5 BACKLOG addition tracks the doc-include via `#![doc = include_str!("../README.md")]` to deduplicate.
4. Verify:
   - `cargo doc -p neurogrim-sdk --features conformance` → builds clean; no rustdoc warnings.
   - Render the README locally (any markdown viewer) — the walkthrough section is internally complete.

(Fork F retained at F1 — Sensor walkthrough only — but this default is *explicit scope reduction* per plan-critic methodology agent C2: V5-MOD-1 + V5-MOD-3 hand-off notes ALSO request walkthroughs; F2 expands to all three. See Forks section for the trade.)

### Phase 5 — epic close-out

1. Update `roadmap/epics/v5-sdk.md` § V5-SDK-2:
   - Flip Status: `Planned (scope reduced after V5-SDK-1 close — 2026-05-03)` → `**PARTIAL COMPLETE 2026-05-03** — feature-gate (deliverable 1) + walkthrough (deliverable 3) shipped; **TestRunner conformance suite (deliverable 2) gated on V5-FOUND-4 → V5-FOUND-3** (V5-FOUND-3 deferred 2026-05-03 to v5.1/v6 — Windows coverage-toolchain gap; see `epics/v5-foundation.md` § "V5-FOUND-3 deferral note").`
   - Flip Done-When checkboxes for deliverables 1 + 3 (lines 130–136).
   - **Fix epic prose drift (plan-critic methodology agent S2):** `epics/v5-sdk.md:122` says "Optional `#[cfg(feature = "conformance")]` feature-gating to keep dev-deps (currently `tempfile`) out of production builds." Update "currently `tempfile`" → "currently `tokio` (promoted from dev-dep to runtime at V5-MOD-1 Phase 5 because the conformance suite uses `tokio::spawn` + `tokio::time::timeout` in its public API)".
2. Update `roadmap/v5-roadmap.md` Theme C status row — replace `V5-SDK-2 planned (scope reduced — V5-SDK-1 absorbed conformance re-exports per Fork C1)` with `V5-SDK-2 PARTIAL COMPLETE 2026-05-03 (feature-gate + walkthrough shipped; TestRunner suite gated on V5-FOUND-4 → V5-FOUND-3 deferral chain)`.
3. Add a v5.5 BACKLOG addition (or expand existing B-46): "B-XX: Add conformance-suite pins to `neurogrim-sdk/tests/sdk_surface_assertion.rs` (gated by feature). V5-SDK-2 partial caught the gap; tracked here for v5.5 polish."
4. Single commit per phase per established cadence.

## Forks (operator-pinnable)

(Fork E dropped — `tests/sdk_surface_assertion.rs` has zero conformance pins today, making E1/E2 no-ops. Tracked as a v5.5 BACKLOG addition instead.)

- **Fork A — feature name**:
  - **A1** = `conformance` (epic default; matches the verbatim phrasing "Optional `#[cfg(feature = "conformance")]`").
  - A2 = `test-conformance` (more specific; reduces ambiguity if a future feature also wants "conformance" as an adjective).
  - A3 = `conformance-suites` (most explicit but verbose).

- **Fork B — feature default** in `neurogrim-sdk`:
  - **B1** = default OFF (consumers explicitly opt in via `features = ["conformance"]`). Matches the goal of "keep dev-deps out of production binaries." Default-off respects the dependency-discipline skill. **Honors VISION proposed-#20 + #8.**
  - B2 = default ON (consumers get conformance for free; production binaries must explicitly disable via `default-features = false`). Strictly worse for the goal.

- **Fork C — `tempfile` posture**:
  - **C1** = no change. The "currently `tempfile`" wording in the epic note was outdated — `tempfile` is already dev-only in `neurogrim-core`. Focus this partial entirely on `tokio` gating + epic prose fix in Phase 5.
  - C2 = belt-and-suspenders. Move the `tempfile` dev-dep behind `[dependencies] tempfile = { ... optional = true }` and bundle into the same `conformance` feature. Probably a pure-noise change since it was already correct.

- **Fork D — Cargo.toml dep posture for `features = ["conformance"]`** (REVISED post-plan-critic — default flipped from D1 to D2):
  - **D2** (NEW DEFAULT) = `[dev-dependencies]` line (only on for `cargo test`). Keeps production builds (`cargo build --workspace`) feature-clean while CI's `cargo nextest run` still exercises the gated paths. Cleaner semantic match: "conformance is a test-time concern." Verified via Phase 3 step 9.
  - D1 = `[dependencies]` line (always on). Simpler but pollutes `cargo build` with `tokio` for every consumer that touches conformance. Plan-critic surfaced this as a soft concern; D2 fixes it.

- **Fork F — README walkthrough scope** (DOCUMENTED scope-reduction tension per plan-critic methodology agent C2):
  - **F1** = inline the Sensor walkthrough (lib.rs:55–150) only. Tighter README; matches the V5-SDK-2 epic Done-When wording "writing a conformant **Sensor** walkthrough" verbatim. **Tension:** V5-MOD-1 + V5-MOD-3 epic hand-off notes (`epics/v5-sdk.md:71, 85, 105`) request "writing a conformant ScoringSource/QueueBackend walkthrough" too. F1 means those two walkthroughs stay rustdoc-only (visible via `cargo doc -p neurogrim-sdk`), with brief README pointers added in this partial. Defensible because (a) lib.rs walkthroughs ARE published with the SDK and reachable via `docs.rs`, (b) crates.io README rendering is short-form and three full walkthroughs would balloon it.
  - F2 = inline all three walkthroughs (Sensor + ScoringSource + QueueBackend). Maximally adopter-friendly. Larger README — adopt if length isn't a concern.

Defaults pinned: **A1 / B1 / C1 / D2 / F1**. Five forks (down from six post-Fork-E drop); user-pinnable.

## Mutual-exclusion + conflict checks (NEW pattern from V5-FOUND-3 plan-critic feedback)

| Combination | Behavior |
|---|---|
| `neurogrim-sdk` consumer with `default-features = false` and NOT `features = ["conformance"]` | OK — minimal trait-only build. The intended posture for production plugins. |
| `neurogrim-sdk` consumer with `features = ["conformance"]` | OK — pulls `tokio` via `neurogrim-core/conformance`. Tests can `use neurogrim_sdk::sensor_conformance::*`. |
| `neurogrim-core` direct consumer (`neurogrim-sensory`, `neurogrim-ecosystem`, three example crates) — `[dev-dependencies] neurogrim-core = { ..., features = ["conformance"] }` | OK — feature-on for `cargo test`, feature-off for `cargo build`. Workspace `cargo nextest run --profile ci` activates dev-deps and the feature unifies across the workspace test build. |
| Workspace `cargo build` (no flags) does NOT pull tokio through neurogrim-core. | Verify: `cargo build --workspace --no-default-features` (Phase 3 step 9) AND `cd crates/neurogrim-core && cargo tree --no-default-features` returns no `tokio` (Phase 1 step 3). Workspace-feature unification means `cargo tree -p neurogrim-core` from the workspace root would still show tokio if any sibling has dev-deps that activate the feature; the `cd` discipline gives the package-scope view. |
| `cargo test -p neurogrim-core --no-default-features` (no features set) | Should still pass — `[dev-dependencies] tokio = { ... features = ["macros", "rt-multi-thread"] }` (restored in Phase 1 step 1) gives `#[tokio::test]` its runtime. The conformance modules themselves are gated off; their tests don't compile. The non-conformance test modules (e.g., `scoring_source.rs::tests`) pass. |

## Exit-code spec

This partial does not add new CLI flags or commands. No exit-code spec needed.

## Verification (consolidated end-to-end)

- `cd crates/neurogrim-core && cargo tree --no-default-features` → no `tokio`.
- `cd crates/neurogrim-sdk && cargo tree --no-default-features` → no `tokio`.
- `cargo build --workspace --no-default-features` → succeeds.
- `cargo build -p neurogrim-core --tests --no-default-features` → succeeds (catches the `#[tokio::test]` regression that plan-critic technical agent C1 flagged).
- `cargo nextest run --workspace --profile ci --color never` → all tests pass; total count matches pre-Phase-1 baseline (Phase 0 captures the baseline before any feature-gate changes).
- `cargo doc -p neurogrim-sdk --features conformance` → no rustdoc warnings.
- `cargo doc -p neurogrim-sdk --no-default-features` → some intra-doc-link warnings expected (the four conformance module references); accepted because the canonical doc surface is `--features conformance` per Phase 4.

## Deliverable shape

Five commits per the established phase cadence:

1. Phase 0 — plan v2 + fork pins commit (this one).
2. Phase 1 — feature-gate `neurogrim-core` + restore `tokio` dev-dep.
3. Phase 2 — feature-gate `neurogrim-sdk`.
4. Phase 3 — update 6 workspace+example consumers + verify CI test counts.
5. Phase 4 — inline walkthrough into SDK README (with `features = ["conformance"]` in the Cargo.toml block).
6. Phase 5 — epic close-out (V5-SDK-2 PARTIAL COMPLETE; epic prose `tempfile`→`tokio` drift fix; v5.5 BACKLOG addition for surface-assertion conformance pins).

(Phase 5 may bundle into Phase 4 if the walkthrough commit is small; the cadence is "each commit independently shippable.")

## Risks / adversary concerns brought forward

🟡 **Default-feature flip is a soft semver event.** Today, third-party consumers of `neurogrim-sdk = "0.1.0"` get `tokio` transitively. After this partial, they don't (Fork B1). A consumer that imports `neurogrim_sdk::sensor_conformance::run_factory_conformance` and runs `cargo build` *without* `features = ["conformance"]` will see a hard compile error. The SDK is at 0.1.0 with `publish = false`; only in-tree consumers exist today. **No external blast radius.**

🟡 **CI must catch a feature-gate regression.** If `cargo nextest run --profile ci` somehow stops activating `[dev-dependencies]` on a workspace member (e.g., a future `cargo` change to feature unification rules), the conformance suite would compile but never run. Mitigation: Phase 3 step 8's "test count must match baseline" assertion catches silent skips. Add a tracking entry to v5.5 BACKLOG B-XX (alongside surface-assertion gap) for a periodic count audit.

🟡 **Two README walkthroughs (lib.rs + README.md) drift over time.** Mitigation deferred to v5.5 BACKLOG follow-up: once the rustdoc system can `#![doc = include_str!("../README.md")]` cleanly, deduplicate. Until then, two copies acceptable for the "lib.rs is canonical" + "README is crates.io surface" split.

🟡 **Methodology — "Modular middleware ships degraded" is only half-mitigated** (plan-critic methodology agent C1). After this partial, conformance suites are reachable via `neurogrim_sdk::*_conformance` BUT only if a third-party adopter (a) discovers the `conformance` feature exists and (b) flips it. The README walkthrough's inlined Cargo.toml block (Phase 4 step 2 — features explicitly shown) is the primary mitigation. Without that, adopters silently build without the suite. **Hardening:** Phase 5 close-out commit message links to the README section explicitly.

🟡 **Honesty floor on epic close-out** (plan-critic methodology agent C3). V5-SDK-2 `TestRunner` deliverable sits behind a deferral chain (V5-FOUND-3 → V5-FOUND-4 → V5-SDK-2 TestRunner) of indeterminate length. Phase 5 marks V5-SDK-2 status as **PARTIAL COMPLETE**, not COMPLETE — the wording is the honesty discipline.

🔵 **Suggestion: `pre-publish-dryrun` gate** for `neurogrim-sdk` that fails when conformance pins drift from the lib.rs re-export count, including across feature gates. v5.5 polish (BACKLOG B-46 already tracks the related semver-aware gate work).

🔵 **Suggestion: surface-assertion conformance pins** as a v5.5 follow-up. The plan-critic technical agent C2 caught that `tests/sdk_surface_assertion.rs` doesn't pin conformance functions today; this partial deliberately doesn't expand its scope to cover that gap. Track via Phase 5 BACKLOG addition.
