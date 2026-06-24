//! BB #19 — Governance Composer.
//!
//! Framework-provided governance pipelines that compose into Surfaced
//! pipelines per BROKER-INTERNALS.md §2.4.
//!
//! ## MVP scope (Wave 4)
//!
//! Four default-composed pipelines per spec:
//! - `check-trust-budget` — refuses if over-budget; first step in Surfaced.
//! - `check-kill-switch` — refuses if armed; scopes: per-pipeline / per-broker / global.
//! - `record-dispatch` — writes audit anchor at dispatch start.
//! - `record-outcome` — written at dispatch end.
//!
//! Plus the operator-controllable Surfaced pipeline:
//! - `arm-kill-switch` — OperatorOnly tunability.
//!
//! ## Trust budget MVP simplification (per ultra-pass U7)
//!
//! Single global pool, unit = `dispatch-count`, fixed-ceiling,
//! manual-reset-only. Degenerate scope; **S1-T must rewrite trust-budget
//! significantly** per the full §4 formalization. MVP code is throwaway-grade
//! for that path.

/// Wave 4 finalizes the API; this is the Wave 0 placeholder.
pub struct GovernanceComposer {
    // Wave 4 adds: trust-budget tracker, kill-switch state, etc.
}

impl GovernanceComposer {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for GovernanceComposer {
    fn default() -> Self {
        Self::new()
    }
}
