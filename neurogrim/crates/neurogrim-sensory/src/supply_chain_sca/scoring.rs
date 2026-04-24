//! Scoring model: advisories + accepted-set → (score, findings, extras).
//!
//! # Rubric (v1)
//!
//! Count-based, non-linear deduction on UNACCEPTED advisories:
//!
//! | Unaccepted advisories | Score |
//! |---|---|
//! | 0 | 100 |
//! | 1 | 75 |
//! | 2 | 50 |
//! | 3 | 25 |
//! | 4+ | 0 |
//!
//! Accepted advisories (matching an entry in
//! `.claude/supply-chain-accepted-advisories.toml`) do NOT contribute
//! to the unaccepted count — they appear in findings with
//! `status = "accepted"` and `points = 0` so the operator can see
//! the full picture without them dragging the score.
//!
//! # Rationale for count-based
//!
//! OSV's `querybatch` response does not carry per-advisory severity.
//! Getting CVSS scores requires a follow-up `GET /v1/vulns/{id}` per
//! advisory. Many RustSec advisories (especially `informational =
//! "unmaintained"`) have no severity rating at all — the rubric must
//! handle absence of severity data as the common case.
//!
//! A count-based model is honest about what we can measure reliably
//! at batch-query time. Severity-weighted scoring may be added in
//! E-SC-8 calibration if the count rubric proves inadequate on the
//! fixture library.

use serde_json::{json, Value};
use std::collections::HashSet;

use super::{Advisory, AdvisorySource};
use crate::cmdb::Finding;

/// Compute the SCA score + findings + extras.
///
/// `advisories` should already be deduplicated by the caller (the
/// parent module unions OSV + RustSec-local and drops exact
/// `(id, package, version)` duplicates).
pub fn compute(
    advisories: &[Advisory],
    accepted: &HashSet<String>,
    _packages_scanned: usize,
) -> (u8, Vec<Finding>, Vec<(&'static str, Value)>) {
    let mut findings = Vec::with_capacity(advisories.len());
    let mut accepted_ids: Vec<&str> = Vec::new();
    let mut unaccepted_count: usize = 0;

    for adv in advisories {
        let is_accepted = accepted.contains(&adv.id);
        if is_accepted {
            accepted_ids.push(&adv.id);
        } else {
            unaccepted_count += 1;
        }
        findings.push(build_finding(adv, is_accepted));
    }

    let score = score_from_count(unaccepted_count);
    let extras: Vec<(&'static str, Value)> = vec![
        ("advisories_found", json!(advisories.len())),
        ("advisories_unaccepted", json!(unaccepted_count)),
        ("advisories_accepted", json!(accepted_ids.len())),
        (
            "accepted_advisory_ids",
            json!(accepted_ids.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
        ),
    ];
    (score, findings, extras)
}

/// Map unaccepted advisory count to score per the rubric above.
pub fn score_from_count(unaccepted: usize) -> u8 {
    match unaccepted {
        0 => 100,
        1 => 75,
        2 => 50,
        3 => 25,
        _ => 0,
    }
}

/// Build a `Finding` for a single advisory.
fn build_finding(adv: &Advisory, is_accepted: bool) -> Finding {
    let source_label = match adv.source {
        AdvisorySource::Osv => "OSV",
        AdvisorySource::RustsecLocal => "RustSec-local",
    };
    let kind_label = adv
        .informational
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("vulnerability");

    let detail = format!(
        "{} {}: {} advisory ({}){}",
        adv.package.name,
        adv.package.version,
        source_label,
        kind_label,
        if is_accepted {
            " — accepted by operator"
        } else {
            ""
        },
    );

    Finding {
        name: adv.id.clone(),
        status: if is_accepted {
            "accepted".to_string()
        } else {
            "advisory".to_string()
        },
        // Per-finding "points" is a deduction hint, not a score-sum.
        // Accepted: 0 (no score impact). Unaccepted: -25 (the slope
        // of the count-based rubric at its fine-grained 1-3 range).
        points: if is_accepted { 0 } else { -25 },
        detail: Some(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supply_chain_sca::Package;

    fn adv(id: &str, pkg: &str, version: &str, informational: Option<&str>) -> Advisory {
        Advisory {
            id: id.to_string(),
            package: Package {
                name: pkg.to_string(),
                version: version.to_string(),
            },
            summary: None,
            source: AdvisorySource::Osv,
            informational: informational.map(str::to_string),
        }
    }

    fn adv_rustsec(id: &str, pkg: &str, version: &str) -> Advisory {
        Advisory {
            id: id.to_string(),
            package: Package {
                name: pkg.to_string(),
                version: version.to_string(),
            },
            summary: None,
            source: AdvisorySource::RustsecLocal,
            informational: Some("unmaintained".to_string()),
        }
    }

    #[test]
    fn score_table() {
        assert_eq!(score_from_count(0), 100);
        assert_eq!(score_from_count(1), 75);
        assert_eq!(score_from_count(2), 50);
        assert_eq!(score_from_count(3), 25);
        assert_eq!(score_from_count(4), 0);
        assert_eq!(score_from_count(100), 0);
    }

    #[test]
    fn empty_advisories_means_perfect_score() {
        let (score, findings, extras) = compute(&[], &HashSet::new(), 100);
        assert_eq!(score, 100);
        assert!(findings.is_empty());
        let found = extras.iter().find(|(k, _)| *k == "advisories_found").unwrap();
        assert_eq!(found.1, json!(0));
    }

    #[test]
    fn single_unaccepted_deducts_to_75() {
        let advisories = vec![adv(
            "RUSTSEC-2026-0104",
            "rustls-webpki",
            "0.103.12",
            None,
        )];
        let (score, findings, extras) = compute(&advisories, &HashSet::new(), 100);
        assert_eq!(score, 75);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].name, "RUSTSEC-2026-0104");
        assert_eq!(findings[0].status, "advisory");
        assert_eq!(findings[0].points, -25);
        let got = extras
            .iter()
            .find(|(k, _)| *k == "advisories_unaccepted")
            .unwrap();
        assert_eq!(got.1, json!(1));
    }

    #[test]
    fn accepted_advisory_does_not_deduct() {
        let advisories = vec![adv_rustsec("RUSTSEC-2024-0436", "paste", "1.0.15")];
        let mut accepted = HashSet::new();
        accepted.insert("RUSTSEC-2024-0436".to_string());

        let (score, findings, extras) = compute(&advisories, &accepted, 100);
        assert_eq!(score, 100);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].status, "accepted");
        assert_eq!(findings[0].points, 0);
        assert!(findings[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("accepted by operator"));

        let unaccepted = extras
            .iter()
            .find(|(k, _)| *k == "advisories_unaccepted")
            .unwrap();
        assert_eq!(unaccepted.1, json!(0));

        let accepted_ids = extras
            .iter()
            .find(|(k, _)| *k == "accepted_advisory_ids")
            .unwrap();
        assert_eq!(accepted_ids.1, json!(["RUSTSEC-2024-0436"]));
    }

    #[test]
    fn mixed_accepted_and_unaccepted() {
        let advisories = vec![
            adv_rustsec("RUSTSEC-2024-0436", "paste", "1.0.15"), // will be accepted
            adv(
                "RUSTSEC-2026-0104",
                "rustls-webpki",
                "0.103.12",
                None,
            ), // unaccepted
            adv(
                "RUSTSEC-2099-XXXX",
                "other-crate",
                "1.0.0",
                Some("notice"),
            ), // unaccepted
        ];
        let mut accepted = HashSet::new();
        accepted.insert("RUSTSEC-2024-0436".to_string());

        let (score, findings, extras) = compute(&advisories, &accepted, 100);
        // 2 unaccepted → 50
        assert_eq!(score, 50);
        assert_eq!(findings.len(), 3);
        // 1 accepted + 2 unaccepted
        assert_eq!(
            findings.iter().filter(|f| f.status == "accepted").count(),
            1
        );
        assert_eq!(
            findings.iter().filter(|f| f.status == "advisory").count(),
            2
        );
        let unaccepted = extras
            .iter()
            .find(|(k, _)| *k == "advisories_unaccepted")
            .unwrap();
        assert_eq!(unaccepted.1, json!(2));
        let accepted_count = extras
            .iter()
            .find(|(k, _)| *k == "advisories_accepted")
            .unwrap();
        assert_eq!(accepted_count.1, json!(1));
    }

    #[test]
    fn four_or_more_clamps_to_zero() {
        let advisories: Vec<Advisory> = (0..5)
            .map(|i| {
                adv(
                    &format!("RUSTSEC-2026-010{i}"),
                    "some-crate",
                    "1.0.0",
                    None,
                )
            })
            .collect();
        let (score, _, extras) = compute(&advisories, &HashSet::new(), 100);
        assert_eq!(score, 0);
        let unaccepted = extras
            .iter()
            .find(|(k, _)| *k == "advisories_unaccepted")
            .unwrap();
        assert_eq!(unaccepted.1, json!(5));
    }

    #[test]
    fn finding_detail_includes_source_and_kind() {
        let advisories = vec![adv_rustsec("RUSTSEC-2024-0436", "paste", "1.0.15")];
        let (_, findings, _) = compute(&advisories, &HashSet::new(), 100);
        let detail = findings[0].detail.as_deref().unwrap();
        assert!(detail.contains("paste"));
        assert!(detail.contains("1.0.15"));
        assert!(detail.contains("RustSec-local"));
        assert!(detail.contains("unmaintained"));
    }

    #[test]
    fn finding_detail_for_real_vuln_says_vulnerability() {
        let advisories = vec![adv(
            "RUSTSEC-2026-0104",
            "rustls-webpki",
            "0.103.12",
            None, // None means "not informational" = real vuln
        )];
        let (_, findings, _) = compute(&advisories, &HashSet::new(), 100);
        let detail = findings[0].detail.as_deref().unwrap();
        assert!(detail.contains("vulnerability"));
        assert!(!detail.contains("unmaintained"));
    }
}
