//! `FunctionSource` — built-in [`crate::scoring_source::ScoringSource`]
//! for the `"function"` source type. **No-op by design.**
//!
//! The `"function"` source type marks a domain whose scoring is
//! computed by an implementation-specific function inside the
//! pipeline (not via a CMDB envelope). Examples: domains that
//! aggregate from other domains (`coherence`), domains that
//! interrogate live state (`docker-topology`), domains whose
//! score is derived in-process from other inputs.
//!
//! For these domains, the [`crate::scoring_source::ScoringSource`]
//! contract has nothing useful to return — the score doesn't
//! come from a `CmdbData` envelope. So this impl always returns
//! `None`. The caller treats `None` as "no envelope; rely on
//! whatever the domain's own scoring logic produces" rather than
//! "no_file_score fallback" — distinguished by the `function`
//! source_type marker upstream.
//!
//! The factory exists so the registry has a known entry for
//! `"function"`. Without it, registry lookups for `"function"`
//! would return `None`, and the dispatch path (Phase 3) would
//! treat that as an unknown source type — incorrectly logging it
//! as a configuration error rather than the intentional no-op
//! it is. Registering this factory is what tells the registry
//! "yes, `function` is a valid source type; it just has nothing
//! to load."

use crate::registry::ScoringSourceConfig;
use crate::scoring::CmdbData;
use crate::scoring_source::{ScoringSource, ScoringSourceFactory};
use async_trait::async_trait;
use std::path::Path;

/// Stable wire-name for the `function` source type.
pub const FUNCTION_SOURCE_TYPE: &str = "function";

/// Built-in [`ScoringSource`] no-op marker for domains scored by
/// implementation-specific functions inside the pipeline.
pub struct FunctionSource;

#[async_trait]
impl ScoringSource for FunctionSource {
    fn source_type_name(&self) -> &'static str {
        FUNCTION_SOURCE_TYPE
    }

    async fn load(
        &self,
        _domain_key: &str,
        _config: &ScoringSourceConfig,
        _project_root: &Path,
    ) -> Option<CmdbData> {
        // Intentional no-op. See module-level docs for the
        // rationale: the `function` source type marks domains
        // whose scoring is computed in-pipeline, not from a
        // CMDB envelope.
        None
    }
}

/// Factory for [`FunctionSource`]. Stateless.
pub struct FunctionSourceFactory;

impl ScoringSourceFactory for FunctionSourceFactory {
    fn source_type_name(&self) -> &'static str {
        FUNCTION_SOURCE_TYPE
    }
    fn build(&self) -> Box<dyn ScoringSource> {
        Box::new(FunctionSource)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn empty_config() -> ScoringSourceConfig {
        ScoringSourceConfig {
            source_type: FUNCTION_SOURCE_TYPE.to_string(),
            path: None,
            endpoint: None,
            interface_version: None,
            score_field: None,
            updated_at_field: None,
            no_file_score: None,
        }
    }

    #[tokio::test]
    async fn always_returns_none() {
        let result = FunctionSource
            .load("any_domain", &empty_config(), Path::new("."))
            .await;
        assert!(
            result.is_none(),
            "FunctionSource is a no-op marker; load must always return None"
        );
    }

    #[tokio::test]
    async fn config_fields_are_ignored() {
        // Even with a fully-populated config, FunctionSource still
        // returns None — it's a marker, not a reader.
        let config = ScoringSourceConfig {
            source_type: FUNCTION_SOURCE_TYPE.to_string(),
            path: Some("ignored.json".to_string()),
            endpoint: Some("http://example.com/".to_string()),
            interface_version: Some("1".to_string()),
            score_field: Some("score".to_string()),
            updated_at_field: Some("updated_at".to_string()),
            no_file_score: Some(50),
        };
        let result = FunctionSource
            .load("any_domain", &config, Path::new("."))
            .await;
        assert!(result.is_none());
    }

    #[test]
    fn factory_source_type_matches_source() {
        let factory = FunctionSourceFactory;
        let source = factory.build();
        assert_eq!(factory.source_type_name(), FUNCTION_SOURCE_TYPE);
        assert_eq!(source.source_type_name(), FUNCTION_SOURCE_TYPE);
    }
}
