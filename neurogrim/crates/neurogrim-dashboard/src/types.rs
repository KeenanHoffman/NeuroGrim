//! Wire-format types for dashboard API endpoints.
//!
//! These are dashboard-specific DTOs — NOT the canonical
//! `neurogrim_core::AgentOutput` shape. Each endpoint owns its own
//! contract; the route handler converts the underlying core data
//! (e.g., `AgentOutput`, `BrainRegistry`) into one of these for
//! transmission.
//!
//! `ts-rs` generates TypeScript types from each `#[derive(TS)]`
//! struct at `cargo test` time. The output lives in
//! `neurogrim-dashboard/bindings/` and is committed to git so the
//! frontend's `tsc` typechecking + the published-crate distribution
//! both have access to up-to-date types.
//!
//! Generation command:
//! ```bash
//! cargo test -p neurogrim-dashboard --lib export_bindings
//! ```
//!
//! Drift gate: a CI step diffs `bindings/` against the latest
//! committed version; non-empty diff = fail. Plan reference:
//! v3.4 Phase 0.4 in `audit/dec-...`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Response body of `GET /api/health`. The simplest endpoint —
/// proves the wire-format pipeline works. Returns ok=true plus a
/// few diagnostic fields the frontend can use to detect server/
/// client version drift.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct HealthResponse {
    pub ok: bool,
    pub registry_path: String,
    /// Server version (matches the dashboard binary's
    /// `CARGO_PKG_VERSION`). Frontends bundled at v3.4 connecting
    /// to a v3.5 server can warn the operator.
    pub version: String,
}

/// Lightweight, prose-tuned summary of a Brain's current state,
/// powering the dashboard's Overview page.
///
/// Distinct from `neurogrim_core::AgentOutput`: this DTO is
/// dashboard-curated (top 3 recs, top 3 strongest signals,
/// federation peer count rather than full peer list) — the goal
/// is "what does a human want to see on first glance," not
/// "every spec'd field." The full AgentOutput is available via
/// `GET /api/agent` for consumers that need it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct OverviewResponse {
    /// Display name for the Brain (from `meta.description` first
    /// sentence; falls back to project_root basename).
    pub project_label: String,
    /// Filesystem path to the registry, surfaced for operator
    /// awareness (which Brain is being shown).
    pub project_root: String,
    /// Total declared domains.
    pub domain_count: u32,
    /// Subset that are weighted (weight > 0).
    pub weighted_count: u32,
    /// Subset that are advisory (weight == 0).
    pub advisory_count: u32,
    /// Unified score 0..=100. None when the Brain is all-advisory
    /// (in that case the score is structurally 0 / N/A).
    pub score: Option<u8>,
    /// Weighted-mean confidence 0..=100 across non-advisory
    /// domains. None paired with `score: None` for all-advisory.
    pub confidence: Option<u8>,
    /// Trajectory classification ("improving" | "degrading" |
    /// "stable" | "volatile" | "no-data"). Stringly-typed at the
    /// wire to keep the frontend simple; mapped to TrajectoryKind
    /// in TS.
    pub trajectory_class: String,
    /// Trajectory velocity (signed; positive = improving).
    pub trajectory_velocity: f64,
    /// Number of score-history samples observed.
    pub trajectory_samples: u32,
    /// Top recommendations (up to 3).
    pub top_recommendations: Vec<RecommendationDto>,
    /// Top strongest signals (up to 3 highest effective scores).
    pub strongest_signals: Vec<DomainSignalDto>,
    /// Count of declared federation peers.
    pub federation_peer_count: u32,
}

/// Per-domain summary used in `OverviewResponse.strongest_signals`
/// and the Domains-page table. The full per-domain detail (CMDB
/// findings, history) is fetched separately via
/// `GET /api/domains/:name`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DomainSignalDto {
    pub name: String,
    /// Humanized display (from registry.principle_map; falls back
    /// to the kebab-case name).
    pub display_name: String,
    pub effective_score: u8,
    pub confidence: u8,
    pub weight: f64,
}

/// Recommendation summary used in `OverviewResponse.top_recommendations`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct RecommendationDto {
    pub domain: String,
    pub gate: String,
    pub status: String,
    pub command: String,
    /// Short human-readable rationale (from
    /// `Recommendation::description`); may be empty if the source
    /// recommendation didn't carry one.
    pub description: String,
}

// =================================================================
// Phase 1.2 — Domains page + detail
// =================================================================

/// Response body of `GET /api/domains` — the sortable list view that
/// powers the Domains page table.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DomainsListResponse {
    pub domains: Vec<DomainListItemDto>,
}

/// One row in the domains table. Includes everything sortable from
/// the page UI (name, weight, scores, confidence, trajectory).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DomainListItemDto {
    pub name: String,
    pub display_name: String,
    pub weight: f64,
    pub raw_score: u8,
    pub effective_score: u8,
    pub confidence: u8,
    /// Trajectory classification at the per-domain level. One of
    /// "improving" | "degrading" | "stable" | "volatile" | "no-data".
    pub trajectory_class: String,
    pub trajectory_velocity: f64,
    pub trajectory_samples: u32,
    /// CMDB `meta.updated_at` (ISO 8601 UTC). `None` when the CMDB
    /// is missing on disk (the domain registers but no sensor has
    /// produced a CMDB yet — the v3.2 stub-domain pattern).
    pub last_updated: Option<String>,
}

/// Response body of `GET /api/domains/:name` — the drill-in detail.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DomainDetailResponse {
    pub name: String,
    pub display_name: String,
    pub weight: f64,
    pub raw_score: u8,
    pub effective_score: u8,
    pub confidence: u8,
    pub trajectory_class: String,
    pub trajectory_velocity: f64,
    pub trajectory_samples: u32,
    /// Operator-supplied sensor authoring intent — the
    /// `_todo_<name>` placeholder on the domain definition (set
    /// via `domain new --sensor-intent`). Useful when the sensor
    /// hasn't been written yet. None when absent.
    pub sensor_intent: Option<String>,
    /// CMDB findings array (preserved verbatim from the on-disk
    /// envelope). Empty array when no CMDB exists or no findings
    /// were emitted.
    pub findings: Vec<FindingDto>,
    /// Score-history sparkline data. Each entry is one tick. May
    /// be empty (Brain just initialized) or sparse.
    pub history: Vec<HistoryPointDto>,
    /// Filesystem path to the CMDB JSON, displayed for operator
    /// awareness ("which file would I edit to refresh this").
    pub cmdb_path: String,
    /// CMDB `meta.updated_at` if present; None when no CMDB on disk.
    pub last_updated: Option<String>,
}

/// CMDB finding entry. Schema mirrors the canonical
/// `cmdb-envelope-v1.schema.json` finding shape.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct FindingDto {
    pub name: String,
    pub status: String,
    pub points: i32,
    /// Optional human-readable explanation. Sensors SHOULD include
    /// it on warnings/errors; for "pass" findings it's commonly
    /// absent.
    pub detail: Option<String>,
}

/// One point in a domain's score-history sparkline.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct HistoryPointDto {
    /// ISO 8601 UTC timestamp.
    pub scored_at: String,
    pub score: u8,
    pub confidence: u8,
}

#[cfg(test)]
mod tests {
    /// Compile-time-style check: all #[derive(TS)] types in this
    /// module re-export to `bindings/` on `cargo test`. The actual
    /// file production happens via the `TS::export_all` machinery
    /// invoked by ts-rs's test harness — declaring the test target
    /// here keeps the trigger discoverable from `cargo test -p
    /// neurogrim-dashboard --lib`.
    ///
    /// (No assertions: ts-rs's test-time generator does the work
    /// during `cargo test`; this test just gives operators a
    /// well-named place to look when they wonder "where's the
    /// bindings generation entry point.")
    #[test]
    fn export_bindings_marker() {
        // Intentionally empty.
    }
}
