//! `package-lock.json` parser for the supply-chain-sca sensor.
//!
//! Supports lockfileVersion 2 + 3 (npm 7+). Version 1 (npm 5/6 era)
//! is rare in 2026 — npm auto-upgrades v1 to v2/v3 on install — and
//! is deferred to a follow-on if real-world demand surfaces.
//!
//! # Schema (v2/v3)
//!
//! ```json
//! {
//!   "name": "my-app",
//!   "version": "1.0.0",
//!   "lockfileVersion": 3,
//!   "packages": {
//!     "": { "name": "my-app", "version": "1.0.0", ... },
//!     "node_modules/lodash": {
//!       "version": "4.17.21",
//!       "resolved": "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz",
//!       "integrity": "sha512-..."
//!     },
//!     "node_modules/@scope/pkg": { "version": "1.0.0", ... }
//!   }
//! }
//! ```
//!
//! # Filtering rules
//!
//! - **Root entry** (key `""`): skip — that's the project itself.
//! - **Workspace deps** (`link: true`): skip — symlinked, not on
//!   npm registry; OSV has no coverage.
//! - **Git/file/tarball-url deps**: skip if `resolved` doesn't
//!   start with `https://registry.npmjs.org/`. Also catches
//!   non-canonical mirrors / private registries by exclusion;
//!   adopters with private registries opt-in via cross-check.
//! - **Aliased deps** (`"foo": "npm:bar@1.0.0"`): npm resolves
//!   these in the lockfile; the `name` field on the entry holds
//!   the actual package name (e.g., `bar`), which is what we
//!   query OSV for. Pull from `name` when present, else extract
//!   from the path.
//!
//! # Trust surface
//!
//! `serde_json` (already a workspace dep). Hand-rolled struct.
//! ZERO new external crates.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

use crate::supply_chain_sca::Package;

/// Parse `<project_root>/package-lock.json` and return deduplicated
/// npm-registry-sourced packages.
pub fn parse_package_lock(project_root: &Path) -> Result<Vec<Package>> {
    let path = project_root.join("package-lock.json");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    parse_package_lock_str(&raw)
}

fn parse_package_lock_str(raw: &str) -> Result<Vec<Package>> {
    let parsed: NpmLockfile = serde_json::from_str(raw)
        .with_context(|| "package-lock.json JSON parse")?;

    // We support v2 and v3. v1 lockfiles use a different
    // hierarchical `dependencies` structure; rare in 2026 (npm
    // auto-upgrades on install). v1 → return empty + log; the
    // operator's npm install will produce v2/v3 next time.
    if let Some(v) = parsed.lockfile_version {
        if v < 2 {
            tracing::warn!(
                "package-lock.json lockfileVersion={v} (v1 era); not supported. \
                 Run `npm install` to upgrade to v2/v3."
            );
            return Ok(Vec::new());
        }
    }

    let packages = match parsed.packages {
        Some(p) => p,
        None => return Ok(Vec::new()),
    };

    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for (path_key, entry) in packages {
        if path_key.is_empty() {
            // Root project entry; skip.
            continue;
        }
        if entry.link.unwrap_or(false) {
            // Symlinked workspace dep; not on npm registry.
            continue;
        }
        if !is_npm_registry_source(entry.resolved.as_deref()) {
            // Git, file:, custom registry — outside OSV coverage.
            continue;
        }
        let Some(version) = entry.version.as_deref() else {
            continue;
        };
        // Aliased deps: lockfile carries `name` field with the
        // resolved (npm-canonical) name. Fall back to path-based
        // name extraction when `name` is absent.
        let name = entry
            .name
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| name_from_path_key(&path_key));
        if name.is_empty() {
            continue;
        }
        seen.insert((name, version.to_string()));
    }

    Ok(seen
        .into_iter()
        .map(|(name, version)| Package::npm(name, version))
        .collect())
}

/// Extract the package name from a `node_modules/...` path key.
///
/// Handles:
/// - `node_modules/lodash` → `lodash`
/// - `node_modules/@scope/pkg` → `@scope/pkg`
/// - `node_modules/foo/node_modules/bar` → `bar` (nested)
fn name_from_path_key(path_key: &str) -> String {
    const SEP: &str = "node_modules/";
    // Find the LAST occurrence of `node_modules/` and take everything
    // after it. Handles nested-resolution paths.
    if let Some(idx) = path_key.rfind(SEP) {
        return path_key[idx + SEP.len()..].to_string();
    }
    // Fallback: no node_modules/ prefix; use the path verbatim.
    path_key.to_string()
}

fn is_npm_registry_source(resolved: Option<&str>) -> bool {
    let Some(url) = resolved else {
        return false;
    };
    // Canonical registry URL. Some lockfiles also reference a sparse
    // mirror or alternative registries; treat only the canonical one
    // as in-scope. Adopters with private registries can use the
    // optional cross-check sensor (deferred follow-on).
    url.starts_with("https://registry.npmjs.org/")
}

// =========================================================================
// package-lock.json schema (v2/v3 only)
// =========================================================================

#[derive(Debug, Deserialize)]
struct NpmLockfile {
    #[serde(rename = "lockfileVersion", default)]
    lockfile_version: Option<u8>,
    #[serde(default)]
    packages: Option<std::collections::BTreeMap<String, NpmPackageEntry>>,
}

#[derive(Debug, Deserialize)]
struct NpmPackageEntry {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    resolved: Option<String>,
    /// Present for aliased deps (`"foo": "npm:bar@1.0.0"`); names
    /// the resolved (canonical) package. We prefer this over
    /// path-based name extraction when it's set.
    #[serde(default)]
    name: Option<String>,
    /// Workspace symlinks set `link: true`. Skip.
    #[serde(default)]
    link: Option<bool>,
    // dev / optional / peer flags exist but we scan all (per the
    // E-SC-2 dev-deps-included-as-attack-surface stance).
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn parses_simple_v3_lockfile() {
        let raw = r#"{
            "name": "my-app",
            "version": "1.0.0",
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "my-app", "version": "1.0.0"},
                "node_modules/lodash": {
                    "version": "4.17.21",
                    "resolved": "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz",
                    "integrity": "sha512-aaa"
                }
            }
        }"#;
        let pkgs = parse_package_lock_str(raw).unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "lodash");
        assert_eq!(pkgs[0].version, "4.17.21");
        assert_eq!(pkgs[0].ecosystem, "npm");
    }

    #[test]
    fn parses_scoped_packages() {
        let raw = r#"{
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "x", "version": "1.0.0"},
                "node_modules/@scope/pkg": {
                    "version": "2.0.0",
                    "resolved": "https://registry.npmjs.org/@scope/pkg/-/pkg-2.0.0.tgz"
                }
            }
        }"#;
        let pkgs = parse_package_lock_str(raw).unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "@scope/pkg");
    }

    #[test]
    fn excludes_git_sources() {
        let raw = r#"{
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "x", "version": "1.0.0"},
                "node_modules/from-git": {
                    "version": "1.0.0",
                    "resolved": "git+https://github.com/x/y#abc"
                }
            }
        }"#;
        let pkgs = parse_package_lock_str(raw).unwrap();
        assert!(pkgs.is_empty());
    }

    #[test]
    fn excludes_file_sources() {
        let raw = r#"{
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "x", "version": "1.0.0"},
                "node_modules/local": {
                    "version": "1.0.0",
                    "resolved": "file:../sibling"
                }
            }
        }"#;
        let pkgs = parse_package_lock_str(raw).unwrap();
        assert!(pkgs.is_empty());
    }

    #[test]
    fn excludes_alt_registry() {
        let raw = r#"{
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "x", "version": "1.0.0"},
                "node_modules/private": {
                    "version": "1.0.0",
                    "resolved": "https://internal.example.com/private/-/private-1.0.0.tgz"
                }
            }
        }"#;
        let pkgs = parse_package_lock_str(raw).unwrap();
        assert!(pkgs.is_empty());
    }

    #[test]
    fn excludes_workspace_links() {
        let raw = r#"{
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "monorepo", "version": "1.0.0"},
                "packages/sub": {
                    "version": "1.0.0",
                    "link": true
                },
                "node_modules/sub": {
                    "resolved": "packages/sub",
                    "link": true
                }
            }
        }"#;
        let pkgs = parse_package_lock_str(raw).unwrap();
        assert!(pkgs.is_empty(), "workspace links must be excluded");
    }

    #[test]
    fn extracts_name_from_alias() {
        // Aliased dep: pkg.json says `"foo": "npm:bar@1.0.0"`.
        // npm resolves the alias and sets `name: "bar"` in the
        // lockfile entry. We must extract "bar", not "foo".
        let raw = r#"{
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "x", "version": "1.0.0"},
                "node_modules/foo": {
                    "name": "bar",
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/bar/-/bar-1.0.0.tgz"
                }
            }
        }"#;
        let pkgs = parse_package_lock_str(raw).unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "bar");
    }

    #[test]
    fn handles_nested_node_modules_paths() {
        let raw = r#"{
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "x", "version": "1.0.0"},
                "node_modules/foo": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/foo/-/foo-1.0.0.tgz"
                },
                "node_modules/foo/node_modules/bar": {
                    "version": "2.0.0",
                    "resolved": "https://registry.npmjs.org/bar/-/bar-2.0.0.tgz"
                }
            }
        }"#;
        let pkgs = parse_package_lock_str(raw).unwrap();
        let names: Vec<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(pkgs.len(), 2);
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
    }

    #[test]
    fn dedups_when_same_pkg_appears_in_multiple_paths() {
        let raw = r#"{
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "x", "version": "1.0.0"},
                "node_modules/foo": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/foo/-/foo-1.0.0.tgz"
                },
                "node_modules/bar/node_modules/foo": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/foo/-/foo-1.0.0.tgz"
                }
            }
        }"#;
        let pkgs = parse_package_lock_str(raw).unwrap();
        let foo_count = pkgs.iter().filter(|p| p.name == "foo").count();
        assert_eq!(foo_count, 1, "duplicate foo@1.0.0 must dedup");
    }

    #[test]
    fn rejects_lockfile_version_1() {
        let raw = r#"{
            "lockfileVersion": 1,
            "dependencies": {
                "lodash": {"version": "4.17.21"}
            }
        }"#;
        let pkgs = parse_package_lock_str(raw).unwrap();
        // v1 returns empty (logged warning); operator can re-run
        // `npm install` to upgrade.
        assert!(pkgs.is_empty());
    }

    #[test]
    fn lockfile_with_no_packages_field_returns_empty() {
        let raw = r#"{"lockfileVersion": 3, "name": "x"}"#;
        let pkgs = parse_package_lock_str(raw).unwrap();
        assert!(pkgs.is_empty());
    }

    #[test]
    fn malformed_json_returns_error() {
        let raw = "this is not json {{{";
        let res = parse_package_lock_str(raw);
        assert!(res.is_err());
    }

    #[test]
    fn parse_from_file_round_trip() {
        let tmp = TempDir::new().unwrap();
        let mut f = std::fs::File::create(tmp.path().join("package-lock.json")).unwrap();
        f.write_all(
            br#"{
                "name": "x",
                "lockfileVersion": 3,
                "packages": {
                    "": {"name": "x", "version": "1.0.0"},
                    "node_modules/express": {
                        "version": "4.21.0",
                        "resolved": "https://registry.npmjs.org/express/-/express-4.21.0.tgz"
                    }
                }
            }"#,
        )
        .unwrap();
        let pkgs = parse_package_lock(tmp.path()).unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "express");
    }
}
