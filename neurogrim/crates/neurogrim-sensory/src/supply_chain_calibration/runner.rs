//! Per-layer calibration runners.
//!
//! Each layer has different evaluation semantics:
//!
//! - **Layer 1** — invoke `analyze_supply_chain_sca` against the
//!   fixture's lockfile artifact; compare advisories detected
//!   against `expected.advisory_ids`.
//! - **Layer 2** — load the fixture's frozen `metadata.json`,
//!   feed each sub-sensor with the snapshot directly, compare
//!   findings against `expected.findings`.
//! - **Layer 3** — load `ticket.json`; verify it parses + has a
//!   non-empty `reference_decision`. v1 ships framework-only;
//!   no actual decision is reached at calibration time.
//!
//! All three runners produce a `LayerReport`. The harness aggregates
//! them into the top-level `CalibrationReport`.

use anyhow::{Context, Result};
use std::path::Path;

use super::fixture::{discover_layer, ExpectedFinding, FixtureLabel, LoadedFixture};
use super::{
    FixtureResult, LayerReport, LayerStatus, L1_TARGET_FN_RATE, L1_TARGET_FP_RATE,
    L2_TARGET_FN_RATE, L2_TARGET_FP_RATE, MIN_SAMPLE_SIZE,
};

/// Run Layer 1 calibration: SCA sensor against lockfile fixtures.
pub fn run_layer_1(library_root: &Path) -> Result<LayerReport> {
    let fixtures = discover_layer(library_root, "1");
    if fixtures.is_empty() {
        return Ok(empty_layer_report("1", L1_TARGET_FP_RATE, L1_TARGET_FN_RATE));
    }

    let mut report = LayerReport {
        layer: "1".to_string(),
        sample_size: fixtures.len(),
        fixtures_evaluated: 0,
        fixtures_errored: 0,
        tp_count: 0,
        tn_count: 0,
        fp_count: 0,
        fn_count: 0,
        fp_rate: None,
        fn_rate: None,
        target_fp_rate: L1_TARGET_FP_RATE,
        target_fn_rate: L1_TARGET_FN_RATE,
        min_sample_size: MIN_SAMPLE_SIZE,
        meets_target: false,
        status: LayerStatus::Pass,
        fixture_results: Vec::new(),
        fixtures_with_reference_decision: None,
        human_agreement_data: None,
        human_agreement_rate: None,
    };

    for fx in &fixtures {
        match evaluate_l1_fixture(fx) {
            Ok(result) => {
                report.fixtures_evaluated += 1;
                report.fp_count += result.fp_count;
                report.fn_count += result.fn_count;
                if result.passed {
                    match fx.metadata.label {
                        FixtureLabel::KnownBad => report.tp_count += 1,
                        FixtureLabel::KnownGood => report.tn_count += 1,
                        FixtureLabel::EdgeCase => {}
                    }
                }
                report.fixture_results.push(result);
            }
            Err(e) => {
                report.fixtures_errored += 1;
                report.fixture_results.push(FixtureResult {
                    id: fx.metadata.id.clone(),
                    label: fx.metadata.label.as_str().to_string(),
                    passed: false,
                    notes: vec![format!("evaluator error: {:#}", e)],
                    fp_count: 0,
                    fn_count: 0,
                    attack_pattern: fx.metadata.attack_pattern.clone(),
                });
            }
        }
    }

    finalize_layer_report(&mut report);
    Ok(report)
}

fn empty_layer_report(layer: &str, target_fp: f64, target_fn: f64) -> LayerReport {
    LayerReport {
        layer: layer.to_string(),
        sample_size: 0,
        fixtures_evaluated: 0,
        fixtures_errored: 0,
        tp_count: 0,
        tn_count: 0,
        fp_count: 0,
        fn_count: 0,
        fp_rate: None,
        fn_rate: None,
        target_fp_rate: target_fp,
        target_fn_rate: target_fn,
        min_sample_size: MIN_SAMPLE_SIZE,
        meets_target: false,
        status: LayerStatus::NoFixtures,
        fixture_results: Vec::new(),
        fixtures_with_reference_decision: None,
        human_agreement_data: None,
        human_agreement_rate: None,
    }
}

fn finalize_layer_report(report: &mut LayerReport) {
    let evaluated = report.fixtures_evaluated as f64;
    if evaluated > 0.0 {
        report.fp_rate = Some(report.fp_count as f64 / evaluated);
        report.fn_rate = Some(report.fn_count as f64 / evaluated);
    }

    let meets_fp = report
        .fp_rate
        .map(|r| r <= report.target_fp_rate)
        .unwrap_or(true);
    let meets_fn = report
        .fn_rate
        .map(|r| r <= report.target_fn_rate)
        .unwrap_or(true);
    report.meets_target = meets_fp && meets_fn;

    // Status precedence: red-miss > target-miss > sample-size warning > pass.
    let any_red_miss = report
        .fixture_results
        .iter()
        .any(|f| f.fn_count > 0 && f.label == "known-bad");
    if any_red_miss {
        report.status = LayerStatus::RedMiss;
    } else if !report.meets_target {
        report.status = LayerStatus::TargetMiss;
    } else if report.sample_size < MIN_SAMPLE_SIZE {
        report.status = LayerStatus::PassWithSampleSizeWarning;
    } else {
        report.status = LayerStatus::Pass;
    }
}

fn evaluate_l1_fixture(fx: &LoadedFixture) -> Result<FixtureResult> {
    use crate::supply_chain_sca::analyze_supply_chain_sca;

    // The fixture's directory is treated as project_root for the
    // SCA sensor. The fixture's lockfile + (optional) accepted-
    // advisories file live there.
    let project_root = fx.dir.to_str().context("fixture path is not utf-8")?;
    // Run a tokio runtime inline; the sensor is async.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime for fixture eval")?;
    let cmdb = runtime.block_on(analyze_supply_chain_sca(project_root));

    // Extract the advisory ids the sensor reported. The CMDB has a
    // `findings[]` array with name/status/detail; advisories are
    // listed there. For unaccepted advisories specifically we check
    // the `advisories_unaccepted` count + the `findings` entries
    // whose status is "warning" (advisory matches deduct points).
    let actual_findings = extract_l1_advisory_ids(&cmdb);

    let mut notes = Vec::new();
    let mut fp_count = 0usize;
    let mut fn_count = 0usize;

    let expected: std::collections::HashSet<String> = fx
        .metadata
        .expected
        .advisory_ids
        .iter()
        .cloned()
        .collect();
    let actual: std::collections::HashSet<String> = actual_findings.into_iter().collect();

    // FNs: expected but not actual.
    for id in expected.difference(&actual) {
        fn_count += 1;
        notes.push(format!("MISSED expected advisory: {}", id));
    }
    // FPs: actual but not expected.
    for id in actual.difference(&expected) {
        fp_count += 1;
        notes.push(format!("UNEXPECTED advisory flagged: {}", id));
    }

    // Score-range check.
    if let Some(min_score) = fx.metadata.expected.min_score {
        let score = cmdb.get("score").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        if score < min_score {
            notes.push(format!(
                "SCORE TOO LOW: actual {} < expected min {}",
                score, min_score
            ));
            // A score below expected-min on a known-good fixture
            // is a false positive class.
            if matches!(fx.metadata.label, FixtureLabel::KnownGood) {
                fp_count += 1;
            }
        }
    }
    if let Some(max_score) = fx.metadata.expected.max_score {
        let score = cmdb.get("score").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        if score > max_score {
            notes.push(format!(
                "SCORE TOO HIGH: actual {} > expected max {}",
                score, max_score
            ));
            // Score-too-high on a known-bad fixture is a missed
            // detection (similar to FN).
            if matches!(fx.metadata.label, FixtureLabel::KnownBad) {
                fn_count += 1;
            }
        }
    }
    // sensor_status check.
    if let Some(expected_status) = &fx.metadata.expected.sensor_status {
        let actual_status = cmdb
            .get("sensor_status")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if &actual_status != expected_status {
            notes.push(format!(
                "sensor_status mismatch: expected {:?}, got {:?}",
                expected_status, actual_status
            ));
            fp_count += 1;
        }
    }

    let passed = fp_count == 0 && fn_count == 0;
    Ok(FixtureResult {
        id: fx.metadata.id.clone(),
        label: fx.metadata.label.as_str().to_string(),
        passed,
        notes,
        fp_count,
        fn_count,
        attack_pattern: fx.metadata.attack_pattern.clone(),
    })
}

/// Extract advisory IDs from a Layer 1 CMDB envelope.
/// Findings whose name matches `<advisory_id>:<package>` (the
/// supply_chain_sca convention) are advisory matches.
fn extract_l1_advisory_ids(cmdb: &serde_json::Value) -> Vec<String> {
    let mut out = std::collections::HashSet::new();
    if let Some(findings) = cmdb.get("findings").and_then(|v| v.as_array()) {
        for f in findings {
            let name = f.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            // Layer 1 finding names look like "RUSTSEC-2024-0436:paste"
            // or "GHSA-xxxx-xxxx-xxxx:pkg". Splitting on ':' gives
            // the advisory id as the first component IF it looks
            // like an advisory id pattern.
            if let Some((maybe_id, _)) = name.split_once(':') {
                if looks_like_advisory_id(maybe_id) {
                    out.insert(maybe_id.to_string());
                }
            }
        }
    }
    // Also check the `accepted_advisory_ids` array in extras.
    if let Some(arr) = cmdb.get("accepted_advisory_ids").and_then(|v| v.as_array()) {
        for v in arr {
            if let Some(s) = v.as_str() {
                out.insert(s.to_string());
            }
        }
    }
    out.into_iter().collect()
}

fn looks_like_advisory_id(s: &str) -> bool {
    // Heuristic: starts with a known prefix.
    s.starts_with("RUSTSEC-")
        || s.starts_with("CVE-")
        || s.starts_with("GHSA-")
        || s.starts_with("OSV-")
        || s.starts_with("PYSEC-")
}

// =========================================================================
// Layer 2 — Vigilance
// =========================================================================

/// Run Layer 2 calibration: vigilance against frozen metadata
/// snapshots.
pub fn run_layer_2(library_root: &Path) -> Result<LayerReport> {
    let fixtures = discover_layer(library_root, "2");
    if fixtures.is_empty() {
        return Ok(empty_layer_report("2", L2_TARGET_FP_RATE, L2_TARGET_FN_RATE));
    }
    let mut report = LayerReport {
        layer: "2".to_string(),
        sample_size: fixtures.len(),
        fixtures_evaluated: 0,
        fixtures_errored: 0,
        tp_count: 0,
        tn_count: 0,
        fp_count: 0,
        fn_count: 0,
        fp_rate: None,
        fn_rate: None,
        target_fp_rate: L2_TARGET_FP_RATE,
        target_fn_rate: L2_TARGET_FN_RATE,
        min_sample_size: MIN_SAMPLE_SIZE,
        meets_target: false,
        status: LayerStatus::Pass,
        fixture_results: Vec::new(),
        fixtures_with_reference_decision: None,
        human_agreement_data: None,
        human_agreement_rate: None,
    };

    for fx in &fixtures {
        match evaluate_l2_fixture(fx) {
            Ok(result) => {
                report.fixtures_evaluated += 1;
                report.fp_count += result.fp_count;
                report.fn_count += result.fn_count;
                if result.passed {
                    match fx.metadata.label {
                        FixtureLabel::KnownBad => report.tp_count += 1,
                        FixtureLabel::KnownGood => report.tn_count += 1,
                        FixtureLabel::EdgeCase => {}
                    }
                }
                report.fixture_results.push(result);
            }
            Err(e) => {
                report.fixtures_errored += 1;
                report.fixture_results.push(FixtureResult {
                    id: fx.metadata.id.clone(),
                    label: fx.metadata.label.as_str().to_string(),
                    passed: false,
                    notes: vec![format!("evaluator error: {:#}", e)],
                    fp_count: 0,
                    fn_count: 0,
                    attack_pattern: fx.metadata.attack_pattern.clone(),
                });
            }
        }
    }

    finalize_layer_report(&mut report);
    Ok(report)
}

fn evaluate_l2_fixture(fx: &LoadedFixture) -> Result<FixtureResult> {
    use crate::supply_chain_vigilance::scan_with_metadata::scan_fixture;

    let metadata_path = fx.artifact_path("metadata.json");
    if !metadata_path.exists() {
        anyhow::bail!("Layer 2 fixture missing metadata.json at {}", metadata_path.display());
    }
    let raw = std::fs::read_to_string(&metadata_path)
        .with_context(|| format!("read {}", metadata_path.display()))?;
    let metadata_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&raw).context("parse metadata.json")?;

    let prior_state_path = fx.artifact_path("prior-state.json");
    let prior_state_raw = if prior_state_path.exists() {
        Some(
            std::fs::read_to_string(&prior_state_path)
                .with_context(|| format!("read {}", prior_state_path.display()))?,
        )
    } else {
        None
    };

    // Run vigilance against the frozen fixtures.
    let findings = scan_fixture(&metadata_map, prior_state_raw.as_deref())?;

    let mut notes = Vec::new();
    let mut fp_count = 0usize;
    let mut fn_count = 0usize;

    // Build sets of (kind, package, ecosystem) tuples for matching.
    let actual_set: std::collections::HashSet<(String, String, String)> = findings
        .iter()
        .map(|f| {
            (
                f.kind.as_str().to_string(),
                f.package.name.clone(),
                f.package.ecosystem.to_string(),
            )
        })
        .collect();

    // FNs: expected findings not in actual.
    for ef in &fx.metadata.expected.findings {
        if !finding_matches_any(ef, &actual_set) {
            fn_count += 1;
            notes.push(format!(
                "MISSED expected finding: kind={}, package={:?}, ecosystem={:?}",
                ef.kind, ef.package, ef.ecosystem
            ));
        }
    }

    // FPs: forbidden findings that fired.
    for ef in &fx.metadata.expected.forbidden_findings {
        if finding_matches_any(ef, &actual_set) {
            fp_count += 1;
            notes.push(format!(
                "FORBIDDEN finding fired: kind={}, package={:?}",
                ef.kind, ef.package
            ));
        }
    }

    // For known-good fixtures: ANY finding (that isn't allowed) is a FP.
    if matches!(fx.metadata.label, FixtureLabel::KnownGood)
        && fx.metadata.expected.findings.is_empty()
        && !findings.is_empty()
    {
        let extra_fps = findings.len();
        // Each spurious finding is a FP; we already counted forbidden;
        // add the rest.
        let already_counted_forbidden = findings
            .iter()
            .filter(|f| {
                fx.metadata
                    .expected
                    .forbidden_findings
                    .iter()
                    .any(|ef| ef.kind == f.kind.as_str())
            })
            .count();
        let remaining_fps = extra_fps.saturating_sub(already_counted_forbidden);
        if remaining_fps > 0 {
            fp_count += remaining_fps;
            notes.push(format!(
                "known-good fixture flagged {} unexpected finding(s)",
                remaining_fps
            ));
        }
    }

    let passed = fp_count == 0 && fn_count == 0;
    Ok(FixtureResult {
        id: fx.metadata.id.clone(),
        label: fx.metadata.label.as_str().to_string(),
        passed,
        notes,
        fp_count,
        fn_count,
        attack_pattern: fx.metadata.attack_pattern.clone(),
    })
}

fn finding_matches_any(
    ef: &ExpectedFinding,
    actuals: &std::collections::HashSet<(String, String, String)>,
) -> bool {
    actuals.iter().any(|(kind, pkg, eco)| {
        kind == &ef.kind
            && ef
                .package
                .as_deref()
                .map(|p| p == pkg.as_str())
                .unwrap_or(true)
            && ef
                .ecosystem
                .as_deref()
                .map(|e| e == eco.as_str())
                .unwrap_or(true)
    })
}

// =========================================================================
// Layer 3 — Agent-assisted review
// =========================================================================

/// Run Layer 3 calibration: framework-only at v1.
///
/// Validates each fixture's `ticket.json` parses + has a non-empty
/// `reference_decision`. No actual decision is reached; v1 has no
/// automated agent.
pub fn run_layer_3(library_root: &Path) -> Result<LayerReport> {
    let fixtures = discover_layer(library_root, "3");
    if fixtures.is_empty() {
        let mut r = empty_layer_report("3", 0.0, 0.0);
        r.status = LayerStatus::FrameworkOnly;
        r.fixtures_with_reference_decision = Some(0);
        r.human_agreement_data = Some(
            "insufficient — no fixtures discovered".to_string(),
        );
        return Ok(r);
    }
    let mut report = LayerReport {
        layer: "3".to_string(),
        sample_size: fixtures.len(),
        fixtures_evaluated: 0,
        fixtures_errored: 0,
        tp_count: 0,
        tn_count: 0,
        fp_count: 0,
        fn_count: 0,
        fp_rate: None,
        fn_rate: None,
        target_fp_rate: 0.0,
        target_fn_rate: 0.0,
        min_sample_size: MIN_SAMPLE_SIZE,
        meets_target: false,
        status: LayerStatus::FrameworkOnly,
        fixture_results: Vec::new(),
        fixtures_with_reference_decision: Some(0),
        human_agreement_data: Some(
            "insufficient — framework just shipped (E-SC-6, 2026-04-26); collect after ≥30 days of real triage".to_string(),
        ),
        human_agreement_rate: None,
    };

    for fx in &fixtures {
        let result = evaluate_l3_fixture(fx);
        if matches!(result.passed, true) {
            // Track which fixtures DO have a reference_decision.
            if fx.metadata.expected.reference_decision.is_some() {
                if let Some(c) = report.fixtures_with_reference_decision.as_mut() {
                    *c += 1;
                }
            }
        }
        report.fixtures_evaluated += 1;
        report.fixture_results.push(result);
    }

    Ok(report)
}

fn evaluate_l3_fixture(fx: &LoadedFixture) -> FixtureResult {
    let mut notes = Vec::new();
    let mut passed = true;

    // 1. Verify ticket.json exists + parses.
    let ticket_path = fx.artifact_path("ticket.json");
    if !ticket_path.exists() {
        passed = false;
        notes.push(format!(
            "L3 fixture missing ticket.json at {}",
            ticket_path.display()
        ));
    } else {
        match std::fs::read_to_string(&ticket_path) {
            Ok(raw) => match serde_json::from_str::<crate::supply_chain_review::ticket::ReviewTicket>(&raw) {
                Ok(_) => {
                    notes.push("ticket.json parses against ReviewTicket schema".to_string());
                }
                Err(e) => {
                    passed = false;
                    notes.push(format!("ticket.json parse failed: {:#}", e));
                }
            },
            Err(e) => {
                passed = false;
                notes.push(format!("ticket.json read failed: {:#}", e));
            }
        }
    }

    // 2. Verify reference_decision is set + valid.
    match fx.metadata.expected.reference_decision.as_deref() {
        Some(d) if matches!(d, "accept" | "reject" | "pin-to-last-good" | "no-action") => {
            notes.push(format!("reference_decision = \"{}\"", d));
        }
        Some(d) => {
            passed = false;
            notes.push(format!(
                "invalid reference_decision: {:?}; must be one of accept|reject|pin-to-last-good|no-action",
                d
            ));
        }
        None => {
            // Not a hard failure — fixture may be informational.
            notes.push(
                "no reference_decision set (informational fixture; will be skipped in human-agreement metric)".to_string(),
            );
        }
    }

    // 3. Optional fixture_author_confidence sanity.
    if let Some(c) = fx.metadata.expected.fixture_author_confidence {
        if !(0.0..=1.0).contains(&c) {
            notes.push(format!(
                "fixture_author_confidence out of range [0.0, 1.0]: {}",
                c
            ));
        }
    }

    FixtureResult {
        id: fx.metadata.id.clone(),
        label: fx.metadata.label.as_str().to_string(),
        passed,
        notes,
        fp_count: 0,
        fn_count: 0,
        attack_pattern: fx.metadata.attack_pattern.clone(),
    }
}
