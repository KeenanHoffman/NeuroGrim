# V5-MOD-2 Sensor Trait + Cargo-Feature-Gated Discovery — Implementation Plan

**Epic:** `roadmap/epics/v5-modular-conversions.md` § V5-MOD-2
**Effort estimate (epic):** L, ~10–14 days
**v5 entry:** Theme A closed (V5-FOUND-1 shipped 2026-05-02); V5-MOD-1 closed 2026-05-02 (commit `fb45d5d`)
**Methodology:** plan-critic before implementation per `v5-roadmap.md` final note

## Context

V5-MOD-2 is the second-highest-leverage seam in v5: convert the 21-arm sensor dispatch in `neurogrim-cli/src/main.rs:599-626` (`run_sensory`) to a trait-based factory pattern with cargo-feature-gated registration. After V5-MOD-2, third-party crates can ship custom `Sensor` impls (Jira tracker, GitHub issues, custom telemetry, etc.) without forking core, and operators can compile-out unused sensors via cargo features (e.g., `--no-default-features --features sensors-test-health,sensors-code-quality`).

V5-MOD-1's trait + factory + conformance pattern is the substrate. V5-MOD-2 reuses 80% of V5-MOD-1's architectural decisions (object-safe trait, hand-rolled `HashMap` registry, two-tier dispatch, inherent `async fn` for built-ins, conformance suite as cross-crate fixture). The new dimension is **cargo-feature gates**: sensors become optional dependencies of `neurogrim-sensory`, registered conditionally based on enabled features.

## File-anchor corrections (vs. the epic file)

The V5-MOD-2 epic story has one stale anchor that recon contradicted:

| Epic says | Reality |
|---|---|
| "Coupling fix: `neurogrim-sensory` no longer depends on `neurogrim-cli`" | **Already fixed.** `grep -n neurogrim-cli neurogrim-sensory/Cargo.toml` returns no Cargo dep; the only mention is a `// CLI helpers` comment at `supply_chain_review.rs:310` describing the FORWARD direction (cli depends on sensory, which is correct). The Done-When item should be marked already-satisfied at V5-MOD-2 open. |

The other anchors are accurate; the epic prose lists `Sensor` trait location, conformance suite scope, and example crate path as planned.

## Recon-confirmed sensor surface

**21 built-in sensors total in source** (`analyze_*` free functions in `neurogrim-sensory/src/`); **20 registered in `run_sensory` dispatch** today; **`secrets_readiness` is the 21st** (orphan — analyzer + 9 tests + `pub` re-export landed in v4.2 S14; dispatch wiring didn't). Fork C ADD-as-21st reclaims it. Each module exposes:
- A `pub async fn analyze_X(project_root: &str) -> Value` (**18 sensors** — including secrets_readiness) or `-> anyhow::Result<Value>` (**3 sensors**: `agent_behavior`, `git_health`, `docker_topology`) — the actual sensing logic. **Important caveat (Subagent 1 finding):** the "infallible" 18 are not actually infallible — they catch errors internally and return *degraded* envelopes (e.g., `score: 0` + a `<sensor>_error` extras key). The trait normalization to `Result` does NOT change wire output but DOES change CLI exit-code semantics: today, sensors that bubble errors via `?` in `main.rs:602` exit nonzero on failure, while silently-degrading sensors exit zero. Phase 3 acceptance criterion extended to include CLI exit-code parity for all 21 sensors, not just CMDB JSON byte-equality.
- A `pub struct XServer` with `tool_router: ToolRouter<Self>` — an MCP server wrapper that exposes the analyzer as an MCP tool. **Audit finding (Subagent 3):** these `XServer` types have **zero call sites outside `neurogrim-sensory/src/` itself** — they are orphans/dead-code in the current binary build. V5-MOD-2 keeps them for zero-diff backward compat but the close-out commit notes the audit and queues "remove or aggregate per-sensor MCP servers" as v5.5/Theme D work.

Common substrate: `neurogrim-sensory/src/cmdb.rs::build_cmdb` produces JSON envelopes conforming to `cmdb-envelope-v1.schema.json` (LSP-Brains spec).

**Dispatch site (the V5-MOD-2 equivalent of V5-MOD-1's `context.rs:218`):** `neurogrim-cli/src/main.rs:599-626` `run_sensory`. 21-arm string `match` keyed on the sensor's wire-name (`git-health`, `code-quality`, etc.).

## Architectural anchors (extending, not inventing)

| Anchor | What we reuse |
|---|---|
| V5-MOD-1's `ScoringSource` trait + `ScoringSourceFactory` + `ScoringSourceRegistry` | Mirror the trait shape (object-safe, `#[async_trait]`, never-panic contract). Hand-rolled `HashMap` registry. Last-write-wins on duplicate `register()`. |
| V5-MOD-1's `BuiltinScoringSource` enum + inherent `async fn load_inherent` | Two-tier dispatch: built-ins skip `#[async_trait]` future-boxing via inherent methods; third-party plugins pay the boxing cost via `Box<dyn Sensor>`. Same perf trick. |
| V5-MOD-1's `scoring_source_conformance::run_factory_conformance` | Generalize to `sensor_conformance::run_factory_conformance(&dyn SensorFactory, &Path) -> ConformanceReport`. T1-T8 cross-cutting tests (name stability, build repeatability, skeletal-input handling, concurrency, idempotency) port over verbatim; add T9-T12 sensor-specific tests (CMDB schema validation, panic recovery, timeout). |
| V5-MOD-1's `examples/scoring-source-prom/` | Same template for V5-MOD-2: `examples/sensor-jira/` (or similar) demonstrates third-party impl. |
| `neurogrim-core/data/schemas/diagnostics-ledger-v1.schema.json` (Phase 2 of V5-FOUND-1) | Same vendoring pattern for `cmdb-envelope-v1.schema.json` — copy from LSP-Brains spec into `neurogrim-core/data/schemas/` for in-process validation. |

## Naming decision — RESOLVED at planning (not a fork)

Unlike V5-MOD-1's `ScoringSource` struct/trait collision, **there is no `Sensor` struct in `neurogrim-core` today**. The trait can take the name unencumbered:

- `pub trait Sensor` — the per-sensor contract (one method: `async fn analyze`).
- `pub trait SensorFactory` — produces `Box<dyn Sensor>` for a given wire-name.
- `pub struct SensorRegistry` — `HashMap<&'static str, Box<dyn SensorFactory>>` plus `register/get/build` API.

No semver-major bump needed for this rename concern (the v5.0.0 bump from V5-MOD-1 already absorbs all v5 breakage in one event). However: the public sensor-module re-exports in `neurogrim-sensory::lib.rs` change shape (the `analyze_*` free functions remain for backward compatibility, but the canonical surface becomes the `Sensor` impls). Document in the V5-MOD-2 close-out commit.

## Fork decisions — RESOLVED 2026-05-02 (operator pin: all 5 plan defaults)

Five fork-points surfaced by the plan-critic round; operator-pinned 2026-05-02 to all five plan defaults. The pre-pin debate is preserved below for traceability.

| Fork | Resolution | Plan default (chosen) | Alternative (rejected) |
|---|---|---|---|
| **A** | **`&str`** | Trait `analyze(&self, project_root: &str) -> anyhow::Result<Value>`. Migration economy + zero analyzer churn + no Windows `to_string_lossy()` regression. SDK-level inconsistency vs `ScoringSource::load(&Path)` is real but small (sensors don't manipulate paths much); v6 promotion path documented in V5-SDK epic. | `&Path` with eager 21-analyzer migration (~+1.5 days) |
| **B** | **DROP** | No two-method `analyze`/`analyze_inherent` dance. Sensor IO at seconds-per-call; ~50ns × 21 boxing overhead is rounding error. Trait rustdoc explicitly notes "no inherent fast-path needed" so future contributors don't cargo-cult the V5-MOD-1 pattern. | KEEP for V5-MOD-1 architectural consistency (cargo-cult tax with zero measurable benefit) |
| **C** | **INCLUDE the orphan (21 total registered)** | `secrets_readiness` is wired into `run_sensory` dispatch + factory + cargo feature. Was a half-shipped v4.2 S14 advisory domain (analyzer + 9 tests + `pub` re-export landed; dispatch wiring didn't). Trait migration is the natural cleanup moment. (Recon at Phase 0 corrected the framing: source has 21 `analyze_*` functions; dispatch had 20 arms; Fork C reclaims the 21st as registered.) | FORMALLY REMOVE as dead code (loses real v4.2 work) |
| **D** | **ADD pass-through** | `neurogrim-cli` + `neurogrim-mcp` Cargo.toml's gain per-sensor feature pass-through so operators can `cargo build --bin neurogrim --no-default-features --features sensor-X`. Aligns with NeuroGrim's `dependency-discipline` ethos + containerized deployments. Maintenance friction (3 Cargo.toml edits per new sensor) mitigated by `register_sensor!` macro (Subagent 2 🔵). +1 day in Phase 4. | ACCEPT the limitation: trait + factory ship, but compile-out benefits only apply to custom binaries |
| **E** | **`sensor-readme-quality`** | Generic, file-system-shaped, zero env vars, runs on any project. Complements V5-MOD-1's `scoring-source-prom` (HTTP-fetch example) with FS-read example — SDK gets two complementary patterns. Demos a sensor that's genuinely useful, not just educational. | `sensor-jira` (brand-specific, ages poorly, auth/JQL noise distracts from trait) or `sensor-rest-counter` / `sensor-issue-tracker` (concept-shaped middle ground) |

## Phases (incremental delivery)

Each phase ships independently. Iteration boundaries are explicit.

### Phase 0 — Setup + audits (Day 1, ~0.75 day) — PREREQUISITE

**Goal:** Vendor the CMDB envelope schema, audit the existing surface, mark the false-positive Done-When item.

**Steps:**
1. **Vendor `cmdb-envelope-v1.schema.json`** from `D:/Brains/LSP-Brains/schemas/` to `neurogrim-core/data/schemas/cmdb-envelope-v1.schema.json` (same pattern as V5-FOUND-1's diagnostics-ledger schema). Add a comment at the top citing the LSP-Brains canonical path; bump the local copy when the spec ships a new schema version.
2. **Ship `xtask schema-drift-check`** (Subagent 3 🟡 finding — no precedent today). One-shot xtask that diffs `neurogrim-core/data/schemas/*.json` against `D:/Brains/LSP-Brains/schemas/` and warns on drift. Establishes the pattern for V5-FOUND-1's diagnostics-ledger schema *and* V5-MOD-2's CMDB-envelope schema simultaneously. CI runs it; close-out commit cites the result.
3. **Audit sensor return-shape variants.** Recon-confirmed: **3 fallible** sensors (`agent_behavior:127`, `git_health:58`, `docker_topology:189`) return `Result<Value>`; **18 infallible** sensors (after Fork C resolves) return `Value` directly. The trait will normalize on `Result<serde_json::Value>` — fallible sensors return their underlying error chain; "infallible" ones `Ok(...)` always BUT **must preserve their internal error-catching semantics** (today they catch + degrade silently to `score: 0`; the trait wrapper preserves this — operator-visible behavior is identical at the JSON level). Capture the full list of 21+1 sensors with return-shape annotations in the Phase 0 commit message.
4. **Audit per-sensor heavy deps.** `cargo-lock` + `yarn-lock-parser` + `semver` + `toml` (supply_chain_sca; toml was missing from prior plan draft — corrected per Subagent 2 🟡), `tar` + `flate2` + `reqwest` (supply_chain_vigilance), `reqwest` (also: a2a, ecosystem, sensory direct), `sha2` (supply_chain_*), `jsonschema` (capability_hygiene + trust_budget). **Mark all of these `optional = true` in `neurogrim-sensory/Cargo.toml` as part of Phase 0** (Subagent 2 🔴 finding — Phase 4's `dep:cargo-lock` syntax assumes optional, but the conversion is an explicit step). Without flipping the flag, Phase 4's feature gates compile the deps regardless.
5. **Two-pronged coupling-fix verification.** Run BOTH `grep neurogrim-cli neurogrim-sensory/Cargo.toml` AND `grep -rn "neurogrim_cli\b" neurogrim-sensory/src/` (Subagent 3 🟡 finding). Cite both in the Phase 0 commit message — verifies the smell is gone at both Cargo-dep and source-import level. Mark already-satisfied; no work needed.

**Ship criterion:** schema vendored; drift-check xtask exists + green; heavy deps marked `optional = true`; recon notes (sensor count, return-shape variance, coupling-fix two-pronged grep result) captured in the Phase 0 commit message; no behavior change in the binary.

### Phase 1 — Define the trait + registry (Day 1–2, ~1 day)

**Goal:** Define `pub trait Sensor` + `pub trait SensorFactory` + `pub struct SensorRegistry` in a new `neurogrim-core/src/sensor.rs` module. No dispatch wired yet — just the contract.

**Files (new):**
- `neurogrim-core/src/sensor.rs`

**Trait shape (sketch — assumes Fork A `&str`, Fork B drop-inherent):**
```rust
/// V5-MOD-2: pluggable contract for sensors that produce CMDB
/// envelopes. Replaces the 20-arm string match at
/// `neurogrim-cli/src/main.rs:599` (`run_sensory`).
///
/// Implementations are object-safe (`Box<dyn Sensor>`) and
/// registered via the factory registry below. Built-in impls
/// (one per existing sensor in `neurogrim-sensory`) preserve v4
/// behavior verbatim; the contract is identical, only the
/// dispatch mechanism changes.
///
/// # Why no `name()` on this trait (Subagent 1 finding)
/// The factory's `name()` is canonical. Built-in dispatch routes
/// `factory.build()` so the wire-name is always reachable via
/// the factory; the trait stays minimal and there's no possible
/// drift between `Sensor::name` and `SensorFactory::name`.
#[async_trait::async_trait]
pub trait Sensor: Send + Sync {
    /// Run the sensor against the project root. Returns a CMDB
    /// envelope (`cmdb-envelope-v1`-conformant JSON) on success;
    /// the error type carries underlying causes for the operator
    /// to debug. The contract MUST NOT panic; every error path
    /// must produce either an `Err(...)` or a degraded `Ok(...)`
    /// envelope (with `score: 0` + a finding describing the
    /// failure). Caller error-handling: if the historical free-
    /// function returned `Result`, the trait impl bubbles the
    /// error; if it returned `Value` (silent-degradation), the
    /// trait impl `Ok(...)`s a degraded envelope. CLI exit-code
    /// parity preserved per Phase 3 acceptance criterion.
    async fn analyze(
        &self,
        project_root: &str,
    ) -> anyhow::Result<serde_json::Value>;
}

pub trait SensorFactory: Send + Sync {
    fn name(&self) -> &'static str;
    fn build(&self) -> Box<dyn Sensor>;
}

pub struct SensorRegistry {
    factories: HashMap<&'static str, Box<dyn SensorFactory>>,
}
```

**Why no two-method dance (Fork B drop-inherent):** sensor IO is at the seconds-per-call scale (git, cargo audit, network calls); `#[async_trait]`'s ~50ns + one allocation per call is rounding error. Plain `#[async_trait]` for built-ins; no separate inherent method. Saves 21× duplicate method declarations and the cognitive overhead of "why two methods" that future readers would otherwise inherit from V5-MOD-1.

**Tests (Phase 1 — trait definition only):**
- Compile-only test: trait is object-safe.
- Empty registry has no factories; `get / build / has` honor empty state.
- Mock `Sensor` + `SensorFactory` exercise `Box<dyn>` round-trip.

**Ship criterion:** `cargo test --workspace` green; new module has rustdoc with examples.

### Phase 2 — Built-in factories (mechanical bulk migration) (Day 2–4, ~2 days)

**Goal:** Convert the 21 existing `analyze_X` functions to `Sensor` impls. Each becomes a unit struct (e.g., `pub struct GitHealthSensor;`) with a `Sensor` impl that delegates to the existing `analyze_git_health` free function.

**Migration strategy — verbatim semantics (revised per Forks A + B):**
```rust
// neurogrim-sensory/src/git_health.rs (additive — keep the existing
// pub async fn analyze_git_health for backward compat)

use neurogrim_core::sensor::{Sensor, SensorFactory};
use async_trait::async_trait;

pub struct GitHealthSensor;

#[async_trait]
impl Sensor for GitHealthSensor {
    async fn analyze(
        &self,
        project_root: &str,
    ) -> anyhow::Result<serde_json::Value> {
        // Pass through to the existing free-function. No path
        // conversion, no to_string_lossy() round-trip, no Windows
        // correctness regression (Subagent 1 🔴 finding). When the
        // free-function returned Value (silent-degradation), the
        // trait wrapper Ok()s it identically.
        analyze_git_health(project_root).await
    }
}

pub struct GitHealthSensorFactory;
impl SensorFactory for GitHealthSensorFactory {
    fn name(&self) -> &'static str { "git-health" }
    fn build(&self) -> Box<dyn Sensor> { Box::new(GitHealthSensor) }
}
```

The trait definition lives in `neurogrim-core::sensor`; `neurogrim-sensory` already has `neurogrim-core = { workspace = true, features = ["sqlite"] }`. Each `analyze_*` function gains a wrapping `XSensor` + `XSensorFactory` pair in the same file.

**Adapter rationale (Fork A `&str`):** the trait takes `&str` to match existing analyzer signatures 1:1. No path conversion, no Windows surrogate-pair lossiness, no round-trip. Long-term the workspace might prefer `&Path` for type safety, but doing it inside V5-MOD-2 would balloon the diff (21+ analyzers to migrate) AND introduce a Windows correctness regression at the trait boundary if done lazily. **Filed as a v5.5 BACKLOG item** ("Sensor analyzers take `&Path` natively"); Subagent 1 cited as the rationale source.

**Special handling — `supply_chain_review` CLI helpers (Subagent 3 🟡 finding):** this module exposes `cli_create`, `cli_resolve`, `cli_list` free functions (~100 lines, called from `neurogrim-cli/src/commands/sca_review.rs`) that are NOT part of the analyzer surface. The `XSensor` impl wraps only `analyze_supply_chain_review`; the CLI helpers stay as free functions, untouched. Phase 2 commit message documents this explicitly so future contributors don't try to fold them into the trait.

**Files (modified):**
- All 21 `neurogrim-sensory/src/<sensor>.rs` files: add `XSensor` + `XSensorFactory`.
- `neurogrim-sensory/src/lib.rs`: re-export the new structs (`pub use git_health::{GitHealthSensor, GitHealthSensorFactory};`).
- `neurogrim-sensory/Cargo.toml`: add `async-trait = { workspace = true }` (already in workspace from V5-MOD-1 Phase 0).

**Tests (Phase 2):**
- Per-sensor: `XSensor.name()` returns the expected wire-name; `XSensor.analyze(<tempdir>)` returns `Ok(...)` (or fails-gracefully if the sensor needs project setup).
- `Box<dyn Sensor>` round-trip for at least 2 sensors (ports the V5-MOD-1 Phase 1 early-validation test pattern).

**Ship criterion:** all 21 sensors have `XSensor` + `XSensorFactory`; existing `analyze_*` free functions still work (backward compat held); `cargo test --workspace` green.

### Phase 3 — Convert the dispatch site (Day 4–5, ~1 day)

**Goal:** Replace the 21-arm match in `neurogrim-cli/src/main.rs:599` with `SensorRegistry`-based dispatch.

**Files (modified):**
- `neurogrim-cli/src/main.rs`: import `neurogrim_core::sensor::SensorRegistry`; build a registry at startup pre-populated with all 21 built-in factories (factory list lives in `neurogrim-sensory::built_in_factories()` or similar — single canonical list); replace `run_sensory`'s match with `registry.get(name).build().analyze(Path::new(project_root)).await`.

**Files (new):**
- `neurogrim-sensory/src/registry.rs`: `pub fn built_in_factories() -> Vec<Box<dyn SensorFactory>>` returning all 21 factories. The `SensorRegistry::with_built_ins()` helper consumes this. Keeps the "list of built-ins" in one place rather than scattered across main.rs.

**Risk:** the existing match has 21 specific error-handling variants; the trait normalizes them. Watch for behavioral changes (e.g., a sensor that returned `Result<Value>` and was unwrapped with `?` now goes through the trait's `Result` and re-`?`s — semantics should be identical, but verify the error-output messages match v4 byte-for-byte for any operator that scrapes them).

**Tests (Phase 3):**
- `cargo test --workspace --all-targets -- --test-threads=1` green (regression bar).
- New integration test: register a fake `MockSensor` factory, dispatch `neurogrim cast mock-sensor`, observe the mock was invoked. Mirrors V5-MOD-1 Phase 3's mock-source test.

**Ship criterion:** `neurogrim cast <each sensor>` produces identical CMDB output to pre-V5-MOD-2 for at least 5 representative sensors (smoke); workspace tests green.

### Phase 4 — Cargo-feature gates + binary-level pass-through (Day 5–8, ~3 days)

**Goal:** Each sensor becomes a `#[cfg(feature = "sensor-X")]`-gated registration. Enabled features in `neurogrim-sensory`'s Cargo.toml control which sensors compile in. **Plus** (Fork D ADD pass-through): `neurogrim-cli` + `neurogrim-mcp` Cargo.toml's gain matching pass-through features so operators can do `cargo build --bin neurogrim --no-default-features --features sensor-git-health,sensor-code-quality` at the binary level. Default feature set preserves v4 behavior (all 21 sensors).

**Cargo.toml strategy:**
```toml
# neurogrim-sensory/Cargo.toml

[features]
default = [
    "sensor-git-health", "sensor-code-quality", "sensor-test-health",
    "sensor-deploy-readiness", "sensor-coherence", "sensor-human-comms",
    "sensor-secret-refs", "sensor-rust-health", "sensor-trust-budget",
    # … all 21 sensors enabled by default
]

# Per-sensor features. Enabling a feature compiles in the sensor +
# any heavy deps the sensor uses. Operators who want a minimal build
# (--no-default-features --features sensor-git-health) get just one
# sensor and its deps.
sensor-git-health = []  # zero heavy deps
sensor-code-quality = []
sensor-test-health = []
# … etc

# Sensors with heavy deps gate the deps inside the feature.
# `toml` added per Subagent 2 finding (was missing from prior draft).
sensor-supply-chain-sca = ["dep:cargo-lock", "dep:yarn-lock-parser", "dep:semver", "dep:toml"]
sensor-supply-chain-vigilance = ["dep:tar", "dep:flate2", "dep:reqwest"]
sensor-capability-hygiene = ["dep:jsonschema"]

# Convenience aggregates (Subagent 2 🔵 suggestion). Operators get
# 3-name shortcuts for common bundles; the underlying per-sensor
# features remain available for fine-grained gating.
sensors-supply-chain = [
    "sensor-supply-chain-sca", "sensor-supply-chain-vigilance",
    "sensor-supply-chain-calibration", "sensor-supply-chain-review",
]
sensors-builtin-core = [
    "sensor-git-health", "sensor-code-quality", "sensor-test-health",
    "sensor-deploy-readiness", "sensor-coherence", "sensor-human-comms",
    "sensor-secret-refs", "sensor-rust-health", "sensor-trust-budget",
]

# Heavy deps marked optional in Phase 0 so they're only pulled in
# when their sensor is enabled. This is THE key step (Subagent 2
# 🔴 finding); without it the gates compile the deps regardless.
[dependencies]
cargo-lock = { workspace = true, optional = true }
yarn-lock-parser = { workspace = true, optional = true }
semver = { workspace = true, optional = true }
toml = { workspace = true, optional = true }
tar = { workspace = true, optional = true }
flate2 = { workspace = true, optional = true }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls-native-roots"], optional = true }
sha2 = { version = "0.10", optional = true }
jsonschema = { version = "0.17", optional = true }
# … etc
```

**`neurogrim-cli` + `neurogrim-mcp` pass-through (Fork D ADD):**
```toml
# neurogrim-cli/Cargo.toml — add per-feature pass-through.
[features]
default = ["neurogrim-sensory/default"]
# Each sensor feature is a thin pass-through to neurogrim-sensory's gate.
sensor-git-health = ["neurogrim-sensory/sensor-git-health"]
sensor-code-quality = ["neurogrim-sensory/sensor-code-quality"]
# … etc
sensors-supply-chain = ["neurogrim-sensory/sensors-supply-chain"]
sensors-builtin-core = ["neurogrim-sensory/sensors-builtin-core"]

[dependencies]
neurogrim-sensory = { workspace = true, default-features = false }  # was no default-features control
# … etc
```

Same pattern for `neurogrim-mcp/Cargo.toml`. Without this, an operator who builds the CLI binary gets all 21 sensors' deps pulled in regardless of `neurogrim-sensory`'s feature gates (because `neurogrim-cli` declares `neurogrim-sensory = { workspace = true }` without `default-features = false`, which activates all defaults).

**Source-level changes:**
- `neurogrim-sensory/src/lib.rs`: each `pub mod X;` becomes `#[cfg(feature = "sensor-X")] pub mod X;`.
- `neurogrim-sensory/src/registry.rs::built_in_factories()`: each entry becomes `#[cfg(feature = "sensor-X")] factories.push(Box::new(XSensorFactory));`.

**Edge cases (revised per Subagent 2 🟡):**
- **Meta-sensors with cross-CMDB reads — `coherence` false-positive-green fix.** `coherence.rs::evaluate_condition` returns `false` on missing variables (line 225); when `sensor-coherence` is ON but its peer sensors are OFF, the CMDB files don't exist → variable lookups return false → all correlations evaluate to "healthy non-firing" → **score stays 100, not "lower"** (the prior plan claim was wrong). Add a sentinel: at the top of `analyze_coherence`, if NO peer CMDB files are found in `<project>/.claude/`, return a degraded envelope with `score: 0` + a finding `coherence:no_peer_cmdbs_found`. Same fix-pattern applies to `domain-calibration` and `federated-patterns` (the other two meta-sensors). All three become honest about their missing-input case.
- `secret-refs` and `human-comms` are tightly coupled to non-feature-gated infrastructure (registry parsing, etc.); they don't need their own feature gate but are part of the default set.

**Test matrix (extended per Subagent 2 🟡 — 5 cells now, not 4):**
- `cargo check --no-default-features` (NEW) — fast type-check of the re-export wall; catches missing `#[cfg(feature)]` on `pub use` lines without running tests. CI runs this on every PR.
- `cargo test --no-default-features` — zero sensors registered; `SensorRegistry::with_built_ins()` is empty; workspace tests still green (sensor-asserting tests are feature-gated to match).
- `cargo test --no-default-features --features sensor-git-health,sensor-code-quality,sensor-test-health` — minimal usable set; the 3 named sensors registered, others absent.
- `cargo test --no-default-features --features sensors-supply-chain` — convenience-aggregate sanity check.
- `cargo test --all-features` — all 21 sensors + all heavy deps; baseline that matches v4 behavior.
- `cargo test` (default) — same as `--all-features` for v4 backward compat.

**CI enforcement:** all 5 builds run on every PR (Subagent 2 🟡 — without CI, the matrix drifts within months). Add to `.github/workflows/` (or whatever CI infra V5-FOUND-1 already established for the workspace).

**Tests (Phase 4):**
- A `cfg!` test that verifies which sensors are registered when only `sensor-git-health` is enabled.
- A doctest in `registry.rs` showing the feature-gated registration pattern.
- A test of the meta-sensor degraded-envelope fall-through (the `coherence:no_peer_cmdbs_found` finding).

**Ship criterion:** all 5 test-matrix builds green in CI; binary size delta documented (e.g., "minimum-features build: -23 MB vs default-features build" — proves the gating actually carved out the heavy deps).

### Phase 5 — Conformance suite (Day 7–9, ~2 days)

**Goal:** Generalize V5-MOD-1's `scoring_source_conformance` into a sensor conformance suite. Add sensor-specific tests beyond the cross-cutting V5-MOD-1 base.

**Files (new):**
- `neurogrim-core/src/sensor_conformance.rs`

**Test count target: ≥10 tests** (V5-MOD-2 epic Done-When says ≥6; we exceed):

Cross-cutting (port from V5-MOD-1's 8 tests):
1. `name_non_empty` — `factory.name()` returns non-empty string.
2. `name_stable_across_calls` — multiple calls return the same string.
3. `factory_sensor_name_consistency` — `factory.build().name() == factory.name()`.
4. `factory_build_repeatable` — 10 successive `build()` calls don't panic.
5. `built_sensors_name_stable_across_builds` — independent builds produce same name.
6. `analyze_with_skeletal_project_root_returns_or_degrades` — empty tempdir; sensor must return `Ok(cmdb)` with degraded score OR `Err(...)` (NOT panic, NOT block).
7. `analyze_is_concurrent_safe` — 50 parallel `analyze()` calls don't deadlock.
8. `analyze_is_idempotent_on_identical_input` — repeated `analyze()` returns same Some/None category.

Sensor-specific (new for V5-MOD-2):
9. `analyze_output_validates_against_cmdb_envelope_v1_schema` — the returned CMDB JSON conforms to `cmdb-envelope-v1.schema.json` (loaded from `neurogrim-core/data/schemas/`). This is the strongest contract test — any sensor that ships malformed envelopes fails here.
10. `analyze_score_is_in_range_0_to_100` — even on degraded paths, `score` field MUST be in `[0, 100]` (defense-in-depth alongside the schema constraint).
11. `analyze_meta_updated_by_matches_factory_name` — the CMDB's `meta.updated_by` field must equal the factory's `name()`. Detects sensors that hardcode the wrong name string in their `build_cmdb` call.
12. `analyze_completes_within_reasonable_timeout` — 30-second ceiling for the skeletal-project-root path. Sensors that hit the timeout violate the "fast-fail on no project state" contract.

**Tests at the suite-level:**
- All 21 built-in factories pass the suite.
- The example sensor crate (Phase 6) passes the suite.

**Ship criterion:** suite passes on all 21 built-ins; documented as the contract any third-party `Sensor` impl must pass.

### Phase 6 — Out-of-tree example crate (Day 9–11, ~2 days)

**Goal:** Ship `examples/sensor-jira/` (or similar — pick a sensor type that's NOT already built-in but reads as obviously useful). Reads from a Jira REST endpoint; produces a CMDB envelope from issue counts (open bugs, P0/P1 counts, etc.).

**Files (new):**
- `examples/sensor-jira/Cargo.toml`
- `examples/sensor-jira/src/lib.rs` (the `JiraSensor` + `JiraSensorFactory`)
- `examples/sensor-jira/tests/conformance.rs` (runs Phase 5 conformance suite)
- `examples/sensor-jira/README.md` (third-party adoption template — same shape as `examples/scoring-source-prom/README.md`)

**Behavior:**
- Configurable via env vars: `JIRA_BASE_URL`, `JIRA_TOKEN`, `JIRA_PROJECT_KEY`. (No `Sensor` config struct yet — defer to Phase 7+ if needed; the V5-MOD-2 trait shape is sufficient with env-var lookup.)
- HTTP-fetches `<base>/rest/api/2/search?jql=project=<key>+AND+priority+in+(P0,P1)+AND+status=open&fields=key`; parses the count.
- Score: `100 - clamp(open_p0_p1_count, 0, 100)` (more open critical bugs = lower score).
- Failure modes: missing env vars / unreachable / non-2xx → `Ok(degraded_envelope)` with score=50 and a finding explaining the degradation. NOT `Err(...)` — operators want to see "Jira unreachable" as a finding, not a CLI error.

**Conformance integration test:** runs `run_factory_conformance(&JiraSensorFactory, tempdir.path())`; asserts all 12 tests pass.

**Ship criterion:** `cargo build -p sensor-jira` succeeds; conformance test passes; README has a third-party Cargo.toml template.

### Phase 7 — Epic close-out + LSP-Brains spec sync (Day 11–12, ~0.5 day)

- Update `v5-modular-conversions.md`: mark V5-MOD-2 status COMPLETE; check off all Done-When items (including the already-fixed coupling-fix item with a "verified at V5-MOD-2 open" note); cross-reference the perf-result + conformance suite.
- **LSP-Brains spec sync**: spec §F (MCP sensory tools) currently describes sensors as a closed catalog. Update to "extensible via the `Sensor` trait" + 2-sentence note. Spec edit is small; no version bump required (descriptive change, not normative).
- **V5-SDK coordination cross-reference**: Theme C's V5-SDK epic should re-export the `Sensor` trait + `SensorFactory` + `SensorRegistry`, plus the conformance suite. Add to V5-SDK epic's hand-off-note section (mirror of what V5-MOD-1 Phase 7 did for `ScoringSource`).

## Files inventory

### New
- `neurogrim-core/data/schemas/cmdb-envelope-v1.schema.json` (Phase 0; vendored)
- `neurogrim-core/src/sensor.rs` (Phase 1)
- `neurogrim-core/src/sensor_conformance.rs` (Phase 5)
- `neurogrim-sensory/src/registry.rs` (Phase 3)
- `examples/sensor-jira/{Cargo.toml,src/lib.rs,tests/conformance.rs,README.md}` (Phase 6)

### Modified
- All 21 `neurogrim-sensory/src/<sensor>.rs` files (Phase 2: add `XSensor` + `XSensorFactory`)
- `neurogrim-sensory/src/lib.rs` (Phase 2: re-exports; Phase 4: `#[cfg(feature)]` gates on `pub mod`)
- `neurogrim-sensory/Cargo.toml` (Phase 4: feature flags + optional deps)
- `neurogrim-cli/src/main.rs` (Phase 3: replace 21-arm match with `SensorRegistry` dispatch)
- `roadmap/epics/v5-modular-conversions.md` (Phase 7: status → Complete; coupling-fix already-done note)
- `roadmap/epics/v5-sdk.md` (Phase 7: V5-MOD-2 hand-off note for SDK)
- `D:/Brains/LSP-Brains/spec/LSP-BRAINS-SPEC.md` § F (Phase 7: extensibility note)

## Risks (from epic + new ones surfaced by this plan)

🟡 **Sensor return-shape variance** — 17 sensors return `Value`; 4 return `Result<Value>`. Trait normalizes on `Result`. The infallible 17 keep their existing free-function signatures; the trait wrapper trivially `Ok(...)`s. Mitigation: Phase 0 audit captures the variance; Phase 2 normalization is mechanical.

🟡 **Cargo-feature combinatorial explosion** — 21 features = 2^21 build matrix. CI cannot test all. Mitigation: test 4 specific combinations (`--no-default-features`, single-feature minimum, supply-chain bundle, `--all-features`). Trust feature-gating to compose in between. Document the test matrix in Phase 4's commit message.

🟡 **Perf — same risk as V5-MOD-1, but much looser ceiling.** Sensor invocations are slow IO (git, cargo audit, network) at the seconds-per-sensor scale. `Box<dyn>` overhead at ~50ns + one allocation is rounding error. **No formal perf-gate needed for V5-MOD-2** (unlike V5-MOD-1, which had 19 domains in tight loop). Phase 4 commit message captures one timed `neurogrim cast <heavy-sensor>` before/after as informal regression check.

🟡 **CMDB schema vendoring drift** — `cmdb-envelope-v1.schema.json` lives canonically in LSP-Brains; the vendored copy in `neurogrim-core/data/schemas/` can drift if the spec ships a v2 schema. Mitigation: vendor-copy comment cites the canonical path + the spec version it was synced against; an `xtask` check (Phase 7 close-out or BACKLOG) compares the two files and warns on drift.

🟡 **MCP per-sensor `XServer` co-existence** — each sensor module today exposes both `analyze_X` (free function) AND `XServer` (MCP server type). After V5-MOD-2 the `XSensor` trait impl is the third surface. **Plan keeps all three** — backward compat held. Future refactor (post-V5-MOD-2 / v5.5): consolidate the three into one — derive the MCP server type from the trait via macro. Out of scope.

🟡 **The "already-fixed" coupling smell** — the V5-MOD-2 Done-When item "neurogrim-sensory no longer depends on neurogrim-cli" is a stale concern from a v3-era observation. Verified at V5-MOD-2 plan recon: no Cargo.toml dep, no `use neurogrim_cli` import. Mitigation: mark already-satisfied at V5-MOD-2 open; cite the verification command in the Phase 0 commit message.

🔵 **Suggestion — a `--list-sensors` CLI flag** (forwarded from V5-MOD-1's same suggestion). Operator visibility into which factories are registered in the current build. Especially useful with cargo-feature gates: helps operators verify their `--features` set actually compiled in what they wanted. v5.5 polish.

🔵 **Suggestion — derive sensor MCP-server type from `Sensor` trait via macro.** Eliminates the per-sensor `XServer` boilerplate. Out of scope for V5-MOD-2; queue as v5.5 / Theme D concern.

## Iteration boundaries

| Iter | Phases | Shippable? | Rough duration |
|---|---|---|---|
| 0 | Phase 0 (schema vendor + audits) | Yes — additive, no behavior change | ~0.5 day |
| 1 | Phase 1 (trait def + registry) | Yes — new module, no dispatch yet | ~1 day |
| 2 | Phase 2 (built-in factories — 21 sensors) | Yes — sensors usable via trait OR free function | ~2 days |
| 3 | Phase 3 (dispatch conversion) | Yes — semantics unchanged, dispatch through trait | ~1 day |
| 4 | Phase 4 (cargo features) | Yes — feature-gated builds work | ~2 days |
| 5 | Phase 5 (conformance suite) | Yes — third-party-impl contract documented | ~2 days |
| 6 | Phase 6 (example crate) | Yes — modularity proven | ~2 days |
| 7 | Phase 7 (close-out) | Yes — epic complete | ~0.5 day |

Total: ~11 days. Within epic L estimate (10–14 days).

## Verification (end-to-end, after Iter 7)

1. `cargo test --workspace --all-targets -- --test-threads=1` green.
2. `cargo test --no-default-features --features sensor-git-health` green; only 1 sensor registered.
3. `cargo test --all-features` green; all 21 sensors registered; binary size compared to default build.
4. `neurogrim cast <each sensor>` produces identical CMDB output to pre-V5-MOD-2 (smoke; sample 5 representative sensors).
5. Conformance suite passes against all 21 built-in factories + the Jira example.
6. `cargo build -p sensor-jira` succeeds; conformance integration test green.
7. `neurogrim doctor` does not regress.

## What this plan does NOT do

- Does **not** implement V5-MOD-3 (QueueBackend factory) — separate Theme B story.
- Does **not** add dynamic plugin loading (cdylib/libloading) — deferred to v5.5 BACKLOG B-40.
- Does **not** add a `--list-sensors` CLI flag — v5.5 polish (forwarded as 🔵 suggestion).
- Does **not** consolidate the three sensor surfaces (`analyze_X`, `XServer`, `XSensor`) — backward compat held, consolidation deferred.
- Does **not** restructure `cmdb-envelope-v1` schema — only validates against it.

## Cross-references

- Epic: `roadmap/epics/v5-modular-conversions.md` § V5-MOD-2
- Master roadmap: `roadmap/v5-roadmap.md`
- V5-MOD-1 close-out (substrate this plan extends): `roadmap/epics/v5-modular-conversions.md` § V5-MOD-1, commit `fb45d5d`
- V5-MOD-1 implementation plan (template this plan mirrors): `.claude/plans/v5-mod-1-scoring-source-trait.md`
- Existing dispatch (Phase 3 target): `neurogrim-cli/src/main.rs:599-626` (`run_sensory`)
- CMDB envelope schema: `D:/Brains/LSP-Brains/schemas/cmdb-envelope-v1.schema.json`
- Pattern reference (trait + registry + conformance): `neurogrim-core/src/scoring_source.rs` + `neurogrim-core/src/scoring_source_conformance.rs`
- V5-MOD-1 example crate (template for Phase 6): `neurogrim/examples/scoring-source-prom/`
