//! Pre-loaded-metadata test entry point for Layer 2 calibration.
//!
//! E-SC-8 calibration fixtures ship FROZEN registry-metadata
//! snapshots (`metadata.json`) so calibration runs are deterministic
//! — no live network, no time-of-day variability. This module
//! provides a way to invoke the sub-sensors against such snapshots
//! WITHOUT going through `registry::fetch_all` (which would do a
//! live HTTP fetch).
//!
//! Scope: Group A + `signature_gap` (5 sensors that work from pure
//! metadata). Group B (`binary_reproducibility`) and Group C
//! (`exfil_indicator`) require live tarballs and are NOT part of
//! the deterministic calibration path. They have their own opt-in
//! activation env vars.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

use super::registry::{
    AttestationStatus, FetchAllResult, MaintainerInfo, PackageMetadata, VersionInfo,
};
use super::scoring::VigilanceFinding;
use super::state::PackageState;
use crate::supply_chain_sca::Package;

/// Run the deterministic sub-sensors against a pre-loaded
/// `metadata.json` (and optionally pre-loaded `prior-state.json`).
///
/// `metadata_map` is keyed `<ecosystem>:<name>` with values matching
/// the `PackageMetadata` shape. Each version may use either
/// absolute `published_at` (ISO-8601) OR relative
/// `published_days_ago` (integer; substituted at load time as
/// `now - N days`). The relative form lets fixtures not decay
/// over time — the same fixture exercises the sensors identically
/// in 6 months.
///
/// `prior_state_raw` is JSON of `{ecosystem:name: PackageState}`
/// for delta sensors that need state. State entries support the
/// same `_days_ago` relative form for `first_scan_at` /
/// `last_scan_at` / per-maintainer `first_seen_at` etc.
pub fn scan_fixture(
    metadata_map: &HashMap<String, serde_json::Value>,
    prior_state_raw: Option<&str>,
) -> Result<Vec<VigilanceFinding>> {
    let now = Utc::now();
    // Reconstruct PackageMetadata from the fixture's JSON.
    let mut packages: Vec<Package> = Vec::new();
    let mut fetch_result = FetchAllResult {
        cache_bypassed: false,
        any_reachable: true,
        ..Default::default()
    };

    for (key, value) in metadata_map {
        let (ecosystem, name) = key
            .split_once(':')
            .with_context(|| format!("metadata key {:?} must be \"<ecosystem>:<name>\"", key))?;
        let normalized = normalize_metadata_value(value, now);
        let meta: PackageMetadata = serde_json::from_value(normalized)
            .with_context(|| format!("parse PackageMetadata for {}", key))?;

        // The lockfile-resolved version: take the LAST version in
        // the metadata's `versions` (sorted ascending by published_at,
        // so last is most recent). Fixture authors who want a
        // different scanned version can structure their metadata
        // accordingly.
        let scanned_version = meta
            .versions
            .last()
            .map(|v| v.version.clone())
            .unwrap_or_else(|| "0.0.0".to_string());
        let pkg = make_package(ecosystem, name, &scanned_version)?;
        fetch_result
            .metadata
            .insert((ecosystem.to_string(), name.to_string()), meta);
        packages.push(pkg);
    }

    // Set up a state directory backed by an in-memory snapshot if
    // `prior_state_raw` is supplied. For simplicity, we write the
    // state to a TempDir and let the regular state::load path
    // handle the read.
    let state_tempdir = std::env::temp_dir().join(format!(
        "neurogrim-vigilance-cal-state-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    if let Some(raw) = prior_state_raw {
        let raw_map: HashMap<String, serde_json::Value> =
            serde_json::from_str(raw).context("parse prior-state.json")?;
        for (key, value) in raw_map {
            let (ecosystem, name) = key
                .split_once(':')
                .with_context(|| format!("state key {:?} must be \"<ecosystem>:<name>\"", key))?;
            let normalized = normalize_state_value(&value, now);
            let state: PackageState = serde_json::from_value(normalized)
                .with_context(|| format!("parse PackageState for {}", key))?;
            write_state_file(&state_tempdir, ecosystem, name, &state)?;
        }
    }

    // Run the deterministic sub-sensors.
    let mut findings: Vec<VigilanceFinding> = Vec::new();
    findings.extend(super::typosquat::scan(&packages));
    findings.extend(super::publish_cadence::scan(&packages, &fetch_result));
    findings.extend(super::transitive_surface::scan(
        &packages,
        &fetch_result,
        &state_tempdir,
    ));
    findings.extend(super::maintainer_delta::scan(
        &packages,
        &fetch_result,
        &state_tempdir,
    ));
    findings.extend(super::signature_gap::scan(
        &packages,
        &fetch_result,
        &state_tempdir,
    ));

    // Cleanup the temp state dir.
    let _ = std::fs::remove_dir_all(&state_tempdir);

    Ok(findings)
}

/// Walk the metadata JSON tree and replace every
/// `published_days_ago` (integer or float) with an ISO-8601
/// `published_at` computed as `now - days_ago`.
fn normalize_metadata_value(value: &serde_json::Value, now: DateTime<Utc>) -> serde_json::Value {
    let mut v = value.clone();
    if let Some(versions) = v
        .get_mut("versions")
        .and_then(|x| x.as_array_mut())
    {
        for ver in versions {
            substitute_days_ago_field(ver, "published_at", "published_days_ago", now);
        }
    }
    v
}

/// Walk the state JSON tree and replace `*_days_ago` markers with
/// real ISO-8601 timestamps.
fn normalize_state_value(value: &serde_json::Value, now: DateTime<Utc>) -> serde_json::Value {
    let mut v = value.clone();
    substitute_days_ago_field(&mut v, "first_scan_at", "first_scan_days_ago", now);
    substitute_days_ago_field(&mut v, "last_scan_at", "last_scan_days_ago", now);
    if let Some(observed) = v
        .get_mut("observed_maintainers")
        .and_then(|x| x.as_object_mut())
    {
        for (_login, obs) in observed {
            substitute_days_ago_field(obs, "first_seen_at", "first_seen_days_ago", now);
            substitute_days_ago_field(obs, "last_seen_at", "last_seen_days_ago", now);
        }
    }
    v
}

/// If `obj` has `<offset_field>: N`, replace it with
/// `<target_field>: <iso-8601 of (now - N days)>`. Removes the
/// offset key.
fn substitute_days_ago_field(
    obj: &mut serde_json::Value,
    target_field: &str,
    offset_field: &str,
    now: DateTime<Utc>,
) {
    let Some(map) = obj.as_object_mut() else {
        return;
    };
    let offset = match map.remove(offset_field) {
        Some(v) => v,
        None => return,
    };
    let days = match offset.as_f64() {
        Some(d) => d,
        None => return,
    };
    let dt = now - chrono::Duration::milliseconds((days * 86_400_000.0) as i64);
    map.insert(
        target_field.to_string(),
        serde_json::Value::String(dt.to_rfc3339()),
    );
}

fn make_package(ecosystem: &str, name: &str, version: &str) -> Result<Package> {
    use crate::supply_chain_sca::osv::{ECOSYSTEM_CRATES_IO, ECOSYSTEM_NPM, ECOSYSTEM_PYPI};
    Ok(match ecosystem {
        x if x == ECOSYSTEM_CRATES_IO => Package::crates_io(name, version),
        x if x == ECOSYSTEM_PYPI => Package::pypi(name, version),
        x if x == ECOSYSTEM_NPM => Package::npm(name, version),
        other => anyhow::bail!("unknown ecosystem in fixture: {:?}", other),
    })
}

fn write_state_file(
    state_dir: &std::path::Path,
    ecosystem: &str,
    name: &str,
    state: &PackageState,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(format!("{}:{}", ecosystem, name).as_bytes());
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    let mut p = state_dir.to_path_buf();
    p.push(ecosystem);
    std::fs::create_dir_all(&p).context("mkdir state ecosystem dir")?;
    p.push(format!("{}.json", digest));
    let raw = serde_json::to_string_pretty(state).context("serialize state")?;
    std::fs::write(&p, raw).context("write state file")?;
    Ok(())
}

/// Helper to build a synthetic `PackageMetadata` from sparse fields.
/// Useful for fixture-authoring scripts.
pub fn synthetic_metadata(
    ecosystem: &str,
    name: &str,
    versions: Vec<(&str, DateTime<Utc>, AttestationStatus, Option<usize>)>,
    owners: Vec<&str>,
) -> PackageMetadata {
    let versions = versions
        .into_iter()
        .map(|(v, t, attest, dep_count)| VersionInfo {
            version: v.to_string(),
            published_at: Some(t),
            yanked: false,
            dependency_count: dep_count,
            dependency_names: None,
            tarball_url: None,
            tarball_sha256: None,
            attestation_status: attest,
        })
        .collect();
    let owners = owners
        .into_iter()
        .map(|login| MaintainerInfo {
            login: login.to_string(),
            name: None,
            url: None,
        })
        .collect();
    PackageMetadata {
        ecosystem: ecosystem.to_string(),
        name: name.to_string(),
        versions,
        owners,
        repository_url: None,
        homepage_url: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn empty_metadata_yields_empty_findings() {
        let map: HashMap<String, serde_json::Value> = HashMap::new();
        let f = scan_fixture(&map, None).unwrap();
        assert!(f.is_empty());
    }

    #[test]
    fn synthetic_typosquat_fixture_fires() {
        // Construct a fake "litelm" package metadata; should fire
        // typosquat against "litellm".
        let meta = synthetic_metadata(
            "PyPI",
            "litelm",
            vec![("1.0.0", Utc::now() - Duration::days(10), AttestationStatus::None, None)],
            vec!["alice"],
        );
        let mut map = HashMap::new();
        map.insert(
            "PyPI:litelm".to_string(),
            serde_json::to_value(meta).unwrap(),
        );
        let f = scan_fixture(&map, None).unwrap();
        assert!(
            f.iter()
                .any(|x| matches!(x.kind, super::super::scoring::VigilanceKind::TyposquatProximity)),
            "expected typosquat finding: {f:?}"
        );
    }

    #[test]
    fn synthetic_known_good_fires_nothing() {
        // Single-version recent release of a totally normal package
        // that's not in any popularity list — no signals should fire.
        let meta = synthetic_metadata(
            "PyPI",
            "neurogrim-test-totally-unrelated-name",
            vec![("1.0.0", Utc::now() - Duration::days(10), AttestationStatus::None, None)],
            vec!["alice"],
        );
        let mut map = HashMap::new();
        map.insert(
            "PyPI:neurogrim-test-totally-unrelated-name".to_string(),
            serde_json::to_value(meta).unwrap(),
        );
        let f = scan_fixture(&map, None).unwrap();
        assert!(
            f.is_empty(),
            "known-good single-version fixture should not flag: {f:?}"
        );
    }
}
