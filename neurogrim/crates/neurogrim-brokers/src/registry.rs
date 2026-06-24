//! BB #14 — Broker Registry.
//!
//! Discovers + loads brokers at harness startup from the cluster manifest TOML.
//! Validates per-broker manifests + role-set declarations.
//!
//! ## Wave 0 scaffold; Wave 3 implements.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("cluster manifest not found: {0}")]
    ManifestNotFound(PathBuf),

    #[error("cluster manifest parse failed: {0}")]
    ManifestParseFailed(String),

    #[error("broker manifest invalid: {broker_id}: {reason}")]
    BrokerManifestInvalid { broker_id: String, reason: String },

    #[error("duplicate broker id: {0}")]
    DuplicateBrokerId(String),
}

/// Broker Registry — Wave 3 implements.
pub struct BrokerRegistry {
    _cluster_manifest_path: PathBuf,
}

impl BrokerRegistry {
    pub fn new(cluster_manifest_path: PathBuf) -> Self {
        Self {
            _cluster_manifest_path: cluster_manifest_path,
        }
    }

    /// Load all brokers declared in the cluster manifest. Wave 3 implements.
    pub fn load(&self) -> Result<(), RegistryError> {
        Err(RegistryError::ManifestNotFound(PathBuf::from(
            "BrokerRegistry::load not yet implemented (Wave 3)",
        )))
    }
}
