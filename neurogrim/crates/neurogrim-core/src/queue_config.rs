//! v4.1 S13-B-3 — per-topic queue configuration.
//!
//! Adopters opt into the SQLite backend (and per-topic retention /
//! ack semantics) via `<brain>/.claude/brain/queue-config.yaml`:
//!
//! ```yaml
//! schema_version: "1"
//! topics:
//!   _neurogrim/approvals:
//!     backend: jsonl
//!     retention_days: 30
//!   pc-state/alerts:
//!     backend: sqlite
//!     retention_messages: 10000
//!     ack_required: true
//! ```
//!
//! **Topics not in the config:** default to JSONL with the standard
//! retention policy (30 days OR 10k messages). Adopters who want
//! exactly-once consumption opt in explicitly.
//!
//! **Why YAML, not JSON:** consistent with `culture.yaml`,
//! `secret-refs.yaml`, `publish-gates.yaml` — adopters edit one
//! file family, not two.
//!
//! ## v1 scope (this stage)
//!
//! - Schema definition + parser
//! - `lookup(&topic)` returns the resolved per-topic config
//!   (with defaults applied for unspecified fields)
//!
//! ## Deferred to v2 (follow-up session, paired with bus.rs wiring)
//!
//! - `neurogrim doctor` validates the manifest against this schema
//! - `bus.rs` reads it at startup + dispatches per-topic to the
//!   right backend
//! - Hot-reload on file change (today's bus assumes the config is
//!   stable for the dashboard's lifetime)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

// V5-MOD-3 Phase 3 (2026-05-02 — Fork B1) — `BackendKind` enum
// previously lived here as a closed-set serde enum (`Jsonl |
// Sqlite`). Replaced with `String` to support third-party backend
// names registered via `QueueBackendRegistry` (V5-MOD-3 Phase 1).
// The wire format in `queue-config.yaml` is unchanged: lowercase
// strings (`backend: jsonl`, `backend: sqlite`, `backend: postgres`,
// `backend: redis`, ...) deserialize to `String` directly. Existing
// YAML files round-trip unchanged.
//
// `validate_with_registry()` (below) replaces the type-system
// invariant `backend != "sqlite"` with a runtime check
// against `QueueBackendFactory::supports_ack()` — third-party
// ack-capable backends are accepted alongside `sqlite`.

/// Default backend wire-name. Used by `TopicConfig::default()` when
/// a topic has no explicit override.
pub const DEFAULT_BACKEND: &str = "jsonl";

/// One topic's resolved configuration. Construct via
/// [`QueueConfig::lookup`] — the lookup applies sensible defaults
/// for unspecified fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicConfig {
    /// Backend wire-name (e.g., `"jsonl"`, `"sqlite"`, or any
    /// third-party-registered backend name). The `QueueBackendRegistry`
    /// resolves this to a concrete `Arc<dyn QueueBackend>` at
    /// dispatch time.
    pub backend: String,
    /// Drop messages older than this many days during compaction.
    /// `None` = no time-based retention.
    pub retention_days: Option<u32>,
    /// Drop messages older than this many entries during compaction
    /// (keep the most-recent N). `None` = no count-based retention.
    pub retention_messages: Option<u32>,
    /// True iff consumers must explicitly ack each message. Validated
    /// against [`QueueConfig::validate_with_registry`] to catch
    /// declaring `ack_required: true` on a non-ack-capable backend.
    pub ack_required: bool,
}

impl Default for TopicConfig {
    fn default() -> Self {
        Self {
            backend: DEFAULT_BACKEND.to_string(),
            retention_days: Some(30),
            retention_messages: Some(10_000),
            ack_required: false,
        }
    }
}

/// Wire shape of one entry in `queue-config.yaml::topics`. All fields
/// are optional in the YAML; missing fields get [`TopicConfig::default`]
/// values when [`QueueConfig::lookup`] resolves them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TopicConfigYaml {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub retention_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub retention_messages: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ack_required: Option<bool>,
}

/// Wire shape of `queue-config.yaml`. Hand-edited by adopters; parsed
/// at dashboard startup + after file changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QueueConfig {
    /// Schema version. v1 is the only valid value today.
    pub schema_version: String,
    /// Per-topic overrides keyed by full topic name (e.g.
    /// `pc-state/alerts`). Topics not listed default to
    /// [`TopicConfig::default`].
    #[serde(default)]
    pub topics: BTreeMap<String, TopicConfigYaml>,
}

impl QueueConfig {
    /// Parse a `queue-config.yaml` text body. Validates structure +
    /// applies cross-field invariants (e.g., `ack_required: true`
    /// requires `backend: sqlite`).
    pub fn from_yaml(text: &str) -> Result<Self> {
        let cfg: QueueConfig = serde_yaml::from_str(text)
            .context("queue-config.yaml: parse")?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Parse the file at `path`. Returns `Ok(None)` when the file
    /// doesn't exist (adopter hasn't authored one yet — every topic
    /// falls back to [`TopicConfig::default`]).
    pub fn from_path(path: &Path) -> Result<Option<Self>> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(Self::from_yaml(&text)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(anyhow::Error::from(e).context(format!(
                "queue-config.yaml: read {}",
                path.display()
            ))),
        }
    }

    /// **Shape-only** invariants. Checks `schema_version`; does NOT
    /// validate the `ack_required ⇒ ack-capable backend` cross-field
    /// invariant — that requires registry awareness, see
    /// [`Self::validate_with_registry`].
    ///
    /// Called by [`Self::from_yaml`] at parse time. Returns an error
    /// describing the first violation found.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != "1" {
            anyhow::bail!(
                "queue-config.yaml: unsupported schema_version {:?} \
                 (only \"1\" is currently valid)",
                self.schema_version
            );
        }
        Ok(())
    }

    /// **Registry-aware** cross-field invariants
    /// (V5-MOD-3 Phase 3, 2026-05-02 — Fork B's 🔴 B2 plan-critic fix).
    ///
    /// Validates that:
    /// - Every named backend is registered in `registry`.
    /// - Topics declaring `ack_required: true` use a backend whose
    ///   factory's `supports_ack()` returns `true`.
    ///
    /// Called by the bus at startup, after the registry is populated
    /// with built-in + third-party factories. Separated from the
    /// shape-only [`Self::validate`] so shape-checks can run without
    /// a registry (e.g., in `from_yaml` at parse time, or in
    /// `neurogrim doctor` lints).
    pub fn validate_with_registry(
        &self,
        registry: &crate::queue_backend::QueueBackendRegistry,
    ) -> Result<()> {
        // Shape checks first (subset of validate()).
        self.validate()?;

        for (topic, raw) in &self.topics {
            let backend = raw
                .backend
                .clone()
                .unwrap_or_else(|| DEFAULT_BACKEND.to_string());
            let ack_required = raw.ack_required.unwrap_or(false);

            let factory = registry.get(&backend).ok_or_else(|| {
                anyhow::anyhow!(
                    "queue-config.yaml: topic {:?} declares backend {:?} \
                     but no factory is registered for that name. \
                     Registered: {:?}",
                    topic,
                    backend,
                    registry.registered_names().copied().collect::<Vec<_>>(),
                )
            })?;

            if ack_required && !factory.supports_ack() {
                anyhow::bail!(
                    "queue-config.yaml: topic {:?} declares \
                     ack_required: true but backend {:?} does not \
                     support ack semantics (factory.supports_ack() = \
                     false). Use a backend that supports ack (e.g., \
                     sqlite, or a third-party ack-capable backend).",
                    topic,
                    backend,
                );
            }
        }
        Ok(())
    }

    /// Resolve the per-topic config for `topic`. When the topic is
    /// not listed, returns [`TopicConfig::default`] (JSONL, 30 days
    /// + 10k messages retention, no ack).
    pub fn lookup(&self, topic: &str) -> TopicConfig {
        match self.topics.get(topic) {
            Some(raw) => {
                let defaults = TopicConfig::default();
                TopicConfig {
                    backend: raw.backend.clone().unwrap_or(defaults.backend),
                    retention_days: raw.retention_days.or(defaults.retention_days),
                    retention_messages: raw
                        .retention_messages
                        .or(defaults.retention_messages),
                    ack_required: raw.ack_required.unwrap_or(defaults.ack_required),
                }
            }
            None => TopicConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn parses_minimal_valid_config() {
        let yaml = r#"schema_version: "1""#;
        let cfg = QueueConfig::from_yaml(yaml).unwrap();
        assert_eq!(cfg.schema_version, "1");
        assert!(cfg.topics.is_empty());
    }

    #[test]
    fn parses_topic_overrides() {
        let yaml = r#"
schema_version: "1"
topics:
  pc-state/alerts:
    backend: sqlite
    retention_messages: 5000
    ack_required: true
  _neurogrim/notifications:
    retention_days: 14
"#;
        let cfg = QueueConfig::from_yaml(yaml).unwrap();
        assert_eq!(cfg.topics.len(), 2);
        let alerts = cfg.lookup("pc-state/alerts");
        assert_eq!(alerts.backend, "sqlite");
        assert_eq!(alerts.retention_messages, Some(5000));
        // retention_days unspecified → falls back to default (30).
        assert_eq!(alerts.retention_days, Some(30));
        assert!(alerts.ack_required);
    }

    #[test]
    fn unspecified_topic_returns_defaults() {
        let yaml = r#"schema_version: "1""#;
        let cfg = QueueConfig::from_yaml(yaml).unwrap();
        let unknown = cfg.lookup("scratch");
        assert_eq!(unknown.backend, "jsonl");
        assert_eq!(unknown.retention_days, Some(30));
        assert_eq!(unknown.retention_messages, Some(10_000));
        assert!(!unknown.ack_required);
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let yaml = r#"schema_version: "2""#;
        let err = QueueConfig::from_yaml(yaml).unwrap_err().to_string();
        assert!(err.contains("schema_version"));
    }

    /// V5-MOD-3 Phase 3 (2026-05-02 — Fork B's 🔴 B2): the
    /// cross-field invariant `ack_required ⇒ ack-capable backend`
    /// moved from `validate()` (shape-only, runs in `from_yaml`)
    /// to `validate_with_registry()` (registry-aware). Test now
    /// shape-parses successfully but fails registry validation.
    #[test]
    fn rejects_ack_required_with_jsonl() {
        let yaml = r#"
schema_version: "1"
topics:
  pc-state/alerts:
    backend: jsonl
    ack_required: true
"#;
        // Shape-only `from_yaml` accepts the YAML (V5-MOD-3 split).
        let cfg = QueueConfig::from_yaml(yaml).expect("shape-parse must succeed");

        // Registry-aware validation rejects the ack-on-jsonl combo.
        let mut registry = crate::queue_backend::QueueBackendRegistry::new();
        registry.register_all(crate::queue_backend::built_in_factories());
        let err = cfg.validate_with_registry(&registry).unwrap_err().to_string();
        assert!(err.contains("ack_required"), "expected ack_required in error: {err}");
        assert!(err.contains("ack"), "expected ack-related guidance: {err}");
    }

    #[test]
    fn rejects_unknown_fields_in_topic() {
        // deny_unknown_fields prevents typos like "rention_days" from
        // silently being ignored (loud on failure beats silent on
        // typo).
        let yaml = r#"
schema_version: "1"
topics:
  pc-state/alerts:
    backend: sqlite
    rention_days: 30
"#;
        let err = QueueConfig::from_yaml(yaml).unwrap_err();
        assert!(format!("{err:#}").contains("rention_days") || format!("{err:#}").contains("unknown field"));
    }

    #[test]
    fn rejects_unknown_fields_at_top_level() {
        let yaml = r#"
schema_version: "1"
unknown_top_level: true
"#;
        let err = QueueConfig::from_yaml(yaml).unwrap_err();
        assert!(format!("{err:#}").contains("unknown_top_level") || format!("{err:#}").contains("unknown field"));
    }

    #[test]
    fn from_path_returns_none_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let cfg = QueueConfig::from_path(&dir.path().join("absent.yaml")).unwrap();
        assert!(cfg.is_none());
    }

    #[test]
    fn from_path_parses_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("queue-config.yaml");
        fs::write(
            &path,
            r#"schema_version: "1"
topics:
  scratch:
    backend: jsonl
"#,
        )
        .unwrap();
        let cfg = QueueConfig::from_path(&path).unwrap().unwrap();
        assert_eq!(cfg.lookup("scratch").backend, "jsonl");
    }

    #[test]
    fn lookup_overrides_pick_individual_fields() {
        // Overrides preserve unspecified-default fields rather than
        // wiping them — adopters can set retention_messages without
        // implicitly clearing retention_days.
        let yaml = r#"
schema_version: "1"
topics:
  pc-state/alerts:
    retention_messages: 100
"#;
        let cfg = QueueConfig::from_yaml(yaml).unwrap();
        let resolved = cfg.lookup("pc-state/alerts");
        assert_eq!(resolved.retention_messages, Some(100));
        // retention_days falls back to default 30, NOT to None.
        assert_eq!(resolved.retention_days, Some(30));
    }

    #[test]
    fn round_trips_through_serde() {
        let cfg = QueueConfig {
            schema_version: "1".to_string(),
            topics: {
                let mut m = BTreeMap::new();
                m.insert(
                    "pc-state/alerts".to_string(),
                    TopicConfigYaml {
                        backend: Some("sqlite".to_string()),
                        retention_days: None,
                        retention_messages: Some(5000),
                        ack_required: Some(true),
                    },
                );
                m
            },
        };
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let parsed = QueueConfig::from_yaml(&yaml).unwrap();
        assert_eq!(parsed, cfg);
    }
}
