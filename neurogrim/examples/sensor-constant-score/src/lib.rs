//! # `sensor-constant-score` — V5-SDK-1 reference example
//!
//! The smallest-possible third-party `Sensor` implementation,
//! depending **only on `neurogrim-sdk`** (not `neurogrim-core`,
//! `neurogrim-sensory`, or any other internal crate). Proves the
//! V5-SDK-1 modularity claim: plugin authors get a stable contract
//! surface with one cargo dep.
//!
//! ## What it does
//!
//! [`ConstantScoreSensor`] always reports `score: 42` regardless
//! of the project state. Useful for:
//!
//! - **Tests** that need a Sensor stub without setting up real
//!   project state (rust-health, git-health, etc., all touch the
//!   filesystem; a constant sensor doesn't).
//! - **Reading the SDK pattern** — this crate's source IS the
//!   minimal-deps reference for "what does a third-party Sensor
//!   look like?". Total: ~80 lines of Rust + ~20 lines of
//!   conformance test.
//!
//! For richer patterns:
//! - File-system: `examples/sensor-readme-quality/` (V5-MOD-2 example)
//! - HTTP-fetch: `examples/scoring-source-prom/` (V5-MOD-1 example)
//! - Queue backend: `examples/queue-backend-memory/` (V5-MOD-3 example)
//!
//! ## How a consuming binary registers it
//!
//! ```ignore
//! use neurogrim_sdk::SensorRegistry;
//! use sensor_constant_score::ConstantScoreSensorFactory;
//!
//! let mut registry = SensorRegistry::new();
//! registry.register(Box::new(ConstantScoreSensorFactory));
//! ```
//!
//! ## Cargo.toml template for true third-party use
//!
//! ```toml
//! [package]
//! name = "my-sensor"
//! version = "0.1.0"
//! edition = "2021"
//!
//! [dependencies]
//! neurogrim-sdk = "0.1"
//! async-trait = "0.1"
//! anyhow = "1"
//! serde_json = "1"
//! chrono = { version = "0.4", features = ["serde"] }
//!
//! [dev-dependencies]
//! tokio = { version = "1", features = ["full"] }
//! tempfile = "3"
//! ```
//!
//! No internal NeuroGrim crates required — `neurogrim-sdk` is the
//! only NeuroGrim dep.

use async_trait::async_trait;
use chrono::Utc;
use neurogrim_sdk::{Sensor, SensorFactory};
use serde_json::{json, Value};

/// Stable wire-name. Must match `ConstantScoreSensorFactory::name()`.
pub const SENSOR_NAME: &str = "constant-score";

/// The constant score this sensor always reports. `42` chosen for
/// the obvious reason; not configurable in this example (production
/// third-party sensors that take config typically read it from env
/// vars, the project root's `.claude/` directory, or a constructor
/// parameter — see `sensor-readme-quality` for env-free patterns
/// or `scoring-source-prom` for env-var patterns).
const CONSTANT_SCORE: u8 = 42;

/// Toy [`Sensor`] that always reports `score: 42` regardless of
/// project state. Stateless, `Send + Sync` (auto-derived for unit
/// structs).
pub struct ConstantScoreSensor;

#[async_trait]
impl Sensor for ConstantScoreSensor {
    async fn analyze(&self, _project_root: &str) -> anyhow::Result<Value> {
        let now = Utc::now().to_rfc3339();
        Ok(json!({
            "meta": {
                "schema_version": "1",
                "updated_at": now,
                "updated_by": SENSOR_NAME,
            },
            "score": CONSTANT_SCORE,
            "updated_at": now,
            "findings": [
                {
                    "name": "constant_score:fixture",
                    "status": "found",
                    "points": 0,
                    "detail": "this sensor always reports score=42",
                }
            ],
        }))
    }
}

/// Factory for [`ConstantScoreSensor`]. Stateless.
pub struct ConstantScoreSensorFactory;

impl SensorFactory for ConstantScoreSensorFactory {
    fn name(&self) -> &'static str {
        SENSOR_NAME
    }

    fn build(&self) -> Box<dyn Sensor> {
        Box::new(ConstantScoreSensor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn always_returns_score_42() {
        let dir = TempDir::new().unwrap();
        let envelope = ConstantScoreSensor
            .analyze(dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(envelope["score"], CONSTANT_SCORE);
        assert_eq!(envelope["meta"]["updated_by"], SENSOR_NAME);
    }

    #[tokio::test]
    async fn factory_round_trip() {
        let factory = ConstantScoreSensorFactory;
        assert_eq!(factory.name(), SENSOR_NAME);
        let sensor = factory.build();
        let envelope = sensor.analyze("/tmp").await.unwrap();
        assert_eq!(envelope["score"], CONSTANT_SCORE);
    }
}
