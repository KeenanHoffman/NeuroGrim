//! Supply-chain calibration harness — E-SC-8.
//!
//! Loads the fixture library at `tests/supply-chain-fixtures/`,
//! runs each layer's sensors against the fixtures, computes
//! FP/FN rates per layer, and emits a calibration report.
//!
//! # Per-layer semantics
//!
//! - **Layer 1 (mechanical SCA)** — regression-check semantics.
//!   Sensor is exact-match deterministic; any non-zero FP/FN is
//!   a critical regression, not a calibration miss.
//! - **Layer 2 (vigilance)** — probabilistic-heuristic semantics.
//!   Target: <5% FP, <20% FN. Per-kind FP/FN tracked.
//! - **Layer 3 (review framework)** — human-agreement metric.
//!   Target: ≥80% human-agreement after first month of operator
//!   triage. v1 ships framework-only; no human-agreement data
//!   exists yet.
//!
//! # Sample-size honesty
//!
//! v1 ships ~10 fixtures per layer. The smallest measurable rate
//! is ~10%, BELOW the scaffolding's <1%/<5% targets. The report
//! status is `pass-with-sample-size-warning` rather than `pass`
//! until the library reaches `MIN_SAMPLE_SIZE` per layer.

pub mod fixture;
pub mod runner;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::Path;

/// Minimum fixture count per layer for true statistical validity
/// (per scaffolding rule-of-thumb). Below this, status is
/// `pass-with-sample-size-warning`.
pub const MIN_SAMPLE_SIZE: usize = 30;

/// Per-spec target FP rate for Layer 1 mechanical SCA.
pub const L1_TARGET_FP_RATE: f64 = 0.01;
pub const L1_TARGET_FN_RATE: f64 = 0.01;

/// Per-spec target FP rate for Layer 2 vigilance.
pub const L2_TARGET_FP_RATE: f64 = 0.05;
pub const L2_TARGET_FN_RATE: f64 = 0.20;

/// Per-spec target human-agreement rate for Layer 3.
pub const L3_TARGET_HUMAN_AGREEMENT: f64 = 0.80;

/// Minimum days of L3 operator-triage data required for promotion-
/// readiness. v1 has 0 by definition (framework just shipped).
pub const L3_MIN_TRIAGE_DAYS: u64 = 30;

/// The harness version stamped into reports. Bump when the
/// fixture-handling logic changes in a way that affects how
/// existing fixtures match against expected outputs.
pub const HARNESS_VERSION: &str = "1.0.0";

/// One run's calibration report. Serialized as JSON; matches the
/// `supply-chain-calibration-report-v1.schema.json` shape (NeuroGrim-
/// internal at v1; spec-promote candidate for LSP-Brains v2.7).
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationReport {
    pub run_id: String,
    pub schema_version: String,
    pub harness_version: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub fixture_library_path: String,
    pub layer_1: LayerReport,
    pub layer_2: LayerReport,
    pub layer_3: LayerReport,
    pub overall_status: OverallStatus,
    pub statistical_validity_note: String,
    pub promotion_ready: PromotionReadiness,
}

/// Per-layer calibration result.
#[derive(Debug, Clone, Serialize)]
pub struct LayerReport {
    /// "1", "2", or "3".
    pub layer: String,
    /// Total fixtures discovered.
    pub sample_size: usize,
    /// Fixtures that loaded + ran successfully.
    pub fixtures_evaluated: usize,
    /// Fixtures that errored (parse failure, etc.).
    pub fixtures_errored: usize,
    /// Tally of true-positive fixtures (known-bad correctly
    /// flagged).
    pub tp_count: usize,
    /// Tally of true-negative fixtures (known-good correctly
    /// silent).
    pub tn_count: usize,
    /// False-positive count.
    pub fp_count: usize,
    /// False-negative count.
    pub fn_count: usize,
    /// FP rate over evaluated fixtures, if measurable.
    pub fp_rate: Option<f64>,
    pub fn_rate: Option<f64>,
    pub target_fp_rate: f64,
    pub target_fn_rate: f64,
    pub min_sample_size: usize,
    pub meets_target: bool,
    pub status: LayerStatus,
    /// Per-fixture details.
    pub fixture_results: Vec<FixtureResult>,
    // ── Layer 3 only ──
    /// Number of fixtures with a non-empty `reference_decision`.
    /// Used to gate on whether we even have anchors for human-
    /// agreement measurement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixtures_with_reference_decision: Option<usize>,
    /// Status of the human-agreement data source. v1: always
    /// `"insufficient"` because the L3 framework just shipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_agreement_data: Option<String>,
    /// Self-reported human-agreement rate, if measurable. v1:
    /// always `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_agreement_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixtureResult {
    pub id: String,
    pub label: String,
    pub passed: bool,
    /// Free-text notes for operator review.
    pub notes: Vec<String>,
    /// FPs this fixture contributed (a known-good fixture that
    /// was flagged → 1 FP per finding kind that fired).
    pub fp_count: usize,
    /// FNs this fixture contributed (a known-bad fixture that
    /// was MISSED entirely → 1 FN per expected finding that
    /// did NOT fire).
    pub fn_count: usize,
    /// Optional attack-pattern tag from the fixture.
    pub attack_pattern: Option<String>,
}

/// Per-layer status enumeration.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerStatus {
    /// Layer met its FP/FN targets AND has sufficient sample size.
    Pass,
    /// Layer met its targets but below `MIN_SAMPLE_SIZE`. Honest
    /// disclosure: we can't claim true rate-bounds yet.
    PassWithSampleSizeWarning,
    /// Layer exceeded a target rate.
    TargetMiss,
    /// A known-bad fixture went UNDETECTED (FN). This is the
    /// critical-regression status.
    RedMiss,
    /// Layer 3 v1: framework shipped, no human-agreement data
    /// collected yet.
    FrameworkOnly,
    /// No fixtures discovered for this layer.
    NoFixtures,
}

impl LayerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LayerStatus::Pass => "pass",
            LayerStatus::PassWithSampleSizeWarning => "pass-with-sample-size-warning",
            LayerStatus::TargetMiss => "target-miss",
            LayerStatus::RedMiss => "red-miss",
            LayerStatus::FrameworkOnly => "framework-only",
            LayerStatus::NoFixtures => "no-fixtures",
        }
    }
}

/// Worst-to-best precedence for combining per-layer statuses into
/// the overall status.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverallStatus {
    /// All layers passed at full sample size.
    Pass,
    /// All layers passed, but at least one is below sample-size
    /// threshold OR Layer 3 is framework-only.
    PassWithSampleSizeWarning,
    /// At least one layer missed its target rate.
    TargetMiss,
    /// At least one layer surfaced a red-miss (critical FN).
    RedMiss,
    /// No fixtures discovered across any layer.
    NoFixtures,
}

impl OverallStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OverallStatus::Pass => "pass",
            OverallStatus::PassWithSampleSizeWarning => "pass-with-sample-size-warning",
            OverallStatus::TargetMiss => "target-miss",
            OverallStatus::RedMiss => "red-miss",
            OverallStatus::NoFixtures => "no-fixtures",
        }
    }

    /// Compute the overall status from the three per-layer ones,
    /// using worst-to-best precedence:
    /// red-miss > target-miss > no-fixtures > pass-with-warning > pass.
    /// (Layer 3 framework-only is treated as warning when other
    /// layers pass, miss when they miss.)
    pub fn from_layers(l1: LayerStatus, l2: LayerStatus, l3: LayerStatus) -> Self {
        // Highest-severity wins.
        for s in [l1, l2, l3] {
            if matches!(s, LayerStatus::RedMiss) {
                return OverallStatus::RedMiss;
            }
        }
        for s in [l1, l2, l3] {
            if matches!(s, LayerStatus::TargetMiss) {
                return OverallStatus::TargetMiss;
            }
        }
        // Special: if EVERY layer is NoFixtures, that's the dominant
        // signal. Otherwise, fixtures-found-on-some-layers + warning
        // collapses to PassWithSampleSizeWarning.
        let all_no_fixtures = matches!(
            (l1, l2, l3),
            (LayerStatus::NoFixtures, LayerStatus::NoFixtures, LayerStatus::NoFixtures)
        );
        if all_no_fixtures {
            return OverallStatus::NoFixtures;
        }
        for s in [l1, l2, l3] {
            if matches!(
                s,
                LayerStatus::PassWithSampleSizeWarning
                    | LayerStatus::FrameworkOnly
                    | LayerStatus::NoFixtures
            ) {
                return OverallStatus::PassWithSampleSizeWarning;
            }
        }
        OverallStatus::Pass
    }
}

/// Promotion-readiness summary. `ready: false` in v1 by design;
/// `gaps[]` names exactly what's missing.
#[derive(Debug, Clone, Serialize)]
pub struct PromotionReadiness {
    pub ready: bool,
    pub gaps: Vec<String>,
}

impl PromotionReadiness {
    /// Compute readiness from the report. v1 is non-ready by
    /// design (we lack ≥30 fixtures + ≥30 days of L3 triage data).
    /// Honest signal for operators wiring this into CI.
    pub fn compute(report_partial: &PartialReportForReadiness) -> Self {
        let mut gaps = Vec::new();

        // Layer 1 size + status.
        if report_partial.l1_sample < MIN_SAMPLE_SIZE {
            gaps.push(format!(
                "Layer 1 sample size {} < required {} for statistical validity",
                report_partial.l1_sample, MIN_SAMPLE_SIZE
            ));
        }
        if !matches!(report_partial.l1_status, LayerStatus::Pass) {
            gaps.push(format!(
                "Layer 1 status: {} (target: pass)",
                report_partial.l1_status.as_str()
            ));
        }

        // Layer 2 size + status.
        if report_partial.l2_sample < MIN_SAMPLE_SIZE {
            gaps.push(format!(
                "Layer 2 sample size {} < required {} for statistical validity",
                report_partial.l2_sample, MIN_SAMPLE_SIZE
            ));
        }
        if !matches!(report_partial.l2_status, LayerStatus::Pass) {
            gaps.push(format!(
                "Layer 2 status: {} (target: pass)",
                report_partial.l2_status.as_str()
            ));
        }

        // Layer 3: framework-only OR insufficient triage history.
        if matches!(
            report_partial.l3_status,
            LayerStatus::FrameworkOnly | LayerStatus::NoFixtures
        ) {
            gaps.push(format!(
                "Layer 3: human-agreement data insufficient (status: {}). Requires ≥{} days of operator triage history.",
                report_partial.l3_status.as_str(),
                L3_MIN_TRIAGE_DAYS
            ));
        }

        Self {
            ready: gaps.is_empty(),
            gaps,
        }
    }
}

/// The fields needed to compute promotion-readiness; extracted so
/// the readiness check can be computed mid-build before the full
/// report is finalized.
pub struct PartialReportForReadiness {
    pub l1_sample: usize,
    pub l1_status: LayerStatus,
    pub l2_sample: usize,
    pub l2_status: LayerStatus,
    pub l3_status: LayerStatus,
}

/// Public entry point: run calibration against a fixture library
/// and emit the report.
pub fn run_calibration(library_root: &Path) -> Result<CalibrationReport> {
    let started_at = Utc::now();
    let run_id = generate_run_id(&started_at);

    let layer_1 = runner::run_layer_1(library_root)?;
    let layer_2 = runner::run_layer_2(library_root)?;
    let layer_3 = runner::run_layer_3(library_root)?;

    let finished_at = Utc::now();
    let overall_status = OverallStatus::from_layers(
        layer_1.status,
        layer_2.status,
        layer_3.status,
    );

    let validity = build_validity_note(&layer_1, &layer_2, &layer_3);
    let promotion_ready = PromotionReadiness::compute(&PartialReportForReadiness {
        l1_sample: layer_1.sample_size,
        l1_status: layer_1.status,
        l2_sample: layer_2.sample_size,
        l2_status: layer_2.status,
        l3_status: layer_3.status,
    });

    Ok(CalibrationReport {
        run_id,
        schema_version: "1".to_string(),
        harness_version: HARNESS_VERSION.to_string(),
        started_at,
        finished_at,
        fixture_library_path: library_root.display().to_string(),
        layer_1,
        layer_2,
        layer_3,
        overall_status,
        statistical_validity_note: validity,
        promotion_ready,
    })
}

/// Generate a stable-per-run id from the timestamp + a nonce.
/// Format: `cal-YYYY-MM-DD-HHMMSS-XXXX` where XXXX is a 4-hex-
/// char nonce derived from process-id-mix-with-time-nanos. Avoids
/// a uuid dep.
fn generate_run_id(started_at: &DateTime<Utc>) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(started_at.timestamp_nanos_opt().unwrap_or_default().to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    let digest = hasher.finalize();
    let nonce: String = digest
        .iter()
        .take(2)
        .map(|b| format!("{:02x}", b))
        .collect();
    format!("cal-{}-{}", started_at.format("%Y-%m-%d-%H%M%S"), nonce)
}

fn build_validity_note(l1: &LayerReport, l2: &LayerReport, l3: &LayerReport) -> String {
    let mut parts = Vec::new();
    if l1.sample_size < MIN_SAMPLE_SIZE {
        parts.push(format!(
            "Layer 1: {} fixtures evaluated; smallest measurable rate ~{:.0}%. Below MIN_SAMPLE_SIZE={}.",
            l1.sample_size,
            100.0 / (l1.sample_size.max(1) as f64),
            MIN_SAMPLE_SIZE
        ));
    }
    if l2.sample_size < MIN_SAMPLE_SIZE {
        parts.push(format!(
            "Layer 2: {} fixtures evaluated; smallest measurable rate ~{:.0}%. Below MIN_SAMPLE_SIZE={}.",
            l2.sample_size,
            100.0 / (l2.sample_size.max(1) as f64),
            MIN_SAMPLE_SIZE
        ));
    }
    if matches!(l3.status, LayerStatus::FrameworkOnly | LayerStatus::NoFixtures) {
        parts.push(
            "Layer 3: framework-only ship; human-agreement data requires ≥30 days of real operator triage history per spec §16.4.".to_string()
        );
    }
    if parts.is_empty() {
        "Statistical validity met across all layers.".to_string()
    } else {
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_layer_report(layer: &str, status: LayerStatus, sample_size: usize) -> LayerReport {
        LayerReport {
            layer: layer.to_string(),
            sample_size,
            fixtures_evaluated: sample_size,
            fixtures_errored: 0,
            tp_count: 0,
            tn_count: 0,
            fp_count: 0,
            fn_count: 0,
            fp_rate: None,
            fn_rate: None,
            target_fp_rate: 0.01,
            target_fn_rate: 0.01,
            min_sample_size: MIN_SAMPLE_SIZE,
            meets_target: true,
            status,
            fixture_results: vec![],
            fixtures_with_reference_decision: None,
            human_agreement_data: None,
            human_agreement_rate: None,
        }
    }

    #[test]
    fn overall_status_red_miss_dominates() {
        let s = OverallStatus::from_layers(
            LayerStatus::Pass,
            LayerStatus::RedMiss,
            LayerStatus::FrameworkOnly,
        );
        assert!(matches!(s, OverallStatus::RedMiss));
    }

    #[test]
    fn overall_status_target_miss_beats_warnings() {
        let s = OverallStatus::from_layers(
            LayerStatus::TargetMiss,
            LayerStatus::PassWithSampleSizeWarning,
            LayerStatus::FrameworkOnly,
        );
        assert!(matches!(s, OverallStatus::TargetMiss));
    }

    #[test]
    fn overall_status_warning_propagates() {
        let s = OverallStatus::from_layers(
            LayerStatus::Pass,
            LayerStatus::PassWithSampleSizeWarning,
            LayerStatus::Pass,
        );
        assert!(matches!(s, OverallStatus::PassWithSampleSizeWarning));
    }

    #[test]
    fn overall_status_l3_framework_only_yields_warning() {
        let s = OverallStatus::from_layers(
            LayerStatus::Pass,
            LayerStatus::Pass,
            LayerStatus::FrameworkOnly,
        );
        assert!(matches!(s, OverallStatus::PassWithSampleSizeWarning));
    }

    #[test]
    fn overall_status_all_pass_full_size() {
        let s = OverallStatus::from_layers(
            LayerStatus::Pass,
            LayerStatus::Pass,
            LayerStatus::Pass,
        );
        assert!(matches!(s, OverallStatus::Pass));
    }

    #[test]
    fn overall_status_all_no_fixtures() {
        let s = OverallStatus::from_layers(
            LayerStatus::NoFixtures,
            LayerStatus::NoFixtures,
            LayerStatus::NoFixtures,
        );
        assert!(matches!(s, OverallStatus::NoFixtures));
    }

    #[test]
    fn promotion_readiness_v1_not_ready() {
        // v1 baseline: small fixture counts + L3 framework-only.
        let partial = PartialReportForReadiness {
            l1_sample: 8,
            l1_status: LayerStatus::PassWithSampleSizeWarning,
            l2_sample: 10,
            l2_status: LayerStatus::PassWithSampleSizeWarning,
            l3_status: LayerStatus::FrameworkOnly,
        };
        let r = PromotionReadiness::compute(&partial);
        assert!(!r.ready, "v1 must not be promotion-ready");
        assert!(r.gaps.iter().any(|g| g.contains("Layer 1 sample size 8")));
        assert!(r.gaps.iter().any(|g| g.contains("Layer 2 sample size 10")));
        assert!(r.gaps.iter().any(|g| g.contains("Layer 3")));
    }

    #[test]
    fn promotion_readiness_full_size_pass_is_ready() {
        let partial = PartialReportForReadiness {
            l1_sample: 35,
            l1_status: LayerStatus::Pass,
            l2_sample: 32,
            l2_status: LayerStatus::Pass,
            l3_status: LayerStatus::Pass,
        };
        let r = PromotionReadiness::compute(&partial);
        assert!(r.ready, "fully-sized pass should be ready");
        assert!(r.gaps.is_empty());
    }

    #[test]
    fn promotion_readiness_target_miss_blocks() {
        let partial = PartialReportForReadiness {
            l1_sample: 35,
            l1_status: LayerStatus::Pass,
            l2_sample: 32,
            l2_status: LayerStatus::TargetMiss,
            l3_status: LayerStatus::Pass,
        };
        let r = PromotionReadiness::compute(&partial);
        assert!(!r.ready);
        assert!(r.gaps.iter().any(|g| g.contains("Layer 2 status: target-miss")));
    }

    #[test]
    fn validity_note_warns_on_small_size() {
        let l1 = empty_layer_report("1", LayerStatus::PassWithSampleSizeWarning, 5);
        let l2 = empty_layer_report("2", LayerStatus::PassWithSampleSizeWarning, 8);
        let l3 = empty_layer_report("3", LayerStatus::FrameworkOnly, 4);
        let note = build_validity_note(&l1, &l2, &l3);
        assert!(note.contains("Layer 1"));
        assert!(note.contains("Layer 2"));
        assert!(note.contains("Layer 3"));
        assert!(note.contains("framework-only"));
    }
}
