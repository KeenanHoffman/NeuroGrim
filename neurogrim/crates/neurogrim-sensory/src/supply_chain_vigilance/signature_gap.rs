//! `signature_gap` sensor — Phase 3 (Group B).
//!
//! Flags packages whose CURRENT (lockfile-resolved) version has a
//! lower attestation level than ANY of its prior versions. Pattern:
//! a package was historically published with sigstore/trustpub
//! attestation, but the current version dropped that attestation —
//! suggests either compromised credentials being used to bypass
//! the trusted-publisher pipeline, OR a maintainer account
//! compromise that pushed via a different path.
//!
//! Pure-data: computed from registry metadata. No per-run state.

use serde_json::json;

use crate::supply_chain_sca::Package;

use super::registry::{AttestationStatus, FetchAllResult};
use super::scoring::{VigilanceFinding, VigilanceKind};

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

        let curr_idx = match meta.versions.iter().position(|v| v.version == pkg.version) {
            Some(i) => i,
            None => continue,
        };
        if curr_idx == 0 {
            continue; // No prior version.
        }
        let curr_status = meta.versions[curr_idx].attestation_status;
        let curr_level = level(curr_status);

        // Compare against the IMMEDIATE prior version. v1 design choice:
        // a single old attested version mixed into a long history of
        // unattested ones (common with crates.io's just-launched
        // trusted-publishing) is too FP-prone. A true "drop" is
        // current-vs-immediate-prior. Calibration in E-SC-8 may relax
        // this back to "any prior" once attestation coverage matures.
        let prev = &meta.versions[curr_idx - 1];
        let max_prior_level = level(prev.attestation_status);
        let max_prior_status = prev.attestation_status;
        let max_prior_version: Option<&str> = Some(prev.version.as_str());

        // Flag if CURRENT level < IMMEDIATE prior level.
        if curr_level < max_prior_level {
            findings.push(VigilanceFinding {
                kind: VigilanceKind::SignatureGap,
                package: pkg.clone(),
                summary: format!(
                    "attestation dropped: prior version {} had {}, current ({}) has {}",
                    max_prior_version.unwrap_or("?"),
                    status_str(max_prior_status),
                    pkg.version,
                    status_str(curr_status),
                ),
                evidence: Some(json!({
                    "current_status": status_str(curr_status),
                    "max_prior_status": status_str(max_prior_status),
                    "max_prior_version": max_prior_version,
                })),
                confidence: 0.6,
            });
        }
    }

    findings
}

fn level(s: AttestationStatus) -> u32 {
    match s {
        AttestationStatus::Trustpub => 3,
        AttestationStatus::Sigstore => 3,
        AttestationStatus::Gpg => 2,
        AttestationStatus::None => 0,
        AttestationStatus::Unknown => 0,
    }
}

fn status_str(s: AttestationStatus) -> &'static str {
    match s {
        AttestationStatus::Trustpub => "trustpub",
        AttestationStatus::Sigstore => "sigstore",
        AttestationStatus::Gpg => "gpg",
        AttestationStatus::None => "none",
        AttestationStatus::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supply_chain_sca::Package;
    use crate::supply_chain_vigilance::registry::{
        AttestationStatus, FetchAllResult, PackageMetadata, VersionInfo,
    };
    use chrono::Utc;

    fn version_with_status(v: &str, status: AttestationStatus) -> VersionInfo {
        VersionInfo {
            version: v.to_string(),
            published_at: Some(Utc::now()),
            yanked: false,
            dependency_count: None,
            dependency_names: None,
            tarball_url: None,
            tarball_sha256: None,
            attestation_status: status,
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
    fn no_attestation_drop_no_finding() {
        let pkg = Package::pypi("fakepkg", "1.0.5");
        let versions = vec![
            version_with_status("1.0.4", AttestationStatus::Sigstore),
            version_with_status("1.0.5", AttestationStatus::Sigstore),
        ];
        let r = meta_with("fakepkg", "PyPI", versions);
        let dir = tempfile::tempdir().unwrap();
        let f = scan(&[pkg], &r, dir.path());
        assert!(f.is_empty());
    }

    #[test]
    fn attestation_drop_flagged() {
        let pkg = Package::pypi("fakepkg", "1.0.5");
        let versions = vec![
            version_with_status("1.0.3", AttestationStatus::Sigstore),
            version_with_status("1.0.4", AttestationStatus::Sigstore),
            version_with_status("1.0.5", AttestationStatus::None),
        ];
        let r = meta_with("fakepkg", "PyPI", versions);
        let dir = tempfile::tempdir().unwrap();
        let f = scan(&[pkg], &r, dir.path());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].kind, VigilanceKind::SignatureGap);
    }

    #[test]
    fn historical_attestation_then_long_gap_no_finding() {
        // Critical FP-suppression test: a SINGLE old attested version
        // mixed into many unattested ones should NOT fire. This was the
        // dominant FP on the dogfood scan against NeuroGrim's deps.
        let pkg = Package::crates_io("fakepkg", "1.5.0");
        let versions = vec![
            version_with_status("0.1.0", AttestationStatus::Trustpub),
            version_with_status("0.2.0", AttestationStatus::None),
            version_with_status("0.3.0", AttestationStatus::None),
            version_with_status("1.0.0", AttestationStatus::None),
            version_with_status("1.5.0", AttestationStatus::None),
        ];
        let r = meta_with("fakepkg", "crates.io", versions);
        let dir = tempfile::tempdir().unwrap();
        let f = scan(&[pkg], &r, dir.path());
        assert!(
            f.is_empty(),
            "v1 only flags drops vs immediate prior, not vs any historical"
        );
    }

    #[test]
    fn never_signed_no_finding() {
        // If a package never had attestation, current=None is not a "drop."
        let pkg = Package::pypi("fakepkg", "1.0.5");
        let versions = vec![
            version_with_status("1.0.4", AttestationStatus::None),
            version_with_status("1.0.5", AttestationStatus::None),
        ];
        let r = meta_with("fakepkg", "PyPI", versions);
        let dir = tempfile::tempdir().unwrap();
        let f = scan(&[pkg], &r, dir.path());
        assert!(f.is_empty());
    }

    #[test]
    fn improvement_no_finding() {
        // Going from None → Sigstore is GOOD, not bad. Don't flag.
        let pkg = Package::pypi("fakepkg", "1.0.5");
        let versions = vec![
            version_with_status("1.0.4", AttestationStatus::None),
            version_with_status("1.0.5", AttestationStatus::Sigstore),
        ];
        let r = meta_with("fakepkg", "PyPI", versions);
        let dir = tempfile::tempdir().unwrap();
        let f = scan(&[pkg], &r, dir.path());
        assert!(f.is_empty());
    }
}
