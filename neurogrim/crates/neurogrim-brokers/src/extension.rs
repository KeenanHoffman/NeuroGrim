//! BB # (substrate addition) — Broker Extension Registry.
//!
//! Operator-extensibility primitive: brokers can OPT IN to receiving
//! declarative extension configs at host-boot time. The substrate provides
//! the scaffolding (disk discovery, schema-version negotiation, dispatch
//! into brokers); each `Extensible` broker defines what its extensions
//! actually do (add facts, register new pipelines, declare new sensors).
//!
//! ## Two-tier extensibility model
//!
//! This module implements **Tier 1** (data-driven, TOML configs). Tier 2
//! is operator-authored Rust code registered via the existing
//! [`crate::host::BrokerFactoryRegistry`] — that path is unchanged.
//!
//! ## On-disk layout
//!
//! Extensions live in their own directory, alongside (not inside)
//! `cluster.toml`. The substrate's default location is
//! `<cluster_manifest_dir>/extensions/<broker_id>/*.toml`.
//!
//! Each config file MUST contain an `[extension]` table declaring:
//! - `schema_version` — must match the target broker's
//!   [`Extensible::extension_schema_version`]; mismatches fail loudly
//! - (any further `[[<section>]]` arrays — broker-specific shape)
//!
//! ## Cluster.toml impact
//!
//! Extensions stay isolated — discovered separately from cluster.toml;
//! the cluster manifest is never auto-edited based on extension contents
//! (per the operator decision recorded in
//! `docs/BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md`, Gate framing).
//! Operators see clearly which brokers are core (cluster.toml) vs which
//! capabilities came from extensions (in their own directory).
//!
//! ## How brokers opt in
//!
//! 1. Implement [`Extensible`] (in addition to [`crate::Broker`]).
//! 2. Override [`crate::Broker::as_extensible`] to return `Some(self)`
//!    (this is what makes the host's discovery loop see the broker as
//!    extensible — Rust's type erasure means we need the explicit hook).
//! 3. The host's boot path calls [`Extensible::apply_extension`] for each
//!    discovered config that targets this broker, in deterministic
//!    file-name order. The broker uses interior mutability (the established
//!    pattern) to store / register what each extension declares.

use crate::broker::Broker;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// A single extension config loaded from disk. Each broker downcasts the
/// raw TOML to its own per-broker schema; the substrate enforces only the
/// shared `[extension]` envelope contract.
#[derive(Debug, Clone)]
pub struct ExtensionConfig {
    /// Source path the config was loaded from (used in error messages so
    /// operators can find the file).
    pub source_path: PathBuf,
    /// Broker ID the config targets — derived from the parent directory
    /// name (`extensions/<broker_id>/*.toml`).
    pub target_broker_id: String,
    /// Schema version declared in `[extension].schema_version`. Brokers
    /// compare this against their `extension_schema_version()` and refuse
    /// on mismatch.
    pub schema_version: String,
    /// Free-form `[extension].authored_by` for operator attribution
    /// (optional; surfaced in audit / error messages).
    pub authored_by: Option<String>,
    /// Raw parsed TOML value — broker downcasts to its own schema via
    /// `serde::Deserialize`.
    pub raw: toml::Value,
}

impl ExtensionConfig {
    /// Convenience: deserialize a section of the raw TOML into a broker-
    /// specific shape. Returns a structured error pointing at the source
    /// path so operators can fix malformed configs.
    pub fn deserialize_section<T: serde::de::DeserializeOwned>(
        &self,
        section: &str,
    ) -> Result<T, ExtensionError> {
        let section_value = self.raw.get(section).cloned().ok_or_else(|| {
            ExtensionError::BrokerRejected {
                broker_id: self.target_broker_id.clone(),
                path: self.source_path.clone(),
                reason: format!("missing required section [{}]", section),
            }
        })?;
        section_value.try_into().map_err(|e: toml::de::Error| {
            ExtensionError::BrokerRejected {
                broker_id: self.target_broker_id.clone(),
                path: self.source_path.clone(),
                reason: format!("section [{}] deserialize failed: {}", section, e),
            }
        })
    }
}

/// Errors during extension discovery + application.
#[derive(Debug, Error)]
pub enum ExtensionError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("toml parse error in {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("extension config missing required [extension] section in {path}")]
    MissingExtensionSection { path: PathBuf },
    #[error(
        "schema version mismatch for broker `{broker_id}` (extension at {path}): \
         extension wants `{requested}`, broker supports `{supported}`"
    )]
    SchemaVersionMismatch {
        broker_id: String,
        path: PathBuf,
        requested: String,
        supported: String,
    },
    #[error("broker `{broker_id}` rejected extension at {path}: {reason}")]
    BrokerRejected {
        broker_id: String,
        path: PathBuf,
        reason: String,
    },
}

/// Brokers OPT IN to receiving extensions by implementing this trait.
///
/// The host's boot path discovers extensions on disk, then iterates each
/// bootstrapped broker. For brokers whose [`Broker::as_extensible`] returns
/// `Some`, the host calls [`Extensible::apply_extension`] for each config
/// targeting that broker_id (deterministic order: lexicographically by
/// source filename).
///
/// Brokers use interior mutability (the established broker pattern, e.g.
/// `Mutex<WorkingState>` or `RwLock<...>`) to store / register what each
/// extension declares — `apply_extension` takes `&self`, not `&mut self`,
/// so brokers remain `Arc<dyn Broker>`-compatible.
#[async_trait]
pub trait Extensible: Broker {
    /// Schema version this broker supports. Extensions declare a target
    /// version in their `[extension].schema_version` field; the host
    /// refuses extensions whose version doesn't match this string exactly.
    /// Bump this when the broker's extension schema changes in a way that
    /// breaks older configs.
    fn extension_schema_version(&self) -> &str;

    /// Apply one extension config to this broker. Called by the host at
    /// boot, after broker construction, before first dispatch. Brokers use
    /// interior mutability to store the extension's contents.
    ///
    /// The substrate has already verified the extension's schema_version
    /// matches `extension_schema_version()` before calling this method.
    /// Brokers focus purely on consuming the operator's declared content.
    async fn apply_extension(&self, config: &ExtensionConfig) -> Result<(), ExtensionError>;
}

/// Registry of all extensions discovered on disk, keyed by target broker ID.
///
/// Built at host-boot time by [`Self::discover_from_disk`], then consumed
/// by [`crate::host::BrokerHost::boot`] which calls `apply_extension` for
/// each Extensible broker.
#[derive(Debug, Default)]
pub struct ExtensionRegistry {
    by_broker: HashMap<String, Vec<ExtensionConfig>>,
}

impl ExtensionRegistry {
    /// Empty registry. Use this in tests / when no extensions are wanted.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Scan `extensions_dir` for per-broker subdirectories containing
    /// `*.toml` extension configs.
    ///
    /// Expected layout:
    /// ```text
    /// <extensions_dir>/
    ///   workspace/
    ///     team-conventions.toml
    ///     deployment-runbook.toml
    ///   sensory/
    ///     doc-quality.toml
    /// ```
    ///
    /// Returns `Ok(empty)` if `extensions_dir` does not exist — extensions
    /// are an opt-in convenience, not required. Returns errors for
    /// malformed configs (parse failure or missing `[extension]` section);
    /// the host should surface these loudly so operators can fix.
    pub fn discover_from_disk(extensions_dir: &Path) -> Result<Self, ExtensionError> {
        let mut registry = Self::empty();
        if !extensions_dir.exists() {
            // Operators who haven't authored any extensions hit this path;
            // not an error.
            return Ok(registry);
        }

        let broker_dirs = std::fs::read_dir(extensions_dir).map_err(|e| ExtensionError::Io {
            path: extensions_dir.to_path_buf(),
            source: e,
        })?;

        // Collect broker dirs so we can sort for deterministic order.
        let mut sorted_broker_dirs: Vec<PathBuf> = Vec::new();
        for entry in broker_dirs.flatten() {
            let path = entry.path();
            if path.is_dir() {
                sorted_broker_dirs.push(path);
            }
        }
        sorted_broker_dirs.sort();

        for broker_dir in sorted_broker_dirs {
            let broker_id = broker_dir
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_default();
            if broker_id.is_empty() {
                continue;
            }

            // Collect *.toml files in this broker dir, sorted for
            // deterministic apply order.
            let mut toml_files: Vec<PathBuf> = std::fs::read_dir(&broker_dir)
                .map_err(|e| ExtensionError::Io {
                    path: broker_dir.clone(),
                    source: e,
                })?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
                .collect();
            toml_files.sort();

            for toml_path in toml_files {
                let config = load_extension_config(&toml_path, &broker_id)?;
                registry
                    .by_broker
                    .entry(broker_id.clone())
                    .or_default()
                    .push(config);
            }
        }

        Ok(registry)
    }

    /// Get all extension configs targeting a specific broker, in the
    /// deterministic order they were discovered. Returns None if no
    /// extensions target this broker.
    pub fn get(&self, broker_id: &str) -> Option<&[ExtensionConfig]> {
        self.by_broker.get(broker_id).map(|v| v.as_slice())
    }

    /// True iff no extensions were discovered.
    pub fn is_empty(&self) -> bool {
        self.by_broker.is_empty()
    }

    /// Iterate broker IDs that have at least one extension.
    pub fn broker_ids(&self) -> impl Iterator<Item = &String> {
        self.by_broker.keys()
    }

    /// Total count of extension configs across all brokers (for tracing).
    pub fn total_count(&self) -> usize {
        self.by_broker.values().map(|v| v.len()).sum()
    }
}

/// Load + validate one extension config file. Reads the file, parses TOML,
/// extracts the `[extension]` envelope, returns a structured `ExtensionConfig`.
fn load_extension_config(
    path: &Path,
    broker_id: &str,
) -> Result<ExtensionConfig, ExtensionError> {
    let contents = std::fs::read_to_string(path).map_err(|e| ExtensionError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let raw: toml::Value = toml::from_str(&contents).map_err(|e| ExtensionError::Parse {
        path: path.to_path_buf(),
        source: e,
    })?;

    let extension_table = raw
        .get("extension")
        .and_then(|v| v.as_table())
        .ok_or_else(|| ExtensionError::MissingExtensionSection {
            path: path.to_path_buf(),
        })?;

    let schema_version = extension_table
        .get("schema_version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ExtensionError::MissingExtensionSection {
            path: path.to_path_buf(),
        })?;

    let authored_by = extension_table
        .get("authored_by")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(ExtensionConfig {
        source_path: path.to_path_buf(),
        target_broker_id: broker_id.to_string(),
        schema_version,
        authored_by,
        raw,
    })
}

/// Apply all discovered extensions to their target brokers.
///
/// Called by [`crate::host::BrokerHost::boot`] after broker construction,
/// before initial materialization. For each broker that's `Extensible`
/// (via [`Broker::as_extensible`]) AND has discovered extensions, applies
/// them in deterministic order. Schema version mismatches fail loudly.
pub async fn apply_all_extensions(
    bootstrapped: &[(String, std::sync::Arc<dyn Broker>)],
    registry: &ExtensionRegistry,
) -> Result<usize, ExtensionError> {
    let mut applied = 0;
    for (broker_id, broker) in bootstrapped {
        let Some(configs) = registry.get(broker_id) else {
            continue;
        };
        let Some(extensible) = broker.as_extensible() else {
            // Broker doesn't opt in to extensions; operator's configs are
            // ignored. Tracing-warn so operators see when their configs
            // miss the target.
            tracing::warn!(
                broker_id = %broker_id,
                config_count = configs.len(),
                "extension configs found for broker `{}` but broker is not Extensible; configs ignored",
                broker_id
            );
            continue;
        };
        let supported_version = extensible.extension_schema_version().to_string();
        for config in configs {
            if config.schema_version != supported_version {
                return Err(ExtensionError::SchemaVersionMismatch {
                    broker_id: broker_id.clone(),
                    path: config.source_path.clone(),
                    requested: config.schema_version.clone(),
                    supported: supported_version.clone(),
                });
            }
            extensible.apply_extension(config).await?;
            applied += 1;
        }
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn discover_empty_dir_returns_empty_registry() {
        let tmp = TempDir::new().unwrap();
        let registry = ExtensionRegistry::discover_from_disk(tmp.path()).unwrap();
        assert!(registry.is_empty());
        assert_eq!(registry.total_count(), 0);
    }

    #[test]
    fn discover_nonexistent_dir_returns_empty_registry() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let registry = ExtensionRegistry::discover_from_disk(&missing).unwrap();
        assert!(registry.is_empty());
    }

    #[test]
    fn discover_parses_per_broker_directories_and_sorts_deterministically() {
        let tmp = TempDir::new().unwrap();
        let ext_dir = tmp.path();

        // Two broker dirs with two configs each.
        fs::create_dir_all(ext_dir.join("workspace")).unwrap();
        fs::create_dir_all(ext_dir.join("sensory")).unwrap();

        for (broker, name) in [
            ("workspace", "zeta.toml"),
            ("workspace", "alpha.toml"),
            ("sensory", "doc-quality.toml"),
            ("sensory", "api-stability.toml"),
        ] {
            fs::write(
                ext_dir.join(broker).join(name),
                format!(
                    r#"
[extension]
schema_version = "1"
authored_by = "operator"

[stub]
name = "{}"
"#,
                    name
                ),
            )
            .unwrap();
        }

        let registry = ExtensionRegistry::discover_from_disk(ext_dir).unwrap();
        assert!(!registry.is_empty());
        assert_eq!(registry.total_count(), 4);

        let ws = registry.get("workspace").expect("workspace configs");
        assert_eq!(ws.len(), 2);
        // Deterministic: alpha before zeta
        assert!(ws[0].source_path.ends_with("alpha.toml"));
        assert!(ws[1].source_path.ends_with("zeta.toml"));

        let sn = registry.get("sensory").expect("sensory configs");
        assert_eq!(sn.len(), 2);
        assert!(sn[0].source_path.ends_with("api-stability.toml"));
        assert!(sn[1].source_path.ends_with("doc-quality.toml"));
    }

    #[test]
    fn parse_error_surfaces_with_path_context() {
        let tmp = TempDir::new().unwrap();
        let ext_dir = tmp.path();
        fs::create_dir_all(ext_dir.join("workspace")).unwrap();
        fs::write(
            ext_dir.join("workspace").join("broken.toml"),
            "this is not = valid toml [",
        )
        .unwrap();

        let err = ExtensionRegistry::discover_from_disk(ext_dir).unwrap_err();
        match err {
            ExtensionError::Parse { path, .. } => {
                assert!(path.ends_with("broken.toml"));
            }
            other => panic!("expected Parse error, got {:?}", other),
        }
    }

    #[test]
    fn missing_extension_section_surfaces_clear_error() {
        let tmp = TempDir::new().unwrap();
        let ext_dir = tmp.path();
        fs::create_dir_all(ext_dir.join("workspace")).unwrap();
        fs::write(
            ext_dir.join("workspace").join("incomplete.toml"),
            r#"
# No [extension] section at all.
[other_section]
key = "value"
"#,
        )
        .unwrap();

        let err = ExtensionRegistry::discover_from_disk(ext_dir).unwrap_err();
        match err {
            ExtensionError::MissingExtensionSection { path } => {
                assert!(path.ends_with("incomplete.toml"));
            }
            other => panic!("expected MissingExtensionSection, got {:?}", other),
        }
    }

    #[test]
    fn deserialize_section_round_trip() {
        let raw: toml::Value = toml::from_str(
            r#"
[extension]
schema_version = "1"

[stub]
name = "test"
count = 42
"#,
        )
        .unwrap();
        let config = ExtensionConfig {
            source_path: PathBuf::from("/tmp/test.toml"),
            target_broker_id: "workspace".to_string(),
            schema_version: "1".to_string(),
            authored_by: None,
            raw,
        };

        #[derive(serde::Deserialize, Debug)]
        struct StubSection {
            #[allow(dead_code)]
            name: String,
            count: u32,
        }

        let stub: StubSection = config.deserialize_section("stub").unwrap();
        assert_eq!(stub.name, "test");
        assert_eq!(stub.count, 42);
    }

    #[test]
    fn deserialize_section_missing_returns_clear_error() {
        let raw: toml::Value = toml::from_str(
            r#"
[extension]
schema_version = "1"
"#,
        )
        .unwrap();
        let config = ExtensionConfig {
            source_path: PathBuf::from("/tmp/test.toml"),
            target_broker_id: "workspace".to_string(),
            schema_version: "1".to_string(),
            authored_by: None,
            raw,
        };

        #[derive(serde::Deserialize, Debug)]
        struct StubSection {
            name: String,
        }

        let err = config.deserialize_section::<StubSection>("stub").unwrap_err();
        match err {
            ExtensionError::BrokerRejected { reason, .. } => {
                assert!(reason.contains("missing required section [stub]"));
            }
            other => panic!("expected BrokerRejected, got {:?}", other),
        }
    }
}
