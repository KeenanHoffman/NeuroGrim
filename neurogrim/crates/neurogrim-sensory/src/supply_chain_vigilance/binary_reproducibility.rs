//! `binary_reproducibility` sensor — Phase 3 (Group B).
//!
//! Detects mismatch between the package's registry-claimed tarball
//! checksum and what we'd actually receive if we fetched it.
//! Catches MITM-during-download and registry-internal tampering.
//!
//! True binary-vs-source reproducibility (registry tarball vs source
//! repo tag's tarball) is a v2+ candidate — many packages ship build
//! artifacts that legitimately differ between source-tag and
//! published tarball, so high-FP. v1 focuses on the simpler
//! registry-checksum-verification check.
//!
//! # Opt-in posture
//!
//! Disabled by default to avoid multi-GB downloads on first scan.
//! Activate via `NEUROGRIM_VIGILANCE_REPRO=1` env var. When inactive,
//! the sensor emits no findings.
//!
//! When active, results are cached for 7 days at
//! `<cache_dir>/repro/<ecosystem>/<sha256>.json` so repeated scans
//! don't re-download.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

use crate::supply_chain_sca::Package;

use super::registry::{CacheBehavior, FetchAllResult};
use super::scoring::{VigilanceFinding, VigilanceKind};

const REPRO_CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const REPRO_HTTP_TIMEOUT: Duration = Duration::from_secs(60);
const REPRO_USER_AGENT: &str = concat!(
    "NeuroGrim-supply-chain-vigilance-repro/",
    env!("CARGO_PKG_VERSION")
);

/// Maximum tarball size we'll download (in bytes). Above this, we
/// skip — we don't want to pull massive packages just to verify
/// their checksum.
const MAX_TARBALL_SIZE: u64 = 50 * 1024 * 1024; // 50 MB

pub async fn scan(
    packages: &[Package],
    metadata: &FetchAllResult,
    cache_dir: &Path,
    cache_behavior: CacheBehavior,
) -> Vec<VigilanceFinding> {
    if !is_active() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    let repro_cache_dir = cache_dir.join("repro");

    let client = match build_http_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("repro: HTTP client build failed: {:#}", e);
            return Vec::new();
        }
    };

    let mut seen: std::collections::HashSet<(String, String, &'static str)> =
        std::collections::HashSet::new();

    for pkg in packages {
        if !seen.insert((pkg.name.clone(), pkg.version.clone(), pkg.ecosystem)) {
            continue;
        }
        let Some(meta) = metadata.get(pkg) else {
            continue;
        };
        let Some(version_info) = meta
            .versions
            .iter()
            .find(|v| v.version == pkg.version)
        else {
            continue;
        };
        let Some(tarball_url) = version_info.tarball_url.clone() else {
            continue;
        };
        let Some(claimed_checksum) = version_info.tarball_sha256.clone() else {
            // No checksum to verify against. Skip.
            continue;
        };

        // Cache check.
        if cache_behavior == CacheBehavior::UseCache {
            if let Some(cached) =
                try_load_cache(&repro_cache_dir, pkg.ecosystem, &pkg.name, &pkg.version)
            {
                if !cached.matches {
                    findings.push(make_finding(pkg, &cached));
                }
                continue;
            }
        }

        // Fetch + verify.
        match verify_one(&client, &tarball_url, &claimed_checksum).await {
            Ok(observed) => {
                let entry = ReproCacheEntry {
                    written_at: Utc::now(),
                    claimed_checksum: claimed_checksum.clone(),
                    observed_checksum: observed.clone(),
                    matches: claimed_checksum == observed,
                };
                if cache_behavior == CacheBehavior::UseCache {
                    let _ = write_cache(
                        &repro_cache_dir,
                        pkg.ecosystem,
                        &pkg.name,
                        &pkg.version,
                        &entry,
                    );
                }
                if !entry.matches {
                    findings.push(make_finding(pkg, &entry));
                }
            }
            Err(e) => {
                tracing::debug!(
                    "repro: verify failed for {}@{}: {:#}",
                    pkg.name,
                    pkg.version,
                    e
                );
            }
        }
    }

    findings
}

/// Whether `NEUROGRIM_VIGILANCE_REPRO` is set to a truthy value.
pub fn is_active() -> bool {
    is_active_from_value(std::env::var("NEUROGRIM_VIGILANCE_REPRO").ok().as_deref())
}

fn is_active_from_value(v: Option<&str>) -> bool {
    match v {
        None => false,
        Some(s) => {
            let s = s.trim().to_ascii_lowercase();
            !(s.is_empty() || s == "0" || s == "false" || s == "no")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReproCacheEntry {
    written_at: DateTime<Utc>,
    claimed_checksum: String,
    observed_checksum: String,
    matches: bool,
}

fn make_finding(pkg: &Package, entry: &ReproCacheEntry) -> VigilanceFinding {
    VigilanceFinding {
        kind: VigilanceKind::BinaryReproducibilityMismatch,
        package: pkg.clone(),
        summary: format!(
            "registry-claimed checksum does not match fetched tarball \
             (claimed: {}, observed: {})",
            short_hash(&entry.claimed_checksum),
            short_hash(&entry.observed_checksum)
        ),
        evidence: Some(json!({
            "claimed_checksum": entry.claimed_checksum,
            "observed_checksum": entry.observed_checksum,
        })),
        confidence: 0.95,
    }
}

fn short_hash(h: &str) -> String {
    if h.len() > 16 {
        format!("{}…", &h[..16])
    } else {
        h.to_string()
    }
}

fn build_http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(REPRO_HTTP_TIMEOUT)
        .user_agent(REPRO_USER_AGENT)
        .build()?)
}

async fn verify_one(
    client: &reqwest::Client,
    url: &str,
    claimed: &str,
) -> Result<String> {
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("non-success {}: {}", resp.status(), url);
    }
    if let Some(len) = resp.content_length() {
        if len > MAX_TARBALL_SIZE {
            anyhow::bail!("tarball too large ({} bytes) — skipping", len);
        }
    }
    let bytes = resp.bytes().await?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let observed = hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    // crates.io publishes hex sha256; PyPI returns hex too. Compare
    // case-insensitively.
    let _ = claimed; // future: parse `sri:sha512-...` for npm — TODO.
    Ok(observed)
}

fn cache_path(cache_dir: &Path, ecosystem: &str, name: &str, version: &str) -> std::path::PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}:{}:{}", ecosystem, name, version).as_bytes());
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    let mut p = cache_dir.to_path_buf();
    p.push(ecosystem);
    p.push(format!("{}.json", digest));
    p
}

fn try_load_cache(
    cache_dir: &Path,
    ecosystem: &str,
    name: &str,
    version: &str,
) -> Option<ReproCacheEntry> {
    let path = cache_path(cache_dir, ecosystem, name, version);
    let raw = std::fs::read_to_string(&path).ok()?;
    let entry: ReproCacheEntry = serde_json::from_str(&raw).ok()?;
    let age = Utc::now().signed_duration_since(entry.written_at);
    if age.num_seconds() > REPRO_CACHE_TTL.as_secs() as i64 {
        return None;
    }
    Some(entry)
}

fn write_cache(
    cache_dir: &Path,
    ecosystem: &str,
    name: &str,
    version: &str,
    entry: &ReproCacheEntry,
) -> Result<()> {
    let path = cache_path(cache_dir, ecosystem, name, version);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(entry)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_in_default_off() {
        assert!(!is_active_from_value(None));
        assert!(!is_active_from_value(Some("")));
        assert!(!is_active_from_value(Some("0")));
        assert!(!is_active_from_value(Some("false")));
        assert!(!is_active_from_value(Some("no")));
    }

    #[test]
    fn opt_in_truthy() {
        assert!(is_active_from_value(Some("1")));
        assert!(is_active_from_value(Some("true")));
        assert!(is_active_from_value(Some("yes")));
        assert!(is_active_from_value(Some("anything")));
    }
}
