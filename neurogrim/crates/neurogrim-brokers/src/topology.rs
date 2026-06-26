//! BB #17 — Topology Broker scaffold.
//!
//! The substrate-side primitive for **broker introspection** — the
//! standard cross-broker discovery surface every multi-broker consumer
//! needs. Today [`crate::registry::BrokerRegistry`] holds the broker
//! roster, but consumers wanting "what brokers exist, with what role-set,
//! exposing what pipelines" must reach into the registry from outside
//! the broker pattern, breaking the trait-uniform interface.
//!
//! This module ships:
//!
//! - [`TopologyBroker`] trait — the `Broker`-supertrait surface for
//!   broker introspection.
//! - [`BrokerInfo`] — the per-broker metadata snapshot consumers see
//!   (id + role-set + advertised pipeline IDs).
//! - [`TopologyOverlay`] — the overlay shape: a `Vec<BrokerInfo>` of the
//!   currently-known brokers.
//! - [`TopologyBrokerV1`] — V1 concrete impl backed by a snapshot of
//!   [`BrokerRegistry`].
//!
//! ## V1 scope (intentionally narrow)
//!
//! V1 is **read-only + permissive ACL**: every consumer sees every
//! broker. ACL-mutation pipelines (`update-acl`, `propose-acl-grant`)
//! are explicitly out of V1 per the substrate plan — they require the
//! Topology Broker's ACL-self-bypass invariant which is V2 work.
//!
//! V1 also takes a **snapshot at construction time** rather than holding
//! a live reference to the registry — registries aren't typically wrapped
//! in `Arc` and the broker should be insulated from registry mutations
//! anyway. Hosts that need to refresh the snapshot after broker
//! registration changes call [`TopologyBrokerV1::refresh_from_registry`]
//! (or, in V2, dispatch a `tick(WorldEvent { topic: "_neurogrim/topology" })`).
//!
//! ## Per-consumer projection via [`OverlayView`]
//!
//! When V2 ACLs land, the per-consumer projection layer uses
//! [`crate::overlay::OverlayView<TopologyOverlay, FilteredOverlay>`] —
//! the View's filter closure is the consumer's ACL function. The
//! `OverlayView` primitive shipped alongside this scaffold is exactly
//! the substrate piece needed for that V2 step. V1 doesn't construct
//! Views today (every consumer sees the full overlay); V2 wires the ACL
//! filter into a per-consumer View.

use crate::broker::{Broker, BrokerError, Role, RoleSet, WorldEvent};
use crate::governance::GovernanceComposer;
use crate::overlay::Overlay;
use crate::pipeline::{
    AuditClass, EffectClass, Pipeline, Step, Tunability, Visibility,
};
use crate::registry::BrokerRegistry;
use crate::runner::{LeafContext, LeafError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

/// Per-broker metadata snapshot exposed via Topology.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrokerInfo {
    pub id: String,
    pub role_set: RoleSet,
    /// Pipeline IDs the broker has registered in its catalog at snapshot
    /// time. Empty `Vec` is a valid value (broker registered without a
    /// catalog via the legacy [`BrokerRegistry::register`] path).
    pub pipeline_ids: Vec<String>,
}

/// Topology Broker's overlay shape — the snapshot of known brokers.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TopologyOverlay {
    pub brokers: Vec<BrokerInfo>,
}

/// Substrate trait for broker introspection. Concrete impls (V1 ships
/// the [`TopologyBrokerV1`] reference; V2 may layer ACL on top) compose
/// this with a [`Broker`] impl.
pub trait TopologyBroker: Broker {
    /// Brokers the `consumer` is permitted to discover. V1 returns the
    /// full set regardless of consumer (permissive ACL); V2 filters via
    /// [`crate::overlay::OverlayView`].
    fn list_reachable_brokers(&self, consumer: &str) -> Vec<String>;

    /// Metadata for a specific broker, if `consumer` is permitted to
    /// reach `target`. V1 returns `Some(_)` for every registered target
    /// regardless of consumer.
    fn query_broker(&self, consumer: &str, target: &str) -> Option<BrokerInfo>;
}

#[derive(Debug, Error)]
pub enum TopologyError {
    #[error("topology broker registry snapshot is empty (was the broker constructed before any brokers were registered?)")]
    EmptySnapshot,
}

pub struct TopologyBrokerV1 {
    id: String,
    overlay: Arc<Overlay<TopologyOverlay>>,
    governance: Arc<GovernanceComposer>,
}

impl TopologyBrokerV1 {
    /// Construct a Topology Broker from a snapshot of the given
    /// [`BrokerRegistry`]. The snapshot is taken once at construction
    /// time; call [`refresh_from_registry`] to re-snapshot after the
    /// registry changes.
    pub fn from_registry_snapshot(
        id: impl Into<String>,
        registry: &BrokerRegistry,
        governance: Arc<GovernanceComposer>,
    ) -> Self {
        let overlay = TopologyOverlay {
            brokers: snapshot_brokers(registry),
        };
        Self {
            id: id.into(),
            overlay: Arc::new(Overlay::new(overlay)),
            governance,
        }
    }

    /// Re-snapshot from the current state of `registry`. Atomically swaps
    /// the overlay; existing reader snapshots survive (no torn reads).
    pub fn refresh_from_registry(&self, registry: &BrokerRegistry) {
        let next = TopologyOverlay {
            brokers: snapshot_brokers(registry),
        };
        self.overlay.swap(next);
    }
}

fn snapshot_brokers(registry: &BrokerRegistry) -> Vec<BrokerInfo> {
    let mut out: Vec<BrokerInfo> = registry
        .iter_brokers()
        .map(|(id, broker)| {
            let pipeline_ids = registry
                .catalog_for(id)
                .map(|cat| cat.iter().map(|p| p.id.clone()).collect())
                .unwrap_or_default();
            BrokerInfo {
                id: id.to_string(),
                role_set: broker.role_set(),
                pipeline_ids,
            }
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[async_trait]
impl Broker for TopologyBrokerV1 {
    fn id(&self) -> &str {
        &self.id
    }

    fn role_set(&self) -> RoleSet {
        RoleSet::single(Role::Sense)
    }

    async fn read_overlay(&self) -> serde_json::Value {
        let snap = self.overlay.load();
        serde_json::to_value(&*snap).unwrap_or(serde_json::Value::Null)
    }

    async fn legal_pipelines(&self) -> Vec<Pipeline> {
        vec![
            Pipeline {
                id: format!("{}/list-reachable-brokers", self.id),
                visibility: Visibility::Internal,
                tunability: Tunability::OperatorOnly,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::ReadOnly,
                params: serde_json::json!({"consumer": "string"}),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: "list_reachable_brokers".to_string(),
                }],
                description: "List broker IDs the caller is permitted to discover.".into(),
                when_to_use: "Cross-broker discovery; consumer enumerates targets before dispatch.".into(),
                bypasses_kill_switch: false,
            },
            Pipeline {
                id: format!("{}/query-broker", self.id),
                visibility: Visibility::Internal,
                tunability: Tunability::OperatorOnly,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::ReadOnly,
                params: serde_json::json!({"consumer": "string", "target": "string"}),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: "query_broker".to_string(),
                }],
                description: "Return BrokerInfo for the named target, if reachable from caller.".into(),
                when_to_use: "Inspect a specific broker's role-set + pipeline catalog.".into(),
                bypasses_kill_switch: false,
            },
        ]
    }

    async fn governance_pipelines(&self) -> Vec<Pipeline> {
        let _ = &self.governance; // V1 holds the composer for future kill-switch wiring
        GovernanceComposer::canonical_governance_pipelines(&self.id)
    }

    async fn tick(&self, _event: WorldEvent) -> Result<(), BrokerError> {
        // V1: tick is a no-op. Hosts call refresh_from_registry() explicitly
        // when the registry changes. V2 may bind tick to a topology-changed
        // WorldEvent that triggers re-snapshot.
        Ok(())
    }

    async fn execute_leaf(
        &self,
        name: &str,
        ctx: LeafContext,
    ) -> Result<serde_json::Value, LeafError> {
        match name {
            "list_reachable_brokers" => {
                let consumer = ctx
                    .params
                    .get("consumer")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let list = self.list_reachable_brokers(consumer);
                Ok(serde_json::json!({"brokers": list}))
            }
            "query_broker" => {
                let consumer = ctx
                    .params
                    .get("consumer")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let target = ctx
                    .params
                    .get("target")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| LeafError::InputInvalid("target required".into()))?;
                match self.query_broker(consumer, target) {
                    Some(info) => Ok(serde_json::to_value(info).unwrap_or(serde_json::Value::Null)),
                    None => Ok(serde_json::Value::Null),
                }
            }
            other => Err(LeafError::NotFound(other.to_string())),
        }
    }
}

impl TopologyBroker for TopologyBrokerV1 {
    fn list_reachable_brokers(&self, _consumer: &str) -> Vec<String> {
        // V1: permissive ACL — return every known broker id. V2 filters
        // by consumer via OverlayView.
        let snap = self.overlay.load();
        snap.brokers.iter().map(|b| b.id.clone()).collect()
    }

    fn query_broker(&self, _consumer: &str, target: &str) -> Option<BrokerInfo> {
        let snap = self.overlay.load();
        snap.brokers.iter().find(|b| b.id == target).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::WorldEvent;
    use crate::governance::GovernanceComposer;
    use crate::pipeline::{AuditClass, EffectClass, Tunability, Visibility};
    use std::sync::Arc;
    use tempfile::TempDir;

    fn governance() -> Arc<GovernanceComposer> {
        Arc::new(GovernanceComposer::new(100))
    }

    struct MiniBroker {
        id: String,
        roles: RoleSet,
    }

    #[async_trait]
    impl Broker for MiniBroker {
        fn id(&self) -> &str {
            &self.id
        }
        fn role_set(&self) -> RoleSet {
            self.roles.clone()
        }
        async fn read_overlay(&self) -> serde_json::Value {
            serde_json::Value::Null
        }
        async fn legal_pipelines(&self) -> Vec<Pipeline> {
            vec![]
        }
        async fn governance_pipelines(&self) -> Vec<Pipeline> {
            vec![]
        }
        async fn tick(&self, _: WorldEvent) -> Result<(), BrokerError> {
            Ok(())
        }
        async fn execute_leaf(
            &self,
            _: &str,
            _: LeafContext,
        ) -> Result<serde_json::Value, LeafError> {
            Ok(serde_json::Value::Null)
        }
    }

    fn write_registry_fixture() -> (TempDir, BrokerRegistry) {
        let tmp = TempDir::new().unwrap();
        let cluster_path = tmp.path().join("cluster.toml");
        std::fs::write(
            &cluster_path,
            r#"
[cluster]
id = "test"
name = "T"
brokers_dir = "./"

[cluster.brokers.alpha]
manifest_path = "alpha.toml"

[cluster.brokers.beta]
manifest_path = "beta.toml"
"#,
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("alpha.toml"),
            r#"[broker]
id = "alpha"
name = "Alpha"
roles = ["sense"]
cold_store_path = "a/"
catalog_path = "a.yaml"
"#,
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("beta.toml"),
            r#"[broker]
id = "beta"
name = "Beta"
roles = ["innate-ability"]
cold_store_path = "b/"
catalog_path = "b.yaml"
"#,
        )
        .unwrap();
        let mut reg = BrokerRegistry::load_manifests(&cluster_path).unwrap();
        reg.register(Arc::new(MiniBroker {
            id: "alpha".into(),
            roles: RoleSet::single(Role::Sense),
        }))
        .unwrap();
        let beta_pipeline = Pipeline {
            id: "beta/p1".into(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorOnly,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::ReadOnly,
            params: serde_json::json!({}),
            preconditions: vec![],
            steps: vec![],
            description: String::new(),
            when_to_use: String::new(),
            bypasses_kill_switch: false,
        };
        reg.register_with_catalog(
            Arc::new(MiniBroker {
                id: "beta".into(),
                roles: RoleSet::single(Role::InnateAbility),
            }),
            vec![beta_pipeline],
        )
        .unwrap();
        (tmp, reg)
    }

    #[tokio::test]
    async fn snapshot_captures_all_registered_brokers() {
        let (_tmp, reg) = write_registry_fixture();
        let topo = TopologyBrokerV1::from_registry_snapshot("topology", &reg, governance());
        let ids = topo.list_reachable_brokers("any-consumer");
        assert_eq!(ids, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[tokio::test]
    async fn query_broker_returns_role_set_and_pipeline_ids() {
        let (_tmp, reg) = write_registry_fixture();
        let topo = TopologyBrokerV1::from_registry_snapshot("topology", &reg, governance());
        let info = topo.query_broker("c", "beta").expect("beta must be queryable");
        assert_eq!(info.id, "beta");
        assert!(info.role_set.contains(&Role::InnateAbility));
        assert_eq!(info.pipeline_ids, vec!["beta/p1".to_string()]);
    }

    #[tokio::test]
    async fn query_broker_returns_none_for_unknown_target() {
        let (_tmp, reg) = write_registry_fixture();
        let topo = TopologyBrokerV1::from_registry_snapshot("topology", &reg, governance());
        assert!(topo.query_broker("c", "nonexistent").is_none());
    }

    #[tokio::test]
    async fn topology_broker_role_set_is_sense_only() {
        let (_tmp, reg) = write_registry_fixture();
        let topo = TopologyBrokerV1::from_registry_snapshot("topology", &reg, governance());
        let rs = topo.role_set();
        assert!(rs.contains(&Role::Sense));
        assert!(!rs.contains(&Role::InnateAbility));
        assert!(!rs.contains(&Role::Embodiment));
    }

    #[tokio::test]
    async fn legal_pipelines_advertises_v1_surface() {
        let (_tmp, reg) = write_registry_fixture();
        let topo = TopologyBrokerV1::from_registry_snapshot("topology", &reg, governance());
        let pipelines = topo.legal_pipelines().await;
        let ids: Vec<&str> = pipelines.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"topology/list-reachable-brokers"));
        assert!(ids.contains(&"topology/query-broker"));
        // V1 does NOT advertise mutation pipelines (V2 territory).
        assert!(!ids.iter().any(|id| id.contains("update-acl") || id.contains("propose-acl")));
    }

    fn params_with(pairs: &[(&str, &str)]) -> crate::pipeline::ParamMap {
        let mut m = crate::pipeline::ParamMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }
        m
    }

    #[tokio::test]
    async fn execute_leaf_list_reachable_brokers_returns_brokers_field() {
        let (_tmp, reg) = write_registry_fixture();
        let topo = TopologyBrokerV1::from_registry_snapshot("topology", &reg, governance());
        let ctx = LeafContext {
            broker_id: "topology".into(),
            pipeline_id: "topology/list-reachable-brokers".into(),
            params: params_with(&[("consumer", "alpha")]),
            overlay_snapshot: serde_json::Value::Null,
            frame: crate::Frame::default(),
        };
        let out = topo
            .execute_leaf("list_reachable_brokers", ctx)
            .await
            .unwrap();
        assert!(out["brokers"].is_array());
        let arr = out["brokers"].as_array().unwrap();
        assert!(arr.iter().any(|v| v == "alpha"));
        assert!(arr.iter().any(|v| v == "beta"));
    }

    #[tokio::test]
    async fn execute_leaf_query_broker_returns_info_for_known_target() {
        let (_tmp, reg) = write_registry_fixture();
        let topo = TopologyBrokerV1::from_registry_snapshot("topology", &reg, governance());
        let ctx = LeafContext {
            broker_id: "topology".into(),
            pipeline_id: "topology/query-broker".into(),
            params: params_with(&[("consumer", "any"), ("target", "alpha")]),
            overlay_snapshot: serde_json::Value::Null,
            frame: crate::Frame::default(),
        };
        let out = topo.execute_leaf("query_broker", ctx).await.unwrap();
        assert_eq!(out["id"], "alpha");
    }

    #[tokio::test]
    async fn execute_leaf_query_broker_returns_null_for_unknown_target() {
        let (_tmp, reg) = write_registry_fixture();
        let topo = TopologyBrokerV1::from_registry_snapshot("topology", &reg, governance());
        let ctx = LeafContext {
            broker_id: "topology".into(),
            pipeline_id: "topology/query-broker".into(),
            params: params_with(&[("consumer", "any"), ("target", "gamma")]),
            overlay_snapshot: serde_json::Value::Null,
            frame: crate::Frame::default(),
        };
        let out = topo.execute_leaf("query_broker", ctx).await.unwrap();
        assert!(out.is_null());
    }

    #[tokio::test]
    async fn refresh_from_registry_updates_snapshot() {
        let (tmp, mut reg) = write_registry_fixture();
        let topo = TopologyBrokerV1::from_registry_snapshot("topology", &reg, governance());
        assert_eq!(topo.list_reachable_brokers("c").len(), 2);

        // Add a 3rd broker; refresh.
        let gamma_path = tmp.path().join("gamma.toml");
        std::fs::write(
            &gamma_path,
            r#"[broker]
id = "gamma"
name = "Gamma"
roles = ["sense"]
cold_store_path = "g/"
catalog_path = "g.yaml"
"#,
        )
        .unwrap();
        let cluster_path = tmp.path().join("cluster.toml");
        let mut cluster_contents = std::fs::read_to_string(&cluster_path).unwrap();
        cluster_contents.push_str(
            "\n[cluster.brokers.gamma]\nmanifest_path = \"gamma.toml\"\n",
        );
        std::fs::write(&cluster_path, cluster_contents).unwrap();
        let new_reg = BrokerRegistry::load_manifests(&cluster_path).unwrap();
        // Reuse old brokers + add gamma to the new registry
        let mut new_reg = new_reg;
        new_reg
            .register(Arc::new(MiniBroker {
                id: "alpha".into(),
                roles: RoleSet::single(Role::Sense),
            }))
            .unwrap();
        new_reg
            .register(Arc::new(MiniBroker {
                id: "beta".into(),
                roles: RoleSet::single(Role::InnateAbility),
            }))
            .unwrap();
        new_reg
            .register(Arc::new(MiniBroker {
                id: "gamma".into(),
                roles: RoleSet::single(Role::Sense),
            }))
            .unwrap();
        topo.refresh_from_registry(&new_reg);
        assert_eq!(topo.list_reachable_brokers("c").len(), 3);
        let _ = reg; // silence
    }
}
