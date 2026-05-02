//! `CmdbSource` — built-in [`crate::scoring_source::ScoringSource`]
//! that reads a JSON CMDB file under the project root.
//!
//! Verbatim port of the `"cmdb"` arm of
//! `neurogrim_mcp::context::load_cmdb_data` (`context.rs:218–254`,
//! pre-V5-MOD-1) and the duplicated cmdb-only branch in
//! `neurogrim_mcp::server::load_cmdb_from_disk` (`server.rs:75`).
//! Phase 3 of V5-MOD-1 will route both call sites through the
//! factory registry, eliminating the duplication.
//!
//! # Wire contract
//!
//! - **`path`** (required): relative path to the CMDB JSON file
//!   under the project root. The reader resolves
//!   `<project_root>/<path>`.
//! - **`score_field`** (optional, default `"score"`): JSON key
//!   that holds the integer score (0–100). Clamped to 100.
//! - **`updated_at_field`** (optional, default `"updated_at"`):
//!   JSON key for the ISO 8601 timestamp.
//! - **`confidence`** (optional CMDB-envelope field): if present,
//!   takes precedence over age-decay (E-B2-1, spec §3.8). When
//!   absent, the aggregator falls back to `exponential_decay`.
//!
//! # Failure modes (all surface as `None`, never panic)
//!
//! - Missing `path` in config → `None` (warn-logged).
//! - File doesn't exist or unreadable → `None` (no log; common
//!   case of "first run, no CMDB written yet").
//! - Malformed JSON → `None` (warn-logged).
//! - Missing required field (`score_field` or `updated_at_field`)
//!   → `None` (warn-logged).
//! - Unparseable timestamp → `None` (warn-logged).
//! - UTF-8 BOM prefix → silently stripped (PowerShell convention).

use crate::registry::ScoringSourceConfig;
use crate::scoring::CmdbData;
use crate::scoring_source::{ScoringSource, ScoringSourceFactory};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;

/// Stable wire-name for the `cmdb` source type. Matches the
/// `source_type` string in `brain-registry.json` domain
/// definitions.
pub const CMDB_SOURCE_TYPE: &str = "cmdb";

/// Built-in [`ScoringSource`] that reads a JSON CMDB file under
/// the project root. Stateless — every `load()` call is a fresh
/// disk read. The factory's `build()` is essentially
/// `Box::new(CmdbSource)` — no per-source state to amortize.
pub struct CmdbSource;

impl CmdbSource {
    /// **Inherent** async load — bypasses `#[async_trait]`'s
    /// `Pin<Box<dyn Future>>` boxing for the perf-critical
    /// dispatch path. Called directly by the
    /// `BuiltinScoringSource` enum's match arm in
    /// `neurogrim-mcp::scoring_source_registry`. The trait impl
    /// below delegates here so trait-based callers (third-party
    /// plugin code) get the same behavior, paying the boxing
    /// cost only when actually using `Box<dyn ScoringSource>`.
    ///
    /// V5-MOD-1 Phase 4-fallback (2026-05-02): introduced after
    /// the perf-gate failure (`p95_ms ≤ 19` exceeded by ~6 ms;
    /// `#[async_trait]` future-boxing identified as the dominant
    /// cause across 19 domains × scoring run).
    pub async fn load_inherent(
        &self,
        domain_key: &str,
        config: &ScoringSourceConfig,
        project_root: &Path,
    ) -> Option<CmdbData> {
        let path = match config.path.as_ref() {
            Some(p) => p,
            None => {
                tracing::warn!(
                    "domain {domain_key}: cmdb source has no `path` field; skipping"
                );
                return None;
            }
        };

        let full = project_root.join(path);
        // Sync I/O inside async fn matches `calibration_ledger.rs`'s
        // pattern (neurogrim-core has no runtime tokio dep — sync
        // file read is fine for CMDB-sized files, microseconds).
        let raw = match fs::read_to_string(&full) {
            Ok(s) => s,
            // Common case: file doesn't exist yet (first run).
            // Caller falls through to no_file_score; no log noise.
            Err(_) => return None,
        };

        // Strip UTF-8 BOM if present. PowerShell writes BOM with
        // `-Encoding UTF8` by default, which serde_json rejects
        // unless we strip first. Same posture as the prior
        // `load_cmdb_data` impl in context.rs.
        let raw = raw.trim_start_matches('\u{FEFF}');

        let cmdb: serde_json::Value = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    "domain {domain_key}: cmdb {} malformed JSON: {e}",
                    full.display()
                );
                return None;
            }
        };

        let sf = config.score_field.as_deref().unwrap_or("score");
        let uf = config.updated_at_field.as_deref().unwrap_or("updated_at");

        let score = match cmdb.get(sf).and_then(|v| v.as_u64()) {
            Some(n) => n,
            None => {
                tracing::warn!(
                    "domain {domain_key}: cmdb {} missing or non-integer field '{sf}'",
                    full.display()
                );
                return None;
            }
        };
        let ts_str = match cmdb.get(uf).and_then(|v| v.as_str()) {
            Some(s) => s,
            None => {
                tracing::warn!(
                    "domain {domain_key}: cmdb {} missing or non-string field '{uf}'",
                    full.display()
                );
                return None;
            }
        };
        let ts = match ts_str.parse::<DateTime<Utc>>() {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    "domain {domain_key}: cmdb {} unparseable {uf}={ts_str:?}: {e}",
                    full.display()
                );
                return None;
            }
        };

        // Optional envelope-supplied confidence (E-B2-1, spec §3.8).
        // When present, takes precedence over age-decay. When absent,
        // the aggregator falls back to `exponential_decay(updated_at,...)`.
        let confidence = cmdb
            .get("confidence")
            .and_then(|v| v.as_u64())
            .map(|n| n.min(100) as u8);

        Some(CmdbData {
            score: score.min(100) as u8,
            updated_at: ts,
            confidence,
        })
    }
}

#[async_trait]
impl ScoringSource for CmdbSource {
    fn source_type_name(&self) -> &'static str {
        CMDB_SOURCE_TYPE
    }

    async fn load(
        &self,
        domain_key: &str,
        config: &ScoringSourceConfig,
        project_root: &Path,
    ) -> Option<CmdbData> {
        // Trait impl delegates to the inherent method. Trait-based
        // callers (third-party plugin code via Box<dyn>) pay the
        // future-boxing cost; the BuiltinScoringSource enum in
        // neurogrim-mcp calls load_inherent directly to bypass it.
        self.load_inherent(domain_key, config, project_root).await
    }
}

/// Factory for [`CmdbSource`]. Stateless — `build()` returns a
/// fresh `Box::new(CmdbSource)` every call.
pub struct CmdbSourceFactory;

impl ScoringSourceFactory for CmdbSourceFactory {
    fn source_type_name(&self) -> &'static str {
        CMDB_SOURCE_TYPE
    }
    fn build(&self) -> Box<dyn ScoringSource> {
        Box::new(CmdbSource)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn config_with_path(p: &str) -> ScoringSourceConfig {
        ScoringSourceConfig {
            source_type: CMDB_SOURCE_TYPE.to_string(),
            path: Some(p.to_string()),
            endpoint: None,
            interface_version: None,
            score_field: None,
            updated_at_field: None,
            no_file_score: None,
        }
    }

    #[tokio::test]
    async fn happy_path_reads_score_and_timestamp() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("cmdb.json"),
            r#"{"score": 75, "updated_at": "2026-05-02T00:00:00Z"}"#,
        )
        .unwrap();
        let result = CmdbSource
            .load("test_domain", &config_with_path("cmdb.json"), dir.path())
            .await
            .expect("happy path must return Some");
        assert_eq!(result.score, 75);
        assert_eq!(result.confidence, None);
        assert_eq!(
            result.updated_at,
            "2026-05-02T00:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
    }

    #[tokio::test]
    async fn happy_path_with_confidence_envelope() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("cmdb.json"),
            r#"{"score": 60, "updated_at": "2026-05-02T00:00:00Z", "confidence": 80}"#,
        )
        .unwrap();
        let result = CmdbSource
            .load("test_domain", &config_with_path("cmdb.json"), dir.path())
            .await
            .unwrap();
        assert_eq!(result.confidence, Some(80));
    }

    #[tokio::test]
    async fn missing_file_returns_none_without_panicking() {
        let dir = TempDir::new().unwrap();
        // No file written.
        let result = CmdbSource
            .load(
                "test_domain",
                &config_with_path("does-not-exist.json"),
                dir.path(),
            )
            .await;
        assert!(result.is_none(), "missing file must NOT error or panic");
    }

    #[tokio::test]
    async fn malformed_json_returns_none() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("cmdb.json"), "this is not json").unwrap();
        let result = CmdbSource
            .load("test_domain", &config_with_path("cmdb.json"), dir.path())
            .await;
        assert!(result.is_none(), "malformed JSON must return None");
    }

    #[tokio::test]
    async fn missing_score_field_returns_none() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("cmdb.json"),
            r#"{"updated_at": "2026-05-02T00:00:00Z"}"#,
        )
        .unwrap();
        let result = CmdbSource
            .load("test_domain", &config_with_path("cmdb.json"), dir.path())
            .await;
        assert!(result.is_none(), "missing score must return None");
    }

    #[tokio::test]
    async fn missing_updated_at_field_returns_none() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("cmdb.json"), r#"{"score": 75}"#).unwrap();
        let result = CmdbSource
            .load("test_domain", &config_with_path("cmdb.json"), dir.path())
            .await;
        assert!(result.is_none(), "missing updated_at must return None");
    }

    #[tokio::test]
    async fn missing_path_in_config_returns_none() {
        let dir = TempDir::new().unwrap();
        let mut config = config_with_path("ignored");
        config.path = None;
        let result = CmdbSource.load("test_domain", &config, dir.path()).await;
        assert!(result.is_none(), "missing path must return None");
    }

    #[tokio::test]
    async fn bom_prefixed_file_is_handled() {
        let dir = TempDir::new().unwrap();
        let mut content = String::from('\u{FEFF}'); // UTF-8 BOM
        content.push_str(r#"{"score": 50, "updated_at": "2026-05-02T00:00:00Z"}"#);
        fs::write(dir.path().join("cmdb.json"), content).unwrap();
        let result = CmdbSource
            .load("test_domain", &config_with_path("cmdb.json"), dir.path())
            .await
            .expect("BOM must be silently stripped");
        assert_eq!(result.score, 50);
    }

    #[tokio::test]
    async fn custom_score_field_name_is_honored() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("cmdb.json"),
            r#"{"my_score": 42, "my_ts": "2026-05-02T00:00:00Z"}"#,
        )
        .unwrap();
        let mut config = config_with_path("cmdb.json");
        config.score_field = Some("my_score".to_string());
        config.updated_at_field = Some("my_ts".to_string());
        let result = CmdbSource
            .load("test_domain", &config, dir.path())
            .await
            .unwrap();
        assert_eq!(result.score, 42);
    }

    #[tokio::test]
    async fn unparseable_timestamp_returns_none() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("cmdb.json"),
            r#"{"score": 75, "updated_at": "not a timestamp"}"#,
        )
        .unwrap();
        let result = CmdbSource
            .load("test_domain", &config_with_path("cmdb.json"), dir.path())
            .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn score_above_100_clamped() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("cmdb.json"),
            r#"{"score": 999, "updated_at": "2026-05-02T00:00:00Z"}"#,
        )
        .unwrap();
        let result = CmdbSource
            .load("test_domain", &config_with_path("cmdb.json"), dir.path())
            .await
            .unwrap();
        assert_eq!(result.score, 100, "score must be clamped to 100");
    }

    #[test]
    fn factory_source_type_matches_source() {
        let factory = CmdbSourceFactory;
        let source = factory.build();
        assert_eq!(factory.source_type_name(), CMDB_SOURCE_TYPE);
        assert_eq!(source.source_type_name(), CMDB_SOURCE_TYPE);
    }
}
