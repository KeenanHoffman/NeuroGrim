//! OSV.dev batch-query client + file-backed response cache.
//!
//! # Design
//!
//! - POST `https://api.osv.dev/v1/querybatch` with a body of
//!   `{queries: [{package: {name, ecosystem: "crates.io"}, version}, ...]}`.
//! - Split large batches at 1000 queries per request (OSV limit).
//! - File-backed cache at `<cache_dir>/<sha256-of-key>.json` where
//!   `<cache_dir>` is typically `<project_root>/.claude/brain/cache/osv/`.
//! - 24h TTL by default; `NEUROGRIM_OSV_NO_CACHE=1` env forces re-query.
//! - Cache-hit metadata (oldest entry age, hit count) surfaced via
//!   `OsvQueryResult` for inclusion in the CMDB envelope extras.
//!
//! # Error posture
//!
//! The OSV endpoint being unreachable is NOT a fatal error. The sensor
//! degrades to "what was in cache" + flags `osv_reachable=false` so
//! downstream can see the partial coverage. Fresh cache writes happen
//! only for successful live queries.
//!
//! # Trust surface
//!
//! Lives on: `reqwest` (rustls-tls-native-roots, OS trust store),
//! `serde_json`, `sha2`, `chrono`, OSV.dev HTTPS endpoint. No scanner
//! binaries.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{Advisory, AdvisorySource, Package};

/// OSV ecosystem string for Rust / crates.io. OSV's own taxonomy.
pub const ECOSYSTEM_CRATES_IO: &str = "crates.io";

/// OSV batch-query endpoint.
const OSV_BATCH_URL: &str = "https://api.osv.dev/v1/querybatch";

/// Maximum queries per OSV batch request.
const OSV_MAX_BATCH: usize = 1000;

/// Cache TTL. Entries older than this are treated as misses.
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Per-request timeout for the live OSV call.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Back-off before the single retry attempt.
const RETRY_BACKOFF: Duration = Duration::from_millis(1000);

/// Rich result from an OSV batch query, with metadata that the parent
/// sensor surfaces in the CMDB envelope extras.
#[derive(Debug, Clone, Default)]
pub struct OsvQueryResult {
    /// Advisories found across all queried packages (union of cache
    /// hits + fresh queries).
    pub advisories: Vec<Advisory>,
    /// Packages served from cache (entry present + not stale).
    pub cache_hits: usize,
    /// Packages queried live this run.
    pub live_queries: usize,
    /// Age of the OLDEST cache entry consulted, in seconds. `None` if
    /// no cache entry was consulted this run.
    pub oldest_cache_age_seconds: Option<u64>,
    /// `true` if the live OSV endpoint responded successfully at
    /// least once this run. `false` means everything was served from
    /// cache OR all live attempts failed.
    pub osv_reachable: bool,
    /// `true` if `NEUROGRIM_OSV_NO_CACHE` was honored (cache skipped).
    pub cache_bypassed: bool,
}

/// Whether cache read/write is active this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheBehavior {
    UseCache,
    Bypass,
}

/// Query OSV.dev for advisories affecting `packages`.
///
/// Uses the cache at `cache_dir` when entries are fresh (< 24h) and
/// `NEUROGRIM_OSV_NO_CACHE` is unset or empty. See module docs for
/// full semantics.
///
/// Always returns a valid `OsvQueryResult`. Network errors are
/// captured in `osv_reachable=false` + a best-effort partial
/// `advisories` list from cache, not propagated as `Err`.
pub async fn query_batch(packages: &[Package], cache_dir: &Path) -> Result<OsvQueryResult> {
    let cache_behavior = cache_behavior_from_env_value(
        std::env::var("NEUROGRIM_OSV_NO_CACHE").ok().as_deref(),
    );
    query_batch_with_options(packages, cache_dir, cache_behavior).await
}

/// Inner entry point that exposes `CacheBehavior` as a parameter — used
/// directly by tests (which should not touch process-wide env vars).
pub async fn query_batch_with_options(
    packages: &[Package],
    cache_dir: &Path,
    cache_behavior: CacheBehavior,
) -> Result<OsvQueryResult> {
    if packages.is_empty() {
        return Ok(OsvQueryResult {
            cache_bypassed: cache_behavior == CacheBehavior::Bypass,
            osv_reachable: true, // vacuously; no calls needed
            ..Default::default()
        });
    }

    // Partition into cache-hits and misses.
    let mut result = OsvQueryResult {
        cache_bypassed: cache_behavior == CacheBehavior::Bypass,
        ..Default::default()
    };
    let mut misses: Vec<&Package> = Vec::new();

    if cache_behavior == CacheBehavior::UseCache {
        for pkg in packages {
            match try_load_cache_entry(cache_dir, ECOSYSTEM_CRATES_IO, pkg) {
                Some((entry, age)) => {
                    // Cache hit.
                    result.cache_hits += 1;
                    let age_secs = age.num_seconds().max(0) as u64;
                    result.oldest_cache_age_seconds = Some(
                        result
                            .oldest_cache_age_seconds
                            .map_or(age_secs, |o| o.max(age_secs)),
                    );
                    for vuln_id in entry.vuln_ids {
                        result.advisories.push(Advisory {
                            id: vuln_id,
                            package: pkg.clone(),
                            summary: None,
                            source: AdvisorySource::Osv,
                            informational: None,
                        });
                    }
                }
                None => misses.push(pkg),
            }
        }
    } else {
        // Bypass: everything is a miss.
        misses.extend(packages.iter());
    }

    // Live-query the misses in batches of up to 1000.
    if !misses.is_empty() {
        let client = build_http_client()?;
        let mut any_live_success = false;

        for chunk in misses.chunks(OSV_MAX_BATCH) {
            match query_osv_live(&client, chunk).await {
                Ok(vuln_map) => {
                    any_live_success = true;
                    result.live_queries += chunk.len();
                    for pkg in chunk {
                        let vuln_ids = vuln_map
                            .get(&(pkg.name.clone(), pkg.version.clone()))
                            .cloned()
                            .unwrap_or_default();

                        // Persist to cache even if empty (absence of
                        // advisories is a first-class fact worth caching).
                        if cache_behavior == CacheBehavior::UseCache {
                            let _ = write_cache_entry(
                                cache_dir,
                                ECOSYSTEM_CRATES_IO,
                                pkg,
                                &vuln_ids,
                            );
                        }

                        for vuln_id in vuln_ids {
                            result.advisories.push(Advisory {
                                id: vuln_id,
                                package: (*pkg).clone(),
                                summary: None,
                                source: AdvisorySource::Osv,
                                informational: None,
                            });
                        }
                    }
                }
                Err(e) => {
                    // Log + continue — other batches may succeed, and
                    // cache hits may still be usable.
                    tracing::warn!(
                        "OSV batch query failed for {} packages: {:#}",
                        chunk.len(),
                        e
                    );
                }
            }
        }

        result.osv_reachable = any_live_success;
    } else {
        // All hits from cache; OSV was not consulted but no failure.
        result.osv_reachable = true;
    }

    Ok(result)
}

// =========================================================================
// Env handling (pure — testable without setting process env)
// =========================================================================

/// Parse a `NEUROGRIM_OSV_NO_CACHE` env value into a `CacheBehavior`.
///
/// Truthy values (anything except `""`, `"0"`, `"false"` in any case)
/// disable the cache. Absence of the variable preserves default
/// caching.
pub fn cache_behavior_from_env_value(val: Option<&str>) -> CacheBehavior {
    match val {
        None => CacheBehavior::UseCache,
        Some(v) => {
            let v = v.trim();
            if v.is_empty()
                || v == "0"
                || v.eq_ignore_ascii_case("false")
                || v.eq_ignore_ascii_case("no")
            {
                CacheBehavior::UseCache
            } else {
                CacheBehavior::Bypass
            }
        }
    }
}

// =========================================================================
// OSV wire types
// =========================================================================

#[derive(Debug, Serialize)]
struct OsvQueryBatchRequest<'a> {
    queries: Vec<OsvQueryItem<'a>>,
}

#[derive(Debug, Serialize)]
struct OsvQueryItem<'a> {
    package: OsvPackage<'a>,
    version: &'a str,
}

#[derive(Debug, Serialize)]
struct OsvPackage<'a> {
    name: &'a str,
    ecosystem: &'a str,
}

#[derive(Debug, Deserialize)]
struct OsvQueryBatchResponse {
    #[serde(default)]
    results: Vec<OsvResultEntry>,
}

#[derive(Debug, Deserialize, Default)]
struct OsvResultEntry {
    #[serde(default)]
    vulns: Vec<OsvVuln>,
}

#[derive(Debug, Deserialize)]
struct OsvVuln {
    id: String,
    // `modified` and other fields are present but we don't need them
    // for Step 5. Full vuln detail (summary, severity, etc.) lives
    // behind `GET /v1/vulns/{id}` and is a follow-on.
}

// =========================================================================
// Live HTTP call
// =========================================================================

fn build_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .context("failed to build reqwest client")
}

/// Issue one POST /v1/querybatch for up to `OSV_MAX_BATCH` packages.
/// Retries once with `RETRY_BACKOFF` sleep on the first failure.
///
/// Returns a map `(package_name, version) -> [vuln_id, ...]` for every
/// package queried (including those with no advisories — they map to
/// an empty Vec).
async fn query_osv_live(
    client: &reqwest::Client,
    packages: &[&Package],
) -> Result<std::collections::HashMap<(String, String), Vec<String>>> {
    // Build request body.
    let queries: Vec<OsvQueryItem> = packages
        .iter()
        .map(|pkg| OsvQueryItem {
            package: OsvPackage {
                name: &pkg.name,
                ecosystem: ECOSYSTEM_CRATES_IO,
            },
            version: &pkg.version,
        })
        .collect();
    let body = OsvQueryBatchRequest { queries };

    // Attempt (try once, retry once on failure).
    let raw = {
        let mut last_err: Option<anyhow::Error> = None;
        let mut out: Option<String> = None;
        for attempt in 0..2 {
            match post_once(client, &body).await {
                Ok(text) => {
                    out = Some(text);
                    break;
                }
                Err(e) => {
                    tracing::debug!("OSV live query attempt {attempt} failed: {:#}", e);
                    last_err = Some(e);
                    if attempt == 0 {
                        tokio::time::sleep(RETRY_BACKOFF).await;
                    }
                }
            }
        }
        out.ok_or_else(|| last_err.unwrap_or_else(|| anyhow::anyhow!("OSV query failed")))?
    };

    let parsed: OsvQueryBatchResponse =
        serde_json::from_str(&raw).with_context(|| "OSV response JSON parse")?;

    if parsed.results.len() != packages.len() {
        tracing::warn!(
            "OSV returned {} results for {} queries",
            parsed.results.len(),
            packages.len()
        );
    }

    let mut map = std::collections::HashMap::new();
    for (pkg, entry) in packages.iter().zip(parsed.results.into_iter()) {
        let ids: Vec<String> = entry.vulns.into_iter().map(|v| v.id).collect();
        map.insert((pkg.name.clone(), pkg.version.clone()), ids);
    }
    Ok(map)
}

async fn post_once<'a>(
    client: &reqwest::Client,
    body: &OsvQueryBatchRequest<'a>,
) -> Result<String> {
    let response = client
        .post(OSV_BATCH_URL)
        .json(body)
        .send()
        .await
        .context("OSV POST send")?;

    if !response.status().is_success() {
        anyhow::bail!("OSV returned HTTP {}", response.status());
    }

    response.text().await.context("OSV POST read body")
}

// =========================================================================
// Cache: persisted shape + I/O helpers
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// ISO8601 UTC timestamp of the write.
    ts: String,
    /// Package name (denormalized for debugging-from-file-system).
    package_name: String,
    /// OSV ecosystem (denormalized).
    ecosystem: String,
    /// Package version (denormalized).
    version: String,
    /// OSV vulnerability IDs. Empty vec = queried but no advisories.
    vuln_ids: Vec<String>,
}

/// Compute the cache-file name for a (ecosystem, name, version) tuple.
/// `sha256(ecosystem "|" name "|" version)` hex-encoded + `.json`.
fn cache_key(ecosystem: &str, name: &str, version: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ecosystem.as_bytes());
    hasher.update(b"|");
    hasher.update(name.as_bytes());
    hasher.update(b"|");
    hasher.update(version.as_bytes());
    let digest = hasher.finalize();
    format!("{digest:x}.json")
}

fn cache_file_path(cache_dir: &Path, ecosystem: &str, pkg: &Package) -> PathBuf {
    cache_dir.join(cache_key(ecosystem, &pkg.name, &pkg.version))
}

/// Try to load a cache entry. Returns `Some((entry, age))` only if the
/// file exists, parses, and is within the TTL. Any other condition
/// (missing, corrupt, stale) returns `None`.
fn try_load_cache_entry(
    cache_dir: &Path,
    ecosystem: &str,
    pkg: &Package,
) -> Option<(CacheEntry, chrono::Duration)> {
    let path = cache_file_path(cache_dir, ecosystem, pkg);
    let raw = fs::read_to_string(&path).ok()?;
    let entry: CacheEntry = serde_json::from_str(&raw).ok()?;
    // Sanity-check denormalized fields. A mismatch means either a key
    // collision (SHA-256 — implausibly unlikely) or a corrupt cache;
    // either way, treat as miss.
    if entry.package_name != pkg.name
        || entry.ecosystem != ecosystem
        || entry.version != pkg.version
    {
        return None;
    }

    let ts = DateTime::parse_from_rfc3339(&entry.ts).ok()?;
    let now = Utc::now();
    let age = now.signed_duration_since(ts.with_timezone(&Utc));

    // TTL check.
    if age.to_std().ok()? > CACHE_TTL {
        return None;
    }
    if age < chrono::Duration::zero() {
        // Clock skew: entry is from the "future". Treat as fresh
        // rather than stale — safer default, and the warning surfaces
        // in tracing.
        tracing::warn!(
            "OSV cache entry has future timestamp ({ts}); using as fresh"
        );
        return Some((entry, chrono::Duration::zero()));
    }
    Some((entry, age))
}

/// Test-only helper: pre-populate the OSV cache for a given package.
///
/// Integration tests use this to avoid flakiness from live OSV calls
/// — a pre-seeded cache entry means `query_batch` treats that
/// package as a cache hit and never touches the network.
///
/// Hidden from public docs; visible to `tests/sensor_behavior.rs`
/// because the integration-test target is a separate crate.
#[doc(hidden)]
pub fn _testing_seed_cache(
    cache_dir: &Path,
    ecosystem: &str,
    pkg: &Package,
    vuln_ids: &[String],
) -> Result<()> {
    write_cache_entry(cache_dir, ecosystem, pkg, vuln_ids)
}

/// Persist a cache entry. Best-effort; failures are logged but do not
/// propagate (the live query already succeeded).
fn write_cache_entry(
    cache_dir: &Path,
    ecosystem: &str,
    pkg: &Package,
    vuln_ids: &[String],
) -> Result<()> {
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("create cache dir {}", cache_dir.display()))?;
    let path = cache_file_path(cache_dir, ecosystem, pkg);
    let entry = CacheEntry {
        ts: Utc::now().to_rfc3339(),
        package_name: pkg.name.clone(),
        ecosystem: ecosystem.to_owned(),
        version: pkg.version.clone(),
        vuln_ids: vuln_ids.to_vec(),
    };
    let raw = serde_json::to_string(&entry).context("serialize cache entry")?;
    fs::write(&path, raw).with_context(|| format!("write cache file {}", path.display()))?;
    Ok(())
}

// =========================================================================
// Tests (pure / cache; live-HTTP tested via integration test in Step 10)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_package() -> Package {
        Package {
            name: "serde".to_string(),
            version: "1.0.228".to_string(),
        }
    }

    #[test]
    fn env_parse_unset_is_use_cache() {
        assert_eq!(
            cache_behavior_from_env_value(None),
            CacheBehavior::UseCache
        );
    }

    #[test]
    fn env_parse_empty_is_use_cache() {
        assert_eq!(
            cache_behavior_from_env_value(Some("")),
            CacheBehavior::UseCache
        );
    }

    #[test]
    fn env_parse_zero_is_use_cache() {
        assert_eq!(
            cache_behavior_from_env_value(Some("0")),
            CacheBehavior::UseCache
        );
    }

    #[test]
    fn env_parse_false_insensitive_is_use_cache() {
        assert_eq!(
            cache_behavior_from_env_value(Some("false")),
            CacheBehavior::UseCache
        );
        assert_eq!(
            cache_behavior_from_env_value(Some("False")),
            CacheBehavior::UseCache
        );
        assert_eq!(
            cache_behavior_from_env_value(Some("FALSE")),
            CacheBehavior::UseCache
        );
    }

    #[test]
    fn env_parse_one_is_bypass() {
        assert_eq!(
            cache_behavior_from_env_value(Some("1")),
            CacheBehavior::Bypass
        );
    }

    #[test]
    fn env_parse_true_is_bypass() {
        assert_eq!(
            cache_behavior_from_env_value(Some("true")),
            CacheBehavior::Bypass
        );
    }

    #[test]
    fn env_parse_arbitrary_truthy_is_bypass() {
        assert_eq!(
            cache_behavior_from_env_value(Some("yes-please")),
            CacheBehavior::Bypass
        );
    }

    #[test]
    fn cache_key_is_deterministic() {
        let a = cache_key("crates.io", "serde", "1.0.228");
        let b = cache_key("crates.io", "serde", "1.0.228");
        assert_eq!(a, b);
        assert!(a.ends_with(".json"));
    }

    #[test]
    fn cache_key_differs_across_ecosystem() {
        let a = cache_key("crates.io", "foo", "1.0.0");
        let b = cache_key("PyPI", "foo", "1.0.0");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_differs_across_version() {
        let a = cache_key("crates.io", "serde", "1.0.228");
        let b = cache_key("crates.io", "serde", "1.0.229");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_write_then_read_roundtrips() {
        let tmp = TempDir::new().unwrap();
        let pkg = sample_package();
        let ids = vec!["RUSTSEC-2026-0104".to_string()];

        write_cache_entry(tmp.path(), ECOSYSTEM_CRATES_IO, &pkg, &ids)
            .expect("write cache");

        let loaded = try_load_cache_entry(tmp.path(), ECOSYSTEM_CRATES_IO, &pkg)
            .expect("cache hit");
        assert_eq!(loaded.0.vuln_ids, ids);
        assert_eq!(loaded.0.package_name, pkg.name);
        assert_eq!(loaded.0.version, pkg.version);
        // Age should be small (we just wrote it).
        assert!(loaded.1.num_seconds() < 5);
    }

    #[test]
    fn cache_read_missing_is_none() {
        let tmp = TempDir::new().unwrap();
        let loaded =
            try_load_cache_entry(tmp.path(), ECOSYSTEM_CRATES_IO, &sample_package());
        assert!(loaded.is_none());
    }

    #[test]
    fn cache_read_corrupt_is_none() {
        let tmp = TempDir::new().unwrap();
        let pkg = sample_package();
        let path = cache_file_path(tmp.path(), ECOSYSTEM_CRATES_IO, &pkg);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not-json-content").unwrap();
        let loaded = try_load_cache_entry(tmp.path(), ECOSYSTEM_CRATES_IO, &pkg);
        assert!(loaded.is_none());
    }

    #[test]
    fn cache_read_stale_is_none() {
        // Write a cache file with a timestamp 25h in the past.
        let tmp = TempDir::new().unwrap();
        let pkg = sample_package();
        let stale_ts = (Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
        let entry = CacheEntry {
            ts: stale_ts,
            package_name: pkg.name.clone(),
            ecosystem: ECOSYSTEM_CRATES_IO.to_string(),
            version: pkg.version.clone(),
            vuln_ids: vec![],
        };
        let path = cache_file_path(tmp.path(), ECOSYSTEM_CRATES_IO, &pkg);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_string(&entry).unwrap()).unwrap();

        let loaded = try_load_cache_entry(tmp.path(), ECOSYSTEM_CRATES_IO, &pkg);
        assert!(
            loaded.is_none(),
            "stale cache entry should be treated as miss"
        );
    }

    #[test]
    fn cache_read_mismatched_denorm_is_none() {
        // Write an entry that has a mismatched package_name field
        // (simulates a hash collision or file-system tampering).
        let tmp = TempDir::new().unwrap();
        let pkg = sample_package();
        let entry = CacheEntry {
            ts: Utc::now().to_rfc3339(),
            package_name: "SOMETHING_ELSE".to_string(),
            ecosystem: ECOSYSTEM_CRATES_IO.to_string(),
            version: pkg.version.clone(),
            vuln_ids: vec!["RUSTSEC-...".to_string()],
        };
        let path = cache_file_path(tmp.path(), ECOSYSTEM_CRATES_IO, &pkg);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_string(&entry).unwrap()).unwrap();

        let loaded = try_load_cache_entry(tmp.path(), ECOSYSTEM_CRATES_IO, &pkg);
        assert!(loaded.is_none(), "denormalization mismatch must fail-closed");
    }

    #[tokio::test]
    async fn bypass_mode_skips_cache_entirely() {
        // With Bypass: even a fresh cache entry is ignored. Without
        // network, the call completes with 0 advisories + osv_reachable
        // false (because no live queries succeeded). We only exercise
        // the cache-bypass branch here — the network side is tested by
        // the integration test in Step 10.
        let tmp = TempDir::new().unwrap();
        let pkg = sample_package();
        write_cache_entry(
            tmp.path(),
            ECOSYSTEM_CRATES_IO,
            &pkg,
            &["RUSTSEC-FAKE-0001".to_string()],
        )
        .unwrap();

        // With UseCache: we'd get a hit.
        let res_cached = query_batch_with_options(
            std::slice::from_ref(&pkg),
            tmp.path(),
            CacheBehavior::UseCache,
        )
        .await
        .unwrap();
        assert_eq!(res_cached.cache_hits, 1);
        assert_eq!(res_cached.live_queries, 0);
        assert_eq!(res_cached.advisories.len(), 1);

        // With Bypass: the cache entry is NOT served. Since live query
        // can't succeed in tests (no network guaranteed), we expect 0
        // cache hits and `osv_reachable` to be false (or true if we
        // happen to have network). The key invariant is cache_hits==0.
        let res_bypass = query_batch_with_options(
            std::slice::from_ref(&pkg),
            tmp.path(),
            CacheBehavior::Bypass,
        )
        .await
        .unwrap();
        assert_eq!(res_bypass.cache_hits, 0);
        assert!(res_bypass.cache_bypassed);
    }

    #[tokio::test]
    async fn empty_packages_input_is_noop() {
        let tmp = TempDir::new().unwrap();
        let res = query_batch_with_options(&[], tmp.path(), CacheBehavior::UseCache)
            .await
            .unwrap();
        assert_eq!(res.cache_hits, 0);
        assert_eq!(res.live_queries, 0);
        assert!(res.advisories.is_empty());
    }
}
