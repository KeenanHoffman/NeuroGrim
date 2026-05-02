//! # `scoring-source-prom` — V5-MOD-1 third-party example
//!
//! This crate demonstrates the modularity claim of V5-MOD-1: a
//! third-party crate can ship a custom
//! [`neurogrim_core::scoring_source::ScoringSource`] impl that
//! plugs into NeuroGrim's scoring pipeline **without forking
//! `neurogrim-core` or `neurogrim-mcp`**. It depends only on the
//! public contract surface in `neurogrim-core`, registers itself
//! at startup via the consuming binary's
//! [`neurogrim_core::scoring_source::ScoringSourceRegistry`], and
//! passes the Phase 5 conformance suite shipped with
//! `neurogrim-core`.
//!
//! ## What it does
//!
//! [`PromSource`] reads a Prometheus instant-query endpoint and
//! converts the resulting scalar value (clamped 0–100) into a
//! [`neurogrim_core::scoring::CmdbData`] envelope. The wire
//! contract:
//!
//! - **`endpoint`** (required): Prometheus query API URL, e.g.
//!   `http://prom.example.com/api/v1/query`.
//! - **`path`** (required): the PromQL expression to evaluate,
//!   e.g. `up{job="api"}`. This crate repurposes the `path`
//!   field — which built-in `cmdb` sources use as a relative
//!   filesystem path — to carry the PromQL query string. This
//!   is intentional: `ScoringSourceConfig` is a closed shape
//!   shared by all source types, and re-using `path` for a
//!   string-shaped query parameter avoids needing schema
//!   changes for every new source type. Document the
//!   convention in the source's `README.md`.
//!
//! ## Failure modes (all surface as `None`, never panic)
//!
//! Same discipline as the built-in `A2aSource`:
//!
//! - Missing `endpoint` or `path` → `None` silently (config gap).
//! - Bad URL → `None` (warn-logged with the offending string).
//! - Unreachable peer / non-2xx HTTP → `None` (warn-logged).
//! - Malformed Prometheus response → `None` (warn-logged).
//! - Empty result vector → `None` (no scalar to score from).
//! - Unparseable scalar value → `None` (warn-logged).
//!
//! ## How a consuming binary registers the source
//!
//! ```ignore
//! use neurogrim_core::scoring_source::ScoringSourceRegistry;
//! use scoring_source_prom::PromSourceFactory;
//!
//! let mut registry = ScoringSourceRegistry::with_core_built_ins();
//! registry.register(Box::new(PromSourceFactory));
//! // … hand `registry` to the dispatch site (V5-MOD-1 Phase 3
//! // converged dispatcher in `neurogrim-mcp`).
//! ```
//!
//! Once registered, a `brain-registry.json` domain entry like
//! the following routes through `PromSource`:
//!
//! ```json
//! {
//!   "scoring_source": {
//!     "type": "prom",
//!     "endpoint": "http://prom.example.com/api/v1/query",
//!     "path": "avg(node_load1{job=\"api\"})"
//!   }
//! }
//! ```
//!
//! ## Conformance
//!
//! The integration test at `tests/conformance.rs` runs the
//! cross-crate suite from
//! [`neurogrim_core::scoring_source_conformance`] against
//! `PromSourceFactory`. Third-party plugin authors should copy
//! that test into their own crate as the canonical contract
//! check. If it passes, the impl honors the negative-path
//! discipline (no panics, never deadlocks, idempotent on identical
//! input, fast-fails on skeletal config) that every built-in
//! source satisfies.

use async_trait::async_trait;
use chrono::Utc;
use neurogrim_core::registry::ScoringSourceConfig;
use neurogrim_core::scoring::CmdbData;
use neurogrim_core::scoring_source::{ScoringSource, ScoringSourceFactory};
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

/// Stable wire-name for the `prom` source type. Must match the
/// `type` field in the consuming binary's `brain-registry.json`
/// domain entries that should route through this source.
pub const PROM_SOURCE_TYPE: &str = "prom";

/// Default per-request HTTP timeout. A Prometheus instant query
/// against a healthy server returns within tens of milliseconds;
/// a 5-second ceiling means a sluggish server fast-fails to
/// `None` rather than blocking the scoring pipeline. Same posture
/// as `A2aSource`'s implicit timeout via `reqwest`'s defaults.
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

/// Third-party [`ScoringSource`] that fetches a Prometheus
/// instant-query result and converts the scalar value into a
/// [`CmdbData`] score (clamped 0–100). Stateless — every
/// `load()` call issues a fresh HTTP request.
///
/// A production-quality impl would cache the `reqwest::Client` on
/// the factory side (the factory is the natural amortization
/// point per the V5-MOD-1 trait split). This example builds a
/// fresh client per call for readability; the perf delta is
/// negligible for typical scoring cadences (<1 Hz).
pub struct PromSource;

impl PromSource {
    /// **Inherent** async load — bypasses `#[async_trait]`'s
    /// future-boxing on the perf-critical dispatch path. See
    /// `neurogrim_core::scoring_sources::cmdb::CmdbSource::load_inherent`
    /// for the rationale (V5-MOD-1 Phase 4-fallback). The trait
    /// impl below delegates here.
    ///
    /// Third-party authors who care about the same perf concern
    /// should follow this two-method pattern; otherwise a single
    /// `async fn load(...)` directly on the trait is fine — the
    /// boxing cost is real but small (~50ns + one allocation
    /// per call).
    pub async fn load_inherent(
        &self,
        domain_key: &str,
        config: &ScoringSourceConfig,
        _project_root: &Path,
    ) -> Option<CmdbData> {
        // Config-gap fast-fails (no warn — it's not a runtime
        // failure, just an unconfigured domain).
        let endpoint_str = config.endpoint.as_ref()?;
        let query = config.path.as_ref()?;

        // Parse the URL up front. A bad URL is a config error
        // worth warn-logging.
        let endpoint = match url::Url::parse(endpoint_str) {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(
                    "domain {domain_key}: prom endpoint {endpoint_str:?} unparseable: {e}"
                );
                return None;
            }
        };

        // Build the HTTP client with the per-request timeout
        // baked in. For a production impl, cache this on the
        // factory and reuse across calls.
        let client = match reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                // reqwest::Client::builder().build() failures are
                // rare (TLS init issues, etc.); warn and skip.
                tracing::warn!(
                    "domain {domain_key}: prom client build failed: {e}"
                );
                return None;
            }
        };

        let response = match client
            .get(endpoint)
            .query(&[("query", query.as_str())])
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    "domain {domain_key}: prom request to {endpoint_str:?} failed: {e}"
                );
                return None;
            }
        };

        if !response.status().is_success() {
            tracing::warn!(
                "domain {domain_key}: prom HTTP {} from {endpoint_str:?}",
                response.status()
            );
            return None;
        }

        let body: PromInstantQueryResponse = match response.json().await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    "domain {domain_key}: prom response from {endpoint_str:?} \
                     not parseable as Prometheus instant-query JSON: {e}"
                );
                return None;
            }
        };

        if body.status != "success" {
            tracing::warn!(
                "domain {domain_key}: prom returned non-success status: {:?}",
                body.status
            );
            return None;
        }

        // Take the first vector entry. Prometheus instant queries
        // can return multi-element vectors when the PromQL
        // expression matches multiple series; aggregating across
        // them is a domain-design concern — this example takes
        // the first element and warns if there are more.
        if body.data.result.len() > 1 {
            tracing::warn!(
                "domain {domain_key}: prom query {query:?} returned {} \
                 series; using the first. Consider an aggregating PromQL \
                 (avg / max / etc.) for deterministic single-value scores.",
                body.data.result.len()
            );
        }
        let result = body.data.result.first()?;

        // Prometheus values are JSON tuples [unix_ts: f64, value: String].
        let value_str = &result.value.1;
        let value: f64 = match value_str.parse() {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    "domain {domain_key}: prom value {value_str:?} not parseable as f64: {e}"
                );
                return None;
            }
        };

        // Clamp to [0, 100] and round to u8. A NaN value would
        // pass through `clamp` unchanged (NaN is propagated by
        // `f64::clamp`), so an explicit `is_finite` check guards
        // against malformed Prometheus output.
        if !value.is_finite() {
            tracing::warn!(
                "domain {domain_key}: prom value {value_str:?} is non-finite (NaN or ±Inf)"
            );
            return None;
        }
        let score = value.clamp(0.0, 100.0).round() as u8;

        // `updated_at` is "now" — Prometheus instant queries are
        // point-in-time, and the response timestamp is when the
        // server evaluated the query. Either choice is defensible;
        // `Utc::now()` matches the A2A source's posture for
        // unparseable peer timestamps.
        Some(CmdbData {
            score,
            updated_at: Utc::now(),
            confidence: None,
        })
    }
}

#[async_trait]
impl ScoringSource for PromSource {
    fn source_type_name(&self) -> &'static str {
        PROM_SOURCE_TYPE
    }

    async fn load(
        &self,
        domain_key: &str,
        config: &ScoringSourceConfig,
        project_root: &Path,
    ) -> Option<CmdbData> {
        // Trait impl delegates to the inherent method. See the
        // module-level docs + CmdbSource for the two-method
        // rationale (V5-MOD-1 Phase 4-fallback).
        self.load_inherent(domain_key, config, project_root).await
    }
}

/// Factory for [`PromSource`]. Stateless — `build()` returns a
/// fresh `Box::new(PromSource)` every call.
///
/// A production impl would cache a `reqwest::Client` here so the
/// connection pool persists across the registry-held factory's
/// lifetime. For this example the source is built fresh per call;
/// see the inherent-load comment above.
pub struct PromSourceFactory;

impl ScoringSourceFactory for PromSourceFactory {
    fn source_type_name(&self) -> &'static str {
        PROM_SOURCE_TYPE
    }

    fn build(&self) -> Box<dyn ScoringSource> {
        Box::new(PromSource)
    }
}

// ────────────────────────────────────────────────────────────────
// Prometheus instant-query response shape
// ────────────────────────────────────────────────────────────────
//
// See https://prometheus.io/docs/prometheus/latest/querying/api/
// — the canonical schema. We deserialize the minimum we need:
// `status`, `data.result[*].value`. Other fields (resultType,
// metric labels, warnings) are ignored — `serde` defaults to
// "skip unknown" without `deny_unknown_fields`.

#[derive(Debug, Deserialize)]
struct PromInstantQueryResponse {
    status: String,
    data: PromData,
}

#[derive(Debug, Deserialize)]
struct PromData {
    result: Vec<PromInstantResult>,
}

#[derive(Debug, Deserialize)]
struct PromInstantResult {
    /// Prometheus serializes the tuple as `[1234567890.0, "42"]`.
    /// The first element is a Unix timestamp (f64); the second is
    /// the value, **always a string** in the Prometheus wire format
    /// (this is intentional, to avoid lossy JSON-number conversion
    /// for full-precision floats).
    value: (f64, String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn config_with(endpoint: Option<&str>, query: Option<&str>) -> ScoringSourceConfig {
        ScoringSourceConfig {
            source_type: PROM_SOURCE_TYPE.to_string(),
            path: query.map(String::from),
            endpoint: endpoint.map(String::from),
            interface_version: None,
            score_field: None,
            updated_at_field: None,
            no_file_score: None,
        }
    }

    #[tokio::test]
    async fn missing_endpoint_returns_none_silently() {
        let result = PromSource
            .load(
                "test_domain",
                &config_with(None, Some("up")),
                Path::new("."),
            )
            .await;
        assert!(result.is_none(), "missing endpoint must return None");
    }

    #[tokio::test]
    async fn missing_query_returns_none_silently() {
        let result = PromSource
            .load(
                "test_domain",
                &config_with(Some("http://prom.example.com/api/v1/query"), None),
                Path::new("."),
            )
            .await;
        assert!(result.is_none(), "missing path/query must return None");
    }

    #[tokio::test]
    async fn bad_url_returns_none() {
        let result = PromSource
            .load(
                "test_domain",
                &config_with(Some("not a url"), Some("up")),
                Path::new("."),
            )
            .await;
        assert!(result.is_none(), "malformed URL must return None");
    }

    #[tokio::test]
    async fn unreachable_peer_returns_none() {
        // Port 1 is almost never bound. If a CI host happens to
        // have something on it, the test still validates the
        // contract: PromSource must return None on any failure.
        let result = PromSource
            .load(
                "test_domain",
                &config_with(Some("http://127.0.0.1:1/api/v1/query"), Some("up")),
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
        let factory = PromSourceFactory;
        let source = factory.build();
        assert_eq!(factory.source_type_name(), PROM_SOURCE_TYPE);
        assert_eq!(source.source_type_name(), PROM_SOURCE_TYPE);
    }

    #[test]
    fn prom_response_shape_round_trips() {
        // Sanity check the deserializer against a real Prometheus
        // instant-query response. Captured from the Prom docs.
        let raw = r#"{
            "status": "success",
            "data": {
                "resultType": "vector",
                "result": [
                    {
                        "metric": {"__name__": "up", "job": "prometheus"},
                        "value": [1435781451.781, "1"]
                    }
                ]
            }
        }"#;
        let parsed: PromInstantQueryResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.status, "success");
        assert_eq!(parsed.data.result.len(), 1);
        assert_eq!(parsed.data.result[0].value.1, "1");
    }
}
