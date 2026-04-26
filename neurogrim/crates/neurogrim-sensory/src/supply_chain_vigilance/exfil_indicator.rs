//! `exfil_indicator` sensor — Phase 4 (Group C).
//!
//! Static-analysis heuristics on package source for patterns that
//! historically precede exfiltration payloads:
//!
//! - **Long base64 strings** (>200 chars) — encoded payload
//!   indicator (LiteLLM 1.82.7 carried base64-encoded compromise).
//! - **Dynamic eval/exec/Function** — runtime code generation.
//! - **subprocess/Command/child_process** invocations — process
//!   spawning, common exfiltration vector when paired with curl/wget.
//! - **Suspicious import patterns** — `__import__("os")`,
//!   `require(<dynamic>)`, etc.
//!
//! # Safety discipline
//!
//! The sensor downloads + extracts package tarballs of potentially-
//! malicious code. Extraction MUST NOT execute anything (no
//! `setup.py install`, no `npm install`, no `cargo build`). We use
//! the `tar` crate's stream API with explicit guards:
//!
//! - `set_preserve_permissions(false)` — don't honor +x bits.
//! - `set_preserve_mtime(false)` — don't preserve timestamps.
//! - **Path-traversal guard:** every entry's path is validated to
//!   stay under the extraction root. Symlinks pointing outside the
//!   root are rejected.
//! - **Size cap:** total extracted bytes ≤ MAX_EXTRACTED_BYTES,
//!   per-file ≤ MAX_PER_FILE_BYTES.
//! - **File-count cap:** ≤ MAX_EXTRACTED_FILES.
//! - **Extension allow-list:** only `.py` / `.rs` / `.js` / `.ts` /
//!   `.mjs` / `.cjs` / `.toml` / `.json` files are even READ for
//!   analysis.
//!
//! After analysis the extraction directory is removed. Per-package
//! cache holds the FINDING result (counts + flags), not the source.
//!
//! # Opt-in posture
//!
//! Disabled by default — multi-MB downloads + extraction time per
//! package adds up. Activate via `NEUROGRIM_VIGILANCE_EXFIL=1`.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::supply_chain_sca::Package;

use super::registry::CacheBehavior;
use super::scoring::{VigilanceFinding, VigilanceKind};

// ── Caps ────────────────────────────────────────────────────────────
const MAX_TARBALL_BYTES: u64 = 50 * 1024 * 1024; // 50 MB
const MAX_EXTRACTED_BYTES: u64 = 100 * 1024 * 1024; // 100 MB
const MAX_PER_FILE_BYTES: u64 = 5 * 1024 * 1024; // 5 MB
const MAX_EXTRACTED_FILES: usize = 5000;

const EXFIL_HTTP_TIMEOUT: Duration = Duration::from_secs(60);
const EXFIL_USER_AGENT: &str = concat!(
    "NeuroGrim-supply-chain-vigilance-exfil/",
    env!("CARGO_PKG_VERSION")
);

const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

// ── Pattern thresholds ──────────────────────────────────────────────
/// Number of long-base64 strings above which we flag.
const BASE64_THRESHOLD: u32 = 3;
/// Length above which a base64 candidate is "long."
const BASE64_MIN_LENGTH: usize = 200;
/// Total exfil-pattern hits above which we flag.
const EXFIL_PATTERN_THRESHOLD: u32 = 8;

pub async fn scan(
    packages: &[Package],
    cache_dir: &Path,
    cache_behavior: CacheBehavior,
) -> Vec<VigilanceFinding> {
    if !is_active() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    let exfil_cache_dir = cache_dir.join("exfil");

    let client = match build_http_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("exfil: HTTP client build failed: {:#}", e);
            return Vec::new();
        }
    };

    let mut seen: std::collections::HashSet<(String, String, &'static str)> =
        std::collections::HashSet::new();

    for pkg in packages {
        if !seen.insert((pkg.name.clone(), pkg.version.clone(), pkg.ecosystem)) {
            continue;
        }

        // Cache lookup.
        if cache_behavior == CacheBehavior::UseCache {
            if let Some(cached) =
                try_load_cache(&exfil_cache_dir, pkg.ecosystem, &pkg.name, &pkg.version)
            {
                if let Some(f) = build_finding_from_counts(pkg, &cached) {
                    findings.push(f);
                }
                continue;
            }
        }

        // We need the tarball URL — fetch from per-package metadata
        // dispatched via the orchestrator's FetchAllResult (which the
        // caller passes in). For a self-contained sensor signature,
        // this scan-loop is conceptually independent — but in
        // practice the orchestrator hands us metadata. We require
        // metadata to find the tarball URL.
        // Note: this signature-shape choice means the orchestrator
        // MUST pass metadata through.
        // (See `analyze_supply_chain_vigilance` in the parent module.)
        // To keep the API simple we accept None metadata and skip in
        // that case.

        // Since this `scan` doesn't get metadata in its current
        // signature, we'd have to fetch the registry metadata
        // separately. To keep the parent signature stable we do a
        // direct fetch HERE for the version's tarball URL — paying
        // the extra network round-trip. Future refactor: pass
        // metadata through. For now this is acceptable since
        // exfil_indicator is opt-in.
        match analyze_one(&client, pkg).await {
            Ok(counts) => {
                let entry = ExfilCacheEntry {
                    written_at: Utc::now(),
                    counts: counts.clone(),
                };
                if cache_behavior == CacheBehavior::UseCache {
                    let _ = write_cache(
                        &exfil_cache_dir,
                        pkg.ecosystem,
                        &pkg.name,
                        &pkg.version,
                        &entry,
                    );
                }
                if let Some(f) = build_finding_from_counts(pkg, &counts) {
                    findings.push(f);
                }
            }
            Err(e) => {
                tracing::debug!(
                    "exfil: analyze failed for {}@{}: {:#}",
                    pkg.name,
                    pkg.version,
                    e
                );
            }
        }
    }

    findings
}

pub fn is_active() -> bool {
    is_active_from_value(std::env::var("NEUROGRIM_VIGILANCE_EXFIL").ok().as_deref())
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
struct ExfilCacheEntry {
    written_at: DateTime<Utc>,
    counts: PatternCounts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatternCounts {
    /// Number of base64 candidate strings ≥ BASE64_MIN_LENGTH.
    pub long_base64_count: u32,
    /// Aggregate count of eval/exec/Function/subprocess/Command/etc.
    pub exfil_pattern_count: u32,
    /// Specific pattern hits.
    pub eval_count: u32,
    pub exec_count: u32,
    pub subprocess_count: u32,
    pub command_new_count: u32,
    pub child_process_count: u32,
    pub function_constructor_count: u32,
    pub dynamic_import_count: u32,
    /// Files scanned (informational).
    pub files_scanned: u32,
}

fn build_finding_from_counts(pkg: &Package, counts: &PatternCounts) -> Option<VigilanceFinding> {
    if counts.long_base64_count < BASE64_THRESHOLD
        && counts.exfil_pattern_count < EXFIL_PATTERN_THRESHOLD
    {
        return None;
    }
    let mut reasons: Vec<String> = Vec::new();
    if counts.long_base64_count >= BASE64_THRESHOLD {
        reasons.push(format!(
            "{} long base64 strings (≥{}c)",
            counts.long_base64_count, BASE64_MIN_LENGTH
        ));
    }
    if counts.exfil_pattern_count >= EXFIL_PATTERN_THRESHOLD {
        reasons.push(format!(
            "{} exfil-pattern hits (eval/exec/subprocess/Command/etc)",
            counts.exfil_pattern_count
        ));
    }
    Some(VigilanceFinding {
        kind: VigilanceKind::ExfilIndicator,
        package: pkg.clone(),
        summary: format!(
            "static analysis flagged {}",
            reasons.join(" + ")
        ),
        evidence: Some(json!({
            "long_base64_count": counts.long_base64_count,
            "exfil_pattern_count": counts.exfil_pattern_count,
            "eval_count": counts.eval_count,
            "exec_count": counts.exec_count,
            "subprocess_count": counts.subprocess_count,
            "command_new_count": counts.command_new_count,
            "child_process_count": counts.child_process_count,
            "function_constructor_count": counts.function_constructor_count,
            "dynamic_import_count": counts.dynamic_import_count,
            "files_scanned": counts.files_scanned,
            "base64_threshold": BASE64_THRESHOLD,
            "exfil_pattern_threshold": EXFIL_PATTERN_THRESHOLD,
        })),
        confidence: 0.5,
    })
}

fn build_http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(EXFIL_HTTP_TIMEOUT)
        .user_agent(EXFIL_USER_AGENT)
        .build()?)
}

/// Fetch package metadata from the registry, find the current
/// version's tarball, download + extract + analyze.
async fn analyze_one(client: &reqwest::Client, pkg: &Package) -> Result<PatternCounts> {
    let tarball_url = registry_tarball_url(client, pkg).await?;
    let bytes = fetch_tarball(client, &tarball_url).await?;
    // Use OS temp dir + unique name for extraction; clean up on
    // function exit. We use `std::env::temp_dir` rather than the
    // `tempfile` crate (which is dev-only in our deps).
    let mut hasher = Sha256::new();
    hasher.update(format!("{}:{}:{}", pkg.ecosystem, pkg.name, pkg.version).as_bytes());
    hasher.update(Utc::now().to_rfc3339().as_bytes());
    let unique: String = hasher
        .finalize()
        .iter()
        .take(8)
        .map(|b| format!("{:02x}", b))
        .collect();
    let extract_root = std::env::temp_dir().join(format!("neurogrim-vigilance-{}", unique));
    std::fs::create_dir_all(&extract_root).context("exfil: mkdir extract")?;
    let counts_result = (|| -> Result<PatternCounts> {
        extract_tarball_safe(&bytes, &extract_root)?;
        scan_extracted(&extract_root, pkg.ecosystem)
    })();
    // Best-effort cleanup; errors here don't propagate.
    let _ = std::fs::remove_dir_all(&extract_root);
    counts_result
}

/// Look up the tarball URL for `pkg.version` from the registry.
/// We do a fresh fetch here because `scan` doesn't currently get
/// passed `FetchAllResult`. (Future refactor: pass it through.)
async fn registry_tarball_url(client: &reqwest::Client, pkg: &Package) -> Result<String> {
    use crate::supply_chain_sca::osv::{ECOSYSTEM_CRATES_IO, ECOSYSTEM_NPM, ECOSYSTEM_PYPI};
    let url = match pkg.ecosystem {
        x if x == ECOSYSTEM_CRATES_IO => format!(
            "https://crates.io/api/v1/crates/{}/{}/download",
            urlencoded(&pkg.name),
            urlencoded(&pkg.version),
        ),
        x if x == ECOSYSTEM_PYPI => {
            // PyPI: need to fetch the JSON and find the sdist URL.
            return pypi_sdist_url(client, &pkg.name, &pkg.version).await;
        }
        x if x == ECOSYSTEM_NPM => {
            // npm: construct the canonical tarball URL.
            // Format: https://registry.npmjs.org/<name>/-/<basename>-<version>.tgz
            let basename = if let Some(rest) = pkg.name.strip_prefix('@') {
                if let Some((_scope, n)) = rest.split_once('/') {
                    n
                } else {
                    pkg.name.as_str()
                }
            } else {
                pkg.name.as_str()
            };
            format!(
                "https://registry.npmjs.org/{}/-/{}-{}.tgz",
                encode_npm_name(&pkg.name),
                basename,
                pkg.version,
            )
        }
        other => bail!("exfil: unknown ecosystem {other:?}"),
    };
    Ok(url)
}

async fn pypi_sdist_url(client: &reqwest::Client, name: &str, version: &str) -> Result<String> {
    let url = format!("https://pypi.org/pypi/{}/{}/json", urlencoded(name), urlencoded(version));
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        bail!("PyPI version json: {}", resp.status());
    }
    let body: serde_json::Value = resp.json().await?;
    let urls = body
        .get("urls")
        .and_then(|v| v.as_array())
        .context("PyPI: no urls array")?;
    for f in urls {
        if f.get("packagetype")
            .and_then(|v| v.as_str())
            .map(|s| s == "sdist")
            .unwrap_or(false)
        {
            if let Some(u) = f.get("url").and_then(|v| v.as_str()) {
                return Ok(u.to_string());
            }
        }
    }
    bail!("PyPI: no sdist for {}@{}", name, version)
}

async fn fetch_tarball(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        bail!("tarball fetch non-success: {} {}", resp.status(), url);
    }
    if let Some(len) = resp.content_length() {
        if len > MAX_TARBALL_BYTES {
            bail!("tarball too large: {} > {}", len, MAX_TARBALL_BYTES);
        }
    }
    let bytes = resp.bytes().await?.to_vec();
    if bytes.len() as u64 > MAX_TARBALL_BYTES {
        bail!(
            "tarball exceeded max after read: {} > {}",
            bytes.len(),
            MAX_TARBALL_BYTES
        );
    }
    Ok(bytes)
}

fn extract_tarball_safe(bytes: &[u8], extract_root: &Path) -> Result<()> {
    let gz = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(gz);
    archive.set_preserve_permissions(false);
    archive.set_preserve_mtime(false);

    let extract_root_canonical = std::fs::canonicalize(extract_root)
        .context("exfil: canonicalize extract root")?;

    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        if file_count >= MAX_EXTRACTED_FILES {
            tracing::debug!("exfil: hit MAX_EXTRACTED_FILES, stopping extraction");
            break;
        }
        let entry_size = entry.size();
        if entry_size > MAX_PER_FILE_BYTES {
            tracing::debug!("exfil: skipping oversized entry ({} bytes)", entry_size);
            continue;
        }
        total_bytes += entry_size;
        if total_bytes > MAX_EXTRACTED_BYTES {
            tracing::debug!("exfil: hit MAX_EXTRACTED_BYTES, stopping extraction");
            break;
        }

        // Path-traversal guard: refuse symlinks; refuse paths
        // containing `..`; refuse absolute paths.
        let header_type = entry.header().entry_type();
        if header_type.is_symlink() || header_type.is_hard_link() {
            continue;
        }
        let path = entry.path()?.into_owned();
        if path.is_absolute() {
            continue;
        }
        if path.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        }) {
            continue;
        }

        let dest = extract_root_canonical.join(&path);

        // After joining, double-check the resulting path is still
        // under the extraction root.
        let dest_parent = dest
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| extract_root_canonical.clone());
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        // Re-canonicalize the parent (file might not exist yet).
        if let Ok(parent_canonical) = std::fs::canonicalize(&dest_parent) {
            if !parent_canonical.starts_with(&extract_root_canonical) {
                tracing::debug!(
                    "exfil: refusing path outside extract root: {:?}",
                    dest
                );
                continue;
            }
        }

        // Only write regular files; skip directories (auto-created
        // above) and special types.
        if header_type.is_dir() {
            continue;
        }
        if !header_type.is_file() {
            continue;
        }
        if let Err(e) = entry.unpack(&dest) {
            tracing::debug!("exfil: unpack failed for {:?}: {:#}", path, e);
            continue;
        }
        file_count += 1;
    }

    Ok(())
}

fn scan_extracted(extract_root: &Path, ecosystem: &str) -> Result<PatternCounts> {
    let mut counts = PatternCounts::default();
    walk_files(extract_root, &mut |path| {
        if !is_source_file(path, ecosystem) {
            return;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            return;
        };
        counts.files_scanned += 1;
        scan_content(&content, ecosystem, &mut counts);
    });
    Ok(counts)
}

fn walk_files(dir: &Path, cb: &mut dyn FnMut(&Path)) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_files(&p, cb);
        } else if p.is_file() {
            cb(&p);
        }
    }
}

fn is_source_file(path: &Path, ecosystem: &str) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    use crate::supply_chain_sca::osv::{ECOSYSTEM_CRATES_IO, ECOSYSTEM_NPM, ECOSYSTEM_PYPI};
    match (ecosystem, ext.as_str()) {
        (e, "py" | "pyx") if e == ECOSYSTEM_PYPI => true,
        (e, "rs") if e == ECOSYSTEM_CRATES_IO => true,
        (e, "js" | "ts" | "mjs" | "cjs" | "jsx" | "tsx") if e == ECOSYSTEM_NPM => true,
        // Always scan TOML/JSON config across ecosystems for base64
        // strings — common payload-stash location.
        (_, "toml" | "json") => true,
        _ => false,
    }
}

fn scan_content(content: &str, ecosystem: &str, counts: &mut PatternCounts) {
    counts.long_base64_count += count_long_base64(content);

    use crate::supply_chain_sca::osv::{ECOSYSTEM_CRATES_IO, ECOSYSTEM_NPM, ECOSYSTEM_PYPI};
    match ecosystem {
        e if e == ECOSYSTEM_PYPI => {
            counts.eval_count += count_substr(content, "eval(");
            counts.exec_count += count_substr(content, "exec(");
            counts.subprocess_count += count_substr(content, "subprocess.")
                + count_substr(content, "os.system(")
                + count_substr(content, "os.popen(");
            counts.dynamic_import_count += count_substr(content, "__import__(");
        }
        e if e == ECOSYSTEM_CRATES_IO => {
            counts.command_new_count += count_substr(content, "Command::new")
                + count_substr(content, "std::process::");
        }
        e if e == ECOSYSTEM_NPM => {
            counts.eval_count += count_substr(content, "eval(");
            counts.function_constructor_count += count_substr(content, "Function(");
            counts.child_process_count += count_substr(content, "child_process")
                + count_substr(content, "execSync(")
                + count_substr(content, "spawnSync(")
                + count_substr(content, "exec(");
            counts.dynamic_import_count += count_substr(content, "require(")
                .saturating_sub(count_substr(content, "require('")) // exclude literal requires
                .saturating_sub(count_substr(content, "require(\""));
        }
        _ => {}
    }
    counts.exfil_pattern_count = counts.eval_count
        + counts.exec_count
        + counts.subprocess_count
        + counts.command_new_count
        + counts.child_process_count
        + counts.function_constructor_count
        + counts.dynamic_import_count;
}

/// Count substrings — simple non-overlapping byte search.
fn count_substr(haystack: &str, needle: &str) -> u32 {
    if needle.is_empty() {
        return 0;
    }
    let mut count = 0u32;
    let mut start = 0usize;
    while let Some(pos) = haystack[start..].find(needle) {
        count += 1;
        start += pos + needle.len();
    }
    count
}

/// Count base64-looking strings of length ≥ BASE64_MIN_LENGTH.
///
/// Base64 alphabet: `[A-Za-z0-9+/=]`. We scan for runs of ≥ N
/// consecutive chars from this set. Matches valid base64; also
/// matches some long alphanumeric tokens. Calibrate via E-SC-8.
fn count_long_base64(content: &str) -> u32 {
    let mut count = 0u32;
    let bytes = content.as_bytes();
    let mut run = 0usize;
    for &b in bytes {
        let is_b64 = matches!(b,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' | b'='
        );
        if is_b64 {
            run += 1;
        } else {
            if run >= BASE64_MIN_LENGTH {
                count += 1;
            }
            run = 0;
        }
    }
    if run >= BASE64_MIN_LENGTH {
        count += 1;
    }
    count
}

// ── URL encoding helpers (mirrored from registry.rs) ────────────────
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

fn encode_npm_name(s: &str) -> String {
    if let Some(rest) = s.strip_prefix('@') {
        if let Some((scope, pkg)) = rest.split_once('/') {
            return format!("@{}%2F{}", urlencoded(scope), urlencoded(pkg));
        }
    }
    urlencoded(s)
}

// ── Cache ───────────────────────────────────────────────────────────
fn cache_path(cache_dir: &Path, ecosystem: &str, name: &str, version: &str) -> PathBuf {
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
) -> Option<PatternCounts> {
    let path = cache_path(cache_dir, ecosystem, name, version);
    let raw = std::fs::read_to_string(&path).ok()?;
    let entry: ExfilCacheEntry = serde_json::from_str(&raw).ok()?;
    let age = Utc::now().signed_duration_since(entry.written_at);
    if age.num_seconds() > CACHE_TTL.as_secs() as i64 {
        return None;
    }
    Some(entry.counts)
}

fn write_cache(
    cache_dir: &Path,
    ecosystem: &str,
    name: &str,
    version: &str,
    entry: &ExfilCacheEntry,
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
        assert!(!is_active_from_value(Some("0")));
        assert!(!is_active_from_value(Some("false")));
    }

    #[test]
    fn opt_in_truthy() {
        assert!(is_active_from_value(Some("1")));
        assert!(is_active_from_value(Some("true")));
    }

    #[test]
    fn count_long_base64_finds_long_runs() {
        let content = format!("foo {} bar", "A".repeat(250));
        assert_eq!(count_long_base64(&content), 1);
    }

    #[test]
    fn count_long_base64_below_threshold() {
        let content = format!("foo {} bar", "A".repeat(50));
        assert_eq!(count_long_base64(&content), 0);
    }

    #[test]
    fn count_long_base64_multiple_runs() {
        let s = format!("{}\n\n{}", "A".repeat(250), "B".repeat(250));
        assert_eq!(count_long_base64(&s), 2);
    }

    #[test]
    fn count_substr_basic() {
        assert_eq!(count_substr("eval(x); eval(y)", "eval("), 2);
        assert_eq!(count_substr("noeval", "eval("), 0);
        assert_eq!(count_substr("aaaa", "aa"), 2); // non-overlapping
    }

    #[test]
    fn scan_python_eval_pattern() {
        let mut counts = PatternCounts::default();
        scan_content("eval(payload)\nexec(other)", "PyPI", &mut counts);
        assert_eq!(counts.eval_count, 1);
        assert_eq!(counts.exec_count, 1);
        assert!(counts.exfil_pattern_count >= 2);
    }

    #[test]
    fn scan_rust_command_new() {
        let mut counts = PatternCounts::default();
        scan_content("Command::new(\"curl\").spawn()", "crates.io", &mut counts);
        assert_eq!(counts.command_new_count, 1);
    }

    #[test]
    fn build_finding_below_thresholds_returns_none() {
        let pkg = Package::pypi("safe", "1.0.0");
        let counts = PatternCounts::default();
        assert!(build_finding_from_counts(&pkg, &counts).is_none());
    }

    #[test]
    fn build_finding_above_base64_threshold() {
        let pkg = Package::pypi("suspect", "1.0.0");
        let counts = PatternCounts {
            long_base64_count: 5,
            ..Default::default()
        };
        let f = build_finding_from_counts(&pkg, &counts);
        assert!(f.is_some());
        assert_eq!(f.unwrap().kind, VigilanceKind::ExfilIndicator);
    }
}
