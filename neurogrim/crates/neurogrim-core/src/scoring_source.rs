//! `ScoringSource` trait — the pluggable contract for loading a
//! domain's scoring data (V5-MOD-1, 2026-05-02).
//!
//! Replaces the string-dispatch in
//! [`neurogrim-mcp::context::load_cmdb_data`] (`context.rs:218`)
//! and the duplicate `cmdb`-only branch in
//! `neurogrim-mcp::server::load_cmdb_from_disk` (`server.rs:75`).
//! Both sites are converged through the trait + factory registry
//! in V5-MOD-1 Phase 3.
//!
//! # The two traits, and why both
//!
//! - [`ScoringSource`] — the source itself. Object-safe so we can
//!   dispatch via `Box<dyn ScoringSource>`. Carries the load
//!   logic for one source-type (e.g., `CmdbSource` reads a JSON
//!   file under the project root; `A2aSource` fetches a peer
//!   Brain's `AgentOutput` over HTTP).
//! - [`ScoringSourceFactory`] — produces a `ScoringSource` impl
//!   for a given `source_type` string. The registry holds
//!   factories (one per known source-type); resolving a domain's
//!   config to a working source is `registry.get(source_type) →
//!   factory.build() → Box<dyn ScoringSource>`.
//!
//! Splitting build-from-config and load lets factories carry
//! per-source initialization that's expensive to redo per call
//! (HTTP clients, connection pools) without conflating that with
//! the call-time `load(...)` contract.
//!
//! # Object-safety + async
//!
//! The trait uses `#[async_trait]` (matches the workspace's
//! existing `Transport` trait convention; promoted to
//! `workspace.dependencies` in V5-MOD-1 Phase 0). Object-safety
//! is empirically proven by the existing
//! `Box<dyn Transport>` dispatch in production
//! (`neurogrim-cli/src/commands/queue.rs:2`) and by the
//! `_object_safety_check` tests at the bottom of this module.
//!
//! # Performance — the 🔴 V5-MOD-1 perf gate
//!
//! `Box<dyn ScoringSource>` dispatch on the hot scoring path is
//! the performance risk the v5 master roadmap calls out as
//! 🔴 BLOCKING. The V5-FOUND-1 baseline
//! (`roadmap/data/v5-scoring-baseline-2026-05-02.json`) sets the
//! ceiling: scoring round-trip `p95_ms ≤ 19`. V5-MOD-1 Phase 4
//! verifies the gate. If it fails, the fallback design (generic-
//! bounded with a small enum for built-ins) is in the V5-MOD-1
//! plan at `.claude/plans/v5-mod-1-scoring-source-trait.md` §
//! Risks.

use crate::registry::ScoringSourceConfig;
use crate::scoring::CmdbData;
use async_trait::async_trait;
use std::path::Path;

/// Pluggable contract for loading a domain's scoring data.
///
/// Implementations correspond 1:1 to the `source_type` strings
/// declared in `brain-registry.json` domain definitions
/// (`cmdb`, `a2a`, `function`, plus any third-party-registered
/// types). Each impl knows how to fetch its data and convert it
/// to the [`CmdbData`] envelope the scoring pipeline consumes.
///
/// # Contract
///
/// - **`source_type_name`** — returns the `&'static str` that
///   matches the wire-format `source_type` field. Stable across
///   the lifetime of the impl.
/// - **`load`** — fetches the domain's scoring data. Returns
///   `None` if the source is unreachable / missing / malformed —
///   caller falls through to `no_file_score` semantics. The
///   contract MUST NOT panic; errors are logged at warn / debug
///   level and surfaced as `None`. This matches v4.x behavior;
///   third-party impls are expected to honor it.
///
/// # Object-safety
///
/// The trait is object-safe (`Box<dyn ScoringSource>`). The
/// `_object_safety_check` test in this module fails to compile
/// if a future change accidentally breaks object-safety.
#[async_trait]
pub trait ScoringSource: Send + Sync {
    /// Stable wire-name matching `ScoringSourceConfig::source_type`.
    /// Used by the factory registry for dispatch.
    fn source_type_name(&self) -> &'static str;

    /// Load this domain's scoring data, or return `None` if the
    /// source is unreachable / missing / malformed. MUST NOT
    /// panic. Errors are logged at warn level by the impl and
    /// surfaced as `None`.
    ///
    /// Args:
    /// - `domain_key`: the registry key for the domain being
    ///   loaded (used in tracing breadcrumbs).
    /// - `config`: the registry's
    ///   [`crate::registry::ScoringSourceConfig`] for this domain.
    ///   The trait impl reads only the fields relevant to its
    ///   `source_type` (e.g., `CmdbSource` reads `path` +
    ///   `score_field` + `updated_at_field`; `A2aSource` reads
    ///   `endpoint` + `interface_version`).
    /// - `project_root`: the project root path for resolving
    ///   relative `path` values (CMDB sources read files under
    ///   this root).
    async fn load(
        &self,
        domain_key: &str,
        config: &ScoringSourceConfig,
        project_root: &Path,
    ) -> Option<CmdbData>;
}

/// Factory: produces a [`ScoringSource`] impl for a given
/// `source_type`. The registry holds factories (one per known
/// source-type); a third-party crate registers its own factory
/// at startup to make a new source-type available.
///
/// Factories are themselves `Send + Sync` so the registry can be
/// shared across the scoring pipeline's tokio runtime without
/// `Arc<Mutex>` ceremony.
///
/// # Why a factory and not just direct registration of
/// `Box<dyn ScoringSource>`?
///
/// Factories let an impl carry per-source initialization that's
/// expensive to redo per call (HTTP clients, connection pools,
/// configuration parsing). The dispatch path is
/// `registry.get(source_type) → factory.build() →
/// Box<dyn ScoringSource>`; the factory amortizes setup, the
/// resulting `ScoringSource` does the per-call work.
///
/// For built-in sources where the source impl is stateless, the
/// factory's `build()` is essentially `Box::new(MySource)` — no
/// overhead. For sources that hold heavy state (an HTTP client,
/// for example), the factory can cache.
pub trait ScoringSourceFactory: Send + Sync {
    /// Stable wire-name. Must match the `source_type_name()` of
    /// the [`ScoringSource`] impls this factory builds.
    fn source_type_name(&self) -> &'static str;

    /// Construct a new [`ScoringSource`] impl. Called by the
    /// dispatch path once per scoring run (or once at startup +
    /// cached, depending on factory implementation).
    fn build(&self) -> Box<dyn ScoringSource>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-only test: the trait MUST be object-safe so it can
    /// be dispatched via `Box<dyn ScoringSource>`. If a future
    /// change accidentally breaks object-safety (e.g., adding a
    /// `Self: Sized` method, a generic non-dispatchable method,
    /// or a method returning `Self`), this function fails to
    /// compile with a clear error.
    #[allow(dead_code)]
    fn _object_safety_check_scoring_source(_: Box<dyn ScoringSource>) {
        // Body intentionally empty; the compile-time check is
        // the assertion. The dispatch is what V5-MOD-1's
        // architecture depends on.
    }

    /// Same compile-only object-safety guard for the factory
    /// trait. The registry holds `Box<dyn ScoringSourceFactory>`
    /// per source_type, so this trait must also be object-safe.
    #[allow(dead_code)]
    fn _object_safety_check_factory(_: Box<dyn ScoringSourceFactory>) {
        // Body intentionally empty.
    }

    /// A minimal test impl exercising the `Box<dyn ScoringSource>`
    /// path end-to-end (per plan-critic 🔵 suggestion: validate
    /// the `async_trait` boxing pattern early so Phase 5's
    /// conformance suite doesn't surface it as a surprise).
    struct MockScoringSource;

    #[async_trait]
    impl ScoringSource for MockScoringSource {
        fn source_type_name(&self) -> &'static str {
            "mock"
        }

        async fn load(
            &self,
            _domain_key: &str,
            _config: &ScoringSourceConfig,
            _project_root: &Path,
        ) -> Option<CmdbData> {
            None
        }
    }

    #[tokio::test]
    async fn boxed_scoring_source_can_be_invoked_through_dyn_dispatch() {
        let source: Box<dyn ScoringSource> = Box::new(MockScoringSource);
        assert_eq!(source.source_type_name(), "mock");

        // Construct a minimal config; the mock ignores it but the
        // call exercises the dyn-dispatch path through async_trait.
        let config = ScoringSourceConfig {
            source_type: "mock".to_string(),
            path: None,
            endpoint: None,
            interface_version: None,
            score_field: None,
            updated_at_field: None,
            no_file_score: None,
        };
        let result = source.load("test_domain", &config, Path::new(".")).await;
        assert!(result.is_none(), "mock returns None by contract");
    }

    /// Same for the factory: build() returns Box<dyn ScoringSource>;
    /// a minimal factory exercises the registry-style dispatch path.
    struct MockFactory;

    impl ScoringSourceFactory for MockFactory {
        fn source_type_name(&self) -> &'static str {
            "mock"
        }
        fn build(&self) -> Box<dyn ScoringSource> {
            Box::new(MockScoringSource)
        }
    }

    #[test]
    fn boxed_factory_can_build_a_scoring_source() {
        let factory: Box<dyn ScoringSourceFactory> = Box::new(MockFactory);
        assert_eq!(factory.source_type_name(), "mock");
        let source = factory.build();
        assert_eq!(source.source_type_name(), "mock");
    }
}
