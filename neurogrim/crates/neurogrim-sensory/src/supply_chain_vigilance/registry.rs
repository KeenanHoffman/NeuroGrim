//! Per-ecosystem registry-API client + 7-day file-backed cache.
//!
//! # Design
//!
//! Each ecosystem has a public JSON metadata API:
//! - **crates.io:** `GET https://crates.io/api/v1/crates/<name>` —
//!   returns `{crate: {...}, versions: [...], keywords: [...]}`.
//!   Owners list is a separate call: `GET .../crates/<name>/owners`.
//! - **PyPI:** `GET https://pypi.org/pypi/<pkg>/json` — returns
//!   `{info, releases, urls, ...}`. `releases` is a dict keyed by
//!   version.
//! - **npm:** `GET https://registry.npmjs.org/<name>` — returns
//!   the full package document including `time`, `versions`,
//!   `maintainers`, `dist-tags`.
//!
//! # Trust-surface posture
//!
//! Three new HTTPS endpoints relative to Layer 1's OSV-only posture.
//! Documented in `audit/TOOL-TRUST-NOTES.md` per the 2026-04-25
//! E-SC-5 entry. All clients use `reqwest` with rustls + native-
//! roots (OS trust store).
//!
//! # Cache
//!
//! - 7-day TTL by default; `NEUROGRIM_VIGILANCE_NO_CACHE=1` env
//!   forces fresh queries.
//! - Cache key: sha256 of `<ecosystem>:<name>` (no version — the
//!   metadata response is per-package, covering all versions).
//! - Cache file: `<cache_dir>/<ecosystem>/<sha256>.json`.
//!
//! # Error posture
//!
//! Same as `osv.rs`: registry unreachable is NOT fatal. The fetch
//! degrades to "what's in cache" + flags `vigilance_reachable=false`
//! (per-ecosystem) so downstream sees partial coverage. Fresh cache
//! writes happen only for successful live fetches.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::supply_chain_sca::osv::{ECOSYSTEM_CRATES_IO, ECOSYSTEM_NPM, ECOSYSTEM_PYPI};
use crate::supply_chain_sca::Package;

/// Cache TTL. Entries older than this are treated as misses.
/// 7 days per the E-SC-5 plan: registry metadata changes slowly.
pub const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Per-request HTTP timeout.
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

/// Inter-request delay to be polite to registries (1 req/sec).
/// Most fetches will be cache-hits so this rarely triggers.
const RATE_LIMIT_DELAY: Duration = Duration::from_millis(250);

/// User agent for our HTTP client. Identifies us politely to
/// registries that log/rate-limit by UA.
const USER_AGENT: &str = concat!(
    "NeuroGrim-supply-chain-vigilance/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/KeenanHoffman/NeuroGrim)"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheBehavior {
    UseCache,
    Bypass,
}

/// Read `NEUROGRIM_VIGILANCE_NO_CACHE` and decide cache behavior.
pub fn cache_behavior_from_env() -> CacheBehavior {
    cache_behavior_from_env_value(
        std::env::var("NEUROGRIM_VIGILANCE_NO_CACHE")
            .ok()
            .as_deref(),
    )
}

pub(crate) fn cache_behavior_from_env_value(v: Option<&str>) -> CacheBehavior {
    match v {
        None => CacheBehavior::UseCache,
        Some(s) => {
            let s = s.trim().to_ascii_lowercase();
            if s.is_empty() || s == "0" || s == "false" || s == "no" {
                CacheBehavior::UseCache
            } else {
                CacheBehavior::Bypass
            }
        }
    }
}

/// Normalized package metadata, the union of fields all 7 sensors
/// need. Different registries surface different field shapes; this
/// struct is the per-ecosystem normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub ecosystem: String,
    pub name: String,
    /// All known versions in publish order with their publish
    /// timestamps. Sorted ascending by `published_at`.
    pub versions: Vec<VersionInfo>,
    /// Current owners / maintainers list. May be empty if the
    /// registry doesn't expose this or if we couldn't fetch.
    pub owners: Vec<MaintainerInfo>,
    /// Repository URL declared in the registry metadata, if any.
    pub repository_url: Option<String>,
    /// Optional homepage URL.
    pub homepage_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub published_at: Option<DateTime<Utc>>,
    pub yanked: bool,
    /// Number of declared (direct) dependencies for this version.
    /// Used by `transitive_surface` sensor.
    pub dependency_count: Option<usize>,
    /// Names of declared dependencies for this version. Optional —
    /// some registries only return counts in the per-package
    /// metadata and require an extra call for the full list.
    pub dependency_names: Option<Vec<String>>,
    /// Tarball or sdist URL for this version. Used by
    /// `binary_reproducibility` and `exfil_indicator` sensors.
    pub tarball_url: Option<String>,
    /// Tarball SHA-256 hex if registry-published. Used as a key
    /// for `binary_reproducibility`.
    pub tarball_sha256: Option<String>,
    /// Cryptographic attestation status:
    /// - "trustpub": crates.io trusted-publishing or PyPI trusted-publisher
    /// - "sigstore": npm sigstore-attestation or PyPI PEP 740 attestation
    /// - "gpg": GPG-signed (legacy PyPI)
    /// - "none": no attestation declared
    /// - "unknown": registry didn't surface the field for this version
    pub attestation_status: AttestationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttestationStatus {
    Trustpub,
    Sigstore,
    Gpg,
    None,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintainerInfo {
    pub login: String,
    /// Display name if available. Falls back to login.
    pub name: Option<String>,
    /// Optional URL of the maintainer's profile on the registry.
    pub url: Option<String>,
}

/// Result of fetching metadata for a batch of packages.
#[derive(Debug, Default)]
pub struct FetchAllResult {
    /// Successfully fetched (or cached) metadata, keyed by
    /// (ecosystem, name).
    pub metadata: HashMap<(String, String), PackageMetadata>,
    pub cache_hits: usize,
    pub live_queries: usize,
    pub oldest_cache_age_seconds: Option<u64>,
    /// `true` if at least one live registry call succeeded this run.
    /// `false` means all fetches went through cache OR all live
    /// attempts failed.
    pub any_reachable: bool,
    /// Per-ecosystem reachability bookkeeping for the CMDB extras.
    pub unreachable_ecosystems: Vec<String>,
    pub cache_bypassed: bool,
    /// 2026-04-26 PRE-RELEASE A11 fix: surfaces an
    /// HTTP-client-setup failure (e.g., TLS cert problem, native
    /// roots load failure) so the CMDB can show the partial-data
    /// state instead of silently degrading. `None` on the happy
    /// path; `Some(detail)` when reqwest::Client::builder().build()
    /// returns Err.
    pub client_error: Option<String>,
}

impl FetchAllResult {
    /// Look up metadata for a package. Returns `None` if no fetch
    /// succeeded (live + cache both empty) — sub-sensors must handle
    /// `None` gracefully (skip the finding for that package).
    pub fn get(&self, pkg: &Package) -> Option<&PackageMetadata> {
        self.metadata
            .get(&(pkg.ecosystem.to_string(), pkg.name.clone()))
    }
}

/// Top-level fetch entry point. Consumed by
/// `supply_chain_vigilance::analyze_supply_chain_vigilance`.
///
/// Always returns a valid result; never propagates a panic.
/// Per-package fetch failures are silent (the sensor degrades
/// gracefully when a registry is unreachable).
pub async fn fetch_all(
    packages: &[Package],
    cache_dir: &Path,
    cache_behavior: CacheBehavior,
) -> FetchAllResult {
    let mut result = FetchAllResult {
        cache_bypassed: cache_behavior == CacheBehavior::Bypass,
        ..Default::default()
    };

    if packages.is_empty() {
        result.any_reachable = true; // vacuously
        return result;
    }

    // Dedup by (ecosystem, name) — same package referenced multiple
    // versions only needs one metadata fetch.
    let mut by_key: HashMap<(String, String), &Package> = HashMap::new();
    for pkg in packages {
        by_key
            .entry((pkg.ecosystem.to_string(), pkg.name.clone()))
            .or_insert(pkg);
    }

    let client = match build_http_client() {
        Ok(c) => Some(c),
        Err(e) => {
            tracing::warn!("vigilance: HTTP client build failed: {:#}", e);
            // 2026-04-26 PRE-RELEASE A11 fix: capture the reason so
            // the CMDB can surface "registry_client_error" rather
            // than only logging at warn level.
            result.client_error = Some(format!("{:#}", e));
            None
        }
    };

    let mut per_ecosystem_reachability: HashMap<String, bool> = HashMap::new();

    for ((ecosystem, name), pkg) in &by_key {
        // Try cache first.
        if cache_behavior == CacheBehavior::UseCache {
            if let Some((meta, age)) = try_load_cache_entry(cache_dir, ecosystem, name) {
                result.cache_hits += 1;
                let age_secs = age.num_seconds().max(0) as u64;
                result.oldest_cache_age_seconds = Some(
                    result
                        .oldest_cache_age_seconds
                        .map_or(age_secs, |o| o.max(age_secs)),
                );
                result.metadata.insert((ecosystem.clone(), name.clone()), meta);
                continue;
            }
        }

        // Live fetch.
        let Some(client) = &client else {
            continue;
        };

        match fetch_one(client, ecosystem, name, pkg).await {
            Ok(meta) => {
                result.live_queries += 1;
                result.any_reachable = true;
                per_ecosystem_reachability
                    .entry(ecosystem.clone())
                    .and_modify(|v| *v |= true)
                    .or_insert(true);
                if cache_behavior == CacheBehavior::UseCache {
                    let _ = write_cache_entry(cache_dir, ecosystem, name, &meta);
                }
                result.metadata.insert((ecosystem.clone(), name.clone()), meta);
            }
            Err(e) => {
                per_ecosystem_reachability
                    .entry(ecosystem.clone())
                    .or_insert(false);
                tracing::debug!(
                    "vigilance: registry fetch failed for {}@{}: {:#}",
                    ecosystem,
                    name,
                    e
                );
            }
        }

        // Be polite to registries.
        tokio::time::sleep(RATE_LIMIT_DELAY).await;
    }

    // Collect ecosystems with zero successful live fetches AND zero
    // cache hits. Those are reported as "unreachable" — operators
    // can see partial coverage in the CMDB extras.
    for ecosystem in [ECOSYSTEM_CRATES_IO, ECOSYSTEM_PYPI, ECOSYSTEM_NPM] {
        let touched: bool = by_key.keys().any(|(eco, _)| eco == ecosystem);
        if !touched {
            continue;
        }
        let any_metadata: bool = result
            .metadata
            .keys()
            .any(|(eco, _)| eco.as_str() == ecosystem);
        if !any_metadata {
            result.unreachable_ecosystems.push(ecosystem.to_string());
        }
    }

    if !result.metadata.is_empty() {
        result.any_reachable = true;
    }

    result
}

fn build_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .context("vigilance: build reqwest client")
}

async fn fetch_one(
    client: &reqwest::Client,
    ecosystem: &str,
    name: &str,
    _pkg: &Package,
) -> Result<PackageMetadata> {
    match ecosystem {
        ECOSYSTEM_CRATES_IO => fetch_cratesio(client, name).await,
        ECOSYSTEM_PYPI => fetch_pypi(client, name).await,
        ECOSYSTEM_NPM => fetch_npm(client, name).await,
        other => anyhow::bail!("vigilance: unknown ecosystem {other:?}"),
    }
}

// =========================================================================
// crates.io fetch
// =========================================================================

async fn fetch_cratesio(client: &reqwest::Client, name: &str) -> Result<PackageMetadata> {
    let url = format!("https://crates.io/api/v1/crates/{}", urlencoded(name));
    let resp = client.get(&url).send().await.context("crates.io GET")?;
    if !resp.status().is_success() {
        anyhow::bail!("crates.io non-success: {}", resp.status());
    }
    let body: Value = resp.json().await.context("crates.io JSON parse")?;

    let mut versions: Vec<VersionInfo> = Vec::new();
    if let Some(arr) = body.get("versions").and_then(|v| v.as_array()) {
        for vobj in arr {
            let version = vobj
                .get("num")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let yanked = vobj
                .get("yanked")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let published_at = vobj
                .get("created_at")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc));
            let tarball_url = vobj
                .get("dl_path")
                .and_then(|v| v.as_str())
                .map(|s| format!("https://crates.io{}", s));
            let tarball_sha256 = vobj
                .get("checksum")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            // crates.io returns `published_by` for trusted-publishing.
            // If present, we treat the version as trustpub-attested.
            // Otherwise: none.
            let attestation_status = if vobj.get("published_by").is_some()
                && !vobj.get("published_by").unwrap().is_null()
            {
                AttestationStatus::Trustpub
            } else {
                AttestationStatus::None
            };
            versions.push(VersionInfo {
                version,
                published_at,
                yanked,
                dependency_count: None, // requires per-version call; deferred
                dependency_names: None,
                tarball_url,
                tarball_sha256,
                attestation_status,
            });
        }
    }
    versions.sort_by_key(|v| v.published_at.unwrap_or_else(Utc::now));

    // Owners list — separate API call. Best-effort; failure is
    // silent (some packages return empty owners array).
    let mut owners: Vec<MaintainerInfo> = Vec::new();
    let owners_url = format!("https://crates.io/api/v1/crates/{}/owners", urlencoded(name));
    if let Ok(o) = client.get(&owners_url).send().await {
        if o.status().is_success() {
            if let Ok(j) = o.json::<Value>().await {
                if let Some(arr) = j.get("users").and_then(|v| v.as_array()) {
                    for u in arr {
                        let login = u
                            .get("login")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let name = u
                            .get("name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let url = u
                            .get("url")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        if !login.is_empty() {
                            owners.push(MaintainerInfo { login, name, url });
                        }
                    }
                }
            }
        }
    }

    let repository_url = body
        .get("crate")
        .and_then(|c| c.get("repository"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let homepage_url = body
        .get("crate")
        .and_then(|c| c.get("homepage"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(PackageMetadata {
        ecosystem: ECOSYSTEM_CRATES_IO.to_string(),
        name: name.to_string(),
        versions,
        owners,
        repository_url,
        homepage_url,
    })
}

// =========================================================================
// PyPI fetch
// =========================================================================

async fn fetch_pypi(client: &reqwest::Client, name: &str) -> Result<PackageMetadata> {
    let url = format!("https://pypi.org/pypi/{}/json", urlencoded(name));
    let resp = client.get(&url).send().await.context("PyPI GET")?;
    if !resp.status().is_success() {
        anyhow::bail!("PyPI non-success: {}", resp.status());
    }
    let body: Value = resp.json().await.context("PyPI JSON parse")?;

    let mut versions: Vec<VersionInfo> = Vec::new();
    if let Some(releases) = body.get("releases").and_then(|v| v.as_object()) {
        for (version, files) in releases {
            // Each release is a list of file objects (sdist + wheels).
            // Use the earliest upload_time as the published_at.
            let mut earliest: Option<DateTime<Utc>> = None;
            let mut yanked = false;
            let mut tarball_url: Option<String> = None;
            let mut tarball_sha256: Option<String> = None;
            let mut attestation_status = AttestationStatus::None;

            if let Some(arr) = files.as_array() {
                for f in arr {
                    let upload_time = f
                        .get("upload_time_iso_8601")
                        .and_then(|v| v.as_str())
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&Utc));
                    if let Some(t) = upload_time {
                        earliest = Some(earliest.map_or(t, |e| e.min(t)));
                    }
                    if f.get("yanked").and_then(|v| v.as_bool()).unwrap_or(false) {
                        yanked = true;
                    }
                    let pkg_type = f
                        .get("packagetype")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    if pkg_type == "sdist" && tarball_url.is_none() {
                        tarball_url = f
                            .get("url")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        tarball_sha256 = f
                            .get("digests")
                            .and_then(|d| d.get("sha256"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                    }
                    // PyPI exposes `has_sigstore` (PEP 740) at the
                    // file level. If any file has sigstore, the
                    // version has sigstore.
                    if f.get("has_sigstore")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        attestation_status = AttestationStatus::Sigstore;
                    }
                }
            }

            versions.push(VersionInfo {
                version: version.clone(),
                published_at: earliest,
                yanked,
                dependency_count: None,
                dependency_names: None,
                tarball_url,
                tarball_sha256,
                attestation_status,
            });
        }
    }
    versions.sort_by_key(|v| v.published_at.unwrap_or_else(Utc::now));

    // PyPI doesn't expose maintainer login lists in the public API.
    // The `info.author` and `info.maintainer` fields are free-text
    // strings (not GitHub-style logins). We capture them as a
    // single "owner" entry so the maintainer-delta sensor at least
    // detects when those fields change between snapshots.
    let mut owners: Vec<MaintainerInfo> = Vec::new();
    if let Some(info) = body.get("info") {
        if let Some(author) = info.get("author").and_then(|v| v.as_str()) {
            if !author.trim().is_empty() {
                owners.push(MaintainerInfo {
                    login: format!("author:{}", author.trim()),
                    name: Some(author.trim().to_string()),
                    url: None,
                });
            }
        }
        if let Some(maintainer) = info.get("maintainer").and_then(|v| v.as_str()) {
            if !maintainer.trim().is_empty() {
                owners.push(MaintainerInfo {
                    login: format!("maintainer:{}", maintainer.trim()),
                    name: Some(maintainer.trim().to_string()),
                    url: None,
                });
            }
        }
    }

    let info = body.get("info");
    let repository_url = info
        .and_then(|i| i.get("project_urls"))
        .and_then(|u| u.as_object())
        .and_then(|m| {
            for (k, v) in m {
                let k_lower = k.to_ascii_lowercase();
                if k_lower.contains("source")
                    || k_lower.contains("repository")
                    || k_lower.contains("github")
                {
                    if let Some(s) = v.as_str() {
                        return Some(s.to_string());
                    }
                }
            }
            None
        });
    let homepage_url = info
        .and_then(|i| i.get("home_page"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    Ok(PackageMetadata {
        ecosystem: ECOSYSTEM_PYPI.to_string(),
        name: name.to_string(),
        versions,
        owners,
        repository_url,
        homepage_url,
    })
}

// =========================================================================
// npm fetch
// =========================================================================

async fn fetch_npm(client: &reqwest::Client, name: &str) -> Result<PackageMetadata> {
    let url = format!("https://registry.npmjs.org/{}", encode_npm_name(name));
    let resp = client.get(&url).send().await.context("npm GET")?;
    if !resp.status().is_success() {
        anyhow::bail!("npm non-success: {}", resp.status());
    }
    let body: Value = resp.json().await.context("npm JSON parse")?;

    let mut versions: Vec<VersionInfo> = Vec::new();

    let time_map = body.get("time").and_then(|v| v.as_object());
    let versions_map = body.get("versions").and_then(|v| v.as_object());

    if let Some(versions_obj) = versions_map {
        for (version, vobj) in versions_obj {
            let published_at = time_map
                .and_then(|tm| tm.get(version))
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc));
            let dependency_count = vobj
                .get("dependencies")
                .and_then(|d| d.as_object())
                .map(|m| m.len());
            let dependency_names = vobj
                .get("dependencies")
                .and_then(|d| d.as_object())
                .map(|m| m.keys().cloned().collect::<Vec<_>>());
            let tarball_url = vobj
                .get("dist")
                .and_then(|d| d.get("tarball"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let tarball_sha256 = vobj
                .get("dist")
                .and_then(|d| d.get("integrity"))
                .and_then(|v| v.as_str())
                .filter(|s| s.starts_with("sha512-") || s.starts_with("sha256-"))
                .map(|s| s.to_string());
            // npm exposes `dist.attestations` (sigstore-via-attestation)
            // for trusted-published packages.
            let attestation_status = if vobj
                .get("dist")
                .and_then(|d| d.get("attestations"))
                .is_some()
            {
                AttestationStatus::Sigstore
            } else {
                AttestationStatus::None
            };

            versions.push(VersionInfo {
                version: version.clone(),
                published_at,
                yanked: false, // npm uses `time.deprecated` differently; v1 sets false.
                dependency_count,
                dependency_names,
                tarball_url,
                tarball_sha256,
                attestation_status,
            });
        }
    }
    versions.sort_by_key(|v| v.published_at.unwrap_or_else(Utc::now));

    // npm `maintainers` is an array of {name, email} objects.
    let mut owners: Vec<MaintainerInfo> = Vec::new();
    if let Some(arr) = body.get("maintainers").and_then(|v| v.as_array()) {
        for m in arr {
            let login = m
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let email = m.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
            if !login.is_empty() {
                owners.push(MaintainerInfo {
                    login,
                    name: email.clone(),
                    url: None,
                });
            }
        }
    }

    let repository_url = body
        .get("repository")
        .and_then(|r| r.get("url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let homepage_url = body
        .get("homepage")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(PackageMetadata {
        ecosystem: ECOSYSTEM_NPM.to_string(),
        name: name.to_string(),
        versions,
        owners,
        repository_url,
        homepage_url,
    })
}

// =========================================================================
// Cache helpers
// =========================================================================

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    written_at: DateTime<Utc>,
    metadata: PackageMetadata,
}

fn cache_file_path(cache_dir: &Path, ecosystem: &str, name: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}:{}", ecosystem, name).as_bytes());
    let digest = hex_digest(&hasher.finalize());
    let mut p = cache_dir.to_path_buf();
    p.push(ecosystem);
    p.push(format!("{}.json", digest));
    p
}

fn try_load_cache_entry(
    cache_dir: &Path,
    ecosystem: &str,
    name: &str,
) -> Option<(PackageMetadata, chrono::Duration)> {
    let path = cache_file_path(cache_dir, ecosystem, name);
    let raw = fs::read_to_string(&path).ok()?;
    let entry: CacheEntry = serde_json::from_str(&raw).ok()?;
    let age = Utc::now().signed_duration_since(entry.written_at);
    let ttl_secs = CACHE_TTL.as_secs() as i64;
    if age.num_seconds() > ttl_secs {
        return None; // stale
    }
    Some((entry.metadata, age))
}

fn write_cache_entry(
    cache_dir: &Path,
    ecosystem: &str,
    name: &str,
    metadata: &PackageMetadata,
) -> Result<()> {
    let path = cache_file_path(cache_dir, ecosystem, name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("vigilance: create cache dir")?;
    }
    let entry = CacheEntry {
        written_at: Utc::now(),
        metadata: metadata.clone(),
    };
    let json = serde_json::to_string_pretty(&entry).context("vigilance: cache serialize")?;
    fs::write(&path, json).context("vigilance: cache write")
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

/// URL-encode a name for use in a path segment. Conservative encoding —
/// percent-encodes anything outside `[A-Za-z0-9._~-]`.
fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'~' | b'-' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// npm scoped names like `@scope/pkg` need their `/` URL-encoded
/// to `%2F` per the npm registry contract.
fn encode_npm_name(s: &str) -> String {
    if let Some(rest) = s.strip_prefix('@') {
        if let Some((scope, pkg)) = rest.split_once('/') {
            return format!("@{}%2F{}", urlencoded(scope), urlencoded(pkg));
        }
    }
    urlencoded(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_behavior_env_parsing() {
        assert_eq!(cache_behavior_from_env_value(None), CacheBehavior::UseCache);
        assert_eq!(
            cache_behavior_from_env_value(Some("")),
            CacheBehavior::UseCache
        );
        assert_eq!(
            cache_behavior_from_env_value(Some("0")),
            CacheBehavior::UseCache
        );
        assert_eq!(
            cache_behavior_from_env_value(Some("false")),
            CacheBehavior::UseCache
        );
        assert_eq!(
            cache_behavior_from_env_value(Some("no")),
            CacheBehavior::UseCache
        );
        assert_eq!(
            cache_behavior_from_env_value(Some("1")),
            CacheBehavior::Bypass
        );
        assert_eq!(
            cache_behavior_from_env_value(Some("true")),
            CacheBehavior::Bypass
        );
        assert_eq!(
            cache_behavior_from_env_value(Some("anything-non-empty")),
            CacheBehavior::Bypass
        );
    }

    #[test]
    fn npm_scoped_name_encoding() {
        assert_eq!(encode_npm_name("@types/node"), "@types%2Fnode");
        assert_eq!(encode_npm_name("react"), "react");
        assert_eq!(encode_npm_name("@scope/pkg-name"), "@scope%2Fpkg-name");
    }

    #[test]
    fn cache_file_path_is_deterministic() {
        let tmp = std::env::temp_dir();
        let p1 = cache_file_path(&tmp, "PyPI", "litellm");
        let p2 = cache_file_path(&tmp, "PyPI", "litellm");
        assert_eq!(p1, p2);
        // Different ecosystem → different path.
        let p3 = cache_file_path(&tmp, "npm", "litellm");
        assert_ne!(p1, p3);
        // Different name → different path.
        let p4 = cache_file_path(&tmp, "PyPI", "requests");
        assert_ne!(p1, p4);
    }

    #[test]
    fn cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let meta = PackageMetadata {
            ecosystem: "PyPI".to_string(),
            name: "fakepkg".to_string(),
            versions: vec![VersionInfo {
                version: "1.0.0".to_string(),
                published_at: Some(Utc::now()),
                yanked: false,
                dependency_count: Some(3),
                dependency_names: Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]),
                tarball_url: None,
                tarball_sha256: None,
                attestation_status: AttestationStatus::None,
            }],
            owners: vec![MaintainerInfo {
                login: "alice".to_string(),
                name: None,
                url: None,
            }],
            repository_url: None,
            homepage_url: None,
        };
        write_cache_entry(dir.path(), "PyPI", "fakepkg", &meta).unwrap();
        let (loaded, age) = try_load_cache_entry(dir.path(), "PyPI", "fakepkg").unwrap();
        assert_eq!(loaded.name, "fakepkg");
        assert_eq!(loaded.versions.len(), 1);
        assert!(age.num_seconds() < 5); // just wrote it
    }

    #[test]
    fn cache_miss_on_unknown() {
        let dir = tempfile::tempdir().unwrap();
        assert!(try_load_cache_entry(dir.path(), "PyPI", "never-cached").is_none());
    }
}
