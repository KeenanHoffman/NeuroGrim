//! Local reader for the pinned `rustsec/advisory-db` clone at
//! `vendor/rustsec-advisory-db/`.
//!
//! # Design
//!
//! Each advisory is a Markdown file at
//! `crates/<package>/RUSTSEC-YYYY-NNNN.md` with a ```toml ... ```
//! frontmatter block:
//!
//! ```text
//! ```toml
//! [advisory]
//! id = "RUSTSEC-2026-0104"
//! package = "rustls-webpki"
//! informational = "unmaintained"    # optional; present for non-CVE
//!
//! [versions]
//! patched = [">= 0.103.13, < 0.104.0-alpha.1"]
//! unaffected = []
//! ```
//!
//! # Title
//!
//! Body text…
//! ```
//!
//! A concrete package version is **affected** by the advisory iff it
//! is NOT matched by ANY `patched` range AND NOT matched by ANY
//! `unaffected` range. Advisories with empty `patched` + empty
//! `unaffected` (typical for `informational = "unmaintained"`
//! notices) affect every version.
//!
//! # Graceful degradation
//!
//! If `vendor/rustsec-advisory-db/` is missing (typical for third-
//! party adopter repos that don't initialize the submodule), this
//! function returns `Ok(empty)` rather than `Err`. OSV.dev remains
//! the primary data source; local-RustSec is cross-reference + OSV-
//! miss coverage for NeuroGrim's own dev environment and opt-in
//! adopter setups.
//!
//! # Trust surface
//!
//! Lives on: `toml`, `semver` (RustLang team), filesystem reads.
//! No network. No scanner binaries.

use anyhow::Result;
use semver::{Version, VersionReq};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::{Advisory, AdvisorySource, Package};

/// Scan the local RustSec advisory-db clone for advisories affecting
/// `packages`. Deduplication against OSV results is the caller's
/// responsibility.
pub fn scan_local(packages: &[Package], project_root: &Path) -> Result<Vec<Advisory>> {
    let Some(crates_dir) = locate_advisory_db(project_root) else {
        tracing::debug!(
            "RustSec advisory-db not present near {}; skipping local scan",
            project_root.display()
        );
        return Ok(Vec::new());
    };

    let mut hits: Vec<Advisory> = Vec::new();
    for pkg in packages {
        let crate_dir = crates_dir.join(&pkg.name);
        if !crate_dir.is_dir() {
            continue;
        }
        scan_crate_dir(&crate_dir, pkg, &mut hits)?;
    }
    Ok(hits)
}

/// Search for `vendor/rustsec-advisory-db/crates/` near
/// `project_root`. Returns the `crates/` dir if found.
///
/// Searches in order:
/// 1. `<project_root>/vendor/rustsec-advisory-db/crates/` (standard;
///    most user repos).
/// 2. `<project_root>/../vendor/rustsec-advisory-db/crates/` (handles
///    NeuroGrim's unusual layout where the cargo workspace lives in a
///    `neurogrim/` subdirectory while the submodule is pinned at the
///    repo root).
fn locate_advisory_db(project_root: &Path) -> Option<PathBuf> {
    let candidates = [
        project_root.join("vendor").join("rustsec-advisory-db").join("crates"),
        project_root
            .parent()
            .map(|p| p.join("vendor").join("rustsec-advisory-db").join("crates"))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.is_dir())
}

/// Walk a single `crates/<name>/` directory and append matching
/// advisories to `hits`.
fn scan_crate_dir(crate_dir: &Path, pkg: &Package, hits: &mut Vec<Advisory>) -> Result<()> {
    let parsed_version = match Version::parse(&pkg.version) {
        Ok(v) => v,
        Err(e) => {
            // Non-semver version (e.g., path-dep with "1" or a custom
            // version string). We can't evaluate ranges, so skip this
            // package entirely — OSV still sees it.
            tracing::warn!(
                "RustSec scan skipping {}@{}: version is not valid semver ({:#})",
                pkg.name,
                pkg.version,
                e
            );
            return Ok(());
        }
    };

    let entries = match std::fs::read_dir(crate_dir) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("RustSec scan: read_dir({}) failed: {:#}", crate_dir.display(), e);
            return Ok(());
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        match load_and_evaluate(&path, pkg, &parsed_version) {
            Ok(Some(adv)) => hits.push(adv),
            Ok(None) => {}
            Err(e) => tracing::warn!(
                "RustSec scan: failed to evaluate {}: {:#}",
                path.display(),
                e
            ),
        }
    }
    Ok(())
}

/// Load a single advisory file and decide whether `pkg_version` is
/// affected. Returns `Some(advisory)` on a match, `None` otherwise.
///
/// On parse error this returns `Err` so the outer loop can log it;
/// one bad advisory file does not halt the scan.
///
/// Advisories with a non-empty `withdrawn` field are treated as
/// inactive — the RustSec advisory-db uses that field to mark
/// retractions (author returned, original issue was mistaken, etc.).
/// OSV filters withdrawn advisories from `querybatch` responses; we
/// match that behavior here so the two sources agree. Known
/// real-world example: `RUSTSEC-2025-0007` on `ring` was withdrawn
/// 2025-02-22 when the author resumed maintenance.
fn load_and_evaluate(
    path: &Path,
    pkg: &Package,
    pkg_version: &Version,
) -> Result<Option<Advisory>> {
    let raw = std::fs::read_to_string(path)?;
    let Some(frontmatter) = extract_toml_frontmatter(&raw) else {
        // File exists in crates/<name>/ but has no TOML frontmatter —
        // unusual; skip rather than fail. (Advisory files without a
        // frontmatter block aren't part of the advisory-db schema.)
        return Ok(None);
    };

    let parsed: AdvisoryFile = toml::from_str(frontmatter)?;

    // Withdrawn advisories are treated as inactive — mirror OSV's
    // behavior (it filters withdrawn from querybatch responses).
    if parsed
        .advisory
        .withdrawn
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        return Ok(None);
    }

    // Defensive: the file's `package` field should match the directory
    // it lives in. If it doesn't, the advisory-db is inconsistent; we
    // trust the field (it's the ID of record) and log.
    if parsed.advisory.package != pkg.name {
        tracing::warn!(
            "RustSec advisory {} at {} claims package={} but directory is for {}",
            parsed.advisory.id,
            path.display(),
            parsed.advisory.package,
            pkg.name
        );
        // Still evaluate against our package — the caller asked about
        // THIS package name, and we found a matching directory.
    }

    if !is_affected(pkg_version, &parsed.versions) {
        return Ok(None);
    }

    Ok(Some(Advisory {
        id: parsed.advisory.id,
        package: pkg.clone(),
        summary: None, // Body text is after the frontmatter; not
                       // extracted in v1 for terseness. Step 8 scoring
                       // doesn't need it.
        source: AdvisorySource::RustsecLocal,
        informational: parsed.advisory.informational,
    }))
}

/// Determine whether `pkg_version` is affected by the advisory.
///
/// Affected iff NOT matched by any `patched` range AND NOT matched by
/// any `unaffected` range. An advisory with empty `patched` and empty
/// `unaffected` affects every version (typical for informational).
fn is_affected(pkg_version: &Version, versions: &VersionsSection) -> bool {
    if matches_any_range(pkg_version, &versions.patched) {
        return false;
    }
    if matches_any_range(pkg_version, &versions.unaffected) {
        return false;
    }
    true
}

fn matches_any_range(version: &Version, reqs: &[String]) -> bool {
    for raw in reqs {
        // RustSec uses comma-separated semver requirements like
        // ">= 0.103.13, < 0.104.0-alpha.1". `VersionReq::parse`
        // accepts that syntax directly.
        match VersionReq::parse(raw) {
            Ok(req) => {
                if req.matches(version) {
                    return true;
                }
            }
            Err(e) => {
                tracing::warn!(
                    "RustSec scan: failed to parse version range '{raw}': {:#}",
                    e
                );
            }
        }
    }
    false
}

/// Extract the first ```` ```toml ... ``` ```` fenced block from a
/// Markdown document. Returns the TOML body as a `&str` slice of the
/// input.
///
/// Both the opening fence (```` ```toml ````) and closing fence
/// (```` ``` ````) must be on their own line (optionally with leading
/// or trailing whitespace). This matches every advisory in the
/// RustSec advisory-db at the 2026-04-24 pin.
fn extract_toml_frontmatter(markdown: &str) -> Option<&str> {
    let mut lines = markdown.char_indices().peekable();
    let mut line_start = 0usize;
    let mut in_block = false;
    let mut block_start: Option<usize> = None;

    // Walk line by line, tracking byte offsets so we can slice the
    // original input for the TOML body.
    while let Some(&(idx, ch)) = lines.peek() {
        if ch == '\n' {
            let line = markdown[line_start..idx].trim_matches('\r').trim();
            if !in_block {
                if line == "```toml" {
                    in_block = true;
                    // TOML body starts at the NEXT line.
                    block_start = Some(idx + ch.len_utf8());
                }
            } else if line == "```" {
                // End of block. Body is [block_start, line_start).
                let start = block_start?;
                let end = line_start;
                // Trim trailing whitespace without losing the interior.
                return Some(markdown[start..end].trim_end_matches(&['\r', '\n'][..]));
            }
            line_start = idx + ch.len_utf8();
        }
        lines.next();
    }

    // File may end without a trailing newline on the last line.
    if in_block {
        let tail = markdown[line_start..].trim();
        if tail != "```" {
            // Unterminated fence — treat as no frontmatter.
            return None;
        }
        let start = block_start?;
        return Some(markdown[start..line_start].trim_end_matches(&['\r', '\n'][..]));
    }

    // Edge case: the entire file is a single-line "```toml" with no
    // newline before EOF. We don't have a body, so None.
    None
}

// =========================================================================
// Advisory TOML schema
// =========================================================================

#[derive(Debug, Deserialize)]
struct AdvisoryFile {
    advisory: AdvisorySection,
    #[serde(default)]
    versions: VersionsSection,
}

#[derive(Debug, Deserialize)]
struct AdvisorySection {
    id: String,
    package: String,
    #[serde(default)]
    informational: Option<String>,
    /// Present (with a date string) if the advisory has been
    /// retracted. Advisories with a non-empty `withdrawn` are skipped
    /// by both this local scan and OSV's querybatch.
    #[serde(default)]
    withdrawn: Option<String>,
    // Other keys (date, url, aliases, categories, keywords, cvss) are
    // intentionally not parsed. We only need id / package /
    // informational / withdrawn for the sensor's scoring + finding
    // output.
}

#[derive(Debug, Default, Deserialize)]
struct VersionsSection {
    #[serde(default)]
    patched: Vec<String>,
    #[serde(default)]
    unaffected: Vec<String>,
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn pkg(name: &str, version: &str) -> Package {
        Package {
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    fn write_advisory(dir: &Path, filename: &str, contents: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(filename);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents.trim_start().as_bytes()).unwrap();
        path
    }

    // --- extract_toml_frontmatter ---

    #[test]
    fn extracts_toml_frontmatter_from_typical_advisory() {
        let raw = r#"```toml
[advisory]
id = "RUSTSEC-2024-0436"
package = "paste"

[versions]
patched = []
```

# paste - no longer maintained

Body text continues here.
"#;
        let toml_body = extract_toml_frontmatter(raw).expect("has frontmatter");
        assert!(toml_body.contains(r#"id = "RUSTSEC-2024-0436""#));
        assert!(toml_body.contains(r#"package = "paste""#));
        assert!(!toml_body.contains("# paste"));
    }

    #[test]
    fn returns_none_when_no_toml_block() {
        let raw = "# Just a regular markdown file\n\nNo fences here.\n";
        assert!(extract_toml_frontmatter(raw).is_none());
    }

    #[test]
    fn returns_none_when_fence_is_bash_not_toml() {
        let raw = "```bash\nsome script\n```\n";
        assert!(extract_toml_frontmatter(raw).is_none());
    }

    #[test]
    fn returns_none_on_unterminated_fence() {
        let raw = "```toml\nid = \"RUSTSEC-...\"\n\n# Title without closing fence\nmore body\n";
        assert!(extract_toml_frontmatter(raw).is_none());
    }

    // --- is_affected ---

    #[test]
    fn empty_patched_and_unaffected_means_affected() {
        let v = Version::parse("1.0.15").unwrap();
        let sections = VersionsSection::default();
        assert!(is_affected(&v, &sections));
    }

    #[test]
    fn version_below_patched_is_affected() {
        let v = Version::parse("0.103.12").unwrap();
        let sections = VersionsSection {
            patched: vec![">= 0.103.13, < 0.104.0-alpha.1".to_string()],
            unaffected: vec![],
        };
        assert!(is_affected(&v, &sections));
    }

    #[test]
    fn version_in_patched_range_is_not_affected() {
        let v = Version::parse("0.103.13").unwrap();
        let sections = VersionsSection {
            patched: vec![">= 0.103.13, < 0.104.0-alpha.1".to_string()],
            unaffected: vec![],
        };
        assert!(!is_affected(&v, &sections));
    }

    #[test]
    fn version_in_unaffected_range_is_not_affected() {
        let v = Version::parse("0.102.0").unwrap();
        let sections = VersionsSection {
            patched: vec![">= 0.103.13".to_string()],
            unaffected: vec!["< 0.103.0".to_string()],
        };
        assert!(!is_affected(&v, &sections));
    }

    #[test]
    fn multiple_patched_ranges_any_match_is_safe() {
        // real shape from RUSTSEC-2026-0104
        let v = Version::parse("0.104.0-alpha.7").unwrap();
        let sections = VersionsSection {
            patched: vec![
                ">= 0.103.13, < 0.104.0-alpha.1".to_string(),
                ">= 0.104.0-alpha.7".to_string(),
            ],
            unaffected: vec![],
        };
        assert!(!is_affected(&v, &sections));
    }

    #[test]
    fn unparseable_range_does_not_crash() {
        let v = Version::parse("1.0.0").unwrap();
        let sections = VersionsSection {
            patched: vec!["this is garbage".to_string()],
            unaffected: vec![],
        };
        // Garbage range means no match → affected.
        assert!(is_affected(&v, &sections));
    }

    // --- scan_local end-to-end (with fixture dirs) ---

    #[test]
    fn scan_returns_empty_when_advisory_db_missing() {
        let tmp = TempDir::new().unwrap();
        let hits = scan_local(&[pkg("serde", "1.0.0")], tmp.path()).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn scan_flags_unmaintained_paste() {
        let tmp = TempDir::new().unwrap();
        let crates_dir = tmp
            .path()
            .join("vendor")
            .join("rustsec-advisory-db")
            .join("crates");
        write_advisory(
            &crates_dir.join("paste"),
            "RUSTSEC-2024-0436.md",
            r#"```toml
[advisory]
id = "RUSTSEC-2024-0436"
package = "paste"
date = "2024-10-07"
informational = "unmaintained"

[versions]
patched = []
```

# paste - no longer maintained

Body.
"#,
        );

        let hits = scan_local(&[pkg("paste", "1.0.15")], tmp.path()).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "RUSTSEC-2024-0436");
        assert_eq!(hits[0].package.name, "paste");
        assert_eq!(hits[0].informational.as_deref(), Some("unmaintained"));
        assert_eq!(hits[0].source, AdvisorySource::RustsecLocal);
    }

    #[test]
    fn scan_flags_real_rustls_webpki() {
        // RUSTSEC-2026-0104 shape: rustls-webpki 0.103.12 should
        // be flagged; 0.103.13 should not.
        let tmp = TempDir::new().unwrap();
        let crates_dir = tmp
            .path()
            .join("vendor")
            .join("rustsec-advisory-db")
            .join("crates");
        write_advisory(
            &crates_dir.join("rustls-webpki"),
            "RUSTSEC-2026-0104.md",
            r#"```toml
[advisory]
id = "RUSTSEC-2026-0104"
package = "rustls-webpki"
date = "2026-04-22"
categories = ["denial-of-service"]

[versions]
patched = [">= 0.103.13, < 0.104.0-alpha.1", ">= 0.104.0-alpha.7"]
```

# Reachable panic…
"#,
        );

        let hits_vuln = scan_local(&[pkg("rustls-webpki", "0.103.12")], tmp.path()).unwrap();
        assert_eq!(hits_vuln.len(), 1);
        assert_eq!(hits_vuln[0].id, "RUSTSEC-2026-0104");
        assert!(hits_vuln[0].informational.is_none());

        let hits_patched = scan_local(&[pkg("rustls-webpki", "0.103.13")], tmp.path()).unwrap();
        assert!(hits_patched.is_empty());
    }

    #[test]
    fn scan_skips_unknown_crate() {
        let tmp = TempDir::new().unwrap();
        let crates_dir = tmp
            .path()
            .join("vendor")
            .join("rustsec-advisory-db")
            .join("crates");
        // Only `paste` has an advisory.
        write_advisory(
            &crates_dir.join("paste"),
            "RUSTSEC-2024-0436.md",
            r#"```toml
[advisory]
id = "RUSTSEC-2024-0436"
package = "paste"
informational = "unmaintained"

[versions]
patched = []
```
"#,
        );

        // Query for a crate that has no advisory directory. Should
        // return empty without error.
        let hits = scan_local(&[pkg("serde", "1.0.0")], tmp.path()).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn scan_handles_malformed_advisory_gracefully() {
        let tmp = TempDir::new().unwrap();
        let crates_dir = tmp
            .path()
            .join("vendor")
            .join("rustsec-advisory-db")
            .join("crates");
        // Valid advisory alongside a malformed one in the same dir.
        write_advisory(
            &crates_dir.join("paste"),
            "RUSTSEC-2024-0436.md",
            r#"```toml
[advisory]
id = "RUSTSEC-2024-0436"
package = "paste"
informational = "unmaintained"

[versions]
patched = []
```
"#,
        );
        write_advisory(
            &crates_dir.join("paste"),
            "RUSTSEC-2099-BROKEN.md",
            "```toml\nthis is not valid toml = = = \n```\n",
        );

        let hits = scan_local(&[pkg("paste", "1.0.15")], tmp.path()).unwrap();
        // The valid advisory still surfaces; the broken one is logged
        // + skipped (doesn't halt the scan).
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "RUSTSEC-2024-0436");
    }

    #[test]
    fn scan_skips_withdrawn_advisory() {
        // Real-world shape: RUSTSEC-2025-0007 on `ring` was withdrawn
        // 2025-02-22. OSV filters these; we must too.
        let tmp = TempDir::new().unwrap();
        let crates_dir = tmp
            .path()
            .join("vendor")
            .join("rustsec-advisory-db")
            .join("crates");
        write_advisory(
            &crates_dir.join("ring"),
            "RUSTSEC-2025-0007.md",
            r#"```toml
[advisory]
id = "RUSTSEC-2025-0007"
package = "ring"
date = "2025-02-20"
informational = "unmaintained"
withdrawn = "2025-02-22"

[versions]
patched = []
unaffected = []
```

# ring is unmaintained
"#,
        );

        let hits = scan_local(&[pkg("ring", "0.17.14")], tmp.path()).unwrap();
        assert!(
            hits.is_empty(),
            "withdrawn advisories should be skipped; got {hits:?}"
        );
    }

    #[test]
    fn scan_keeps_non_withdrawn_advisory() {
        // Sanity: the withdrawn-field check must not accidentally
        // filter advisories that lack the field entirely.
        let tmp = TempDir::new().unwrap();
        let crates_dir = tmp
            .path()
            .join("vendor")
            .join("rustsec-advisory-db")
            .join("crates");
        write_advisory(
            &crates_dir.join("paste"),
            "RUSTSEC-2024-0436.md",
            r#"```toml
[advisory]
id = "RUSTSEC-2024-0436"
package = "paste"
informational = "unmaintained"

[versions]
patched = []
```
"#,
        );
        let hits = scan_local(&[pkg("paste", "1.0.15")], tmp.path()).unwrap();
        assert_eq!(hits.len(), 1, "non-withdrawn must be surfaced");
    }

    #[test]
    fn scan_handles_non_semver_package_version() {
        // Edge case: a Package whose version doesn't parse as semver
        // (vanishingly rare in crates.io-sourced deps, but defensive).
        let tmp = TempDir::new().unwrap();
        let crates_dir = tmp
            .path()
            .join("vendor")
            .join("rustsec-advisory-db")
            .join("crates");
        write_advisory(
            &crates_dir.join("weird"),
            "RUSTSEC-2099-0001.md",
            r#"```toml
[advisory]
id = "RUSTSEC-2099-0001"
package = "weird"
informational = "notice"

[versions]
patched = []
```
"#,
        );

        let pkg = Package {
            name: "weird".to_string(),
            version: "not-semver".to_string(),
        };
        // Returns empty (logged as warning); doesn't panic.
        let hits = scan_local(&[pkg], tmp.path()).unwrap();
        assert!(hits.is_empty());
    }
}
