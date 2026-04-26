//! `maintainer_delta` sensor — Phase 2.
//!
//! Flags packages where a maintainer was first observed within the
//! last `MAINTAINER_WINDOW_DAYS` (default 30 days). Uses
//! `state::PackageState`'s observation history — a maintainer that's
//! been recorded for >30 days is considered stable.
//!
//! First-run posture: on the very first scan of a package, all
//! maintainers are recorded in state but no findings are emitted —
//! this is a calibration scan. Subsequent scans flag genuinely new
//! arrivals.

use serde_json::json;

use crate::supply_chain_sca::Package;

use super::registry::FetchAllResult;
use super::scoring::{VigilanceFinding, VigilanceKind};
use super::state;

/// New-maintainer detection window. Default 30 days.
const MAINTAINER_WINDOW_DAYS: i64 = 30;

pub fn scan(
    packages: &[Package],
    metadata: &FetchAllResult,
    state_dir: &std::path::Path,
) -> Vec<VigilanceFinding> {
    let mut findings = Vec::new();
    let now = chrono::Utc::now();
    let mut seen: std::collections::HashSet<(String, &'static str)> =
        std::collections::HashSet::new();

    for pkg in packages {
        if !seen.insert((pkg.name.clone(), pkg.ecosystem)) {
            continue;
        }
        let Some(meta) = metadata.get(pkg) else {
            continue;
        };
        let pkg_state = state::load(state_dir, pkg.ecosystem, &pkg.name);

        // First-run check: if state has no observations, this is a
        // calibration scan. We rely on the orchestrator to persist
        // observations after the scan; do not flag.
        if pkg_state.observed_maintainers.is_empty() {
            continue;
        }

        let new_in_window =
            state::maintainers_new_in_window(&pkg_state, &meta.owners, MAINTAINER_WINDOW_DAYS, now);
        if new_in_window.is_empty() {
            continue;
        }

        // One finding per package, listing all new maintainers.
        let logins: Vec<String> = new_in_window.iter().map(|m| m.login.clone()).collect();
        let summary = if logins.len() == 1 {
            format!(
                "new maintainer in last {} days: {}",
                MAINTAINER_WINDOW_DAYS, logins[0]
            )
        } else {
            format!(
                "{} new maintainers in last {} days",
                logins.len(),
                MAINTAINER_WINDOW_DAYS
            )
        };

        findings.push(VigilanceFinding {
            kind: VigilanceKind::MaintainerDelta,
            package: pkg.clone(),
            summary,
            evidence: Some(json!({
                "new_maintainers": logins,
                "window_days": MAINTAINER_WINDOW_DAYS,
                "current_maintainer_count": meta.owners.len(),
                "historical_maintainer_count": pkg_state.observed_maintainers.len(),
            })),
            confidence: 0.7,
        });
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supply_chain_sca::Package;
    use crate::supply_chain_vigilance::registry::{
        FetchAllResult, MaintainerInfo, PackageMetadata,
    };
    use crate::supply_chain_vigilance::state::{MaintainerObservation, PackageState};
    use chrono::{TimeZone, Utc};
    use std::fs;

    fn meta_with_owners(
        name: &str,
        ecosystem: &str,
        owners: Vec<MaintainerInfo>,
    ) -> FetchAllResult {
        let mut r = FetchAllResult::default();
        r.metadata.insert(
            (ecosystem.to_string(), name.to_string()),
            PackageMetadata {
                ecosystem: ecosystem.to_string(),
                name: name.to_string(),
                versions: vec![],
                owners,
                repository_url: None,
                homepage_url: None,
            },
        );
        r
    }

    fn write_state(dir: &std::path::Path, ecosystem: &str, name: &str, state: &PackageState) {
        // Reuse the public `persist_after_scan` path indirectly: write JSON.
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(format!("{}:{}", ecosystem, name).as_bytes());
        let digest: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let mut p = dir.to_path_buf();
        p.push(ecosystem);
        fs::create_dir_all(&p).unwrap();
        p.push(format!("{}.json", digest));
        fs::write(&p, serde_json::to_string_pretty(state).unwrap()).unwrap();
    }

    fn maintainer(login: &str) -> MaintainerInfo {
        MaintainerInfo {
            login: login.to_string(),
            name: None,
            url: None,
        }
    }

    #[test]
    fn first_run_no_finding() {
        // No state on disk → first-run posture.
        let pkg = Package::npm("fakepkg", "1.0.0");
        let r = meta_with_owners("fakepkg", "npm", vec![maintainer("alice")]);
        let dir = tempfile::tempdir().unwrap();
        let _ = pkg;
        let pkg2 = Package::npm("fakepkg", "1.0.0");
        let f = scan(&[pkg2], &r, dir.path());
        assert!(f.is_empty());
    }

    #[test]
    fn maintainer_first_seen_recently_flagged() {
        let pkg = Package::npm("fakepkg", "1.0.0");
        // State is mature (first_scan_at well over 30 days ago).
        // carol joined recently; alice was an incumbent.
        let now = Utc::now();
        let watch_start = now - chrono::Duration::days(180);
        let mut s = PackageState::default();
        s.first_scan_at = Some(watch_start);
        s.observed_maintainers.insert(
            "alice".to_string(),
            MaintainerObservation {
                display_name: None,
                url: None,
                first_seen_at: watch_start, // incumbent
                last_seen_at: now,
            },
        );
        s.observed_maintainers.insert(
            "carol".to_string(),
            MaintainerObservation {
                display_name: None,
                url: None,
                first_seen_at: now - chrono::Duration::days(3), // joined recently
                last_seen_at: now,
            },
        );
        let dir = tempfile::tempdir().unwrap();
        write_state(dir.path(), "npm", "fakepkg", &s);

        let r = meta_with_owners(
            "fakepkg",
            "npm",
            vec![maintainer("alice"), maintainer("carol")],
        );
        let pkg2 = Package::npm("fakepkg", "1.0.0");
        let f = scan(&[pkg2], &r, dir.path());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].kind, VigilanceKind::MaintainerDelta);
        let ev = f[0].evidence.as_ref().unwrap();
        let new_maintainers = ev.get("new_maintainers").unwrap().as_array().unwrap();
        let logins: Vec<String> = new_maintainers
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(logins, vec!["carol"]);
    }

    #[test]
    fn maintainer_seen_long_ago_not_flagged() {
        let pkg = Package::npm("fakepkg", "1.0.0");
        let now = Utc::now();
        let watch_start = now - chrono::Duration::days(180);
        let mut s = PackageState::default();
        s.first_scan_at = Some(watch_start);
        s.observed_maintainers.insert(
            "alice".to_string(),
            MaintainerObservation {
                display_name: None,
                url: None,
                first_seen_at: now - chrono::Duration::days(100),
                last_seen_at: now,
            },
        );
        let dir = tempfile::tempdir().unwrap();
        write_state(dir.path(), "npm", "fakepkg", &s);

        let r = meta_with_owners("fakepkg", "npm", vec![maintainer("alice")]);
        let f = scan(&[pkg], &r, dir.path());
        assert!(f.is_empty());
    }

    #[test]
    fn calibration_period_suppresses_findings() {
        let pkg = Package::npm("fakepkg", "1.0.0");
        let now = Utc::now();
        let mut s = PackageState::default();
        // Just-created state — first_scan_at is now.
        s.first_scan_at = Some(now);
        s.observed_maintainers.insert(
            "alice".to_string(),
            MaintainerObservation {
                display_name: None,
                url: None,
                first_seen_at: now,
                last_seen_at: now,
            },
        );
        let dir = tempfile::tempdir().unwrap();
        write_state(dir.path(), "npm", "fakepkg", &s);

        let r = meta_with_owners("fakepkg", "npm", vec![maintainer("alice")]);
        let f = scan(&[pkg], &r, dir.path());
        assert!(
            f.is_empty(),
            "calibration period (state < 30 days old) should suppress findings"
        );
    }
}
