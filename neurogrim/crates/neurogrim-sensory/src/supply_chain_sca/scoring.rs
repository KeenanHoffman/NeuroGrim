//! Scoring model: advisories + accepted-set → (score, findings, extras).
//!
//! **Implementation status: Step 8 (pending).** Current behavior is
//! a stub that returns `(100, empty, empty)` — safe when all upstream
//! modules return empty (the Step 3-4 scaffold state).
//!
//! # Planned design (Step 8)
//!
//! Count-based, non-linear deduction:
//!
//! | Unaccepted advisories | Score |
//! |---|---|
//! | 0 | 100 |
//! | 1 | 75 |
//! | 2 | 50 |
//! | 3 | 25 |
//! | 4+ | 0 |
//!
//! Rationale: OSV batch responses don't include per-advisory
//! severity scores (those require a follow-up `/v1/vulns/<id>`
//! call), and many advisories (e.g., RustSec unmaintained-notices)
//! have empty severity arrays regardless. A count-based rubric is
//! honest about what we can reliably measure at batch-query time.
//!
//! Severity-weighted scoring may be added in E-SC-8 calibration if
//! the count-based model proves inadequate on the fixture library.
//!
//! Accepted advisories (from `accepted::read`) do NOT contribute to
//! the deduction count but DO appear in the CMDB extras under
//! `accepted_advisory_ids` for transparency.

use serde_json::{json, Value};
use std::collections::HashSet;

use super::Advisory;

/// Compute the score + findings + extras for the supply-chain-sca
/// CMDB envelope.
///
/// **Stub — Step 8 pending.** Currently returns a score of 100 with
/// empty findings and empty extras, which is the correct result when
/// the Step 3-4 scaffold's upstream modules return empty advisory
/// lists.
pub fn compute(
    _advisories: &[Advisory],
    _accepted: &HashSet<String>,
    _packages_scanned: usize,
) -> (u8, Vec<crate::cmdb::Finding>, Vec<(&'static str, Value)>) {
    // Step 8 will replace this with the real count-based scoring
    // model. Current extras are empty; the parent module adds its
    // own (total_packages_scanned, sensor_status, etc).
    let score: u8 = 100;
    let findings: Vec<crate::cmdb::Finding> = Vec::new();
    let extras: Vec<(&'static str, Value)> = vec![
        ("advisories_found", json!(0)),
        ("advisories_unaccepted", json!(0)),
        ("advisories_accepted", json!(0)),
        ("accepted_advisory_ids", json!(Vec::<String>::new())),
    ];
    (score, findings, extras)
}
