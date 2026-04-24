//! Reader for `.claude/supply-chain-accepted-advisories.toml`.
//!
//! **Implementation status: Step 7 (pending).** Current behavior is
//! a stub that returns an empty accepted-set.
//!
//! # Planned design (Step 7)
//!
//! Operator-curated list of advisory IDs that have been reviewed and
//! consciously accepted as "not-a-risk-for-us-right-now." Example
//! content:
//!
//! ```toml
//! # .claude/supply-chain-accepted-advisories.toml
//!
//! [[accepted]]
//! id = "RUSTSEC-2024-0436"
//! package = "paste"
//! note = "Unmaintained-notice; transitive via rmcp; proc-macro-only; accepted 2026-04-24."
//! # optional: expires_at = "2026-10-24"  (re-review deadline)
//! ```
//!
//! Accepted advisories still appear in the CMDB extras under
//! `accepted_advisory_ids` for transparency; they simply don't
//! deduct from the score. This is v1's lightweight answer to
//! operator triage; a richer 2-phase append-only ledger lands in
//! E-SC-6 (`supply-chain-decision-ledger.jsonl`).
//!
//! # Hygiene requirement (planned)
//!
//! The `note` field is non-optional — v1 rejects acceptance entries
//! that don't explain WHY. The pressure to document the reasoning
//! is the hygiene lever that keeps the file from becoming a
//! silent dumping ground.

use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

/// Read the optional `.claude/supply-chain-accepted-advisories.toml`
/// and return the set of accepted advisory IDs.
///
/// **Stub — Step 7 pending.** Currently returns an empty `HashSet`.
///
/// When the file is missing, this function returns `Ok(empty)` —
/// operator acceptance is opt-in.
pub fn read(_project_root: &Path) -> Result<HashSet<String>> {
    // Step 7 will replace this body with the real TOML parser.
    Ok(HashSet::new())
}
