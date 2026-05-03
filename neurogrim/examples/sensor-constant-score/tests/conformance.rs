//! Cross-crate conformance test: `ConstantScoreSensorFactory` must
//! pass every test in
//! [`neurogrim_sdk::sensor_conformance::run_factory_conformance`]
//! (V5-SDK-1 Phase 2, 2026-05-02).
//!
//! **CRITICAL** — this test file imports `run_factory_conformance`
//! from `neurogrim_sdk`, NOT `neurogrim_core`. That's the
//! modularity claim: third-party authors get the conformance
//! contract from the SDK without coupling to internal crates.

use neurogrim_sdk::sensor_conformance::run_factory_conformance;
use sensor_constant_score::ConstantScoreSensorFactory;
use tempfile::TempDir;

#[tokio::test]
async fn constant_score_factory_passes_full_conformance_suite() {
    let factory = ConstantScoreSensorFactory;
    let dir = TempDir::new().expect("tempdir for project_root");
    let report = run_factory_conformance(&factory, dir.path()).await;

    assert!(
        report.all_passed(),
        "{}/{} conformance tests failed for ConstantScoreSensorFactory: {:#?}",
        report.failures().len(),
        report.total(),
        report.failures()
    );

    // V5-MOD-2 Phase 5 epic Done-When: ≥10 tests.
    assert!(
        report.total() >= 10,
        "conformance suite must have ≥10 tests; got {}",
        report.total()
    );
}
