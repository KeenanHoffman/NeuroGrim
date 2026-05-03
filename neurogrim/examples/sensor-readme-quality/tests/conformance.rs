//! Cross-crate conformance test: `ReadmeQualitySensorFactory` must
//! pass every test in [`neurogrim_core::sensor_conformance`]
//! (V5-MOD-2 Phase 6, 2026-05-02).
//!
//! This file is the canonical contract check that third-party
//! sensor authors **copy verbatim** into their own crate's
//! `tests/` directory (renaming `ReadmeQualitySensorFactory` to
//! their own factory type). If this passes, the impl honors the
//! negative-path discipline that every built-in sensor satisfies:
//!
//! - No panic from `factory.build()` or `sensor.analyze(...)`
//! - `name()` is non-empty + stable across calls
//! - `build()` produces independent instances
//! - `analyze()` with skeletal `project_root` returns within 30s
//!   (silent-degrade `Ok` or fallible `Err` both fine)
//! - 50 parallel `analyze()` calls don't deadlock or panic
//! - Output validates against `cmdb-envelope-v1` shape (required
//!   fields, RFC3339 timestamps, score in [0, 100])
//! - `meta` block well-formed (schema_version=="1", non-empty
//!   updated_by, RFC3339 updated_at)
//! - Repeated `analyze()` on identical input returns same Ok/Err
//!   category
//!
//! Per-sensor happy-path tests (the ReadmeQualitySensor unit tests
//! against fixtures with various README quality levels) live
//! alongside the sensor's own code in `src/lib.rs`. The conformance
//! suite is the *universal negative-path discipline* — what makes
//! the V5-MOD-2 modularity claim verifiable across all impls.

use neurogrim_core::sensor_conformance::run_factory_conformance;
use sensor_readme_quality::ReadmeQualitySensorFactory;
use tempfile::TempDir;

#[tokio::test]
async fn readme_quality_factory_passes_full_conformance_suite() {
    let factory = ReadmeQualitySensorFactory;
    let dir = TempDir::new().expect("tempdir for project_root");
    let report = run_factory_conformance(&factory, dir.path()).await;

    assert!(
        report.all_passed(),
        "{}/{} conformance tests failed for ReadmeQualitySensorFactory: {:#?}",
        report.failures().len(),
        report.total(),
        report.failures()
    );

    // The suite must have ≥10 tests per the V5-MOD-2 Phase 5 epic
    // Done-When. This guards against accidental test-removal during
    // refactors.
    assert!(
        report.total() >= 10,
        "conformance suite must have ≥10 tests; got {}",
        report.total()
    );
}
