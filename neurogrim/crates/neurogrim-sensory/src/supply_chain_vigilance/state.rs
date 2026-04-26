//! Per-package historical state for cross-run delta sensors.
//!
//! Most Layer 2 sensors compute their findings purely from the
//! registry metadata returned by `registry::fetch_all`. The
//! exception is `maintainer_delta`, which needs to know when each
//! maintainer was FIRST seen across our scan history — registries
//! don't expose maintainer-onboarding timestamps directly.
//!
//! The shape kept here is intentionally minimal: we record the union
//! of all maintainer logins we've ever observed per package, with a
//! `first_seen_at` timestamp set the first time we saw them. On each
//! subsequent scan, the maintainer-delta sensor compares CURRENT
//! maintainers vs this union to detect new arrivals within a
//! configured window.
//!
//! Storage: one JSON file per `(ecosystem, name)` at
//! `<state_dir>/<ecosystem>/<sha256>.json`. Same hashing scheme as
//! the registry-metadata cache, different parent directory. Files
//! are gitignored.
//!
//! Persistence happens at the END of the scan via
//! `persist_after_scan` — sensors during the scan READ state but
//! only the orchestrator writes (so a partial scan doesn't pollute
//! state with half-observations).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::supply_chain_sca::Package;

use super::registry::{FetchAllResult, MaintainerInfo};

/// Recorded historical state for one package.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackageState {
    /// Set of (login → first_seen_at) observations across all our
    /// scans. Maintainers we've seen are recorded here permanently
    /// (no expiration); the maintainer-delta sensor uses the
    /// `first_seen_at` to decide whether the maintainer is "new in
    /// window."
    pub observed_maintainers: HashMap<String, MaintainerObservation>,
    /// Last time we scanned this package. Diagnostic only.
    pub last_scan_at: Option<DateTime<Utc>>,
    /// First time we scanned this package. Set once on the first
    /// successful state-save; never updated thereafter. Used by
    /// `maintainers_new_in_window` to distinguish "maintainer added
    /// during our watch period" from "maintainer was here when we
    /// started watching but our state is just young."
    pub first_scan_at: Option<DateTime<Utc>>,
    /// Schema version. Bump on shape changes.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintainerObservation {
    /// Display name (or email for npm) — captured at first sight,
    /// preserved across scans.
    pub display_name: Option<String>,
    /// Optional URL on the registry.
    pub url: Option<String>,
    /// Timestamp of our FIRST observation.
    pub first_seen_at: DateTime<Utc>,
    /// Timestamp of our most recent observation. Maintainers can
    /// disappear (registry rotation); we don't delete them from
    /// `observed_maintainers` but `last_seen_at` lets future logic
    /// reason about activity.
    pub last_seen_at: DateTime<Utc>,
}

/// Read the on-disk state for one package. Returns default
/// (empty) state if file is missing or malformed — first-run
/// behavior.
pub fn load(state_dir: &Path, ecosystem: &str, name: &str) -> PackageState {
    match try_load(state_dir, ecosystem, name) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(
                "vigilance state load: returning default for {}/{} ({:#})",
                ecosystem,
                name,
                e
            );
            PackageState::default()
        }
    }
}

fn try_load(state_dir: &Path, ecosystem: &str, name: &str) -> Result<PackageState> {
    let path = state_file_path(state_dir, ecosystem, name);
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let state: PackageState = serde_json::from_str(&raw).context("parse state JSON")?;
    Ok(state)
}

/// Write per-package state at the end of a scan.
///
/// For each scanned package: load its prior state, merge the
/// currently-observed maintainers (updating `last_seen_at` on
/// existing ones; setting `first_seen_at` on new ones), and write
/// back. Per-package atomic write (write to tmp, rename) avoids
/// partial state on crash.
pub fn persist_after_scan(
    packages: &[Package],
    metadata_result: &FetchAllResult,
    state_dir: &Path,
) {
    let now = Utc::now();
    // Dedup by (ecosystem, name) — multiple package entries pointing
    // at the same registry name only need one state-file update.
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

    for pkg in packages {
        let key = (pkg.ecosystem.to_string(), pkg.name.clone());
        if !seen.insert(key.clone()) {
            continue;
        }
        let Some(meta) = metadata_result.get(pkg) else {
            continue; // No metadata fetched; can't update observations.
        };
        let mut state = load(state_dir, pkg.ecosystem, &pkg.name);
        update_observations(&mut state, &meta.owners, now);
        state.last_scan_at = Some(now);
        if state.first_scan_at.is_none() {
            state.first_scan_at = Some(now);
        }
        state.schema_version = 1;
        if let Err(e) = save(state_dir, pkg.ecosystem, &pkg.name, &state) {
            tracing::warn!(
                "vigilance: failed to persist state for {}/{}: {:#}",
                pkg.ecosystem,
                pkg.name,
                e
            );
        }
    }
}

fn update_observations(
    state: &mut PackageState,
    current_owners: &[MaintainerInfo],
    now: DateTime<Utc>,
) {
    for owner in current_owners {
        let entry = state
            .observed_maintainers
            .entry(owner.login.clone())
            .or_insert_with(|| MaintainerObservation {
                display_name: owner.name.clone(),
                url: owner.url.clone(),
                first_seen_at: now,
                last_seen_at: now,
            });
        entry.last_seen_at = now;
        // Refresh display_name + url in case they changed.
        if owner.name.is_some() {
            entry.display_name = owner.name.clone();
        }
        if owner.url.is_some() {
            entry.url = owner.url.clone();
        }
    }
}

fn save(state_dir: &Path, ecosystem: &str, name: &str, state: &PackageState) -> Result<()> {
    let path = state_file_path(state_dir, ecosystem, name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create state dir")?;
    }
    let json = serde_json::to_string_pretty(state).context("serialize state")?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).context("write tmp state")?;
    fs::rename(&tmp, &path).context("rename tmp into place")?;
    Ok(())
}

fn state_file_path(state_dir: &Path, ecosystem: &str, name: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}:{}", ecosystem, name).as_bytes());
    let digest = hex_digest(&hasher.finalize());
    let mut p = state_dir.to_path_buf();
    p.push(ecosystem);
    p.push(format!("{}.json", digest));
    p
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

/// Filter maintainers in the current set that are "new in window" —
/// added to the package AFTER our state started tracking it.
///
/// Used by `maintainer_delta::scan`. Calibration discipline: until
/// our `first_scan_at` is older than `window_days`, the package is
/// still in its calibration window and we suppress all findings.
/// Once mature, we flag maintainers whose `first_seen_at` was
/// strictly later than `first_scan_at` (they appeared after we
/// started watching).
pub fn maintainers_new_in_window<'a>(
    state: &'a PackageState,
    current_owners: &'a [MaintainerInfo],
    window_days: i64,
    now: DateTime<Utc>,
) -> Vec<&'a MaintainerInfo> {
    let window = chrono::Duration::days(window_days);

    // Calibration period: state is too young to detect deltas.
    // `first_scan_at` is set on the first state-save; if we haven't
    // tracked the package for at least `window_days`, ANY maintainer
    // we observe is indistinguishable from "was here when we started
    // watching." Suppress findings.
    let Some(first_scan) = state.first_scan_at else {
        return Vec::new();
    };
    if now.signed_duration_since(first_scan) < window {
        return Vec::new();
    }

    let mut out = Vec::new();
    for owner in current_owners {
        if let Some(obs) = state.observed_maintainers.get(&owner.login) {
            // Maintainer is in our observation history. Flag only if
            // their first_seen_at is AFTER our first_scan_at — that
            // means they appeared during our watch period, not at
            // the start of it. AND within the window.
            let added_during_watch = obs.first_seen_at > first_scan;
            let recent = now.signed_duration_since(obs.first_seen_at) <= window;
            if added_during_watch && recent {
                out.push(owner);
            }
        }
        // If maintainer not in observation history, we have NEVER
        // seen them — but the orchestrator's persist_after_scan
        // updates state BEFORE this function runs in the next
        // invocation, so this case only arises in unit tests that
        // synthesize state directly.
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn maintainer(login: &str) -> MaintainerInfo {
        MaintainerInfo {
            login: login.to_string(),
            name: None,
            url: None,
        }
    }

    #[test]
    fn first_run_records_observations_no_findings() {
        let mut state = PackageState::default();
        let owners = vec![maintainer("alice"), maintainer("bob")];
        let now = Utc::now();
        update_observations(&mut state, &owners, now);

        // Both should be recorded as first-seen-now.
        assert_eq!(state.observed_maintainers.len(), 2);
        assert_eq!(
            state.observed_maintainers.get("alice").unwrap().first_seen_at,
            now
        );

        // But maintainers_new_in_window on a state THAT HAS NO PRIOR
        // OBSERVATIONS — meaning, a state we haven't called
        // update_observations on yet — should return empty (first-run
        // is a calibration scan, no findings).
        let fresh_state = PackageState::default();
        let in_window = maintainers_new_in_window(&fresh_state, &owners, 30, now);
        assert!(in_window.is_empty());
    }

    #[test]
    fn maintainer_seen_long_ago_not_flagged() {
        let mut state = PackageState::default();
        let long_ago = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 25, 0, 0, 0).unwrap();
        state.first_scan_at = Some(long_ago); // mature state
        state.observed_maintainers.insert(
            "alice".to_string(),
            MaintainerObservation {
                display_name: None,
                url: None,
                first_seen_at: long_ago,
                last_seen_at: long_ago,
            },
        );
        let owners = vec![maintainer("alice")];
        let in_window = maintainers_new_in_window(&state, &owners, 30, now);
        assert!(in_window.is_empty(), "alice was seen >30 days ago");
    }

    #[test]
    fn maintainer_first_seen_recently_flagged() {
        let mut state = PackageState::default();
        // State is mature (first_scan_at is well over 30 days ago).
        let watch_start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let recent = Utc.with_ymd_and_hms(2026, 4, 10, 0, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 25, 0, 0, 0).unwrap();
        state.first_scan_at = Some(watch_start);
        state.observed_maintainers.insert(
            "carol".to_string(),
            MaintainerObservation {
                display_name: None,
                url: None,
                first_seen_at: recent,
                last_seen_at: now,
            },
        );
        let owners = vec![maintainer("carol")];
        let in_window = maintainers_new_in_window(&state, &owners, 30, now);
        assert_eq!(in_window.len(), 1);
        assert_eq!(in_window[0].login, "carol");
    }

    #[test]
    fn calibration_period_suppresses_findings() {
        // State just saved — first_scan_at is now. Even though
        // maintainers exist with first_seen_at == now, we should NOT
        // flag because state hasn't matured for window_days yet.
        let mut state = PackageState::default();
        let now = Utc::now();
        state.first_scan_at = Some(now);
        state.observed_maintainers.insert(
            "carol".to_string(),
            MaintainerObservation {
                display_name: None,
                url: None,
                first_seen_at: now,
                last_seen_at: now,
            },
        );
        let owners = vec![maintainer("carol")];
        let in_window = maintainers_new_in_window(&state, &owners, 30, now);
        assert!(
            in_window.is_empty(),
            "calibration period should suppress findings"
        );
    }

    #[test]
    fn maintainer_present_at_watch_start_not_flagged() {
        // Maintainer was already there when we started watching.
        // first_seen_at == first_scan_at — she's an incumbent, not
        // a new arrival.
        let mut state = PackageState::default();
        let watch_start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 25, 0, 0, 0).unwrap();
        state.first_scan_at = Some(watch_start);
        state.observed_maintainers.insert(
            "alice".to_string(),
            MaintainerObservation {
                display_name: None,
                url: None,
                first_seen_at: watch_start, // same as first_scan_at
                last_seen_at: now,
            },
        );
        let owners = vec![maintainer("alice")];
        let in_window = maintainers_new_in_window(&state, &owners, 30, now);
        assert!(
            in_window.is_empty(),
            "incumbent maintainer should not flag"
        );
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = PackageState::default();
        state.observed_maintainers.insert(
            "alice".to_string(),
            MaintainerObservation {
                display_name: Some("Alice A.".to_string()),
                url: None,
                first_seen_at: Utc::now(),
                last_seen_at: Utc::now(),
            },
        );
        save(dir.path(), "PyPI", "fakepkg", &state).unwrap();
        let loaded = load(dir.path(), "PyPI", "fakepkg");
        assert_eq!(loaded.observed_maintainers.len(), 1);
        assert!(loaded.observed_maintainers.contains_key("alice"));
    }

    #[test]
    fn load_missing_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let s = load(dir.path(), "npm", "never-saved");
        assert!(s.observed_maintainers.is_empty());
    }
}
