# `neurogrim-sdk`

Stable contract surface for [NeuroGrim](https://github.com/KeenanHoffman/NeuroGrim)
plugin authors. Re-exports the V5 trait + factory + registry
contracts as a versioned-independently SDK.

NeuroGrim is the reference implementation of the
[LSP Brains specification](https://github.com/KeenanHoffman/LSP-Brains)
— a methodology for giving AI agents continuous project health
awareness through MCP-based sensory tools, cross-domain
correlation, trajectory intelligence, and gated governance. The
SDK exposes the trait surfaces third-party crates use to extend
NeuroGrim with custom sensors, scoring sources, queue backends,
A2A transports, or secrets backends.

## What's here

Five trait surfaces:

| Trait | Source | Theme |
|-------|--------|-------|
| `ScoringSource` + factory + registry | `neurogrim-core` | V5-MOD-1 |
| `Sensor` + factory + registry | `neurogrim-core` | V5-MOD-2 |
| `QueueBackend` + factory + registry | `neurogrim-core` | V5-MOD-3 |
| `Transport` (A2A peer protocol) | `neurogrim-a2a` | v3.x |
| `SecretBackend` (encrypted-secrets backend) | `neurogrim-secrets` | v4.2 S14 |

Three conformance suites — cross-cutting tests every conformant
impl must pass. Third-party authors copy `tests/conformance.rs`
verbatim and have a verifiable "passes the same contract as
built-ins" guarantee:

- `scoring_source_conformance::run_factory_conformance` (V5-MOD-1)
- `sensor_conformance::run_factory_conformance` (V5-MOD-2)
- `queue_backend_conformance::run_factory_conformance` (V5-MOD-3)

Plus the canonical `conformance::ConformanceReport` +
`conformance::TestResult` types — single nominal types shared
across all three suites so consumers writing multiple plugin
types don't hit nominal-type mismatches.

## Stability

**`0.x` — pre-1.0.** The V5-SDK epic explicitly allows
trait-shape changes between minor bumps if Theme B reveals a
flaw post-ship. Promotion to `1.0` requires:

1. ≥6 weeks of soak post-Theme-B-completion (Theme B closed
   2026-05-02 → earliest `1.0` ~2026-06-13).
2. At least one external adopter confirming the surface works
   for their use case.

Until `1.0`, pin to `neurogrim-sdk = "0.1"` and check the
release notes when bumping minor versions.

## Quick start

A toy `Sensor` that always reports `score: 100`:

```rust
use neurogrim_sdk::{Sensor, SensorFactory};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct GreenSensor;

#[async_trait]
impl Sensor for GreenSensor {
    async fn analyze(&self, _project_root: &str) -> anyhow::Result<Value> {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(json!({
            "meta": { "schema_version": "1", "updated_at": now, "updated_by": "green" },
            "score": 100,
            "updated_at": now,
            "findings": [],
        }))
    }
}

pub struct GreenSensorFactory;
impl SensorFactory for GreenSensorFactory {
    fn name(&self) -> &'static str { "green" }
    fn build(&self) -> Box<dyn Sensor> { Box::new(GreenSensor) }
}
```

`Cargo.toml`:

```toml
[dependencies]
neurogrim-sdk = "0.1"
async-trait = "0.1"
anyhow = "1"
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
```

For richer patterns covering each trait surface, see the four
example crates in the
[NeuroGrim repository](https://github.com/KeenanHoffman/NeuroGrim/tree/main/neurogrim/examples):

- **Sensor (minimal-deps reference):** `sensor-constant-score`
- **Sensor (file-system):** `sensor-readme-quality`
- **ScoringSource (HTTP-fetch):** `scoring-source-prom`
- **QueueBackend (in-memory + ack):** `queue-backend-memory`

## Writing a conformant `Sensor`

Sensors produce CMDB envelopes that the scoring pipeline
consumes. Use this when you want to plug a new data source
(Jira, GitHub, custom telemetry) into NeuroGrim's `cast`
dispatch. Built-in sensors live in `neurogrim-sensory` and
cover ~21 domains; third-party sensors register alongside.

`Cargo.toml`:

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
# V5-SDK-2 partial Phase 4 — opt into the conformance feature at
# test-build time so tests/conformance.rs can reach the suite.
# Production builds (no --tests) stay tokio-clean.
neurogrim-sdk = { version = "0.1", features = ["conformance"] }
```

Minimum-viable impl (stateless, infallible-degrading; matches
the contract of 18-of-21 built-in sensors):

```rust,ignore
use async_trait::async_trait;
use neurogrim_sdk::{Sensor, SensorFactory};
use serde_json::{json, Value};

pub struct MySensor;

#[async_trait]
impl Sensor for MySensor {
    async fn analyze(
        &self,
        project_root: &str,
    ) -> anyhow::Result<Value> {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(json!({
            "meta": {
                "schema_version": "1",
                "updated_at": now,
                "updated_by": "my-sensor",
            },
            "score": 100,
            "updated_at": now,
            "findings": [],
        }))
    }
}

pub struct MySensorFactory;

impl SensorFactory for MySensorFactory {
    fn name(&self) -> &'static str { "my-sensor" }
    fn build(&self) -> Box<dyn Sensor> { Box::new(MySensor) }
}
```

Conformance test (`tests/conformance.rs` in your crate):

```rust,ignore
use neurogrim_sdk::sensor_conformance::run_factory_conformance;
use my_sensor::MySensorFactory;
use tempfile::TempDir;

#[tokio::test]
async fn passes_full_conformance_suite() {
    let dir = TempDir::new().unwrap();
    let report = run_factory_conformance(&MySensorFactory, dir.path()).await;
    assert!(
        report.all_passed(),
        "{}/{} failed: {:#?}",
        report.failures().len(),
        report.total(),
        report.failures()
    );
}
```

**Contract pitfalls to avoid:**

- Never panic. The conformance suite catches panics in
  `analyze` calls; if your sensor encounters an unexpected
  project state, return an `Err(anyhow!(...))` or a degraded
  `Ok(envelope)` with `score: 0` + a finding describing the
  failure.
- The `meta.schema_version` field MUST equal `"1"` (string,
  not integer).
- The top-level `score` MUST be an integer in `[0, 100]`.
- Both `meta.updated_at` and the top-level `updated_at` MUST
  be RFC3339 strings.
- Don't take a long time on skeletal input. The conformance
  suite has a 30-second timeout; sensors that block on
  missing-file IO should fast-fail.

Reference: `examples/sensor-readme-quality/` (file-system
pattern), `examples/sensor-constant-score/` (minimal-deps
pattern; SDK reference example).

For `ScoringSource` (V5-MOD-1) and `QueueBackend` (V5-MOD-3)
walkthroughs — covering HTTP-fetch and in-memory ack-supported
patterns respectively — see the canonical rustdoc on
[docs.rs/neurogrim-sdk](https://docs.rs/neurogrim-sdk) (each
walkthrough mirrors this Sensor pattern with the trait-specific
contract pitfalls). Brief pointers:

- **`ScoringSource`** — implements `async fn load(...) -> Option<CmdbData>`;
  contract is "return None on any failure, never panic." Reference:
  `examples/scoring-source-prom/` (Prometheus instant-query pattern).
- **`QueueBackend`** — implements `append`/`read_from`/`len` with
  `Send + Sync` interior mutability; per-consumer-group ack tracking
  needs `BTreeSet<u64>`, NOT `HashMap<String, u64>` high-water-mark
  (out-of-order acks must be representable). Reference:
  `examples/queue-backend-memory/` (full ack-supported in-memory pattern).

## Conformance

The SDK ships three published conformance suites covering every
trait's contract. Third-party authors run them as integration
tests against their own factories — passing means the impl
honors the same negative-path discipline (no panics, never
deadlocks, idempotent on identical input, fast-fails on skeletal
config) that every built-in honors.

```rust
use neurogrim_sdk::sensor_conformance::run_factory_conformance;
use my_crate::MySensorFactory;
use tempfile::TempDir;

#[tokio::test]
async fn passes_full_conformance_suite() {
    let dir = TempDir::new().unwrap();
    let report = run_factory_conformance(&MySensorFactory, dir.path()).await;
    assert!(
        report.all_passed(),
        "{}/{} failed: {:#?}",
        report.failures().len(),
        report.total(),
        report.failures()
    );
}
```

The conformance report types (`ConformanceReport`, `TestResult`)
are single nominal types across all three suites — write a
helper that takes `&ConformanceReport` and reuse it across your
sensor + scoring source + queue backend impls.

## What's NOT here

- **Implementation crates** (`JsonlBackend`, `SqliteBackend`,
  the 21 built-in sensors, the cmdb/a2a/function scoring sources):
  live in `neurogrim-core`, `neurogrim-sensory`, etc. The SDK is
  the **contract** crate; impls reach via direct dependency on
  the appropriate internal crate for adopters who need them.
- **`ScoringSourceConfig`** (the `brain-registry.json` serde
  shape): bound to the registry schema, can drift independently
  of the trait. SDK consumers depend on the trait's *behavior*,
  not the config's serde layout.
- **`TestRunner` (V5-FOUND-4):** unshipped at SDK 0.1.0 release.
  Will be added as a pure additive minor bump (`0.2.0`) when
  V5-FOUND-4 lands.

## License

Licensed under the same terms as NeuroGrim itself
(see `LICENSE` in the repo).

## Cross-references

- [NeuroGrim repository](https://github.com/KeenanHoffman/NeuroGrim)
- [LSP Brains specification](https://github.com/KeenanHoffman/LSP-Brains)
- [V5-SDK epic](https://github.com/KeenanHoffman/NeuroGrim/blob/main/roadmap/epics/v5-sdk.md)
