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
