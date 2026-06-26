//! A.2.1 — `SensoryBroker` trait + `SensorBackedBroker` wrapper.
//!
//! Substrate-side pattern for [Sense]-role brokers whose overlay IS a
//! CMDB. The substrate provides:
//!
//! - **[`SensoryBroker`]** — opt-in trait on top of [`Broker`] +
//!   [`Extensible`]. Declares the sensor's wire-name; default
//!   `cmdb_path()` returns the canonical
//!   `<project_root>/.claude/<sensor>-cmdb.json` path.
//! - **[`SensorBackedBroker`]** — generic concrete impl that wraps any
//!   [`neurogrim_core::sensor::SensorFactory`] as a SensoryBroker. Used
//!   by A.2.4's bulk migration of the 26 built-in sensors: one
//!   `SensorBackedBroker::new(factory, project_root)` per sensor.
//!
//! ## CMDB path contract (per Gate 2)
//!
//! Every SensoryBroker's `cmdb_path()` returns the canonical path
//! `<project_root>/.claude/<sensor>-cmdb.json`. This is the **HARD
//! CONSTRAINT** that lets existing CMDB consumers (scoring engine,
//! dashboard at port 8420, MCP `brain_query`, A2A peer queries,
//! coherence sensor) continue reading the same files without change.
//!
//! Registry override (cluster.toml's `domain_definitions.<id>.scoring_source.path`)
//! is consulted by the host BEFORE calling `cmdb_path()`; brokers
//! don't need to know about overrides.
//!
//! ## Catalog shape
//!
//! Each SensorBackedBroker exposes 2 pipelines:
//! - `<broker_id>/query-current-score` — Internal, ReadOnly. Returns the
//!   most recent CMDB envelope (cached in overlay; updated on dispatch).
//! - `<broker_id>/run-sensor` — Surfaced, ColdStoreWrite, InnateAbility.
//!   Recomputes the sensor on demand; refreshes overlay + writes CMDB
//!   via [`crate::materializer::cmdb_writer::CmdbMaterializer`].
//!
//! ## Sensory Queue enforcer (BB #18)
//!
//! Built-in sensors (the 26 in `neurogrim-sensory`) bypass the enforcer —
//! they're pre-trusted code. Tier 1 declarative sensor extensions (A.2.3)
//! and Tier 2 operator-authored sensors MUST write through the enforcer
//! per [`crate::sensory_queue::SensoryQueueEnforcerV1`].
//!
//! Bypassing: built-in sensors instantiated via `SensorBackedBroker::new`
//! constitute the "pre-trusted" set. Extension-loaded sensors call
//! `with_enforcer()` to wire the enforcer in.

use crate::broker::{Broker, BrokerError, Role, RoleSet, WorldEvent};
use crate::extension::{ExtensionConfig, ExtensionError, Extensible};
use crate::overlay::Overlay;
use crate::pipeline::{
    AuditClass, EffectClass, Pipeline, Step, Tunability, Visibility,
};
use crate::runner::{LeafContext, LeafError};
use async_trait::async_trait;
use neurogrim_core::sensor::{Sensor, SensorFactory};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Opt-in trait every [Sense]-role broker that exports a CMDB implements.
/// Composes with [`Broker`] + [`Extensible`].
pub trait SensoryBroker: Broker + Extensible {
    /// Stable sensor wire-name (e.g., "secret-refs", "coherence").
    /// Used by the canonical CMDB path derivation.
    fn sensor_name(&self) -> &str;

    /// Canonical CMDB path relative to project root. Matches the legacy
    /// sensor CMDB path contract that scoring engine, dashboard, MCP,
    /// and A2A peers all depend on. Override only if your sensor needs
    /// a non-canonical path.
    fn canonical_cmdb_path(&self) -> PathBuf {
        PathBuf::from(format!(".claude/{}-cmdb.json", self.sensor_name()))
    }
}

// ============================================================================
// SensorBackedBroker — generic SensoryBroker wrapping a SensorFactory
// ============================================================================

/// Per-broker overlay: the most recent CMDB envelope produced by the
/// wrapped sensor. The Overlay snapshot pattern means readers get an
/// atomic-swap view; the CmdbMaterializer serializes this to disk on
/// every materialization pass.
#[derive(Debug, Clone, Default)]
pub struct SensorOverlay {
    /// Most recent CMDB envelope (cmdb-envelope-v1 shape). Empty
    /// `serde_json::Value::Null` before first dispatch.
    pub current_envelope: Value,
    /// ISO 8601 UTC timestamp of last successful sensor run; None
    /// before first dispatch.
    pub last_run_at: Option<String>,
    /// Count of successful dispatches since broker boot (operator-
    /// observability signal).
    pub run_count: u64,
}

/// Concrete SensoryBroker that wraps a [`SensorFactory`]. Used by every
/// sensor migrated in A.2.4 — instantiate one per sensor, give it the
/// factory + project root, register with the broker host.
pub struct SensorBackedBroker {
    id: String,
    sensor_name: String,
    project_root: PathBuf,
    factory: Arc<dyn SensorFactory>,
    overlay: Arc<Overlay<SensorOverlay>>,
    /// Optional Sensory Queue enforcer. None = pre-trusted (built-in
    /// sensors bypass). Some = sensor writes flow through the enforcer
    /// (Tier 1/Tier 2 extension-loaded sensors).
    enforcer: Option<Arc<crate::sensory_queue::SensoryQueueEnforcerV1>>,
}

impl SensorBackedBroker {
    /// Construct a SensorBackedBroker wrapping the given factory.
    /// The broker_id defaults to the sensor's wire-name; override via
    /// `with_id()` if multiple instances of the same sensor are needed.
    pub fn new(factory: Arc<dyn SensorFactory>, project_root: PathBuf) -> Self {
        let sensor_name = factory.name().to_string();
        Self {
            id: sensor_name.clone(),
            sensor_name,
            project_root,
            factory,
            overlay: Arc::new(Overlay::new(SensorOverlay::default())),
            enforcer: None,
        }
    }

    /// Override the broker_id (default = sensor_name). Use when you
    /// have multiple instances of the same sensor wrapped under
    /// different broker IDs (rare; mostly for testing).
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Wire a Sensory Queue enforcer for this broker. Extension-loaded
    /// sensors (Tier 1/Tier 2) MUST set this; built-in sensors bypass.
    pub fn with_enforcer(
        mut self,
        enforcer: Arc<crate::sensory_queue::SensoryQueueEnforcerV1>,
    ) -> Self {
        self.enforcer = Some(enforcer);
        self
    }

    pub fn catalog(&self) -> Vec<Pipeline> {
        let mut pipelines = vec![
            Pipeline {
                id: format!("{}/query-current-score", self.id),
                visibility: Visibility::Internal,
                tunability: Tunability::Untunable,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::ReadOnly,
                params: serde_json::json!({"type": "object", "properties": {}}),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: "query_current_score".to_string(),
                }],
                description: format!(
                    "Read the most recent CMDB envelope for {} from cache.",
                    self.sensor_name
                ),
                when_to_use: "Operator dashboard / scoring engine; cheap O(1) read.".to_string(),
                bypasses_kill_switch: false,
            },
            Pipeline {
                id: format!("{}/run-sensor", self.id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorConfirmed,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::ColdStoreWrite,
                params: serde_json::json!({"type": "object", "properties": {}}),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: "run_sensor".to_string(),
                }],
                description: format!(
                    "Re-run the {} sensor against the current project state; refreshes CMDB.",
                    self.sensor_name
                ),
                when_to_use: "When the agent needs a fresh score (e.g., after a code change).".to_string(),
                bypasses_kill_switch: false,
            },
        ];
        pipelines.extend(
            crate::governance::GovernanceComposer::canonical_governance_pipelines(&self.id),
        );
        pipelines
    }
}

#[async_trait]
impl Broker for SensorBackedBroker {
    fn id(&self) -> &str {
        &self.id
    }

    fn role_set(&self) -> RoleSet {
        RoleSet::single(Role::Sense)
    }

    async fn read_overlay(&self) -> Value {
        let snap = self.overlay.load();
        // Project the current envelope verbatim — this is what the
        // CmdbMaterializer serializes to .claude/<sensor>-cmdb.json.
        // External consumers expect cmdb-envelope-v1 shape.
        snap.current_envelope.clone()
    }

    async fn legal_pipelines(&self) -> Vec<Pipeline> {
        self.catalog()
            .into_iter()
            .filter(|p| matches!(p.visibility, Visibility::Surfaced))
            .collect()
    }

    async fn governance_pipelines(&self) -> Vec<Pipeline> {
        crate::governance::GovernanceComposer::canonical_governance_pipelines(&self.id)
    }

    async fn tick(&self, _: WorldEvent) -> Result<(), BrokerError> {
        // No automatic refresh on tick (sensors run on operator/agent
        // demand). Materializer re-emits the cached envelope each pass.
        Ok(())
    }

    async fn execute_leaf(
        &self,
        name: &str,
        _ctx: LeafContext,
    ) -> Result<Value, LeafError> {
        match name {
            "query_current_score" => {
                let snap = self.overlay.load();
                Ok(snap.current_envelope.clone())
            }
            "run_sensor" => {
                // Build a sensor instance + analyze.
                let sensor: Box<dyn Sensor> = self.factory.build();
                let project_root_str = self.project_root.to_string_lossy().to_string();
                let envelope = sensor
                    .analyze(&project_root_str)
                    .await
                    .map_err(|e| LeafError::ExecutionFailed(format!("sensor analyze: {}", e)))?;

                // Enforcer gate (if wired). Built-in sensors bypass.
                if let Some(enforcer) = &self.enforcer {
                    let outcome = enforcer.enforce(&self.id, &envelope);
                    if !outcome.allowed {
                        return Err(LeafError::ExecutionFailed(format!(
                            "sensory queue enforcer refused: {:?}",
                            outcome.refusal_reason
                        )));
                    }
                }

                // Update overlay (atomic swap).
                let prev = self.overlay.load();
                let new_snap = SensorOverlay {
                    current_envelope: envelope.clone(),
                    last_run_at: Some(chrono::Utc::now().to_rfc3339()),
                    run_count: prev.run_count + 1,
                };
                self.overlay.swap(new_snap);

                Ok(serde_json::json!({
                    "ok": true,
                    "score": envelope.get("score").cloned().unwrap_or(Value::Null),
                    "run_count": prev.run_count + 1,
                }))
            }
            "arm_kill_switch" | "disengage_kill_switch" => {
                // Framework governance pipelines; not for sensor brokers.
                Err(LeafError::NotFound(name.to_string()))
            }
            other => Err(LeafError::NotFound(other.to_string())),
        }
    }

    fn as_extensible(&self) -> Option<&dyn Extensible> {
        Some(self)
    }

    fn cmdb_path(&self) -> Option<PathBuf> {
        // Canonical path — matches the legacy sensor CMDB path contract.
        Some(PathBuf::from(format!(".claude/{}-cmdb.json", self.sensor_name)))
    }
}

pub const SENSOR_BROKER_EXTENSION_SCHEMA_VERSION: &str = "1";

#[async_trait]
impl Extensible for SensorBackedBroker {
    fn extension_schema_version(&self) -> &str {
        SENSOR_BROKER_EXTENSION_SCHEMA_VERSION
    }

    /// SensorBackedBroker is itself a sensor instance — it doesn't accept
    /// further per-broker extensions. Tier 1 sensor declarations create
    /// NEW SensorBackedBroker instances at boot (via the substrate's
    /// extension loader) rather than extending existing ones. Returns Ok
    /// silently if extensions target this broker (operator may have
    /// authored a misplaced config); host's apply_all_extensions logs
    /// the issue.
    async fn apply_extension(&self, _config: &ExtensionConfig) -> Result<(), ExtensionError> {
        // No-op. Sensor extensions register NEW brokers, not extend existing.
        Ok(())
    }
}

impl SensoryBroker for SensorBackedBroker {
    fn sensor_name(&self) -> &str {
        &self.sensor_name
    }
}

// ============================================================================
// A.2.4 — Bulk sensor wrapping helper
// ============================================================================

/// Wrap an iterator of [`SensorFactory`] implementations into
/// [`SensorBackedBroker`] instances rooted at `project_root`.
///
/// Returns a Vec of (broker_id, broker) tuples ready for registration
/// with the [`crate::host::BrokerHost`]. Built-in factories from
/// `neurogrim_sensory::built_in_factories()` plug straight in (the
/// idiomatic A.2.4 wiring; see `BROKER-AUTHORING.md`).
///
/// All wrapped brokers are pre-trusted (no enforcer). Tier 1/Tier 2
/// extension-loaded sensors should construct individually with
/// `.with_enforcer(...)` instead.
///
/// ### Usage
///
/// ```rust,no_run
/// # use std::path::PathBuf;
/// # use std::sync::Arc;
/// # use neurogrim_brokers::wrap_all_sensors_into_brokers;
/// # use neurogrim_core::sensor::SensorFactory;
/// # fn factories() -> Vec<Box<dyn SensorFactory>> { vec![] }
/// let project_root = PathBuf::from("/path/to/project");
/// let brokers = wrap_all_sensors_into_brokers(factories(), project_root);
/// // `brokers` is Vec<(String, Arc<SensorBackedBroker>)> ready to register
/// // with BrokerHost via BrokerFactoryRegistry.
/// ```
pub fn wrap_all_sensors_into_brokers(
    factories: impl IntoIterator<Item = Box<dyn SensorFactory>>,
    project_root: PathBuf,
) -> Vec<(String, Arc<SensorBackedBroker>)> {
    factories
        .into_iter()
        .map(|factory| {
            let name = factory.name().to_string();
            let factory_arc: Arc<dyn SensorFactory> = Arc::from(factory);
            let broker = Arc::new(SensorBackedBroker::new(factory_arc, project_root.clone()));
            (name, broker)
        })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::ParamMap;
    use anyhow::Result;
    use tempfile::TempDir;

    // Fixture sensor factory — produces a sensor that emits a fixed
    // cmdb-envelope-v1-compliant envelope. Used to verify the
    // SensorBackedBroker wrapping pattern works end-to-end without
    // requiring real sensor side effects (file I/O, git commands).
    struct FixtureSensor {
        score: u8,
    }

    #[async_trait]
    impl Sensor for FixtureSensor {
        async fn analyze(&self, _project_root: &str) -> Result<Value> {
            Ok(serde_json::json!({
                "meta": {
                    "schema_version": "1",
                    "updated_at": "2026-06-26T00:00:00Z",
                    "updated_by": "fixture-sensor"
                },
                "score": self.score,
                "updated_at": "2026-06-26T00:00:00Z",
                "findings": []
            }))
        }
    }

    struct FixtureFactory {
        name: &'static str,
        score: u8,
    }

    impl SensorFactory for FixtureFactory {
        fn name(&self) -> &'static str {
            self.name
        }
        fn build(&self) -> Box<dyn Sensor> {
            Box::new(FixtureSensor { score: self.score })
        }
    }

    fn make_broker(score: u8) -> (TempDir, SensorBackedBroker) {
        let tmp = TempDir::new().unwrap();
        let factory: Arc<dyn SensorFactory> = Arc::new(FixtureFactory {
            name: "fixture-sensor",
            score,
        });
        let broker = SensorBackedBroker::new(factory, tmp.path().to_path_buf());
        (tmp, broker)
    }

    async fn dispatch(
        broker: &SensorBackedBroker,
        leaf: &str,
        params: ParamMap,
    ) -> Result<Value, LeafError> {
        let ctx = LeafContext {
            broker_id: broker.id.clone(),
            pipeline_id: format!("fixture-sensor/{}", leaf),
            params,
            overlay_snapshot: Value::Null,
            frame: crate::frame::Frame::default(),
        };
        broker.execute_leaf(leaf, ctx).await
    }

    #[tokio::test]
    async fn sensor_backed_broker_role_set_is_sense() {
        let (_tmp, broker) = make_broker(75);
        assert!(broker.role_set().contains(&Role::Sense));
        assert!(!broker.role_set().contains(&Role::Embodiment));
    }

    #[tokio::test]
    async fn sensor_backed_broker_cmdb_path_is_canonical() {
        let (_tmp, broker) = make_broker(75);
        let path = broker.cmdb_path().unwrap();
        assert_eq!(path, PathBuf::from(".claude/fixture-sensor-cmdb.json"));
    }

    #[tokio::test]
    async fn sensor_backed_broker_default_id_equals_sensor_name() {
        let (_tmp, broker) = make_broker(75);
        assert_eq!(broker.id(), "fixture-sensor");
        assert_eq!(broker.sensor_name(), "fixture-sensor");
    }

    #[tokio::test]
    async fn sensor_backed_broker_with_id_overrides_default() {
        let tmp = TempDir::new().unwrap();
        let factory: Arc<dyn SensorFactory> = Arc::new(FixtureFactory {
            name: "fixture-sensor",
            score: 50,
        });
        let broker = SensorBackedBroker::new(factory, tmp.path().to_path_buf())
            .with_id("custom-id");
        assert_eq!(broker.id(), "custom-id");
        assert_eq!(broker.sensor_name(), "fixture-sensor"); // unchanged
    }

    #[tokio::test]
    async fn sensor_backed_broker_catalog_has_2_pipelines_plus_governance() {
        let (_tmp, broker) = make_broker(75);
        let catalog = broker.catalog();
        // 1 Internal (query-current-score), 1 Surfaced run-sensor + 2 Surfaced
        // governance (arm-kill-switch + disengage-kill-switch). Don't pin
        // exact counts (governance pipelines can grow); just verify the
        // 2 broker-specific pipelines are present.
        assert!(catalog.iter().any(|p| p.id.ends_with("/run-sensor")
            && matches!(p.visibility, Visibility::Surfaced)));
        assert!(catalog.iter().any(|p| p.id.ends_with("/query-current-score")
            && matches!(p.visibility, Visibility::Internal)));
    }

    #[tokio::test]
    async fn sensor_backed_broker_query_current_score_returns_null_before_first_run() {
        let (_tmp, broker) = make_broker(75);
        let result = dispatch(&broker, "query_current_score", ParamMap::new())
            .await
            .unwrap();
        assert!(result.is_null());
    }

    #[tokio::test]
    async fn sensor_backed_broker_run_sensor_caches_envelope() {
        let (_tmp, broker) = make_broker(75);
        let result = dispatch(&broker, "run_sensor", ParamMap::new())
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["score"], 75);
        assert_eq!(result["run_count"], 1);

        // Cached envelope is now accessible.
        let cached = dispatch(&broker, "query_current_score", ParamMap::new())
            .await
            .unwrap();
        assert_eq!(cached["score"], 75);
        assert_eq!(cached["meta"]["updated_by"], "fixture-sensor");
    }

    #[tokio::test]
    async fn sensor_backed_broker_run_count_increments_per_dispatch() {
        let (_tmp, broker) = make_broker(75);
        let r1 = dispatch(&broker, "run_sensor", ParamMap::new()).await.unwrap();
        let r2 = dispatch(&broker, "run_sensor", ParamMap::new()).await.unwrap();
        let r3 = dispatch(&broker, "run_sensor", ParamMap::new()).await.unwrap();
        assert_eq!(r1["run_count"], 1);
        assert_eq!(r2["run_count"], 2);
        assert_eq!(r3["run_count"], 3);
    }

    #[tokio::test]
    async fn sensor_backed_broker_read_overlay_returns_envelope_directly() {
        let (_tmp, broker) = make_broker(42);
        dispatch(&broker, "run_sensor", ParamMap::new()).await.unwrap();
        let overlay = broker.read_overlay().await;
        // Overlay IS the envelope (so CmdbMaterializer serializes it
        // verbatim to the canonical CMDB path).
        assert_eq!(overlay["score"], 42);
        assert_eq!(overlay["meta"]["schema_version"], "1");
    }

    #[tokio::test]
    async fn sensor_backed_broker_unknown_leaf_returns_not_found() {
        let (_tmp, broker) = make_broker(75);
        let err = dispatch(&broker, "bogus_leaf", ParamMap::new())
            .await
            .unwrap_err();
        matches!(err, LeafError::NotFound(_));
    }

    /// A.2.4 — bulk wrapping helper turns N SensorFactories into N
    /// SensorBackedBrokers, ready for registration with the host. This
    /// is the integration shape used by NeuroGrim's broker-serve binary
    /// to wire all 26 built-in sensors at boot.
    #[tokio::test]
    async fn wrap_all_sensors_helper_constructs_one_broker_per_factory() {
        let tmp = TempDir::new().unwrap();
        let factories: Vec<Box<dyn SensorFactory>> = vec![
            Box::new(FixtureFactory {
                name: "sensor-a",
                score: 10,
            }),
            Box::new(FixtureFactory {
                name: "sensor-b",
                score: 50,
            }),
            Box::new(FixtureFactory {
                name: "sensor-c",
                score: 90,
            }),
        ];
        let brokers = wrap_all_sensors_into_brokers(factories, tmp.path().to_path_buf());
        assert_eq!(brokers.len(), 3);
        // Each broker independent + correctly named.
        assert_eq!(brokers[0].0, "sensor-a");
        assert_eq!(brokers[1].0, "sensor-b");
        assert_eq!(brokers[2].0, "sensor-c");
        // Each broker dispatches independently — proves the wrap doesn't
        // share state across instances.
        for (i, (_id, broker)) in brokers.iter().enumerate() {
            let expected_score = [10, 50, 90][i];
            let ctx = LeafContext {
                broker_id: broker.id().to_string(),
                pipeline_id: format!("{}/run-sensor", broker.id()),
                params: ParamMap::new(),
                overlay_snapshot: serde_json::Value::Null,
                frame: crate::frame::Frame::default(),
            };
            let result = broker.execute_leaf("run_sensor", ctx).await.unwrap();
            assert_eq!(result["score"], expected_score);
            let overlay = broker.read_overlay().await;
            assert_eq!(overlay["score"], expected_score);
        }
    }
}
