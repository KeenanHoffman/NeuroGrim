---
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# `sensor-constant-score` — V5-SDK-1 reference example

The smallest-possible third-party `Sensor` implementation,
depending **only on `neurogrim-sdk`**. Proves the V5-SDK-1
modularity claim end-to-end: plugin authors get a stable contract
surface with one cargo dep.

## Why this example

V5-MOD-2's `sensor-readme-quality` and V5-MOD-1's
`scoring-source-prom` and V5-MOD-3's `queue-backend-memory` all
demonstrate **realistic** third-party patterns (file-system,
HTTP-fetch, in-memory). This example deliberately strips that
realism away — `ConstantScoreSensor` always returns `score: 42` —
to isolate **the SDK surface** as the variable being demonstrated.

If you want to know "what does a Sensor depend on, minimally?",
this is the answer in ~80 lines of Rust + ~20 lines of
conformance test.

## Dependency claim

```toml
[dependencies]
neurogrim-sdk = "0.1"        # ← ONLY NeuroGrim dep
async-trait = "0.1"          # ← required for #[async_trait]
anyhow = "1"                 # ← Sensor::analyze return type
serde_json = "1"             # ← CMDB envelope construction
chrono = "0.4"               # ← RFC3339 timestamps
```

**Five dependencies total**, only one of them NeuroGrim. The
example crate does NOT import `neurogrim-core`, `neurogrim-sensory`,
or any other internal crate. The SDK re-exports everything needed.

## Reading order

1. `Cargo.toml` — confirms the dependency claim above.
2. `src/lib.rs` — `ConstantScoreSensor` + `ConstantScoreSensorFactory`
   + 2 unit tests.
3. `tests/conformance.rs` — runs the V5-MOD-2 conformance suite
   from `neurogrim_sdk::sensor_conformance::*`. Asserts all 10+
   tests pass.

## How a consuming binary registers it

```rust
use neurogrim_sdk::SensorRegistry;
use sensor_constant_score::ConstantScoreSensorFactory;

let mut registry = SensorRegistry::new();
registry.register(Box::new(ConstantScoreSensorFactory));
```

## Stability promise

The SDK surface this crate uses is `0.x`. Per the V5-SDK epic,
trait shapes can change in minor bumps until `1.0` (≥6 weeks of
soak + external-adopter validation). When the SDK promotes to
`1.0`, this example's `Cargo.toml` updates `neurogrim-sdk = "1"`
and the contract is locked.

## Cross-references

- **V5-SDK-1 plan:** `.claude/plans/v5-sdk-1-thin-reexport.md`
- **`neurogrim-sdk`:** `crates/neurogrim-sdk/`
- **Companion examples:**
  `examples/{scoring-source-prom,sensor-readme-quality,queue-backend-memory}`
