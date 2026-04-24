//! OSV.dev batch-query client + file-backed response cache.
//!
//! **Implementation status: Step 5 (pending).** Current behavior is
//! a stub that returns an empty advisory list. The stub shape lets
//! the parent module (`supply_chain_sca::analyze_supply_chain_sca`)
//! wire its orchestration end-to-end while remaining modules land.
//!
//! # Planned design (Step 5)
//!
//! - POST `https://api.osv.dev/v1/querybatch` with a body of
//!   `{queries: [{package: {name, ecosystem: "crates.io"}, version}, ...]}`.
//! - Split large batches at 1000 queries per request.
//! - File-backed cache at `.claude/brain/cache/osv/<sha256-of-key>.json`.
//! - 24h TTL by default; `NEUROGRIM_OSV_NO_CACHE=1` env forces re-query.
//! - Cache age (seconds since oldest entry retrieved this run) is
//!   surfaced in the top-level CMDB envelope extras via the
//!   `osv_cache_age_seconds` field (handled by the parent module).
//!
//! # Error handling (planned)
//!
//! - Network failure: fall back to the RustSec local cross-check
//!   and flag `osv_unreachable: true` in CMDB extras. Never silently
//!   pass.
//! - Cache read corruption: log a warning, re-query OSV, overwrite
//!   the cache entry.
//! - HTTP non-2xx: propagate as `anyhow::Error` to the parent; the
//!   parent decides whether to degrade to RustSec-only or surface
//!   the failure as a finding.

use anyhow::Result;

use super::{Advisory, Package};

/// Query OSV.dev for advisories affecting `packages`.
///
/// **Stub — Step 5 pending.** Currently returns an empty `Vec`.
pub async fn query_batch(_packages: &[Package]) -> Result<Vec<Advisory>> {
    // Step 5 will replace this body with the real batch-query +
    // cache implementation. Returning empty is safe here because
    // the parent module's scoring will compute 100 (no advisories)
    // when both OSV and RustSec-local return empty.
    Ok(Vec::new())
}
