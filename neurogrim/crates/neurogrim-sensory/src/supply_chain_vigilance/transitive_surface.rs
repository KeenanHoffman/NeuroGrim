//! `transitive_surface_delta` sensor — Phase 2.
//!
//! Detects sudden surges in declared (direct) dependencies between
//! adjacent versions. A patch release (1.82.6 → 1.82.7) that
//! suddenly pulls in 40 new transitive deps is suspicious — both
//! a typical infection pattern (the malicious payload often pulls
//! in new utility libraries) and a soft-quality red flag.
//!
//! Heuristic:
//! - Find the package's CURRENT (lockfile-resolved) version in the
//!   metadata `versions` list.
//! - Find the immediate predecessor version (latest version with
//!   `published_at` strictly before the current's).
//! - If both have `dependency_count` populated AND the delta is
//!   ≥ ABSOLUTE_DELTA OR ≥ RELATIVE_DELTA × predecessor → flag.
//!
//! Pure-data: computed from registry metadata. No per-run state.

use serde_json::json;

use crate::supply_chain_sca::Package;

use super::registry::{FetchAllResult, VersionInfo};
use super::scoring::{VigilanceFinding, VigilanceKind};

/// Absolute dep-count delta that flags. Tunable.
const ABSOLUTE_DELTA: usize = 10;

/// Relative dep-count delta (e.g., 0.5 = 50% increase).
const RELATIVE_DELTA: f64 = 0.5;

pub fn scan(
    packages: &[Package],
    metadata: &FetchAllResult,
    _state_dir: &std::path::Path,
) -> Vec<VigilanceFinding> {
    let mut findings = Vec::new();
    let mut seen: std::collections::HashSet<(String, String, &'static str)> =
        std::collections::HashSet::new();

    for pkg in packages {
        if !seen.insert((pkg.name.clone(), pkg.version.clone(), pkg.ecosystem)) {
            continue;
        }
        let Some(meta) = metadata.get(pkg) else {
            continue;
        };

        // Find current version's index.
        let curr_idx = match meta.versions.iter().position(|v| v.version == pkg.version) {
            Some(i) => i,
            None => continue,
        };
        let curr: &VersionInfo = &meta.versions[curr_idx];
        if curr_idx == 0 {
            // No predecessor.
            continue;
        }
        let prev: &VersionInfo = &meta.versions[curr_idx - 1];

        // Both must have dep counts.
        let (curr_count, prev_count) = match (curr.dependency_count, prev.dependency_count) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };

        if curr_count <= prev_count {
            // No surge.
            continue;
        }
        let delta = curr_count - prev_count;
        let relative = if prev_count == 0 {
            f64::INFINITY
        } else {
            (delta as f64) / (prev_count as f64)
        };

        let absolute_trigger = delta >= ABSOLUTE_DELTA;
        let relative_trigger = relative >= RELATIVE_DELTA && delta >= 3;

        if absolute_trigger || relative_trigger {
            findings.push(VigilanceFinding {
                kind: VigilanceKind::TransitiveSurfaceDelta,
                package: pkg.clone(),
                summary: format!(
                    "dep count grew {} → {} (+{}, +{:.0}%) between {} and {}",
                    prev_count,
                    curr_count,
                    delta,
                    (relative * 100.0).min(9999.0),
                    prev.version,
                    curr.version,
                ),
                evidence: Some(json!({
                    "prev_version": prev.version,
                    "prev_dep_count": prev_count,
                    "curr_dep_count": curr_count,
                    "absolute_delta": delta,
                    "relative_delta": relative,
                    "absolute_threshold": ABSOLUTE_DELTA,
                    "relative_threshold": RELATIVE_DELTA,
                })),
                confidence: 0.5,
            });
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
    use chrono::{Duration, Utc};

    fn version_with_deps(v: &str, days_ago: i64, deps: usize) -> VersionInfo {
        VersionInfo {
            version: v.to_string(),
            published_at: Some(Utc::now() - Duration::days(days_ago)),
            yanked: false,
            dependency_count: Some(deps),
            dependency_names: None,
            tarball_url: None,
            tarball_sha256: None,
            attestation_status: AttestationStatus::None,
        }
    }

    fn meta_with(name: &str, ecosystem: &str, versions: Vec<VersionInfo>) -> FetchAllResult {
        let mut r = FetchAllResult::default();
        r.metadata.insert(
            (ecosystem.to_string(), name.to_string()),
            PackageMetadata {
                ecosystem: ecosystem.to_string(),
                name: name.to_string(),
                versions,
                owners: vec![],
                repository_url: None,
                homepage_url: None,
            },
        );
        r
    }

    #[test]
    fn steady_dep_count_no_finding() {
        let pkg = Package::npm("fakepkg", "1.0.5");
        let versions = vec![
            version_with_deps("1.0.0", 50, 5),
            version_with_deps("1.0.1", 40, 5),
            version_with_deps("1.0.2", 30, 5),
            version_with_deps("1.0.3", 20, 5),
            version_with_deps("1.0.4", 10, 5),
            version_with_deps("1.0.5", 1, 5),
        ];
        let r = meta_with("fakepkg", "npm", versions);
        let dir = tempfile::tempdir().unwrap();
        let f = scan(&[pkg], &r, dir.path());
        assert!(f.is_empty());
    }

    #[test]
    fn absolute_surge_flagged() {
        let pkg = Package::npm("fakepkg", "1.0.5");
        let versions = vec![
            version_with_deps("1.0.4", 10, 5),
            version_with_deps("1.0.5", 1, 20), // 5 → 20 = +15, ≥ 10
        ];
        let r = meta_with("fakepkg", "npm", versions);
        let dir = tempfile::tempdir().unwrap();
        let f = scan(&[pkg], &r, dir.path());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].kind, VigilanceKind::TransitiveSurfaceDelta);
    }

    #[test]
    fn relative_surge_flagged() {
        let pkg = Package::npm("fakepkg", "1.0.5");
        let versions = vec![
            version_with_deps("1.0.4", 10, 4),
            version_with_deps("1.0.5", 1, 8), // 4 → 8 = +4, +100% rel; relative_trigger
        ];
        let r = meta_with("fakepkg", "npm", versions);
        let dir = tempfile::tempdir().unwrap();
        let f = scan(&[pkg], &r, dir.path());
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn small_relative_below_threshold() {
        let pkg = Package::npm("fakepkg", "1.0.5");
        // 4 → 5 is +25% relative AND +1 absolute — neither
        // trigger should fire.
        let versions = vec![
            version_with_deps("1.0.4", 10, 4),
            version_with_deps("1.0.5", 1, 5),
        ];
        let r = meta_with("fakepkg", "npm", versions);
        let dir = tempfile::tempdir().unwrap();
        let f = scan(&[pkg], &r, dir.path());
        assert!(f.is_empty());
    }

    #[test]
    fn dep_decrease_not_flagged() {
        let pkg = Package::npm("fakepkg", "1.0.5");
        let versions = vec![
            version_with_deps("1.0.4", 10, 20),
            version_with_deps("1.0.5", 1, 5),
        ];
        let r = meta_with("fakepkg", "npm", versions);
        let dir = tempfile::tempdir().unwrap();
        let f = scan(&[pkg], &r, dir.path());
        assert!(f.is_empty(), "dep count decreasing should not flag");
    }

    #[test]
    fn missing_dep_count_skipped() {
        let pkg = Package::npm("fakepkg", "1.0.5");
        let mut versions = vec![
            version_with_deps("1.0.4", 10, 4),
            version_with_deps("1.0.5", 1, 30),
        ];
        // Remove dep count from current — sensor should skip.
        versions[1].dependency_count = None;
        let r = meta_with("fakepkg", "npm", versions);
        let dir = tempfile::tempdir().unwrap();
        let f = scan(&[pkg], &r, dir.path());
        assert!(f.is_empty());
    }
}
