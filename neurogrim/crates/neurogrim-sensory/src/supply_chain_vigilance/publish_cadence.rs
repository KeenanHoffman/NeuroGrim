//! `publish_cadence` sensor — Phase 2.
//!
//! Detects step-function changes in release frequency:
//!
//! 1. **Acceleration:** any inter-release gap in the *recent window*
//!    that is shorter than `0.1 ×` the historical median gap. e.g.,
//!    a package that historically released every 30 days suddenly
//!    releases two versions 2 days apart → flagged.
//! 2. **Post-dormancy:** any release within the last
//!    `POST_DORMANCY_WINDOW_DAYS` after a gap of ≥
//!    `POST_DORMANCY_DORMANCY_DAYS` (default: 365 days) → flagged.
//!
//! Both signals are based purely on the registry-metadata response;
//! no per-package state is needed.

use chrono::Duration;
use serde_json::json;

use crate::supply_chain_sca::Package;

use super::registry::{FetchAllResult, VersionInfo};
use super::scoring::{VigilanceFinding, VigilanceKind};

/// Minimum number of historical releases needed before we'll
/// produce an acceleration finding. Below this, the median is too
/// noisy to trust.
const MIN_HISTORY_FOR_ACCELERATION: usize = 5;

/// "Recent" window for both signals: any release within this many
/// days of `now` triggers signal evaluation.
const RECENT_WINDOW_DAYS: i64 = 30;

/// Acceleration multiplier: a recent inter-release gap < this × the
/// historical-median gap is flagged.
const ACCELERATION_RATIO: f64 = 0.1;

/// Dormancy threshold for post-dormancy signal.
const POST_DORMANCY_DORMANCY_DAYS: i64 = 365;

pub fn scan(packages: &[Package], metadata: &FetchAllResult) -> Vec<VigilanceFinding> {
    let mut findings = Vec::new();
    let now = chrono::Utc::now();

    // Dedup by (ecosystem, name) — we only need to score each package
    // once even if it appears at multiple versions in the lockfile.
    let mut seen: std::collections::HashSet<(String, &'static str)> =
        std::collections::HashSet::new();

    for pkg in packages {
        if !seen.insert((pkg.name.clone(), pkg.ecosystem)) {
            continue;
        }
        let Some(meta) = metadata.get(pkg) else {
            continue;
        };
        let versions: Vec<&VersionInfo> = meta
            .versions
            .iter()
            .filter(|v| v.published_at.is_some())
            .collect();

        if versions.len() < 2 {
            continue;
        }

        // Compute inter-release gaps in days.
        let mut gaps: Vec<i64> = Vec::with_capacity(versions.len() - 1);
        for w in versions.windows(2) {
            let prev = w[0].published_at.unwrap();
            let curr = w[1].published_at.unwrap();
            let d = curr.signed_duration_since(prev).num_days().max(0);
            gaps.push(d);
        }

        // ── Acceleration ───────────────────────────────────────────
        // v1 design (post-dogfood tuning, 2026-04-25): only flag the
        // gap LEADING TO our scanned version. The earlier "any recent
        // gap" heuristic flagged healthy-active crates whose RECENT
        // versions (newer than what we have installed) happened to
        // ship close together, which doesn't tell us anything about
        // OUR scanned version. The vigilance signal is "was THIS
        // installed version published anomalously fast vs predecessor
        // pattern."
        //
        // Filter same-day (0-day) gaps — they're typical fix-then-fix
        // CI cycles, not adversarial cadence. v1 minimum: ≥ 1 day.
        if gaps.len() >= MIN_HISTORY_FOR_ACCELERATION {
            let scanned_idx = versions
                .iter()
                .position(|v| v.version == pkg.version);
            if let Some(idx) = scanned_idx {
                if idx > 0 {
                    let scanned_at = versions[idx].published_at.unwrap();
                    let gap_to_scanned = gaps[idx - 1];
                    let in_window =
                        now.signed_duration_since(scanned_at) <= Duration::days(RECENT_WINDOW_DAYS);
                    if in_window && gap_to_scanned >= 1 {
                        // Median of historical gaps EXCLUDING the gap
                        // leading to our scanned version.
                        let mut historical: Vec<i64> = gaps
                            .iter()
                            .enumerate()
                            .filter_map(|(i, &g)| if i != idx - 1 { Some(g) } else { None })
                            .collect();
                        historical.sort_unstable();
                        let median = if historical.is_empty() {
                            0
                        } else if historical.len() % 2 == 0 {
                            (historical[historical.len() / 2 - 1]
                                + historical[historical.len() / 2])
                                / 2
                        } else {
                            historical[historical.len() / 2]
                        };
                        if median > 0
                            && (gap_to_scanned as f64) < (median as f64) * ACCELERATION_RATIO
                        {
                            findings.push(VigilanceFinding {
                                kind: VigilanceKind::PublishCadenceAcceleration,
                                package: pkg.clone(),
                                summary: format!(
                                    "scanned version {} published {}x faster than \
                                     historical median ({}d → {}d gap)",
                                    pkg.version,
                                    (median as f64 / gap_to_scanned.max(1) as f64).round() as u64,
                                    median,
                                    gap_to_scanned,
                                ),
                                evidence: Some(json!({
                                    "median_historical_gap_days": median,
                                    "gap_leading_to_scanned_days": gap_to_scanned,
                                    "acceleration_ratio_threshold": ACCELERATION_RATIO,
                                    "recent_window_days": RECENT_WINDOW_DAYS,
                                    "scanned_version": pkg.version,
                                })),
                                confidence: 0.7,
                            });
                        }
                    }
                }
            }
        }

        // ── Post-dormancy ─────────────────────────────────────────
        // Find the longest historical gap. If it >= POST_DORMANCY_DORMANCY_DAYS
        // AND the very next release after it falls within the recent window,
        // flag.
        for (i, w) in versions.windows(2).enumerate() {
            let g = gaps[i];
            if g < POST_DORMANCY_DORMANCY_DAYS {
                continue;
            }
            let post_dormancy_release = w[1].published_at.unwrap();
            if now.signed_duration_since(post_dormancy_release)
                <= Duration::days(RECENT_WINDOW_DAYS)
            {
                findings.push(VigilanceFinding {
                    kind: VigilanceKind::PublishCadencePostDormancy,
                    package: pkg.clone(),
                    summary: format!(
                        "release after {}-day dormancy",
                        g
                    ),
                    evidence: Some(json!({
                        "dormancy_days": g,
                        "dormancy_threshold_days": POST_DORMANCY_DORMANCY_DAYS,
                        "post_dormancy_version": w[1].version,
                        "recent_window_days": RECENT_WINDOW_DAYS,
                    })),
                    confidence: 0.65,
                });
                break; // one finding per package; don't double-flag.
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supply_chain_sca::Package;
    use crate::supply_chain_vigilance::registry::{
        AttestationStatus, FetchAllResult, PackageMetadata, VersionInfo,
    };
    use chrono::Utc;

    fn version(v: &str, days_ago: i64) -> VersionInfo {
        VersionInfo {
            version: v.to_string(),
            published_at: Some(Utc::now() - Duration::days(days_ago)),
            yanked: false,
            dependency_count: None,
            dependency_names: None,
            tarball_url: None,
            tarball_sha256: None,
            attestation_status: AttestationStatus::None,
        }
    }

    fn metadata_with_versions(versions: Vec<VersionInfo>) -> FetchAllResult {
        let mut r = FetchAllResult::default();
        let pkg_meta = PackageMetadata {
            ecosystem: "PyPI".to_string(),
            name: "fakepkg".to_string(),
            versions,
            owners: vec![],
            repository_url: None,
            homepage_url: None,
        };
        r.metadata
            .insert(("PyPI".to_string(), "fakepkg".to_string()), pkg_meta);
        r
    }

    #[test]
    fn empty_history_no_findings() {
        let pkg = Package::pypi("fakepkg", "1.0.0");
        let r = metadata_with_versions(vec![]);
        let f = scan(&[pkg], &r);
        assert!(f.is_empty());
    }

    #[test]
    fn steady_cadence_no_findings() {
        let pkg = Package::pypi("fakepkg", "1.0.0");
        // Every 30 days for 6 versions; latest 30d ago.
        let versions: Vec<VersionInfo> = (0..6)
            .map(|i| version(&format!("1.0.{}", i), 30 * (5 - i)))
            .collect();
        let r = metadata_with_versions(versions);
        let f = scan(&[pkg], &r);
        assert!(f.is_empty(), "steady cadence should not flag: {f:?}");
    }

    #[test]
    fn acceleration_flagged() {
        // Scanned version is 1.0.7. Historical gaps are 30 days each.
        // Gap leading to 1.0.7 is 1 day. 1d ≪ 0.1 × 30 = 3d, so flag.
        let pkg = Package::pypi("fakepkg", "1.0.7");
        let mut versions: Vec<VersionInfo> = (0..6)
            .map(|i| version(&format!("1.0.{}", i), 30 * (8 - i) + 5))
            .collect();
        versions.push(version("1.0.6", 5));
        versions.push(version("1.0.7", 4));
        let r = metadata_with_versions(versions);
        let f = scan(&[pkg], &r);
        assert!(
            f.iter()
                .any(|x| x.kind == VigilanceKind::PublishCadenceAcceleration),
            "expected acceleration finding: {f:?}"
        );
    }

    #[test]
    fn acceleration_zero_day_gap_not_flagged() {
        // Same-day fix release: 0-day gap leading to scanned version.
        // v1 filters these as too noisy (CI fix patterns are normal).
        let pkg = Package::pypi("fakepkg", "1.0.7");
        let mut versions: Vec<VersionInfo> = (0..6)
            .map(|i| version(&format!("1.0.{}", i), 30 * (8 - i) + 5))
            .collect();
        versions.push(version("1.0.6", 5));
        versions.push(version("1.0.7", 5)); // Same day as 1.0.6
        let r = metadata_with_versions(versions);
        let f = scan(&[pkg], &r);
        assert!(
            !f.iter()
                .any(|x| x.kind == VigilanceKind::PublishCadenceAcceleration),
            "0-day gap should be filtered as v1-noise: {f:?}"
        );
    }

    #[test]
    fn acceleration_only_flags_gap_to_scanned_version() {
        // Healthy active crate: scanned version (1.0.0) is months
        // old; subsequent releases happen close together but they
        // post-date our scanned version. v1 only cares about the gap
        // leading TO the scanned version. This avoids the FP swarm
        // we saw on tokio/wasm-bindgen-style active crates.
        let pkg = Package::pypi("fakepkg", "1.0.0");
        let versions = vec![
            version("0.1.0", 200), // 200d old, gap 100d to 1.0.0
            version("1.0.0", 100), // scanned
            version("1.0.1", 5),   // 95d gap, then close-together releases
            version("1.0.2", 4),
            version("1.0.3", 3),
        ];
        let r = metadata_with_versions(versions);
        let f = scan(&[pkg], &r);
        assert!(
            !f.iter()
                .any(|x| x.kind == VigilanceKind::PublishCadenceAcceleration),
            "scanned version not in recent window — should not flag: {f:?}"
        );
    }

    #[test]
    fn post_dormancy_flagged() {
        let pkg = Package::pypi("fakepkg", "1.0.0");
        // 1.0.0 released 500 days ago. 1.0.1 released 5 days ago.
        // 500-day gap is well over dormancy threshold.
        let versions = vec![version("1.0.0", 500), version("1.0.1", 5)];
        let r = metadata_with_versions(versions);
        let f = scan(&[pkg], &r);
        assert!(
            f.iter()
                .any(|x| x.kind == VigilanceKind::PublishCadencePostDormancy),
            "expected post-dormancy finding: {f:?}"
        );
    }

    #[test]
    fn dormancy_long_ago_not_flagged() {
        let pkg = Package::pypi("fakepkg", "1.0.0");
        // 1.0.0: 1500 days ago. 1.0.1: 1000 days ago. 500-day gap,
        // BUT post-dormancy release was 1000 days ago — outside the
        // recent window. Don't flag.
        let versions = vec![version("1.0.0", 1500), version("1.0.1", 1000)];
        let r = metadata_with_versions(versions);
        let f = scan(&[pkg], &r);
        assert!(
            !f.iter()
                .any(|x| x.kind == VigilanceKind::PublishCadencePostDormancy),
            "dormancy ended long ago should not flag: {f:?}"
        );
    }

    #[test]
    fn missing_metadata_skipped() {
        let pkg = Package::pypi("never-fetched", "1.0.0");
        let r = FetchAllResult::default();
        let f = scan(&[pkg], &r);
        assert!(f.is_empty());
    }
}
