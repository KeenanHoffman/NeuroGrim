//! Cross-crate conformance test: `MemoryQueueBackendFactory` must
//! pass every test in [`neurogrim_core::queue_backend_conformance`]
//! (V5-MOD-3 Phase 5, 2026-05-02).
//!
//! Third-party `QueueBackend` authors copy this test verbatim into
//! their own crate's `tests/` directory (renaming the factory).
//! If it passes, the impl honors the contract every built-in
//! backend (`JsonlBackend`, `SqliteBackend`) satisfies.

use neurogrim_core::queue_backend_conformance::run_factory_conformance;
use queue_backend_memory::MemoryQueueBackendFactory;
use tempfile::TempDir;

#[tokio::test]
async fn memory_factory_passes_full_conformance_suite() {
    let factory = MemoryQueueBackendFactory::default();
    let dir = TempDir::new().expect("tempdir for queue_root");
    // The in-memory backend doesn't touch disk, but the conformance
    // suite still passes a queue_root to factories — the test just
    // verifies our factory ignores it correctly.
    let report = run_factory_conformance(&factory, dir.path()).await;

    assert!(
        report.all_passed(),
        "{}/{} conformance tests failed for MemoryQueueBackendFactory: {:#?}",
        report.failures().len(),
        report.total(),
        report.failures()
    );

    assert!(
        report.total() >= 10,
        "conformance suite must have ≥10 tests; got {}",
        report.total()
    );
}
