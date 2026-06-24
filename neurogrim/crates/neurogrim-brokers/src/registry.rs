//! BB #14 — Broker Registry.
//!
//! Loads cluster + per-broker manifests; holds the registered broker
//! instances; provides lookup + iteration for the harness runtime.
//!
//! ## Wave 3 design
//!
//! The Registry does NOT dynamically load broker code (that would require
//! plugin infrastructure that's not in MVP scope). Instead:
//!
//! 1. **Operator-side configuration** lives in TOML manifests (per
//!    CLUSTER-MANIFEST-SCHEMA.md + BROKER-MANIFEST-SCHEMA.md).
//! 2. **Operator's main binary** (`neurogrim broker-serve`, Wave 5)
//!    constructs concrete broker instances + registers them via
//!    `BrokerRegistry::register()`.
//! 3. **Registry** validates manifest-vs-registered-broker consistency
//!    (declared broker IDs match registered instances; role-sets agree).
//!
//! Post-MVP S1-T: a plugin mechanism (dlopen-style or per-binary builds)
//! could enable runtime broker loading from disk. Not in scope for MVP.

use crate::broker::{Broker, RoleSet};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("cluster manifest not found: {0}")]
    ManifestNotFound(PathBuf),

    #[error("cluster manifest parse failed: {0}")]
    ManifestParseFailed(String),

    #[error("broker manifest invalid for `{broker_id}`: {reason}")]
    BrokerManifestInvalid { broker_id: String, reason: String },

    #[error("broker id `{0}` declared in cluster manifest but no concrete broker registered")]
    BrokerNotRegistered(String),

    #[error("broker id `{0}` registered but not declared in cluster manifest")]
    BrokerNotDeclared(String),

    #[error("duplicate broker id registered: {0}")]
    DuplicateBrokerId(String),

    #[error("role-set mismatch for `{broker_id}`: manifest declares {manifest:?}, registered broker is {registered:?}")]
    RoleSetMismatch {
        broker_id: String,
        manifest: RoleSet,
        registered: RoleSet,
    },

    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Top-level cluster manifest (TOML).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterManifest {
    pub cluster: ClusterConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    pub id: String,
    pub name: String,
    pub brokers_dir: String,
    pub brokers: HashMap<String, ClusterBrokerEntry>,
    #[serde(default)]
    pub materializer: MaterializerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterBrokerEntry {
    pub manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializerConfig {
    /// Operator-declared segment composition order. Governance is ALWAYS
    /// placed first regardless (R-O-3 closure; Untunable per Phase 9.1).
    /// This list controls relative ordering of NON-governance segments.
    #[serde(default)]
    pub composition_order: Vec<String>,
    #[serde(default = "default_output_path")]
    pub output_path: String,
    #[serde(default = "default_segments_dir")]
    pub segments_dir: String,
    /// Context-window budget for the composed output. If exceeded,
    /// composer falls back to governance-only (per R-O-3 closure
    /// truncation alarm path).
    #[serde(default = "default_context_budget")]
    pub context_budget_chars: usize,
}

fn default_output_path() -> String {
    ".claude/brain/broker/current-projection.md".to_string()
}

fn default_segments_dir() -> String {
    ".claude/brain/broker/segments".to_string()
}

fn default_context_budget() -> usize {
    16_384
}

impl Default for MaterializerConfig {
    fn default() -> Self {
        Self {
            composition_order: vec![],
            output_path: default_output_path(),
            segments_dir: default_segments_dir(),
            context_budget_chars: default_context_budget(),
        }
    }
}

/// Per-broker manifest (TOML).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerManifest {
    pub broker: BrokerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    pub id: String,
    pub name: String,
    pub roles: Vec<String>,
    pub cold_store_path: String,
    pub catalog_path: String,
}

impl BrokerConfig {
    pub fn role_set(&self) -> Result<RoleSet, RegistryError> {
        use crate::broker::Role;
        let mut roles = Vec::new();
        for r in &self.roles {
            roles.push(match r.as_str() {
                "sense" => Role::Sense,
                "innate-ability" => Role::InnateAbility,
                "embodiment" => Role::Embodiment,
                other => {
                    return Err(RegistryError::BrokerManifestInvalid {
                        broker_id: self.id.clone(),
                        reason: format!("unknown role: {}", other),
                    })
                }
            });
        }
        Ok(RoleSet { roles })
    }
}

/// Broker Registry.
pub struct BrokerRegistry {
    cluster: ClusterManifest,
    cluster_manifest_dir: PathBuf,
    per_broker_manifests: HashMap<String, BrokerManifest>,
    brokers: HashMap<String, Arc<dyn Broker>>,
    broker_catalogs: HashMap<String, Vec<crate::Pipeline>>,
}

impl BrokerRegistry {
    /// Load a cluster manifest + all referenced per-broker manifests from disk.
    /// Returns an unpopulated Registry (no concrete brokers registered yet);
    /// operator's main binary must call `register()` for each broker before
    /// calling `validate()`.
    pub fn load_manifests(cluster_manifest_path: &Path) -> Result<Self, RegistryError> {
        let contents = std::fs::read_to_string(cluster_manifest_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RegistryError::ManifestNotFound(cluster_manifest_path.to_path_buf())
            } else {
                RegistryError::IoError(e)
            }
        })?;
        let cluster: ClusterManifest =
            toml::from_str(&contents).map_err(|e| RegistryError::ManifestParseFailed(e.to_string()))?;

        let cluster_manifest_dir = cluster_manifest_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        // Load each per-broker manifest
        let mut per_broker = HashMap::new();
        for (id, entry) in &cluster.cluster.brokers {
            let broker_manifest_path = cluster_manifest_dir.join(&entry.manifest_path);
            let broker_contents = std::fs::read_to_string(&broker_manifest_path)?;
            let bm: BrokerManifest = toml::from_str(&broker_contents)
                .map_err(|e| RegistryError::BrokerManifestInvalid {
                    broker_id: id.clone(),
                    reason: e.to_string(),
                })?;
            if bm.broker.id != *id {
                return Err(RegistryError::BrokerManifestInvalid {
                    broker_id: id.clone(),
                    reason: format!(
                        "broker manifest id `{}` does not match cluster manifest entry id `{}`",
                        bm.broker.id, id
                    ),
                });
            }
            // Validate roles parse successfully
            bm.broker.role_set()?;
            per_broker.insert(id.clone(), bm);
        }

        Ok(Self {
            cluster,
            cluster_manifest_dir,
            per_broker_manifests: per_broker,
            brokers: HashMap::new(),
            broker_catalogs: HashMap::new(),
        })
    }

    /// Register a concrete broker instance. ID must match a broker declared
    /// in the cluster manifest.
    pub fn register(&mut self, broker: Arc<dyn Broker>) -> Result<(), RegistryError> {
        let id = broker.id().to_string();
        if !self.per_broker_manifests.contains_key(&id) {
            return Err(RegistryError::BrokerNotDeclared(id));
        }
        if self.brokers.contains_key(&id) {
            return Err(RegistryError::DuplicateBrokerId(id));
        }
        // Validate registered role-set matches manifest
        let manifest_rs = self.per_broker_manifests[&id].broker.role_set()?;
        let registered_rs = broker.role_set();
        if manifest_rs != registered_rs {
            return Err(RegistryError::RoleSetMismatch {
                broker_id: id,
                manifest: manifest_rs,
                registered: registered_rs,
            });
        }
        self.brokers.insert(id, broker);
        Ok(())
    }

    /// Validate that every declared broker has been registered.
    /// Call after all `register()` calls before starting the harness loop.
    pub fn validate(&self) -> Result<(), RegistryError> {
        for declared_id in self.per_broker_manifests.keys() {
            if !self.brokers.contains_key(declared_id) {
                return Err(RegistryError::BrokerNotRegistered(declared_id.clone()));
            }
        }
        Ok(())
    }

    pub fn cluster(&self) -> &ClusterConfig {
        &self.cluster.cluster
    }

    pub fn cluster_manifest_dir(&self) -> &Path {
        &self.cluster_manifest_dir
    }

    pub fn broker(&self, id: &str) -> Option<Arc<dyn Broker>> {
        self.brokers.get(id).cloned()
    }

    pub fn broker_manifest(&self, id: &str) -> Option<&BrokerManifest> {
        self.per_broker_manifests.get(id)
    }

    pub fn iter_brokers(&self) -> impl Iterator<Item = (&str, Arc<dyn Broker>)> {
        self.brokers.iter().map(|(k, v)| (k.as_str(), v.clone()))
    }

    /// Aggregate every registered broker's catalog into a single Vec.
    /// V0-RETROSPECTIVE.md §C5 closure (gap #13). The PipelineRunner accepts
    /// a catalog parameter per-dispatch; this method produces the
    /// cluster-wide catalog the runner consumes.
    ///
    /// Brokers expose their catalog via a side-channel `BrokerWithCatalog`
    /// trait OR via downcasting; since the type-erased `Broker` trait can't
    /// return its catalog generically, brokers that need their catalog
    /// surfaced should expose it via concrete-type access AND register the
    /// catalog separately. For MVP simplicity, this method returns the
    /// catalogs registered via `register_with_catalog()`.
    pub fn full_catalog(&self) -> Vec<crate::Pipeline> {
        let mut out = Vec::new();
        for catalog in self.broker_catalogs.values() {
            out.extend(catalog.iter().cloned());
        }
        out
    }

    /// Register a concrete broker with its catalog. Preferred over `register()`
    /// for brokers whose catalog should be available via `full_catalog()`.
    pub fn register_with_catalog(
        &mut self,
        broker: Arc<dyn Broker>,
        catalog: Vec<crate::Pipeline>,
    ) -> Result<(), RegistryError> {
        let id = broker.id().to_string();
        self.register(broker)?;
        self.broker_catalogs.insert(id, catalog);
        Ok(())
    }

    /// Catalog for a specific broker (None if not registered or registered
    /// via `register()` without catalog).
    pub fn catalog_for(&self, broker_id: &str) -> Option<&Vec<crate::Pipeline>> {
        self.broker_catalogs.get(broker_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::{BrokerError, Role, WorldEvent};
    use crate::pipeline::Pipeline;
    use crate::runner::{LeafContext, LeafError};
    use async_trait::async_trait;
    use tempfile::TempDir;

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
            serde_json::json!({})
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

    fn write_cluster_fixture() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let cluster_path = tmp.path().join("cluster.toml");
        let broker_path = tmp.path().join("work-broker.toml");
        std::fs::write(
            &cluster_path,
            r#"
[cluster]
id = "test-cluster"
name = "Test"
brokers_dir = "./"

[cluster.brokers.work-broker]
manifest_path = "work-broker.toml"
"#,
        )
        .unwrap();
        std::fs::write(
            &broker_path,
            r#"
[broker]
id = "work-broker"
name = "Work Broker"
roles = ["innate-ability"]
cold_store_path = ".cold/work-broker/"
catalog_path = "work-broker-catalog.yaml"
"#,
        )
        .unwrap();
        (tmp, cluster_path)
    }

    #[test]
    fn registry_loads_cluster_and_broker_manifests() {
        let (_tmp, cluster_path) = write_cluster_fixture();
        let reg = BrokerRegistry::load_manifests(&cluster_path).unwrap();
        assert_eq!(reg.cluster().id, "test-cluster");
        assert!(reg.broker_manifest("work-broker").is_some());
    }

    #[test]
    fn registry_validate_fails_on_unregistered_broker() {
        let (_tmp, cluster_path) = write_cluster_fixture();
        let reg = BrokerRegistry::load_manifests(&cluster_path).unwrap();
        let err = reg.validate().unwrap_err();
        assert!(matches!(err, RegistryError::BrokerNotRegistered(_)));
    }

    #[test]
    fn registry_register_validates_role_set() {
        let (_tmp, cluster_path) = write_cluster_fixture();
        let mut reg = BrokerRegistry::load_manifests(&cluster_path).unwrap();
        // Register with WRONG roles
        let wrong = Arc::new(MiniBroker {
            id: "work-broker".to_string(),
            roles: RoleSet::single(Role::Sense),
        });
        let err = reg.register(wrong).unwrap_err();
        assert!(matches!(err, RegistryError::RoleSetMismatch { .. }));
    }

    #[test]
    fn registry_register_rejects_unknown_broker_id() {
        let (_tmp, cluster_path) = write_cluster_fixture();
        let mut reg = BrokerRegistry::load_manifests(&cluster_path).unwrap();
        let unknown = Arc::new(MiniBroker {
            id: "evil-broker".to_string(),
            roles: RoleSet::single(Role::Sense),
        });
        let err = reg.register(unknown).unwrap_err();
        assert!(matches!(err, RegistryError::BrokerNotDeclared(_)));
    }

    #[test]
    fn registry_register_then_validate_success() {
        let (_tmp, cluster_path) = write_cluster_fixture();
        let mut reg = BrokerRegistry::load_manifests(&cluster_path).unwrap();
        let ok = Arc::new(MiniBroker {
            id: "work-broker".to_string(),
            roles: RoleSet::single(Role::InnateAbility),
        });
        reg.register(ok).unwrap();
        reg.validate().unwrap();
        assert!(reg.broker("work-broker").is_some());
    }

    #[test]
    fn registry_full_catalog_aggregates_per_broker_catalogs() {
        use crate::pipeline::{AuditClass, EffectClass, Tunability, Visibility};
        let (_tmp, cluster_path) = write_cluster_fixture();
        let mut reg = BrokerRegistry::load_manifests(&cluster_path).unwrap();
        let broker = Arc::new(MiniBroker {
            id: "work-broker".to_string(),
            roles: RoleSet::single(Role::InnateAbility),
        });
        let catalog = vec![Pipeline {
            id: "work-broker/p1".to_string(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorOnly,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::ReadOnly,
            params: serde_json::json!({}),
            preconditions: vec![],
            steps: vec![],
            description: String::new(),
            when_to_use: String::new(),
        }];
        reg.register_with_catalog(broker, catalog.clone()).unwrap();
        let full = reg.full_catalog();
        assert_eq!(full.len(), 1);
        assert_eq!(full[0].id, "work-broker/p1");
        // Pipeline doesn't impl PartialEq (Value field doesn't); check by id
        let retrieved = reg.catalog_for("work-broker").unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].id, "work-broker/p1");
    }
}
