//! A.0.3 — CMDB Materializer.
//!
//! Parallel to [`crate::materializer::hot_store::HotStoreMaterializer`] and
//! [`crate::materializer::awareness::AwarenessMaterializer`]. Writes
//! per-broker overlay JSON to a CMDB file path, atomically.
//!
//! ## Why a separate materializer (not extending HotStore)
//!
//! HotStore writes Markdown segments for the agent's awareness routing.
//! CMDB JSON is consumed by:
//! - the scoring engine (`neurogrim-core::scoring_sources::cmdb`)
//! - the dashboard at port 8420 (`neurogrim-dashboard::routes::brain_domain_detail`)
//! - the MCP `brain_query` tool
//! - A2A peer queries
//! - the coherence sensor (reads sibling CMDBs)
//!
//! Mixing markdown segment composition with JSON CMDB writes in one
//! materializer would muddy two distinct contracts. Separate
//! materializers keep each output format clean.
//!
//! ## Path resolution (per Gate 2)
//!
//! The substrate's resolution chain is:
//! 1. **Registry override** (e.g., `brain-registry.json`'s
//!    `domain_definitions.<broker_id>.scoring_source.path`). Host
//!    consults this BEFORE constructing the materializer.
//! 2. **Broker default** ([`crate::Broker::cmdb_path`]). The substrate
//!    consults this when the host doesn't supply an override.
//! 3. **None** — broker doesn't export a CMDB; materializer is a no-op
//!    for this broker.
//!
//! V1 substrate skips step (1) — the materializer is constructed with the
//! final resolved path. The host's broker-iteration loop computes the
//! path per broker by calling `cmdb_path()`. Future revisions can add
//! registry-override consultation at the host level without changing
//! the materializer API.
//!
//! ## Atomic write semantics
//!
//! Uses `std::fs::rename` for the temp→final swap. Per Rust stdlib docs,
//! `std::fs::rename` is cross-platform safe for atomic-replace on Windows
//! and POSIX (Windows uses MoveFileEx with MOVEFILE_REPLACE_EXISTING
//! internally). External CMDB consumers reading the file mid-write see
//! either the old contents or the new contents, never partial.

use crate::broker::Broker;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CmdbMatError {
    #[error("io error writing CMDB to {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("serialization failed for broker `{broker_id}`: {source}")]
    SerializationFailed {
        broker_id: String,
        #[source]
        source: serde_json::Error,
    },
}

pub struct CmdbMaterializer {
    broker_id: String,
    /// Final resolved CMDB path. None = skip this broker. The host
    /// supplies the resolved path (registry override OR broker default).
    cmdb_path: Option<PathBuf>,
}

impl CmdbMaterializer {
    /// Construct with the host-resolved CMDB path (or None to skip).
    pub fn new(broker_id: impl Into<String>, cmdb_path: Option<PathBuf>) -> Self {
        Self {
            broker_id: broker_id.into(),
            cmdb_path,
        }
    }

    /// Convenience: construct by reading the broker's default
    /// [`Broker::cmdb_path`]. Used by the host when no registry override
    /// applies. Project root is prepended IFF the broker's declared path
    /// is relative.
    pub fn from_broker_default(broker: &dyn Broker, project_root: &Path) -> Self {
        let resolved = broker.cmdb_path().map(|p| {
            if p.is_absolute() {
                p
            } else {
                project_root.join(p)
            }
        });
        Self::new(broker.id().to_string(), resolved)
    }

    /// The resolved CMDB output path, or None if this broker doesn't
    /// export a CMDB.
    pub fn cmdb_path(&self) -> Option<&Path> {
        self.cmdb_path.as_deref()
    }

    /// Write the broker's current overlay JSON to its CMDB file.
    /// No-op when `cmdb_path` is None.
    ///
    /// Atomic via temp + rename. If `read_overlay()` returns a value that
    /// doesn't conform to `cmdb-envelope-v1.schema.json`, this method
    /// still writes it — schema enforcement is the broker's responsibility
    /// (the Sensory Queue enforcer in A.2.5 will validate sensor
    /// extension writes; sensor brokers that themselves emit ill-formed
    /// envelopes is a broker bug, not a substrate concern).
    pub async fn materialize(&self, broker: Arc<dyn Broker>) -> Result<(), CmdbMatError> {
        let Some(path) = &self.cmdb_path else {
            return Ok(()); // broker doesn't export a CMDB; skip silently
        };
        let payload = broker.read_overlay().await;
        let serialized = serde_json::to_string_pretty(&payload).map_err(|e| {
            CmdbMatError::SerializationFailed {
                broker_id: self.broker_id.clone(),
                source: e,
            }
        })?;
        write_atomically(path, &serialized).map_err(|e| CmdbMatError::Io {
            path: path.clone(),
            source: e,
        })?;
        Ok(())
    }
}

/// Atomic write: write to a temp file in the same directory as `path`,
/// then `rename` it onto `path`. `std::fs::rename` is cross-platform
/// safe (POSIX guarantees atomicity within a filesystem; Windows uses
/// MoveFileEx with MOVEFILE_REPLACE_EXISTING since Rust 1.5+).
///
/// External consumers reading the file mid-write see either the OLD
/// contents or the NEW contents — never partial.
fn write_atomically(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp_path = make_temp_sibling(path);
    std::fs::write(&temp_path, content)?;
    // rename is atomic-replace on both Windows + POSIX.
    if let Err(e) = std::fs::rename(&temp_path, path) {
        // Clean up the temp file if rename failed (so we don't leave
        // crud lying around in the CMDB directory).
        let _ = std::fs::remove_file(&temp_path);
        return Err(e);
    }
    Ok(())
}

/// Build a temp-file path in the same directory as `path`, with a
/// suffix unique to this process + nanosecond timestamp. Same-directory
/// is required for `rename` to be atomic (cross-filesystem renames are
/// not atomic on either platform).
fn make_temp_sibling(path: &Path) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let suffix = format!(".tmp.{}.{}", pid, nanos);
    let mut new_name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    new_name.push(&suffix);
    path.with_file_name(new_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::{BrokerError, Role, RoleSet, WorldEvent};
    use crate::pipeline::Pipeline;
    use crate::runner::{LeafContext, LeafError};
    use async_trait::async_trait;
    use tempfile::TempDir;

    struct FakeCmdbBroker {
        id: String,
        cmdb_path: Option<PathBuf>,
        payload: serde_json::Value,
    }

    #[async_trait]
    impl Broker for FakeCmdbBroker {
        fn id(&self) -> &str {
            &self.id
        }
        fn role_set(&self) -> RoleSet {
            RoleSet::single(Role::Sense)
        }
        async fn read_overlay(&self) -> serde_json::Value {
            self.payload.clone()
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
        fn cmdb_path(&self) -> Option<PathBuf> {
            self.cmdb_path.clone()
        }
    }

    #[tokio::test]
    async fn materializes_overlay_to_cmdb_path() {
        let tmp = TempDir::new().unwrap();
        let cmdb_path = tmp.path().join(".claude/test-broker-cmdb.json");
        let mat = CmdbMaterializer::new("test-broker", Some(cmdb_path.clone()));
        let broker = Arc::new(FakeCmdbBroker {
            id: "test-broker".to_string(),
            cmdb_path: Some(cmdb_path.clone()),
            payload: serde_json::json!({"score": 42, "findings": []}),
        });
        mat.materialize(broker).await.unwrap();
        let contents = std::fs::read_to_string(&cmdb_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["score"], 42);
    }

    #[tokio::test]
    async fn no_cmdb_path_means_no_write() {
        let tmp = TempDir::new().unwrap();
        let mat = CmdbMaterializer::new("test-broker", None);
        let broker = Arc::new(FakeCmdbBroker {
            id: "test-broker".to_string(),
            cmdb_path: None,
            payload: serde_json::json!({"score": 42}),
        });
        mat.materialize(broker).await.unwrap();
        // No CMDB file should exist in tmp.
        let count = std::fs::read_dir(tmp.path()).unwrap().count();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn from_broker_default_resolves_relative_to_project_root() {
        let tmp = TempDir::new().unwrap();
        let broker = FakeCmdbBroker {
            id: "test-broker".to_string(),
            cmdb_path: Some(PathBuf::from(".claude/test-broker-cmdb.json")),
            payload: serde_json::json!({"score": 1}),
        };
        let mat = CmdbMaterializer::from_broker_default(&broker, tmp.path());
        let expected = tmp.path().join(".claude/test-broker-cmdb.json");
        assert_eq!(mat.cmdb_path(), Some(expected.as_path()));
    }

    #[tokio::test]
    async fn from_broker_default_respects_absolute_path() {
        let tmp = TempDir::new().unwrap();
        let absolute = tmp.path().join("absolute/somewhere/else.json");
        let broker = FakeCmdbBroker {
            id: "test-broker".to_string(),
            cmdb_path: Some(absolute.clone()),
            payload: serde_json::json!({"score": 1}),
        };
        let project_root = tmp.path().join("unrelated");
        let mat = CmdbMaterializer::from_broker_default(&broker, &project_root);
        assert_eq!(mat.cmdb_path(), Some(absolute.as_path()));
    }

    #[tokio::test]
    async fn atomic_write_replaces_existing_file_safely() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("existing.json");
        std::fs::write(&path, r#"{"old": true}"#).unwrap();

        let mat = CmdbMaterializer::new("test-broker", Some(path.clone()));
        let broker = Arc::new(FakeCmdbBroker {
            id: "test-broker".to_string(),
            cmdb_path: Some(path.clone()),
            payload: serde_json::json!({"new": true, "score": 99}),
        });
        mat.materialize(broker).await.unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["new"], true);
        assert_eq!(parsed["score"], 99);
        assert!(parsed.get("old").is_none(), "old contents should be overwritten");

        // Temp file should not be left behind
        let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains(".tmp.")
            })
            .collect();
        assert!(leftovers.is_empty(), "no temp files should be left behind");
    }

    #[tokio::test]
    async fn write_creates_parent_directories() {
        let tmp = TempDir::new().unwrap();
        // Deep nested path; parents don't exist yet.
        let path = tmp.path().join("a/b/c/d/nested-cmdb.json");
        let mat = CmdbMaterializer::new("test-broker", Some(path.clone()));
        let broker = Arc::new(FakeCmdbBroker {
            id: "test-broker".to_string(),
            cmdb_path: Some(path.clone()),
            payload: serde_json::json!({"deep": true}),
        });
        mat.materialize(broker).await.unwrap();
        assert!(path.exists());
    }
}
