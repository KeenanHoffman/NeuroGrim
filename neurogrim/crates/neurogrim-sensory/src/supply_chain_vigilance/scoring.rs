//! Layer 2 vigilance scoring + CMDB envelope assembly.
//!
//! Count-based v1 rubric: starting at 100, each finding deducts a
//! fixed amount tuned per signal kind. Caps at 0. This mirrors Layer
//! 1's posture (honest about what the sensor can measure reliably);
//! severity-weighted scoring is an E-SC-8 calibration candidate.
//!
//! Per-finding deduction (v1):
//! - typosquat hit: 25 pts (high confidence; very few false positives
//!   when distance ≤ 1 and target package is in the top-1000).
//! - publish_cadence flag: 15 pts.
//! - maintainer_delta: 15 pts.
//! - transitive_surface_delta: 10 pts.
//! - signature_gap: 10 pts.
//! - binary_reproducibility mismatch: 20 pts.
//! - exfil_indicator (per-pattern): 25 pts.

use crate::cmdb::Finding as CmdbFinding;
use crate::supply_chain_sca::Package;
use serde_json::{json, Value};
use std::collections::BTreeMap;

use super::registry::FetchAllResult;

/// One Layer 2 finding. Each sub-sensor produces zero or more.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VigilanceFinding {
    /// Stable identifier for this finding kind (e.g.,
    /// `"typosquat-proximity"`, `"publish-cadence-acceleration"`).
    pub kind: VigilanceKind,
    /// The package the finding is about.
    pub package: Package,
    /// One-sentence summary of WHAT was detected. Non-attributive
    /// language per spec §16.4 — describe behavior, not maintainer
    /// intent.
    pub summary: String,
    /// Optional structured evidence (counts, timestamps, distances)
    /// supporting the summary.
    pub evidence: Option<Value>,
    /// Confidence in [0, 1]. Used by future severity-weighted
    /// scoring; v1 rubric ignores confidence (count-based only).
    pub confidence: f32,
}

/// Closed enum of finding kinds. Each Layer 2 sub-sensor emits one
/// or more of these. Stable identifiers — operators may filter on
/// these strings in tooling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VigilanceKind {
    /// `typosquat::scan` — Levenshtein ≤ 1 to a top-1000 popular package.
    TyposquatProximity,
    /// `publish_cadence::scan` — release-frequency step-function.
    PublishCadenceAcceleration,
    /// `publish_cadence::scan` — long dormancy then a release.
    PublishCadencePostDormancy,
    /// `maintainer_delta::scan` — new maintainer in window.
    MaintainerDelta,
    /// `transitive_surface::scan` — dep-count surge.
    TransitiveSurfaceDelta,
    /// `signature_gap::scan` — attestation/signature dropped vs prior.
    SignatureGap,
    /// `binary_reproducibility::scan` — registry tarball ≠ source-tag tarball.
    BinaryReproducibilityMismatch,
    /// `exfil_indicator::scan` — added base64/eval/exec/subprocess/network in a recent version.
    ExfilIndicator,
    /// Sensor-internal infrastructure issue (e.g., registry unreachable for one package).
    /// Surfaced for operator visibility but does not deduct from score (informational).
    SensorDegradation,
}

impl VigilanceKind {
    /// Stable string identifier for a finding kind. Used in CMDB
    /// findings, in operator-tooling filters, and in future
    /// severity-weighted scoring tables.
    pub fn as_str(&self) -> &'static str {
        match self {
            VigilanceKind::TyposquatProximity => "typosquat-proximity",
            VigilanceKind::PublishCadenceAcceleration => "publish-cadence-acceleration",
            VigilanceKind::PublishCadencePostDormancy => "publish-cadence-post-dormancy",
            VigilanceKind::MaintainerDelta => "maintainer-delta",
            VigilanceKind::TransitiveSurfaceDelta => "transitive-surface-delta",
            VigilanceKind::SignatureGap => "signature-gap",
            VigilanceKind::BinaryReproducibilityMismatch => "binary-reproducibility-mismatch",
            VigilanceKind::ExfilIndicator => "exfil-indicator",
            VigilanceKind::SensorDegradation => "sensor-degradation",
        }
    }

    /// v1 count-based deduction per finding. See module docs for
    /// rationale on the per-kind weights.
    pub fn deduction(&self) -> u32 {
        match self {
            VigilanceKind::TyposquatProximity => 25,
            VigilanceKind::PublishCadenceAcceleration => 15,
            VigilanceKind::PublishCadencePostDormancy => 15,
            VigilanceKind::MaintainerDelta => 15,
            VigilanceKind::TransitiveSurfaceDelta => 10,
            VigilanceKind::SignatureGap => 10,
            VigilanceKind::BinaryReproducibilityMismatch => 20,
            VigilanceKind::ExfilIndicator => 25,
            VigilanceKind::SensorDegradation => 0,
        }
    }
}

/// Compute the final 0..=100 score given all findings.
///
/// Starting from 100, deduct each finding's `deduction()` value.
/// Floor at 0; never negative.
pub fn score(findings: &[VigilanceFinding]) -> u32 {
    let total: u32 = findings.iter().map(|f| f.kind.deduction()).sum();
    100u32.saturating_sub(total)
}

/// Build the CMDB envelope from the score + findings + counters.
pub fn build_cmdb_envelope(
    score_value: u32,
    findings: &[VigilanceFinding],
    packages: &[Package],
    packages_by_ecosystem: &BTreeMap<&'static str, usize>,
    metadata_result: &FetchAllResult,
    parse_errors: &[String],
) -> Value {
    let mut cmdb_findings: Vec<CmdbFinding> = Vec::with_capacity(findings.len() + 4);

    // One CMDB finding per VigilanceFinding (operator-readable).
    for f in findings {
        let detail = match &f.evidence {
            Some(ev) => format!(
                "{} [{}@{}, {}] {}",
                f.kind.as_str(),
                f.package.name,
                f.package.version,
                f.package.ecosystem,
                short_evidence(ev),
            ),
            None => format!(
                "{} [{}@{}, {}]",
                f.kind.as_str(),
                f.package.name,
                f.package.version,
                f.package.ecosystem,
            ),
        };
        let status = match f.kind {
            VigilanceKind::SensorDegradation => "info",
            _ => "warning",
        }
        .to_string();
        cmdb_findings.push(CmdbFinding {
            name: format!(
                "{}:{}:{}",
                f.kind.as_str(),
                f.package.ecosystem,
                f.package.name
            ),
            status,
            points: f.kind.deduction() as i32,
            detail: Some(format!("{} — {}", detail, f.summary)),
        });
    }

    // Counters per kind for CMDB extras.
    let mut findings_by_kind: BTreeMap<&'static str, u32> = BTreeMap::new();
    for f in findings {
        *findings_by_kind.entry(f.kind.as_str()).or_insert(0) += 1;
    }

    // Per-ecosystem package counts as JSON.
    let by_eco_json = packages_by_ecosystem
        .iter()
        .map(|(k, v)| (k.to_string(), json!(*v)))
        .collect::<serde_json::Map<_, _>>();

    let by_kind_json = findings_by_kind
        .iter()
        .map(|(k, v)| (k.to_string(), json!(*v)))
        .collect::<serde_json::Map<_, _>>();

    let extras: Vec<(&str, Value)> = vec![
        ("total_packages_scanned", json!(packages.len())),
        ("ecosystems_scanned", json!(packages_by_ecosystem.keys().collect::<Vec<_>>())),
        ("packages_by_ecosystem", json!(by_eco_json)),
        ("findings_total", json!(findings.len())),
        ("findings_by_kind", json!(by_kind_json)),
        ("registry_cache_hits", json!(metadata_result.cache_hits)),
        ("registry_live_queries", json!(metadata_result.live_queries)),
        (
            "registry_cache_bypassed",
            json!(metadata_result.cache_bypassed),
        ),
        (
            "registry_oldest_cache_age_seconds",
            metadata_result
                .oldest_cache_age_seconds
                .map_or(json!(null), |v| json!(v)),
        ),
        ("vigilance_reachable", json!(metadata_result.any_reachable)),
        (
            "registry_unreachable_ecosystems",
            json!(metadata_result.unreachable_ecosystems),
        ),
        // 2026-04-26 PRE-RELEASE A11 fix: when the HTTP client
        // itself failed to build, surface the reason so operators
        // see "client setup failed" not only "all registries
        // unreachable". Null on the happy path.
        (
            "registry_client_error",
            metadata_result
                .client_error
                .as_deref()
                .map(|s| json!(s))
                .unwrap_or(json!(null)),
        ),
        (
            "lockfile_parse_errors",
            json!(parse_errors.iter().cloned().collect::<Vec<_>>()),
        ),
        (
            "_impl_status",
            json!(
                "E-SC-5 (Layer 2 vigilance): all 7 sub-sensors active. \
                 typosquat + publish-cadence + maintainer-delta + \
                 transitive-surface + signature-gap + binary-reproducibility + \
                 exfil-indicator. Registry-metadata 7-day file cache. NO external \
                 scanner binaries. Default weight 0.0 (advisory) per spec §16.3."
            ),
        ),
    ];

    crate::cmdb::build_cmdb(
        "supply-chain-vigilance",
        score_value as u8,
        cmdb_findings,
        Some(extras),
    )
}

/// Render structured `evidence` as a short inline summary for CMDB
/// `detail` fields. Avoids dumping multi-line JSON into the CMDB.
fn short_evidence(ev: &Value) -> String {
    match ev {
        Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .take(4)
                .map(|(k, v)| format!("{}={}", k, short_value(v)))
                .collect();
            format!("({})", parts.join(", "))
        }
        other => format!("({})", short_value(other)),
    }
}

fn short_value(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            if s.len() > 40 {
                format!("\"{}…\"", &s[..40])
            } else {
                format!("\"{}\"", s)
            }
        }
        Value::Array(a) => format!("[len={}]", a.len()),
        Value::Object(o) => format!("{{len={}}}", o.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supply_chain_sca::Package;

    fn finding(kind: VigilanceKind) -> VigilanceFinding {
        VigilanceFinding {
            kind,
            package: Package::crates_io("test", "1.0.0"),
            summary: "test finding".to_string(),
            evidence: None,
            confidence: 1.0,
        }
    }

    #[test]
    fn empty_findings_score_100() {
        assert_eq!(score(&[]), 100);
    }

    #[test]
    fn one_typosquat_finding_score_75() {
        let findings = vec![finding(VigilanceKind::TyposquatProximity)];
        assert_eq!(score(&findings), 75);
    }

    #[test]
    fn five_typosquat_findings_floor_at_0() {
        let findings = vec![
            finding(VigilanceKind::TyposquatProximity),
            finding(VigilanceKind::TyposquatProximity),
            finding(VigilanceKind::TyposquatProximity),
            finding(VigilanceKind::TyposquatProximity),
            finding(VigilanceKind::TyposquatProximity),
        ];
        assert_eq!(score(&findings), 0);
    }

    #[test]
    fn sensor_degradation_does_not_deduct() {
        let findings = vec![
            finding(VigilanceKind::SensorDegradation),
            finding(VigilanceKind::SensorDegradation),
        ];
        assert_eq!(score(&findings), 100);
    }

    #[test]
    fn mixed_findings_sum_correctly() {
        // 25 (typosquat) + 15 (publish-cadence-acceleration) +
        // 10 (transitive-surface) = 50 deduction; score = 50.
        let findings = vec![
            finding(VigilanceKind::TyposquatProximity),
            finding(VigilanceKind::PublishCadenceAcceleration),
            finding(VigilanceKind::TransitiveSurfaceDelta),
        ];
        assert_eq!(score(&findings), 50);
    }

    #[test]
    fn kind_string_identifiers_are_kebab_case() {
        // Stability check: operator tooling and future calibration
        // tables key off these strings. Don't change without
        // versioning.
        assert_eq!(
            VigilanceKind::TyposquatProximity.as_str(),
            "typosquat-proximity"
        );
        assert_eq!(
            VigilanceKind::ExfilIndicator.as_str(),
            "exfil-indicator"
        );
        assert_eq!(
            VigilanceKind::BinaryReproducibilityMismatch.as_str(),
            "binary-reproducibility-mismatch"
        );
    }
}
