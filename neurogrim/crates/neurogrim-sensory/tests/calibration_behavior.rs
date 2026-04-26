//! E-SC-8 calibration integration tests.
//!
//! Exercises `supply_chain_calibration::run_calibration` against
//! the in-tree fixture library at `tests/supply-chain-fixtures/`.
//! These tests are meant to:
//!
//! 1. Verify the harness loads + iterates fixtures correctly.
//! 2. Catch regressions in per-layer evaluators (a fixture that
//!    used to pass starts failing → either the fixture or the
//!    sensor changed).
//! 3. Provide the dogfood baseline for the ecosystem repo's
//!    publish-readiness gate (E-SC-10).
//!
//! NETWORK NOTE: Layer 1 evaluation runs the live SCA sensor,
//! which queries OSV.dev. Tests that depend on advisory matching
//! (`04-rust-with-rustsec-paste`) require network OR a populated
//! local cache. v1 design choice: tests don't assert specific
//! advisory IDs were detected; they assert the SENSOR DOESN'T
//! PANIC on the fixture and that obvious-error fixtures
//! (no-lockfile, etc.) report graceful degradation.

use neurogrim_sensory::supply_chain_calibration::{
    fixture, run_calibration, LayerStatus, OverallStatus,
};
use std::path::PathBuf;

fn library_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/neurogrim-sensory/ -> ../../tests/supply-chain-fixtures/
    manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("supply-chain-fixtures")
}

#[test]
fn library_root_exists() {
    let root = library_root();
    assert!(
        root.is_dir(),
        "fixture library not found at {} — did the test run from the wrong directory?",
        root.display()
    );
}

#[test]
fn discovers_layer_1_fixtures() {
    let root = library_root();
    let fixtures = fixture::discover_layer(&root, "1");
    assert!(
        !fixtures.is_empty(),
        "expected ≥1 Layer 1 fixture under {}",
        root.display()
    );
    // All fixtures should have layer = "1".
    for f in &fixtures {
        assert_eq!(f.metadata.layer, "1", "fixture {} has wrong layer", f.id());
    }
}

#[test]
fn discovers_layer_2_fixtures() {
    let root = library_root();
    let fixtures = fixture::discover_layer(&root, "2");
    assert!(
        fixtures.len() >= 5,
        "expected ≥5 Layer 2 fixtures (got {}) — v1 ships 8",
        fixtures.len()
    );
    for f in &fixtures {
        assert_eq!(f.metadata.layer, "2");
    }
}

#[test]
fn discovers_layer_3_fixtures() {
    let root = library_root();
    let fixtures = fixture::discover_layer(&root, "3");
    assert!(
        fixtures.len() >= 3,
        "expected ≥3 Layer 3 fixtures (got {}) — v1 ships 4",
        fixtures.len()
    );
    for f in &fixtures {
        assert_eq!(f.metadata.layer, "3");
    }
}

#[test]
fn layer_2_calibration_runs_to_completion() {
    let root = library_root();
    let report = neurogrim_sensory::supply_chain_calibration::runner::run_layer_2(&root)
        .expect("layer-2 run failed");
    assert!(report.sample_size > 0);
    assert_eq!(report.fixtures_errored, 0,
        "no L2 fixtures should error: {:?}", report.fixture_results);
}

#[test]
fn layer_2_known_good_fixture_no_findings() {
    let root = library_root();
    let report = neurogrim_sensory::supply_chain_calibration::runner::run_layer_2(&root)
        .expect("layer-2 run failed");
    let known_good = report
        .fixture_results
        .iter()
        .find(|f| f.id == "06-known-good-stable-pkg");
    assert!(
        known_good.is_some(),
        "known-good fixture not found in results"
    );
    let kg = known_good.unwrap();
    assert!(
        kg.passed,
        "known-good fixture failed (FPs: {} | FNs: {} | notes: {:?})",
        kg.fp_count, kg.fn_count, kg.notes
    );
}

#[test]
fn layer_2_typosquat_fixture_fires() {
    let root = library_root();
    let report = neurogrim_sensory::supply_chain_calibration::runner::run_layer_2(&root)
        .expect("layer-2 run failed");
    let typosquat = report
        .fixture_results
        .iter()
        .find(|f| f.id == "01-typosquat-pypi-litelm");
    assert!(typosquat.is_some(), "typosquat fixture missing");
    let t = typosquat.unwrap();
    assert!(
        t.passed,
        "typosquat fixture should pass (sensor MUST detect): notes={:?}",
        t.notes
    );
}

#[test]
fn layer_3_runs_framework_only() {
    let root = library_root();
    let report = neurogrim_sensory::supply_chain_calibration::runner::run_layer_3(&root)
        .expect("layer-3 run failed");
    assert!(matches!(report.status, LayerStatus::FrameworkOnly));
    assert!(report.human_agreement_data.is_some());
    assert_eq!(report.human_agreement_rate, None);
    // Each L3 fixture should validate (ticket parses, reference_decision set).
    for r in &report.fixture_results {
        assert!(
            r.passed,
            "L3 fixture {} failed: {:?}",
            r.id, r.notes
        );
    }
}

#[test]
fn full_run_produces_valid_report() {
    let root = library_root();
    let report = run_calibration(&root).expect("run_calibration failed");
    assert_eq!(report.schema_version, "1");
    assert!(report.run_id.starts_with("cal-"));
    assert!(report.layer_1.sample_size > 0);
    assert!(report.layer_2.sample_size > 0);
    assert!(report.layer_3.sample_size > 0);
    // v1 ships ~10 fixtures per layer; should NOT report `pass` due
    // to sample-size warning.
    assert!(
        !matches!(report.overall_status, OverallStatus::Pass),
        "v1 should be pass-with-sample-size-warning at minimum, got {:?}",
        report.overall_status
    );
    // promotion_ready MUST be false in v1 (we lack ≥30 fixtures + ≥30
    // days of L3 triage data).
    assert!(
        !report.promotion_ready.ready,
        "v1 must not be promotion-ready"
    );
    assert!(!report.promotion_ready.gaps.is_empty());
}

#[test]
fn report_serializes_to_valid_json() {
    let root = library_root();
    let report = run_calibration(&root).expect("run_calibration failed");
    let json = serde_json::to_string_pretty(&report).expect("serialize report");
    assert!(json.contains("schema_version"));
    assert!(json.contains("\"layer_1\""));
    assert!(json.contains("\"layer_2\""));
    assert!(json.contains("\"layer_3\""));
    assert!(json.contains("statistical_validity_note"));
    assert!(json.contains("promotion_ready"));
    // Re-parse to validate.
    let _: serde_json::Value = serde_json::from_str(&json).expect("re-parse JSON");
}
