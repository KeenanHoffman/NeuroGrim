//! Python lockfile parsers for the supply-chain-sca sensor.
//!
//! Two formats supported (per E-SC-3 locked decisions; 2026-04-24):
//!
//! 1. **`uv.lock`** — Astral's resolved lockfile (TOML).
//!    Each `[[package]]` entry carries `name`, `version`, and a
//!    `source` table. Packages with `source.registry == "https://pypi.org/simple"`
//!    are included; anything else (git, path, file, alternative
//!    registry) is excluded.
//!
//! 2. **`requirements*.txt`** — line-based PEP-440 pins. ONLY exact
//!    pins (`name==X.Y.Z`) are honored — version specifiers without
//!    pins (`>=`, `~=`, etc.) are skipped because they are not
//!    resolved versions. Editable installs (`-e .`), recursive
//!    includes (`-r other.txt`), pip flags (`--`), and direct-URL
//!    deps (`@ file://`, `git+`) are skipped per the same posture
//!    the cargo parser uses for non-registry deps.
//!
//! Trust surface: `toml` (already in workspace deps) +
//! stdlib only. ZERO new external crates per E-SC-3 locked
//! decisions. PEP-508/PEP-440 specifier parsing is not required
//! for the resolved-lockfile-only posture.
//!
//! Out of scope (deferred to BACKLOG B-22): `poetry.lock`,
//! `Pipfile.lock`, `pyproject.toml` direct (unresolved).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

use crate::supply_chain_sca::Package;

/// Parse `<project_root>/uv.lock` and return deduplicated PyPI-
/// sourced packages. Excludes git, file, path, and alternative-
/// registry entries.
pub fn parse_uv_lock(project_root: &Path) -> Result<Vec<Package>> {
    let path = project_root.join("uv.lock");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    parse_uv_lock_str(&raw)
}

fn parse_uv_lock_str(raw: &str) -> Result<Vec<Package>> {
    let parsed: UvLockfile =
        toml::from_str(raw).with_context(|| "uv.lock TOML parse")?;

    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for pkg in parsed.package {
        if !is_pypi_source(&pkg.source) {
            continue;
        }
        seen.insert((pkg.name, pkg.version));
    }

    Ok(seen
        .into_iter()
        .map(|(name, version)| Package::pypi(name, version))
        .collect())
}

/// Parse a `requirements*.txt` file at `path` (full path, not
/// project_root) and return deduplicated PyPI-pinned packages.
///
/// Caller specifies the file path because requirements files don't
/// have a single canonical name (`requirements.txt`,
/// `requirements-lock.txt`, `requirements-prod.txt`, etc.).
pub fn parse_requirements_txt(path: &Path) -> Result<Vec<Package>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    Ok(parse_requirements_txt_str(&raw))
}

fn parse_requirements_txt_str(raw: &str) -> Vec<Package> {
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for line in raw.lines() {
        if let Some((name, version)) = parse_requirements_line(line) {
            seen.insert((name, version));
        }
    }
    seen.into_iter()
        .map(|(name, version)| Package::pypi(name, version))
        .collect()
}

/// Extract a `(name, version)` from a single requirements.txt line.
/// Returns `None` for comments, blanks, flags, editable installs,
/// non-pinned specifiers, and direct-URL deps.
fn parse_requirements_line(raw: &str) -> Option<(String, String)> {
    let line = raw.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    // pip flags, recursive includes, editable installs, constraint
    // includes — all skipped.
    if line.starts_with('-') {
        return None;
    }
    // Direct-URL deps: `name @ file://...`, `name @ git+...`, etc.
    if line.contains(" @ ") {
        return None;
    }
    // VCS-direct on the URL form (rare standalone, but seen in older
    // formats): `git+https://...`, `hg+https://...`, etc.
    if line.starts_with("git+")
        || line.starts_with("hg+")
        || line.starts_with("svn+")
        || line.starts_with("bzr+")
    {
        return None;
    }
    // Strip an inline comment after the spec.
    let line = match line.split_once('#') {
        Some((before, _)) => before.trim_end(),
        None => line,
    };
    // Strip pip-marker tail: `pkg==1.0 ; python_version<'3.11'`.
    let line = match line.split_once(';') {
        Some((before, _)) => before.trim_end(),
        None => line,
    };
    // The format we honor: NAME[extras]==VERSION
    // Split on the first `==`. If absent, this is a non-pinned
    // specifier (`>=`, `~=`, `<`, `=`, etc.) and we skip — only
    // resolved exact pins map cleanly to OSV queries.
    let (lhs, rhs) = line.split_once("==")?;
    if rhs.contains("==") {
        // Multiple `==` — unusual; skip for safety.
        return None;
    }
    // Strip the `[extra1,extra2]` suffix from the name.
    let name = match lhs.split_once('[') {
        Some((n, _)) => n.trim(),
        None => lhs.trim(),
    };
    if name.is_empty() {
        return None;
    }
    let version = rhs.trim();
    if version.is_empty() {
        return None;
    }
    // Reject if name has invalid characters (defensive; PyPI names
    // are A-Z 0-9 _ . - only).
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return None;
    }
    Some((name.to_string(), version.to_string()))
}

fn is_pypi_source(source: &Option<UvSource>) -> bool {
    let Some(s) = source else {
        return false;
    };
    let Some(reg) = s.registry.as_deref() else {
        return false;
    };
    // Standard PyPI URLs. Both forms appear in uv.lock files in
    // the wild depending on uv version.
    reg == "https://pypi.org/simple"
        || reg == "https://pypi.org/simple/"
        || reg == "https://pypi.org"
}

// =========================================================================
// uv.lock TOML schema
// =========================================================================

#[derive(Debug, Deserialize)]
struct UvLockfile {
    #[serde(default)]
    package: Vec<UvPackage>,
}

#[derive(Debug, Deserialize)]
struct UvPackage {
    name: String,
    version: String,
    #[serde(default)]
    source: Option<UvSource>,
}

#[derive(Debug, Deserialize)]
struct UvSource {
    #[serde(default)]
    registry: Option<String>,
    // Other variants (git, path, url) just have nothing here for
    // our purposes — `is_pypi_source` returns false because
    // `registry` is None or non-PyPI.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    // ---------- uv.lock tests ----------

    #[test]
    fn uv_parses_simple_pypi_package() {
        let raw = r#"
            version = 1

            [[package]]
            name = "jinja2"
            version = "3.1.4"
            source = { registry = "https://pypi.org/simple" }
        "#;
        let pkgs = parse_uv_lock_str(raw).unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "jinja2");
        assert_eq!(pkgs[0].version, "3.1.4");
        assert_eq!(pkgs[0].ecosystem, "PyPI");
    }

    #[test]
    fn uv_excludes_git_source() {
        let raw = r#"
            version = 1

            [[package]]
            name = "from-git"
            version = "0.1.0"
            source = { git = "https://github.com/example/repo" }
        "#;
        let pkgs = parse_uv_lock_str(raw).unwrap();
        assert!(pkgs.is_empty());
    }

    #[test]
    fn uv_excludes_path_source() {
        let raw = r#"
            version = 1

            [[package]]
            name = "local"
            version = "0.1.0"
            source = { path = "../sibling" }
        "#;
        let pkgs = parse_uv_lock_str(raw).unwrap();
        assert!(pkgs.is_empty());
    }

    #[test]
    fn uv_excludes_alt_registry() {
        let raw = r#"
            version = 1

            [[package]]
            name = "internal"
            version = "1.0.0"
            source = { registry = "https://internal.example.com/simple" }
        "#;
        let pkgs = parse_uv_lock_str(raw).unwrap();
        assert!(pkgs.is_empty());
    }

    #[test]
    fn uv_mixed_keeps_only_pypi() {
        let raw = r#"
            version = 1

            [[package]]
            name = "jinja2"
            version = "3.1.4"
            source = { registry = "https://pypi.org/simple" }

            [[package]]
            name = "from-git"
            version = "0.1.0"
            source = { git = "https://github.com/example/repo" }

            [[package]]
            name = "anyio"
            version = "4.6.0"
            source = { registry = "https://pypi.org/simple" }

            [[package]]
            name = "local-only"
            version = "0.1.0"
        "#;
        let pkgs = parse_uv_lock_str(raw).unwrap();
        let names: Vec<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(pkgs.len(), 2);
        assert!(names.contains(&"jinja2"));
        assert!(names.contains(&"anyio"));
    }

    #[test]
    fn uv_dedups_repeated_entries() {
        let raw = r#"
            version = 1

            [[package]]
            name = "jinja2"
            version = "3.1.4"
            source = { registry = "https://pypi.org/simple" }

            [[package]]
            name = "jinja2"
            version = "3.1.4"
            source = { registry = "https://pypi.org/simple" }
        "#;
        let pkgs = parse_uv_lock_str(raw).unwrap();
        assert_eq!(pkgs.len(), 1);
    }

    #[test]
    fn uv_load_from_file_round_trip() {
        let tmp = TempDir::new().unwrap();
        let mut f = std::fs::File::create(tmp.path().join("uv.lock")).unwrap();
        f.write_all(
            br#"version = 1

[[package]]
name = "pyyaml"
version = "6.0.1"
source = { registry = "https://pypi.org/simple" }
"#,
        )
        .unwrap();
        let pkgs = parse_uv_lock(tmp.path()).unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "pyyaml");
    }

    // ---------- requirements.txt tests ----------

    #[test]
    fn requirements_parses_simple_pin() {
        let raw = "jinja2==3.1.4\n";
        let pkgs = parse_requirements_txt_str(raw);
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "jinja2");
        assert_eq!(pkgs[0].version, "3.1.4");
    }

    #[test]
    fn requirements_skips_unpinned_specifiers() {
        let raw = "jinja2>=3.0\nflask~=2.0\nrequests<3\nclick=1\n";
        let pkgs = parse_requirements_txt_str(raw);
        assert!(pkgs.is_empty(), "unpinned specifiers must not parse: {pkgs:?}");
    }

    #[test]
    fn requirements_skips_comments_and_blanks() {
        let raw = "# header\n\n   \njinja2==3.1.4\n# trailing comment\n";
        let pkgs = parse_requirements_txt_str(raw);
        assert_eq!(pkgs.len(), 1);
    }

    #[test]
    fn requirements_skips_pip_flags() {
        let raw = "--index-url https://pypi.org\n-e .\n-r other.txt\n--no-deps\n";
        let pkgs = parse_requirements_txt_str(raw);
        assert!(pkgs.is_empty());
    }

    #[test]
    fn requirements_skips_direct_url() {
        let raw = "package @ file:///some/path\nother @ git+https://github.com/x/y\n";
        let pkgs = parse_requirements_txt_str(raw);
        assert!(pkgs.is_empty());
    }

    #[test]
    fn requirements_skips_vcs_url_form() {
        let raw = "git+https://github.com/x/y\nhg+https://other\n";
        let pkgs = parse_requirements_txt_str(raw);
        assert!(pkgs.is_empty());
    }

    #[test]
    fn requirements_strips_extras_from_name() {
        let raw = "fastapi[standard]==0.115.0\nuvicorn[standard]==0.30.0\n";
        let pkgs = parse_requirements_txt_str(raw);
        let names: Vec<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"fastapi"));
        assert!(names.contains(&"uvicorn"));
    }

    #[test]
    fn requirements_strips_environment_markers() {
        let raw = "tomli==2.0.1 ; python_version<'3.11'\n";
        let pkgs = parse_requirements_txt_str(raw);
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "tomli");
        assert_eq!(pkgs[0].version, "2.0.1");
    }

    #[test]
    fn requirements_strips_inline_comments() {
        let raw = "jinja2==3.1.4  # transitive via flask\n";
        let pkgs = parse_requirements_txt_str(raw);
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].version, "3.1.4");
    }

    #[test]
    fn requirements_dedups_repeated_entries() {
        let raw = "jinja2==3.1.4\njinja2==3.1.4\n";
        let pkgs = parse_requirements_txt_str(raw);
        assert_eq!(pkgs.len(), 1);
    }

    #[test]
    fn requirements_load_from_file_round_trip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("requirements-lock.txt");
        std::fs::write(&path, "anyio==4.6.0\nhttpx==0.27.0\n").unwrap();
        let pkgs = parse_requirements_txt(&path).unwrap();
        assert_eq!(pkgs.len(), 2);
    }

    #[test]
    fn requirements_realistic_freeze_output() {
        // A snippet shaped like real `pip freeze --exclude-editable`
        // output mixed with conventions seen in pip-tools-generated
        // `requirements-lock.txt` files.
        let raw = r#"#
# This file is autogenerated by pip-compile.
#
anyio==4.6.0
    # via httpx
certifi==2024.8.30
    # via httpx
httpcore==1.0.5
httpx==0.27.0
idna==3.10
jinja2==3.1.4
    # via flask
"#;
        let pkgs = parse_requirements_txt_str(raw);
        let names: Vec<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"anyio"));
        assert!(names.contains(&"certifi"));
        assert!(names.contains(&"httpcore"));
        assert!(names.contains(&"httpx"));
        assert!(names.contains(&"idna"));
        assert!(names.contains(&"jinja2"));
        assert_eq!(pkgs.len(), 6);
    }
}
