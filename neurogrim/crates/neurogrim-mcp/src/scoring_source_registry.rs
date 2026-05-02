//! Global default `ScoringSourceRegistry` for the MCP / CLI
//! dispatch path (V5-MOD-1 Phase 3, 2026-05-02).
//!
//! Lazily initialized via `OnceLock`. First access populates
//! the registry with the two `neurogrim-core` built-ins
//! (`cmdb`, `function`) plus the `a2a` factory that lives in
//! `neurogrim-ecosystem`. Subsequent calls return the cached
//! reference — registration happens once per process.
//!
//! # Why a global
//!
//! The dispatch path is called from multiple sites — `BrainContext::
//! load` (CLI scoring), `BrainServer::load_cmdb_from_disk`
//! (MCP-server scoring), `doctor::check_cmdb_paths` (validation).
//! All three need the same factory inventory. A global is the
//! simplest way to keep them in lock-step without threading a
//! registry parameter through every signature.
//!
//! # Why not register at startup in main()
//!
//! Two reasons:
//!
//! 1. The existing CLI dispatch in main.rs is per-subcommand;
//!    adding a `setup_registry()` call there would duplicate
//!    initialization logic across many subcommands.
//! 2. `OnceLock::get_or_init` is thread-safe and idempotent —
//!    multiple call sites can safely race to initialize without
//!    deadlocking. First-call overhead is negligible (HashMap
//!    insert × 3).
//!
//! Future iterations (Phase 6 — out-of-tree examples) may add a
//! `register_third_party_factories(...)` API that consuming
//! binaries call before first dispatch. For Phase 3 the global
//! is built-ins-only.

use neurogrim_core::scoring_source::ScoringSourceRegistry;
use neurogrim_ecosystem::scoring_source::A2aSourceFactory;
use std::sync::OnceLock;

/// Lazy-init global registry. Initialized on first call; thread-
/// safe via `OnceLock::get_or_init`.
static GLOBAL_REGISTRY: OnceLock<ScoringSourceRegistry> = OnceLock::new();

/// Get the default scoring-source registry. Initialized on first
/// call with `cmdb` + `function` + `a2a` factories. Subsequent
/// calls return the cached `&'static ScoringSourceRegistry`.
pub fn default_registry() -> &'static ScoringSourceRegistry {
    GLOBAL_REGISTRY.get_or_init(|| {
        let mut reg = ScoringSourceRegistry::with_core_built_ins();
        reg.register(Box::new(A2aSourceFactory));
        reg
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_three_built_ins() {
        let reg = default_registry();
        assert!(reg.has("cmdb"), "cmdb factory must be registered");
        assert!(reg.has("function"), "function factory must be registered");
        assert!(reg.has("a2a"), "a2a factory must be registered");
        assert_eq!(
            reg.registered_names().count(),
            3,
            "Phase 3 ships exactly 3 built-ins (cmdb + function + a2a)"
        );
    }

    #[test]
    fn default_registry_is_idempotent() {
        // First call initializes; second call returns the same
        // reference. Both must produce a registry with the
        // expected factories.
        let r1 = default_registry();
        let r2 = default_registry();
        assert!(std::ptr::eq(r1, r2), "global registry should be shared");
    }

    #[test]
    fn default_registry_can_build_each_source() {
        let reg = default_registry();
        let cmdb_source = reg.build("cmdb").expect("cmdb factory builds");
        assert_eq!(cmdb_source.source_type_name(), "cmdb");
        let fn_source = reg.build("function").expect("function factory builds");
        assert_eq!(fn_source.source_type_name(), "function");
        let a2a_source = reg.build("a2a").expect("a2a factory builds");
        assert_eq!(a2a_source.source_type_name(), "a2a");
    }
}
