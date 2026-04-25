//! `pnpm-lock.yaml` parser for the supply-chain-sca sensor.
//!
//! pnpm is the third major Node package manager (alongside npm and
//! yarn) and has gained traction for monorepo work since 2022.
//! pnpm-lock.yaml is YAML; the schema has shifted across pnpm
//! versions:
//!
//! - **v6 (pnpm 7-8)**: keys like `/lodash@4.17.21` (leading slash)
//!   or `/@scope/pkg@1.0.0`. Resolution metadata under
//!   `resolution: { integrity: ... }`.
//! - **v9 (pnpm 9+)**: keys like `lodash@4.17.21` (no leading slash)
//!   or `@scope/pkg@1.0.0`. Schema otherwise similar.
//! - **v5 (pnpm <7)**: very old; not supported in MVP.
//!
//! # Filtering rules
//!
//! - **Git/file/local**: pnpm may include these via `link:` or
//!   `git+https://...` resolutions. Skip.
//! - **Workspace deps**: keyed under `importers:` rather than
//!   `packages:`; we parse `packages:` only, so these are
//!   automatically excluded.
//!
//! # Trust surface
//!
//! `serde_yaml` (already a workspace dep). Hand-rolled struct.
//! ZERO new external crates.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

use crate::supply_chain_sca::Package;

/// Parse `<project_root>/pnpm-lock.yaml` and return deduplicated
/// npm-registry-sourced packages.
pub fn parse_pnpm_lock(project_root: &Path) -> Result<Vec<Package>> {
    let path = project_root.join("pnpm-lock.yaml");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    parse_pnpm_lock_str(&raw)
}

fn parse_pnpm_lock_str(raw: &str) -> Result<Vec<Package>> {
    let parsed: PnpmLockfile =
        serde_yaml::from_str(raw).with_context(|| "pnpm-lock.yaml YAML parse")?;

    let packages = match parsed.packages {
        Some(p) => p,
        None => return Ok(Vec::new()),
    };

    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for (key, entry) in packages {
        let Some((name, version)) = parse_pnpm_key(&key) else {
            continue;
        };
        // Skip non-registry resolutions. pnpm stores resolution
        // metadata in `resolution.integrity` (npm registry) or
        // `resolution.tarball` / `resolution.repo` for non-registry.
        if !is_npm_registry_resolution(&entry.resolution) {
            continue;
        }
        seen.insert((name, version));
    }

    Ok(seen
        .into_iter()
        .map(|(name, version)| Package::npm(name, version))
        .collect())
}

/// Extract `(name, version)` from a pnpm-lock package key.
///
/// Accepts both pnpm v6 (`/pkg@version`) and v9 (`pkg@version`)
/// formats. Handles `@scope/pkg@version`. Returns `None` for
/// keys that include peer-dependency suffixes like
/// `pkg@1.0.0(peer@2.0.0)` (those are still parseable; we strip
/// the suffix).
fn parse_pnpm_key(raw: &str) -> Option<(String, String)> {
    let key = raw.trim_start_matches('/');
    // Strip peer-dep suffix: `lodash@4.17.21(react@18.0.0)` →
    // `lodash@4.17.21`. The version part of the canonical pnpm key
    // never contains `(`.
    let key = match key.find('(') {
        Some(idx) => &key[..idx],
        None => key,
    };
    // Find the LAST `@` — for scoped packages the leading `@scope/`
    // contains an `@` we must skip past.
    let at_idx = key.rfind('@')?;
    if at_idx == 0 {
        // Key starts with `@` — that's a scoped name with no version
        // separator. Not parseable.
        return None;
    }
    let name = &key[..at_idx];
    let version = &key[at_idx + 1..];
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name.to_string(), version.to_string()))
}

fn is_npm_registry_resolution(resolution: &Option<PnpmResolution>) -> bool {
    let Some(r) = resolution else {
        // No resolution metadata → can't tell; conservative skip.
        return false;
    };
    // npm registry resolutions carry an `integrity` (sha-512 hash);
    // git/tarball/file have `tarball` or `repo` fields instead.
    // Presence of `integrity` AND absence of `tarball`/`type` is the
    // canonical "from npm registry" signal.
    if r.integrity.is_some() && r.tarball.is_none() && r.kind.is_none() {
        return true;
    }
    // Some pnpm-lock files include explicit registry tarball URLs:
    // `tarball: https://registry.npmjs.org/...`. Allow that too.
    if let Some(tarball) = r.tarball.as_deref() {
        return tarball.starts_with("https://registry.npmjs.org/");
    }
    false
}

// =========================================================================
// pnpm-lock.yaml schema (subset we care about)
// =========================================================================

#[derive(Debug, Deserialize)]
struct PnpmLockfile {
    #[serde(default)]
    packages: Option<std::collections::BTreeMap<String, PnpmPackageEntry>>,
}

#[derive(Debug, Deserialize)]
struct PnpmPackageEntry {
    #[serde(default)]
    resolution: Option<PnpmResolution>,
}

#[derive(Debug, Deserialize)]
struct PnpmResolution {
    #[serde(default)]
    integrity: Option<String>,
    #[serde(default)]
    tarball: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    // ---------- parse_pnpm_key ----------

    #[test]
    fn key_v6_simple() {
        let r = parse_pnpm_key("/lodash@4.17.21");
        assert_eq!(r, Some(("lodash".into(), "4.17.21".into())));
    }

    #[test]
    fn key_v9_simple() {
        let r = parse_pnpm_key("lodash@4.17.21");
        assert_eq!(r, Some(("lodash".into(), "4.17.21".into())));
    }

    #[test]
    fn key_v6_scoped() {
        let r = parse_pnpm_key("/@scope/pkg@1.2.3");
        assert_eq!(r, Some(("@scope/pkg".into(), "1.2.3".into())));
    }

    #[test]
    fn key_v9_scoped() {
        let r = parse_pnpm_key("@scope/pkg@1.2.3");
        assert_eq!(r, Some(("@scope/pkg".into(), "1.2.3".into())));
    }

    #[test]
    fn key_with_peer_dep_suffix() {
        let r = parse_pnpm_key("react@18.0.0(react-dom@18.0.0)");
        assert_eq!(r, Some(("react".into(), "18.0.0".into())));
    }

    #[test]
    fn key_only_scope_is_invalid() {
        assert_eq!(parse_pnpm_key("@scope"), None);
        assert_eq!(parse_pnpm_key("/@scope"), None);
    }

    // ---------- parse_pnpm_lock_str ----------

    #[test]
    fn parses_simple_v9_lockfile() {
        let raw = r#"
lockfileVersion: '9.0'

packages:
  lodash@4.17.21:
    resolution: {integrity: sha512-abc}
"#;
        let pkgs = parse_pnpm_lock_str(raw).unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "lodash");
        assert_eq!(pkgs[0].version, "4.17.21");
        assert_eq!(pkgs[0].ecosystem, "npm");
    }

    #[test]
    fn parses_simple_v6_lockfile() {
        let raw = r#"
lockfileVersion: '6.0'

packages:
  /lodash@4.17.21:
    resolution: {integrity: sha512-abc}
"#;
        let pkgs = parse_pnpm_lock_str(raw).unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "lodash");
    }

    #[test]
    fn parses_scoped_packages() {
        let raw = r#"
lockfileVersion: '9.0'

packages:
  '@scope/pkg@1.0.0':
    resolution: {integrity: sha512-xyz}
"#;
        let pkgs = parse_pnpm_lock_str(raw).unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "@scope/pkg");
        assert_eq!(pkgs[0].version, "1.0.0");
    }

    #[test]
    fn excludes_git_resolution() {
        let raw = r#"
lockfileVersion: '9.0'

packages:
  some-pkg@1.0.0:
    resolution:
      type: git
      repo: https://github.com/x/y
"#;
        let pkgs = parse_pnpm_lock_str(raw).unwrap();
        assert!(pkgs.is_empty());
    }

    #[test]
    fn excludes_alt_registry_tarball() {
        let raw = r#"
lockfileVersion: '9.0'

packages:
  internal@1.0.0:
    resolution:
      tarball: https://internal.example.com/internal/1.0.0.tgz
"#;
        let pkgs = parse_pnpm_lock_str(raw).unwrap();
        assert!(pkgs.is_empty());
    }

    #[test]
    fn keeps_canonical_tarball_url() {
        let raw = r#"
lockfileVersion: '9.0'

packages:
  pkg@1.0.0:
    resolution:
      tarball: https://registry.npmjs.org/pkg/-/pkg-1.0.0.tgz
"#;
        let pkgs = parse_pnpm_lock_str(raw).unwrap();
        assert_eq!(pkgs.len(), 1);
    }

    #[test]
    fn excludes_no_resolution_block() {
        let raw = r#"
lockfileVersion: '9.0'

packages:
  pkg@1.0.0: {}
"#;
        let pkgs = parse_pnpm_lock_str(raw).unwrap();
        assert!(pkgs.is_empty());
    }

    #[test]
    fn dedups_repeats() {
        let raw = r#"
lockfileVersion: '9.0'

packages:
  lodash@4.17.21:
    resolution: {integrity: sha512-abc}
  lodash@4.17.21(peer@1.0.0):
    resolution: {integrity: sha512-abc}
"#;
        let pkgs = parse_pnpm_lock_str(raw).unwrap();
        assert_eq!(pkgs.len(), 1, "peer-dep suffix variants must dedup");
    }

    #[test]
    fn parse_from_file_round_trip() {
        let tmp = TempDir::new().unwrap();
        let mut f = std::fs::File::create(tmp.path().join("pnpm-lock.yaml")).unwrap();
        f.write_all(
            br#"lockfileVersion: '9.0'

packages:
  express@4.21.0:
    resolution: {integrity: sha512-aaa}
"#,
        )
        .unwrap();
        let pkgs = parse_pnpm_lock(tmp.path()).unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "express");
    }
}
