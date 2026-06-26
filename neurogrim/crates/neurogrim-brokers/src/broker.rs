//! BB #1 — Broker capsule.
//!
//! The `Broker` trait every broker implements. Resolves
//! BROKER-SPEC-GAPS.md gap #1: trait signature finalized via async_trait +
//! interior-mutability pattern (so `Arc<dyn Broker>` is dyn-compatible
//! despite mutation-during-tick).
//!
//! ## Type-erasure choice (Wave 1 decision)
//!
//! The trait surface uses `serde_json::Value` for overlay state (type-erased)
//! rather than associated types. This sacrifices Rust type-safety at the
//! consumer boundary for **dyn-compatibility** — the Broker Registry (BB #14)
//! holds `Vec<Arc<dyn Broker>>` across heterogeneous broker implementations.
//! Concrete brokers retain typed Overlay<T> + WorkingState<W> internally;
//! they JSON-serialize at the trait boundary.
//!
//! Trade-off: agents and other consumers see Value not typed structs. The
//! Awareness Materializer (BB #24) projects to human-readable Markdown
//! anyway, so the loss is minor at the boundary that matters.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::pipeline::Pipeline;

/// World events that drive broker tick re-projection. Per BB #15 Tick Source
/// spec; MVP triggered by PostToolUse hook + manual operator command.
/// Resolves BROKER-SPEC-GAPS.md gap #4 (WorldEvent shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEvent {
    /// Bus topic the event arrived on (per BB #4 queue conventions).
    pub topic: String,

    /// Source class — `system-diagnostics` | `sensor` | `agent-action` |
    /// `operator-command` | `tick-cadence`. Wave 2 may expand the enum.
    pub source_class: String,

    /// Typed payload (broker decides shape per topic).
    pub payload: serde_json::Value,

    /// ISO 8601 UTC timestamp.
    pub ts: String,
}

impl WorldEvent {
    /// Convenience: create a tick-cadence event (the default trigger).
    pub fn tick_cadence(ts: impl Into<String>) -> Self {
        Self {
            topic: "_neurogrim/tick".into(),
            source_class: "tick-cadence".into(),
            payload: serde_json::Value::Null,
            ts: ts.into(),
        }
    }
}

/// Role-set per BROKER-CONTRACT.md §"Broker roles". Brokers carry a SUBSET of
/// `{Sense, InnateAbility, Embodiment}`; multi-role brokers are first-class.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleSet {
    pub roles: Vec<Role>,
}

impl RoleSet {
    pub fn single(role: Role) -> Self {
        Self { roles: vec![role] }
    }

    pub fn contains(&self, role: &Role) -> bool {
        self.roles.contains(role)
    }
}

/// One of the three architectural role classes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Sense,
    InnateAbility,
    Embodiment,
}

/// Errors brokers can produce.
#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("broker not initialized: {0}")]
    NotInitialized(String),

    #[error("cold-store unavailable: {0}")]
    ColdStoreUnavailable(String),

    #[error("tick handling failed: {0}")]
    TickFailed(String),

    #[error("other: {0}")]
    Other(#[from] anyhow::Error),
}

/// The Broker capsule contract. All methods use `&self` + interior mutability
/// (typically via `Arc<Mutex<WorkingState<W>>>`) so the trait is
/// dyn-compatible AND brokers can mutate state during tick.
#[async_trait]
pub trait Broker: Send + Sync {
    /// Unique broker identifier (matches manifest `id` field; used as the
    /// prefix in pipeline IDs like `<broker_id>/<pipeline_name>`).
    fn id(&self) -> &str;

    /// Role-set declaration. Wave 3 Broker Registry composes role-specific
    /// scaffolding from this.
    fn role_set(&self) -> RoleSet;

    /// Returns the consumer-facing read-only Overlay state, JSON-serialized.
    /// Per BROKER-CONTRACT.md §"The Overlay contract": atomic-swap snapshot.
    /// Concrete brokers internally use `Overlay<T>` + arc-swap; the trait
    /// surface returns a Value.
    async fn read_overlay(&self) -> serde_json::Value;

    /// Returns the currently-legal Surfaced pipelines. Per BROKER-CONTRACT.md
    /// central invariant: the LLM never sees a capability whose preconditions
    /// aren't met. MVP version returns all Surfaced pipelines from the
    /// catalog (Wave 2 catalog loader gates by precondition evaluation
    /// against hot store).
    async fn legal_pipelines(&self) -> Vec<Pipeline>;

    /// Returns governance pipelines via the sidecar channel split (per
    /// BROKER-INTERNALS.md §4 reachability invariant). NOT subject to Skill
    /// Filter ranking; always-reachable per LB-3 closure.
    async fn governance_pipelines(&self) -> Vec<Pipeline>;

    /// Tick handler: re-project Overlays in response to world events. Called
    /// by the Tick Source (MVP: PostToolUse hook + manual operator command).
    async fn tick(&self, event: WorldEvent) -> Result<(), BrokerError>;

    /// Execute a named leaf-op (Tier 3 plain function) registered by this
    /// broker. Called by the Pipeline Runner during step execution. Brokers
    /// typically implement this with a `match name { ... }` over their
    /// registered leaf-op names.
    ///
    /// Per BROKER-INTERNALS.md §1.4 Tier 3 minimization: leaf-ops are the
    /// small set of plain functions a broker exposes; everything else
    /// (preconditions, governance, materialization, etc.) is framework-handled.
    async fn execute_leaf(
        &self,
        name: &str,
        ctx: crate::runner::LeafContext,
    ) -> Result<serde_json::Value, crate::runner::LeafError>;

    /// Optional opt-in to the broker extension system. Brokers that accept
    /// declarative operator extensions (Tier 1 TOML configs at
    /// `.claude/brain/broker/extensions/<broker_id>/*.toml`) override this
    /// to return `Some(self)` — Rust's type erasure means we need the
    /// explicit hook so the host can dispatch into the broker's
    /// `Extensible::apply_extension` at boot time.
    ///
    /// Default implementation returns `None` — broker silently ignores any
    /// operator-authored extension configs targeting it. The host logs a
    /// `tracing::warn!` in that case so operators see when configs miss.
    ///
    /// See [`crate::extension::Extensible`] for the trait brokers implement
    /// to participate.
    fn as_extensible(&self) -> Option<&dyn crate::extension::Extensible> {
        None
    }

    /// A.0.3 — Broker-declared CMDB output path, relative to project root.
    ///
    /// Brokers that export a CMDB (canonically, [Sense] brokers wrapping
    /// sensors per A.2) override this to return the path their CMDB should
    /// be written to. The substrate's [`crate::materializer::cmdb_writer::CmdbMaterializer`]
    /// reads this on each materialization pass and writes the broker's
    /// [`read_overlay`] output as JSON to that path (atomic temp+rename).
    ///
    /// Default: `None` — broker does NOT export a CMDB (workspace,
    /// embodiment, agent-action brokers). The CmdbMaterializer skips
    /// these brokers entirely.
    ///
    /// **Path resolution precedence** (per docs/BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md
    /// Gate 2): registry override → this method's return → None. The host
    /// consults registry override BEFORE calling this method, so brokers
    /// don't need to know about overrides.
    fn cmdb_path(&self) -> Option<std::path::PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::Visibility;
    use std::sync::Arc;

    /// Minimal mock broker for testing the Broker trait contract.
    pub struct MockBroker {
        id: String,
        roles: RoleSet,
        tick_count: tokio::sync::Mutex<u32>,
    }

    impl MockBroker {
        pub fn new(id: impl Into<String>, roles: RoleSet) -> Self {
            Self {
                id: id.into(),
                roles,
                tick_count: tokio::sync::Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl Broker for MockBroker {
        fn id(&self) -> &str {
            &self.id
        }

        fn role_set(&self) -> RoleSet {
            self.roles.clone()
        }

        async fn read_overlay(&self) -> serde_json::Value {
            serde_json::json!({"mock": true, "broker_id": self.id})
        }

        async fn legal_pipelines(&self) -> Vec<Pipeline> {
            vec![]
        }

        async fn governance_pipelines(&self) -> Vec<Pipeline> {
            vec![]
        }

        async fn tick(&self, _event: WorldEvent) -> Result<(), BrokerError> {
            let mut count = self.tick_count.lock().await;
            *count += 1;
            Ok(())
        }

        async fn execute_leaf(
            &self,
            name: &str,
            _ctx: crate::runner::LeafContext,
        ) -> Result<serde_json::Value, crate::runner::LeafError> {
            match name {
                "echo" => Ok(serde_json::json!({"mock_echo": true})),
                _ => Err(crate::runner::LeafError::NotFound(name.to_string())),
            }
        }
    }

    #[tokio::test]
    async fn broker_trait_is_dyn_compatible_via_arc() {
        let brokers: Vec<Arc<dyn Broker>> = vec![
            Arc::new(MockBroker::new("a", RoleSet::single(Role::Sense))),
            Arc::new(MockBroker::new(
                "b",
                RoleSet {
                    roles: vec![Role::Sense, Role::Embodiment],
                },
            )),
            Arc::new(MockBroker::new(
                "c",
                RoleSet::single(Role::InnateAbility),
            )),
        ];
        assert_eq!(brokers.len(), 3);

        for broker in &brokers {
            let overlay = broker.read_overlay().await;
            assert!(overlay.get("mock").is_some());
            assert_eq!(broker.role_set().roles.len() >= 1, true);
        }
    }

    #[tokio::test]
    async fn broker_tick_uses_interior_mutability() {
        let b = MockBroker::new("tick-test", RoleSet::single(Role::Sense));
        // tick() takes &self, not &mut self — but updates internal state via Mutex.
        b.tick(WorldEvent::tick_cadence("2026-06-24T00:00:00Z"))
            .await
            .unwrap();
        b.tick(WorldEvent::tick_cadence("2026-06-24T00:00:01Z"))
            .await
            .unwrap();
        assert_eq!(*b.tick_count.lock().await, 2);
    }

    #[tokio::test]
    async fn broker_returns_legal_and_governance_pipelines_separately() {
        let b = MockBroker::new("split-test", RoleSet::single(Role::Sense));
        let legal = b.legal_pipelines().await;
        let governance = b.governance_pipelines().await;
        // Reachability channel split: both surfaces accessible independently.
        assert_eq!(legal.len(), 0);
        assert_eq!(governance.len(), 0);
        // The fact that these are SEPARATE methods (not a flag on one method)
        // is the structural guarantee per §4 reachability invariant.
        let _ = Visibility::Surfaced; // sanity: pipeline types reachable
    }

    #[test]
    fn role_set_contains() {
        let rs = RoleSet {
            roles: vec![Role::Sense, Role::Embodiment],
        };
        assert!(rs.contains(&Role::Sense));
        assert!(rs.contains(&Role::Embodiment));
        assert!(!rs.contains(&Role::InnateAbility));
    }
}
