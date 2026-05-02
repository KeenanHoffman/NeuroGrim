//! `A2aSource` — built-in [`neurogrim_core::scoring_source::ScoringSource`]
//! that fetches a child Brain's `AgentOutput` via A2A
//! (V5-MOD-1 Phase 2, 2026-05-02).
//!
//! Verbatim port of the `"a2a"` arm of
//! `neurogrim_mcp::context::load_cmdb_data` plus the
//! `load_a2a_domain` helper (`context.rs:256–270 + 288–348`,
//! pre-V5-MOD-1). Phase 3 of V5-MOD-1 routes the dispatch site
//! through the registry and removes the duplicated helper.
//!
//! # Why this lives in `neurogrim-ecosystem`, not `neurogrim-core`
//!
//! `A2aSource::load` calls [`crate::invoke_child`], which depends
//! on `neurogrim-a2a`. `neurogrim-a2a` already depends on
//! `neurogrim-core`. Putting `A2aSource` in `neurogrim-core`
//! would force a `neurogrim-core → neurogrim-a2a` dep, creating
//! a cycle. Hosting `A2aSource` here keeps the dep graph
//! acyclic.
//!
//! Consuming binaries (e.g., `neurogrim-cli`) register the
//! A2A factory at startup:
//!
//! ```ignore
//! use neurogrim_core::scoring_source::ScoringSourceRegistry;
//! use neurogrim_ecosystem::scoring_source::A2aSourceFactory;
//!
//! let mut registry = ScoringSourceRegistry::with_core_built_ins();
//! registry.register(Box::new(A2aSourceFactory));
//! ```
//!
//! # Wire contract
//!
//! - **`endpoint`** (required): A2A peer base URL
//!   (e.g., `http://127.0.0.1:8421/a2a/v1/`).
//! - **`interface_version`** (optional, default `"1"`): expected
//!   peer agent-output interface version. Pre-flight version
//!   negotiation per spec §6.
//!
//! # Failure modes (all surface as `None`, never panic)
//!
//! - Missing `endpoint` → `None` (silent — it's a config gap, not
//!   a runtime failure).
//! - Bad URL → `None` (warn-logged with the offending string).
//! - Peer unreachable / version mismatch → `None` (warn-logged).
//! - Unparseable `scored_at` timestamp on the peer's response →
//!   `Some(...)` with `updated_at = Utc::now()` (the fetch *was*
//!   synchronous, so "now" is honest if the timestamp is bad).

use crate::invoke_child;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use neurogrim_core::ecosystem::{ChildEntry, ChildTransport};
use neurogrim_core::registry::ScoringSourceConfig;
use neurogrim_core::scoring::CmdbData;
use neurogrim_core::scoring_source::{ScoringSource, ScoringSourceFactory};
use std::path::Path;

/// Stable wire-name for the `a2a` source type.
pub const A2A_SOURCE_TYPE: &str = "a2a";

/// Built-in [`ScoringSource`] that fetches a peer Brain's
/// `AgentOutput` via A2A. Stateless — every `load()` issues a
/// fresh A2A request via [`crate::invoke_child`]. Connection
/// pooling lives inside `neurogrim-a2a`'s `HttpSseTransport`.
pub struct A2aSource;

impl A2aSource {
    /// **Inherent** async load — bypasses `#[async_trait]`
    /// future-boxing for the perf-critical dispatch path. See
    /// `neurogrim_core::scoring_sources::cmdb::CmdbSource::
    /// load_inherent` for rationale (V5-MOD-1 Phase 4-fallback,
    /// 2026-05-02). The trait impl below delegates here.
    pub async fn load_inherent(
        &self,
        domain_key: &str,
        config: &ScoringSourceConfig,
        _project_root: &Path,
    ) -> Option<CmdbData> {
        let endpoint_str = config.endpoint.as_ref()?;
        let endpoint = match url::Url::parse(endpoint_str) {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(
                    "domain {domain_key}: bad A2A endpoint {endpoint_str:?}: {e}"
                );
                return None;
            }
        };
        let interface_version = config
            .interface_version
            .clone()
            .unwrap_or_else(|| "1".to_string());

        let entry = ChildEntry {
            id: domain_key.to_string(),
            display_name: None,
            transport: ChildTransport::A2A {
                a2a_endpoint: endpoint,
                agent_card_url: None,
            },
            depends_on: Vec::new(),
            weight: 1.0,
            interface_version,
            enabled: true,
        };

        // AgentOutput.scored_at is a String (RFC3339); parse into DateTime<Utc>.
        // Fall back to Utc::now() if the peer sent an unparseable timestamp —
        // the fetch itself was synchronous so "now" is honest.
        match invoke_child(&entry).await {
            Ok(agent_output) => {
                let ts = agent_output
                    .scored_at
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now());
                // E-B2-1 reader-fallback: AgentOutput's
                // unified_confidence is left as a no-op for v1
                // (matches the pre-V5-MOD-1 behavior). When the
                // E-B2-1 C6 carry-over lands, this will become
                // Some(agent_output.unified_confidence).
                Some(CmdbData {
                    score: agent_output.score,
                    updated_at: ts,
                    confidence: None,
                })
            }
            Err(e) => {
                tracing::warn!(
                    "domain {domain_key}: A2A fetch failed ({e}); \
                     falling back to no_file_score"
                );
                None
            }
        }
    }
}

#[async_trait]
impl ScoringSource for A2aSource {
    fn source_type_name(&self) -> &'static str {
        A2A_SOURCE_TYPE
    }

    async fn load(
        &self,
        domain_key: &str,
        config: &ScoringSourceConfig,
        project_root: &Path,
    ) -> Option<CmdbData> {
        // Trait impl delegates to the inherent method. See
        // `neurogrim_core::scoring_sources::cmdb::CmdbSource::load`
        // for the rationale (V5-MOD-1 Phase 4-fallback).
        self.load_inherent(domain_key, config, project_root).await
    }
}

/// Factory for [`A2aSource`]. Stateless.
pub struct A2aSourceFactory;

impl ScoringSourceFactory for A2aSourceFactory {
    fn source_type_name(&self) -> &'static str {
        A2A_SOURCE_TYPE
    }
    fn build(&self) -> Box<dyn ScoringSource> {
        Box::new(A2aSource)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn config_with_endpoint(endpoint: Option<&str>) -> ScoringSourceConfig {
        ScoringSourceConfig {
            source_type: A2A_SOURCE_TYPE.to_string(),
            path: None,
            endpoint: endpoint.map(String::from),
            interface_version: None,
            score_field: None,
            updated_at_field: None,
            no_file_score: None,
        }
    }

    #[tokio::test]
    async fn missing_endpoint_returns_none_silently() {
        let result = A2aSource
            .load("test_domain", &config_with_endpoint(None), Path::new("."))
            .await;
        assert!(result.is_none(), "missing endpoint must return None");
    }

    #[tokio::test]
    async fn bad_url_returns_none() {
        let result = A2aSource
            .load(
                "test_domain",
                &config_with_endpoint(Some("not a url")),
                Path::new("."),
            )
            .await;
        assert!(result.is_none(), "malformed URL must return None");
    }

    #[tokio::test]
    async fn unreachable_peer_returns_none() {
        // Use a port that's almost certainly not bound. If a CI
        // host happens to have something on it, the test would
        // hit a different code path — but the contract still
        // holds: A2aSource must return None on any failure.
        let result = A2aSource
            .load(
                "test_domain",
                &config_with_endpoint(Some("http://127.0.0.1:1/")),
                Path::new("."),
            )
            .await;
        assert!(
            result.is_none(),
            "unreachable peer must return None, got {:?}",
            result
        );
    }

    #[test]
    fn factory_source_type_matches_source() {
        let factory = A2aSourceFactory;
        let source = factory.build();
        assert_eq!(factory.source_type_name(), A2A_SOURCE_TYPE);
        assert_eq!(source.source_type_name(), A2A_SOURCE_TYPE);
    }
}
