//! Cross-crate conformance test: `PromSourceFactory` must pass
//! every test in [`neurogrim_core::scoring_source_conformance`]
//! (V5-MOD-1 Phase 6, 2026-05-02).
//!
//! This file is the canonical contract check that third-party
//! plugin authors **copy verbatim** into their own crate's
//! `tests/` directory (renaming `PromSourceFactory` to their own
//! factory type). If this passes, the impl honors the
//! negative-path discipline that every built-in source satisfies:
//!
//! - No panic from `factory.build()` or `source.load(...)`
//! - `source_type_name` is non-empty + stable across calls
//! - Factory-and-source `source_type_name` agree
//! - `build()` produces independent instances
//! - `load()` with skeletal config fast-fails to `None` (≤5s)
//! - 50 parallel `load()` calls don't deadlock or panic
//! - Repeated `load()` on identical input returns the same
//!   `Some`/`None` category
//!
//! Per-source happy-path tests (PromSource against a real
//! Prometheus endpoint, with mock servers) live alongside the
//! source's own tests in `src/lib.rs`. The conformance suite is
//! the *universal negative-path discipline* — what makes the
//! V5-MOD-1 modularity claim verifiable across all impls.

use neurogrim_core::scoring_source_conformance::run_factory_conformance;
use scoring_source_prom::PromSourceFactory;
use tempfile::TempDir;

#[tokio::test]
async fn prom_factory_passes_full_conformance_suite() {
    let factory = PromSourceFactory;
    let dir = TempDir::new().expect("tempdir for project_root");
    let report = run_factory_conformance(&factory, dir.path()).await;

    assert!(
        report.all_passed(),
        "{}/{} conformance tests failed for PromSourceFactory: {:#?}",
        report.failures().len(),
        report.total(),
        report.failures()
    );

    // The suite must have ≥8 tests per the Phase 5 epic Done-When.
    // This guards against accidental test-removal during refactors.
    assert!(
        report.total() >= 8,
        "conformance suite must have ≥8 tests; got {}",
        report.total()
    );
}
