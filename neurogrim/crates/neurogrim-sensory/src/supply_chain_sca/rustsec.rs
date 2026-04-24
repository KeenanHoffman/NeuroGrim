//! Local reader for the pinned `rustsec/advisory-db` clone at
//! `vendor/rustsec-advisory-db/`.
//!
//! **Implementation status: Step 6 (pending).** Current behavior is
//! a stub that returns an empty advisory list.
//!
//! # Planned design (Step 6)
//!
//! - Walk `vendor/rustsec-advisory-db/crates/<name>/*.md` for each
//!   `Package` in scope.
//! - Each advisory file is a Markdown document with a ```toml ... ```
//!   frontmatter block containing:
//!   - `advisory.id`, `advisory.package`, `advisory.date`,
//!     `advisory.informational` (Option; "unmaintained" / "notice"),
//!     `advisory.aliases` (GHSA, CVE, etc.),
//!   - `versions.patched` (Vec<VersionReq>), `versions.unaffected`
//!     (Vec<VersionReq>).
//! - Parse the frontmatter TOML via the `toml` crate.
//! - A package-version is "affected" if it is not covered by any
//!   `patched` or `unaffected` range.
//! - Return advisories that are NOT already in the OSV result set
//!   (union-subtract by advisory id) so the CMDB doesn't double-
//!   count. Union-subtract is the parent module's job; this function
//!   just returns all matches.
//!
//! # Submodule discovery
//!
//! The advisory-db submodule lives at `vendor/rustsec-advisory-db/`
//! in the NeuroGrim repo. When running against a third-party
//! project, the submodule isn't present, so we fall back to "skip
//! silently" — OSV remains the primary data source, local RustSec is
//! a cross-reference for NeuroGrim's own dev environment and adopter
//! repos that opt in.

use anyhow::Result;
use std::path::Path;

use super::{Advisory, Package};

/// Scan the local RustSec advisory-db clone for advisories affecting
/// `packages`.
///
/// **Stub — Step 6 pending.** Currently returns an empty `Vec`.
///
/// The `project_root` parameter is used to locate the submodule at
/// `<project_root>/vendor/rustsec-advisory-db/`. When the submodule
/// is missing (most third-party adopter usage), the function returns
/// `Ok(empty)` rather than an error.
pub fn scan_local(_packages: &[Package], _project_root: &Path) -> Result<Vec<Advisory>> {
    // Step 6 will replace this body with the real TOML-frontmatter
    // walk + version-range matching.
    Ok(Vec::new())
}
