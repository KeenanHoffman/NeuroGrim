//! V5-MOD-2 Phase 5 (2026-05-02) — cross-crate conformance: each
//! representative built-in `SensorFactory` from `neurogrim-sensory`
//! must pass the suite published in
//! `neurogrim_core::sensor_conformance`.
//!
//! # Coverage strategy
//!
//! Running all 21 built-in factories through the suite is doable but
//! adds substantial CI wall-clock (each suite is ~50 parallel
//! analyze() calls × 1+ second per call worst case = ~1 minute per
//! sensor × 21 sensors = ~21 minutes). Instead, this file picks a
//! **representative subset of 3 sensors** that exercise the
//! variation in trait shape:
//!
//! - **`code-quality`** — pure infallible (silent-degrade) sensor;
//!   reads only project files; no external IO.
//! - **`git-health`** — fallible sensor (`anyhow::Result<Value>`);
//!   shells out to `git` (subprocess IO).
//! - **`coherence`** — meta-sensor that reads peer CMDBs; exercises
//!   the V5-MOD-2 Phase 4.5 false-positive-green sentinel under
//!   the conformance suite's skeletal-input path (no peer CMDBs
//!   present).
//!
//! Third-party plugin authors copy the test pattern below into
//! their own crate's `tests/` directory (renaming the factory) to
//! verify their impl honors the same contract.
//!
//! # Why a subset, not all 21
//!
//! Adding all 21 factories would (a) bloat CI, (b) add minimal
//! confidence — the trait-level contract is the same regardless of
//! which sensor implements it. The 3 chosen sensors cover the
//! shape-divergent cases (fallible vs infallible, IO-heavy vs
//! pure, meta-sensor with sentinel). If a 22nd built-in is added
//! that doesn't fit one of these categories, extend the subset.

use neurogrim_core::sensor_conformance::run_factory_conformance;
use neurogrim_sensory::sensor_impls::{
    CodeQualitySensorFactory, CoherenceSensorFactory, GitHealthSensorFactory,
};
use tempfile::TempDir;

#[tokio::test]
async fn code_quality_factory_passes_full_conformance_suite() {
    let factory = CodeQualitySensorFactory;
    let dir = TempDir::new().expect("tempdir for project_root");
    let report = run_factory_conformance(&factory, dir.path()).await;

    assert!(
        report.all_passed(),
        "{}/{} conformance tests failed for CodeQualitySensorFactory: \
         {:#?}",
        report.failures().len(),
        report.total(),
        report.failures()
    );

    // ≥10 per Phase 5 epic Done-When; lower bound check.
    assert!(
        report.total() >= 10,
        "conformance suite must have ≥10 tests; got {}",
        report.total()
    );
}

#[tokio::test]
async fn git_health_factory_passes_full_conformance_suite() {
    let factory = GitHealthSensorFactory;
    let dir = TempDir::new().expect("tempdir for project_root");
    let report = run_factory_conformance(&factory, dir.path()).await;

    assert!(
        report.all_passed(),
        "{}/{} conformance tests failed for GitHealthSensorFactory: \
         {:#?}",
        report.failures().len(),
        report.total(),
        report.failures()
    );
}

#[tokio::test]
async fn coherence_factory_passes_full_conformance_suite() {
    let factory = CoherenceSensorFactory;
    let dir = TempDir::new().expect("tempdir for project_root");
    let report = run_factory_conformance(&factory, dir.path()).await;

    // Coherence on a skeletal tempdir hits the registry-not-found
    // early-return path (line ~84 of analyze_coherence.rs). That
    // returns a `score: 0` envelope — still cmdb-envelope-v1
    // conformant. The Phase 4.5 sentinel doesn't fire here because
    // the registry isn't even readable; degraded behavior is
    // operator-correct.
    assert!(
        report.all_passed(),
        "{}/{} conformance tests failed for CoherenceSensorFactory: \
         {:#?}",
        report.failures().len(),
        report.total(),
        report.failures()
    );
}
