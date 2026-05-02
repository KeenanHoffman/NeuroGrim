//! Scoring-source dispatch registry for the MCP / CLI dispatch
//! sites (V5-MOD-1 Phase 3 + Phase 4-fallback, 2026-05-02).
//!
//! # Two-tier dispatch (Phase 4-fallback)
//!
//! Phase 4's perf-gate measurement showed `Box<dyn ScoringSource>`
//! + `#[async_trait]` future-boxing was too costly for the hot
//! scoring path (p95 went from 18 → 24-28 ms across 19 domains;
//! 5%-ceiling of 19 ms breached). The plan-documented fallback,
//! adopted 2026-05-02 per operator pin: built-ins go through an
//! inlined enum match calling each Source's INHERENT
//! `async fn load_inherent(...)` (no future-boxing — the compiler
//! monomorphizes); third-party plugins still use `Box<dyn
//! ScoringSource>` and pay the dyn cost when used.
//!
//! - **Fast path**: [`BuiltinScoringSource`] enum + match dispatch.
//!   Used for `cmdb` / `a2a` / `function` source types. Zero
//!   per-call heap allocations, zero virtual dispatch.
//! - **Plugin path**: `ScoringSourceRegistry` + `Box<dyn
//!   ScoringSource>`. Used by third-party impls registered via
//!   the trait + factory. Phase 6 ships the first example.
//!
//! Operators see no behavior difference. Built-in dispatch is
//! ~6 ms p95 faster than the all-Box<dyn> v3 path.
//!
//! # Why a global
//!
//! The dispatch path is called from multiple sites — `BrainContext::
//! load` (CLI scoring), `BrainServer::load_cmdb_from_disk`
//! (MCP-server scoring), `doctor::check_cmdb_paths` (validation).
//! All three need the same dispatch logic. A global is the
//! simplest way to keep them in lock-step.

use neurogrim_core::registry::ScoringSourceConfig;
use neurogrim_core::scoring::CmdbData;
use neurogrim_core::scoring_source::ScoringSourceRegistry;
use neurogrim_core::scoring_sources::cmdb::CmdbSource;
use neurogrim_core::scoring_sources::function::FunctionSource;
use neurogrim_ecosystem::scoring_source::A2aSource;
use std::path::Path;
use std::sync::OnceLock;

/// Closed-set enum over the three built-in scoring sources.
/// Each variant holds the corresponding stateless source struct;
/// `load(...)` dispatches via inlined match to each source's
/// inherent `async fn load_inherent` — bypassing
/// `#[async_trait]`'s `Pin<Box<dyn Future>>` allocation that the
/// V5-MOD-1 perf-gate flagged as the regression's dominant cause.
///
/// **Adding a new built-in** here requires (a) adding a variant,
/// (b) updating [`Self::from_source_type`] to map the wire-name,
/// (c) updating [`Self::load`] to delegate to the source's
/// `load_inherent`, (d) updating [`Self::source_type_name`]. The
/// closed-set is intentional — built-ins are a finite list owned
/// by the workspace; third-party sources go through the plugin
/// path (registry + Box<dyn>).
pub enum BuiltinScoringSource {
    Cmdb(CmdbSource),
    A2a(A2aSource),
    Function(FunctionSource),
}

impl BuiltinScoringSource {
    /// Resolve a `source_type` wire-name to its built-in variant,
    /// or `None` if not a built-in (caller falls through to the
    /// plugin registry).
    pub fn from_source_type(source_type: &str) -> Option<Self> {
        match source_type {
            "cmdb" => Some(Self::Cmdb(CmdbSource)),
            "a2a" => Some(Self::A2a(A2aSource)),
            "function" => Some(Self::Function(FunctionSource)),
            _ => None,
        }
    }

    /// Stable wire-name. Equivalent to the trait's
    /// `source_type_name()`; provided here so callers don't
    /// need to construct a trait object for the lookup.
    pub fn source_type_name(&self) -> &'static str {
        match self {
            Self::Cmdb(_) => "cmdb",
            Self::A2a(_) => "a2a",
            Self::Function(_) => "function",
        }
    }

    /// Load this domain's scoring data via inlined-match dispatch
    /// to each source's inherent `load_inherent` method. The
    /// match is monomorphized; no `Pin<Box<dyn Future>>`
    /// allocation per call. This is the V5-MOD-1 Phase 4-fallback
    /// fast path.
    pub async fn load(
        &self,
        domain_key: &str,
        config: &ScoringSourceConfig,
        project_root: &Path,
    ) -> Option<CmdbData> {
        match self {
            Self::Cmdb(s) => s.load_inherent(domain_key, config, project_root).await,
            Self::A2a(s) => s.load_inherent(domain_key, config, project_root).await,
            Self::Function(s) => s.load_inherent(domain_key, config, project_root).await,
        }
    }
}

/// Lazy-init plugin registry — empty in V5-MOD-1 Phase 4 (no
/// third-party impls until Phase 6's example crate). Reserved
/// for the plugin path.
static PLUGIN_REGISTRY: OnceLock<ScoringSourceRegistry> = OnceLock::new();

/// Get the plugin registry. V5-MOD-1 Phase 4-fallback: empty by
/// default; built-ins are NOT registered here (they go through
/// the [`BuiltinScoringSource`] enum's fast path). Phase 6 ships
/// the first plugin (`scoring-source-prom` example crate); future
/// iterations add a `register_third_party_factories(...)` API
/// callable from `main.rs` before first dispatch.
pub fn plugin_registry() -> &'static ScoringSourceRegistry {
    PLUGIN_REGISTRY.get_or_init(ScoringSourceRegistry::new)
}

/// Two-tier dispatch result: a built-in (fast path) or a plugin
/// (Box<dyn>). Callers `match` on this and call `load(...)`.
pub enum Dispatcher {
    Builtin(BuiltinScoringSource),
    Plugin(Box<dyn neurogrim_core::scoring_source::ScoringSource>),
}

impl Dispatcher {
    /// Resolve a `source_type` wire-name to either the built-in
    /// fast path or the plugin path. Returns `None` if neither.
    pub fn for_source_type(source_type: &str) -> Option<Self> {
        if let Some(builtin) = BuiltinScoringSource::from_source_type(source_type) {
            return Some(Self::Builtin(builtin));
        }
        if let Some(plugin) = plugin_registry().build(source_type) {
            return Some(Self::Plugin(plugin));
        }
        None
    }

    /// Load this domain's scoring data via the appropriate path.
    pub async fn load(
        &self,
        domain_key: &str,
        config: &ScoringSourceConfig,
        project_root: &Path,
    ) -> Option<CmdbData> {
        match self {
            Self::Builtin(b) => b.load(domain_key, config, project_root).await,
            Self::Plugin(p) => p.load(domain_key, config, project_root).await,
        }
    }
}

/// True if a dispatcher is registered for the given source_type
/// (built-in OR plugin). Used by `doctor::check_cmdb_paths` to
/// emit a finding when the type is unknown.
pub fn is_registered(source_type: &str) -> bool {
    BuiltinScoringSource::from_source_type(source_type).is_some()
        || plugin_registry().has(source_type)
}

/// All registered source-type names (built-ins + plugins). Used
/// for diagnostic messages.
pub fn all_registered_names() -> Vec<&'static str> {
    let mut names = vec!["cmdb", "a2a", "function"];
    names.extend(plugin_registry().registered_names().copied());
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_resolves_three_known_types() {
        assert!(BuiltinScoringSource::from_source_type("cmdb").is_some());
        assert!(BuiltinScoringSource::from_source_type("a2a").is_some());
        assert!(BuiltinScoringSource::from_source_type("function").is_some());
    }

    #[test]
    fn builtin_rejects_unknown_type() {
        assert!(BuiltinScoringSource::from_source_type("not-a-type").is_none());
    }

    #[test]
    fn builtin_source_type_name_matches_construction() {
        let cmdb = BuiltinScoringSource::from_source_type("cmdb").unwrap();
        assert_eq!(cmdb.source_type_name(), "cmdb");
        let a2a = BuiltinScoringSource::from_source_type("a2a").unwrap();
        assert_eq!(a2a.source_type_name(), "a2a");
        let func = BuiltinScoringSource::from_source_type("function").unwrap();
        assert_eq!(func.source_type_name(), "function");
    }

    #[test]
    fn dispatcher_resolves_built_ins() {
        assert!(matches!(
            Dispatcher::for_source_type("cmdb"),
            Some(Dispatcher::Builtin(_))
        ));
        assert!(matches!(
            Dispatcher::for_source_type("a2a"),
            Some(Dispatcher::Builtin(_))
        ));
        assert!(matches!(
            Dispatcher::for_source_type("function"),
            Some(Dispatcher::Builtin(_))
        ));
    }

    #[test]
    fn dispatcher_returns_none_for_unknown() {
        assert!(Dispatcher::for_source_type("totally-unknown").is_none());
    }

    #[test]
    fn is_registered_covers_built_ins() {
        assert!(is_registered("cmdb"));
        assert!(is_registered("a2a"));
        assert!(is_registered("function"));
        assert!(!is_registered("not-a-thing"));
    }

    #[test]
    fn all_registered_names_includes_three_builtins() {
        let names = all_registered_names();
        assert!(names.contains(&"cmdb"));
        assert!(names.contains(&"a2a"));
        assert!(names.contains(&"function"));
        assert!(names.len() >= 3);
    }

    #[tokio::test]
    async fn function_dispatch_returns_none() {
        let dispatcher = Dispatcher::for_source_type("function").unwrap();
        let config = ScoringSourceConfig {
            source_type: "function".to_string(),
            path: None,
            endpoint: None,
            interface_version: None,
            score_field: None,
            updated_at_field: None,
            no_file_score: None,
        };
        let result = dispatcher
            .load("test_domain", &config, Path::new("."))
            .await;
        assert!(result.is_none(), "function source is a no-op marker");
    }
}
