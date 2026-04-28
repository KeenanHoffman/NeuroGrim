//! Lockfile dispatch hub for the supply-chain-sca sensor.
//!
//! Each ecosystem's lockfile parser is its own submodule. The
//! parent sensor calls `detect()` to enumerate which lockfile
//! formats are present in `project_root`, then dispatches to the
//! appropriate parser per result.
//!
//! E-SC-2 (the original sensor) only had Cargo.lock; the historic
//! call site was `lockfile::parse(root)`. That entry point is
//! preserved here as a re-export of `cargo::parse` so existing
//! call sites and tests keep working unchanged. E-SC-3 + E-SC-4
//! introduce `detect()` + per-ecosystem variants; the parent
//! sensor migrates to the new dispatch path in Step E3.5.
//!
//! Modules:
//! - `cargo` — `Cargo.lock` (E-SC-2)
//! - `python` — `uv.lock` + `requirements*.txt` (E-SC-3, pending)
//! - `npm` — `package-lock.json` (E-SC-4, pending)
//! - `yarn` — `yarn.lock` (E-SC-4, pending)
//! - `pnpm` — `pnpm-lock.yaml` (E-SC-4, pending)

pub mod cargo;
pub mod npm;
pub mod pnpm;
pub mod python;
pub mod yarn;

// Backwards-compatible re-export. The original `lockfile::parse(root)`
// call site keeps working unchanged through Step E3.1; Step E3.5
// replaces the parent's call site with the new `detect()`-based
// dispatch and this re-export becomes unused.
pub use cargo::parse;

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::Package;

/// One detected lockfile in the project root, with enough metadata
/// for the dispatch hub to route to the right parser.
///
/// Variants:
/// - `Cargo` — Rust (E-SC-2; SHIPPED). v3.1: carries the workspace
///   directory containing `Cargo.lock` because Rust workspaces
///   sometimes live in a subdir (NeuroGrim's `neurogrim/` convention)
///   rather than at the repo root.
/// - `UvLock` — Python via Astral's uv (E-SC-3; SHIPPED).
/// - `RequirementsTxt` — Python pip-style pinned (E-SC-3; SHIPPED;
///   carries the file's full path because the conventional name
///   varies — `requirements.txt`, `requirements-lock.txt`, etc.).
/// - `PackageLockJson` — npm (E-SC-4; SHIPPED).
/// - `YarnLock` — Yarn 1.x and Berry (E-SC-4; SHIPPED).
/// - `PnpmLock` — pnpm v6 + v9 (E-SC-4; SHIPPED).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedLockfile {
    /// Rust workspace containing `Cargo.lock`. The path is the
    /// directory containing `Cargo.lock`, which may be `<project_root>`
    /// (standard layout) or `<project_root>/neurogrim` (NeuroGrim
    /// workspace-in-subdir layout). v3.1+.
    Cargo(PathBuf),
    /// `<project_root>/uv.lock` — Astral uv resolved lockfile (E-SC-3).
    UvLock,
    /// A `requirements*.txt` file with PEP-440 `==` pins (E-SC-3).
    /// The path varies (`requirements.txt`, `requirements-lock.txt`,
    /// `requirements-prod.txt`, etc.) so the variant carries the
    /// concrete path.
    RequirementsTxt(PathBuf),
    /// `<project_root>/package-lock.json` (npm 7+; E-SC-4).
    PackageLockJson,
    /// `<project_root>/yarn.lock` (Yarn 1.x or Berry; E-SC-4).
    YarnLock,
    /// `<project_root>/pnpm-lock.yaml` (pnpm v6+/v9+; E-SC-4).
    PnpmLock,
}

/// Walk `project_root` for known lockfile basenames and return the
/// list of detected formats. Non-recursive — the lockfile must be at
/// the directory root the user pointed the sensor at.
///
/// Multiple lockfiles can coexist (e.g., a Tauri project with
/// Cargo.lock + package-lock.json); callers iterate the result and
/// dispatch each variant to its parser.
///
/// `requirements*.txt` detection: matches files whose name starts
/// with `requirements` and ends with `.txt`. We deliberately accept
/// multiple — projects often have both `requirements.txt` and
/// `requirements-dev.txt` etc., and the operator may want all
/// scanned. The dispatcher dedupes by `(ecosystem, name, version)`
/// so over-coverage doesn't double-count.
pub fn detect(project_root: &Path) -> Vec<DetectedLockfile> {
    let mut out = Vec::new();

    // Cargo.lock: probe project_root first, then `<project_root>/neurogrim`
    // (NeuroGrim workspace-in-subdir layout). Same dual-probe pattern
    // `rust_health.rs` uses for Cargo.toml. First hit wins; we don't
    // expect both to coexist in practice.
    let cargo_candidates = [
        project_root.to_path_buf(),
        project_root.join("neurogrim"),
    ];
    for candidate in cargo_candidates {
        if candidate.join("Cargo.lock").is_file() {
            out.push(DetectedLockfile::Cargo(candidate));
            break;
        }
    }

    if project_root.join("uv.lock").is_file() {
        out.push(DetectedLockfile::UvLock);
    }
    if project_root.join("package-lock.json").is_file() {
        out.push(DetectedLockfile::PackageLockJson);
    }
    if project_root.join("yarn.lock").is_file() {
        out.push(DetectedLockfile::YarnLock);
    }
    if project_root.join("pnpm-lock.yaml").is_file() {
        out.push(DetectedLockfile::PnpmLock);
    }
    if let Ok(entries) = std::fs::read_dir(project_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if name.starts_with("requirements") && name.ends_with(".txt") {
                out.push(DetectedLockfile::RequirementsTxt(path));
            }
        }
    }
    out
}

/// Parse a single detected lockfile to a `Vec<Package>` via the
/// ecosystem-appropriate parser.
pub fn parse_detected(detected: &DetectedLockfile, project_root: &Path) -> Result<Vec<Package>> {
    match detected {
        // For Cargo, use the carried workspace directory rather than
        // project_root — handles the NeuroGrim workspace-in-subdir case.
        DetectedLockfile::Cargo(workspace) => cargo::parse(workspace),
        DetectedLockfile::UvLock => python::parse_uv_lock(project_root),
        DetectedLockfile::RequirementsTxt(path) => python::parse_requirements_txt(path),
        DetectedLockfile::PackageLockJson => npm::parse_package_lock(project_root),
        DetectedLockfile::YarnLock => yarn::parse_yarn_lock(project_root),
        DetectedLockfile::PnpmLock => pnpm::parse_pnpm_lock(project_root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_minimal_cargo_lock(dir: &Path) {
        let mut f = std::fs::File::create(dir.join("Cargo.lock")).unwrap();
        f.write_all(
            b"version = 3\n\n[[package]]\nname = \"x\"\nversion = \"0.1.0\"\nsource = \
              \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \
              \"0000000000000000000000000000000000000000000000000000000000000000\"\n",
        )
        .unwrap();
    }

    #[test]
    fn detect_returns_empty_for_empty_dir() {
        let tmp = TempDir::new().unwrap();
        assert!(detect(tmp.path()).is_empty());
    }

    #[test]
    fn detect_finds_cargo_lock() {
        let tmp = TempDir::new().unwrap();
        write_minimal_cargo_lock(tmp.path());
        let detected = detect(tmp.path());
        assert_eq!(detected, vec![DetectedLockfile::Cargo(tmp.path().to_path_buf())]);
    }

    #[test]
    fn detect_finds_cargo_lock_in_neurogrim_subdir() {
        // NeuroGrim convention: workspace lives in `neurogrim/` subdir,
        // not at the repo root. detect() should still find the lockfile
        // and report the subdir as the carried workspace path.
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("neurogrim");
        std::fs::create_dir_all(&workspace).unwrap();
        write_minimal_cargo_lock(&workspace);
        let detected = detect(tmp.path());
        assert_eq!(detected, vec![DetectedLockfile::Cargo(workspace)]);
    }

    #[test]
    fn detect_prefers_root_cargo_lock_over_subdir() {
        // If both root/Cargo.lock and root/neurogrim/Cargo.lock exist,
        // root takes precedence (standard Rust layout wins; the subdir
        // probe is a fallback for workspace-in-subdir layouts).
        let tmp = TempDir::new().unwrap();
        write_minimal_cargo_lock(tmp.path());
        let subdir = tmp.path().join("neurogrim");
        std::fs::create_dir_all(&subdir).unwrap();
        write_minimal_cargo_lock(&subdir);
        let detected = detect(tmp.path());
        assert_eq!(detected, vec![DetectedLockfile::Cargo(tmp.path().to_path_buf())]);
    }

    #[test]
    fn parse_detected_routes_cargo_to_cargo_parser() {
        let tmp = TempDir::new().unwrap();
        write_minimal_cargo_lock(tmp.path());
        let pkgs = parse_detected(
            &DetectedLockfile::Cargo(tmp.path().to_path_buf()),
            tmp.path(),
        )
        .unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "x");
        assert_eq!(pkgs[0].ecosystem, "crates.io");
    }

    #[test]
    fn parse_detected_uses_carried_workspace_for_cargo() {
        // The carried workspace path on the Cargo variant is what
        // cargo::parse uses, NOT the project_root parameter. This
        // verifies the workspace-in-subdir case routes correctly.
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("neurogrim");
        std::fs::create_dir_all(&workspace).unwrap();
        write_minimal_cargo_lock(&workspace);
        let pkgs = parse_detected(
            &DetectedLockfile::Cargo(workspace.clone()),
            tmp.path(),
        )
        .unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "x");
    }

    #[test]
    fn detect_finds_uv_lock() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("uv.lock"),
            "version = 1\n\n[[package]]\nname = \"x\"\nversion = \"0.1.0\"\nsource = { registry = \"https://pypi.org/simple\" }\n",
        ).unwrap();
        let detected = detect(tmp.path());
        assert!(detected.iter().any(|d| matches!(d, DetectedLockfile::UvLock)));
    }

    #[test]
    fn detect_finds_requirements_txt() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("requirements.txt"), "x==1.0.0\n").unwrap();
        std::fs::write(tmp.path().join("requirements-dev.txt"), "y==2.0.0\n").unwrap();
        let detected = detect(tmp.path());
        let req_count = detected
            .iter()
            .filter(|d| matches!(d, DetectedLockfile::RequirementsTxt(_)))
            .count();
        assert_eq!(req_count, 2, "expected both requirements files; got {detected:?}");
    }

    #[test]
    fn detect_finds_package_lock_json() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("package-lock.json"),
            r#"{"name": "x", "lockfileVersion": 3}"#,
        )
        .unwrap();
        let detected = detect(tmp.path());
        assert!(detected
            .iter()
            .any(|d| matches!(d, DetectedLockfile::PackageLockJson)));
    }

    #[test]
    fn detect_finds_yarn_lock() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("yarn.lock"), "# yarn lockfile v1\n").unwrap();
        let detected = detect(tmp.path());
        assert!(detected
            .iter()
            .any(|d| matches!(d, DetectedLockfile::YarnLock)));
    }

    #[test]
    fn detect_finds_pnpm_lock_yaml() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\n",
        )
        .unwrap();
        let detected = detect(tmp.path());
        assert!(detected
            .iter()
            .any(|d| matches!(d, DetectedLockfile::PnpmLock)));
    }

    #[test]
    fn detect_finds_multiple_ecosystems_in_one_root() {
        let tmp = TempDir::new().unwrap();
        write_minimal_cargo_lock(tmp.path());
        std::fs::write(
            tmp.path().join("uv.lock"),
            "version = 1\n",
        )
        .unwrap();
        std::fs::write(tmp.path().join("requirements.txt"), "x==1.0.0\n").unwrap();
        let detected = detect(tmp.path());
        assert!(detected.iter().any(|d| matches!(d, DetectedLockfile::Cargo(_))));
        assert!(detected.iter().any(|d| matches!(d, DetectedLockfile::UvLock)));
        assert!(
            detected
                .iter()
                .any(|d| matches!(d, DetectedLockfile::RequirementsTxt(_)))
        );
    }
}

