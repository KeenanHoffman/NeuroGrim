//! Layer 3 scoring + CMDB envelope assembly.
//!
//! v1 score model (per E-SC-6 plan): `100 - 10 × open_tickets`,
//! floor 0. Simple, honest. Score 100 = no pending review work.
//! Score deteriorates as tickets pile up. E-SC-8 calibration may
//! introduce a richer model (stale-pending weighting, no-decision-
//! on-flagged, etc.) once the workflow is exercised.

use crate::cmdb::Finding as CmdbFinding;
use serde_json::{json, Value};
use std::collections::BTreeMap;

use super::ledger::{LedgerEntry, LedgerFold};
use super::ticket::ReviewTicket;

/// Per-open-ticket point deduction.
const OPEN_TICKET_DEDUCTION: u32 = 10;

/// Compute the unified score given current tickets + folded ledger.
pub fn score(tickets: &[ReviewTicket]) -> u32 {
    let open: u32 = tickets.iter().filter(|t| t.is_open()).count() as u32;
    100u32.saturating_sub(open.saturating_mul(OPEN_TICKET_DEDUCTION))
}

/// Build the supply-chain-review CMDB envelope.
pub fn build_cmdb_envelope(
    score_value: u32,
    tickets: &[ReviewTicket],
    ledger: &[LedgerEntry],
    fold: &LedgerFold,
) -> Value {
    let open_tickets: Vec<&ReviewTicket> = tickets.iter().filter(|t| t.is_open()).collect();
    let resolved_tickets: Vec<&ReviewTicket> =
        tickets.iter().filter(|t| !t.is_open()).collect();

    // One CMDB finding per open ticket so operators can scan them
    // in `neurogrim health` output.
    let mut cmdb_findings: Vec<CmdbFinding> = Vec::with_capacity(open_tickets.len() + 4);
    for t in &open_tickets {
        let kinds: Vec<&str> = t
            .triggering_signals
            .iter()
            .map(|s| s.signal_kind.as_str())
            .collect();
        cmdb_findings.push(CmdbFinding {
            name: format!("open-ticket:{}", t.id),
            status: "warning".to_string(),
            points: OPEN_TICKET_DEDUCTION as i32,
            detail: Some(format!(
                "Layer 3 review pending — {}@{} [{}], signals: {} — opened by {} on {}",
                t.package.name,
                t.from_version.as_deref().unwrap_or("?"),
                t.package.ecosystem,
                if kinds.is_empty() { "(none)".to_string() } else { kinds.join(", ") },
                t.created_by,
                t.created_at.format("%Y-%m-%d"),
            )),
        });
    }

    // Counters per decision kind from the folded ledger.
    let mut decision_kinds_seen: BTreeMap<&'static str, u32> = BTreeMap::new();
    for entry in ledger {
        *decision_kinds_seen.entry(entry.kind()).or_insert(0) += 1;
    }
    let by_kind_json: serde_json::Map<String, Value> = decision_kinds_seen
        .iter()
        .map(|(k, v)| (k.to_string(), json!(*v)))
        .collect();

    // Per-package decision summary (latest-state).
    let by_package_json: serde_json::Map<String, Value> = fold
        .by_package
        .iter()
        .map(|((eco, name), entry)| {
            (
                format!("{}:{}", eco, name),
                json!({
                    "kind": entry.kind(),
                    "ts": entry.ts(),
                }),
            )
        })
        .collect();

    let extras: Vec<(&str, Value)> = vec![
        ("tickets_open", json!(open_tickets.len())),
        ("tickets_resolved_total", json!(resolved_tickets.len())),
        ("tickets_total", json!(tickets.len())),
        ("ledger_entries_total", json!(ledger.len())),
        ("decision_kinds_seen", json!(by_kind_json)),
        ("latest_decision_per_package", json!(by_package_json)),
        ("score_model", json!("open-ticket-count-v1")),
        (
            "_impl_status",
            json!(
                "E-SC-6 (Layer 3 review framework): supply-chain-auditor hat \
                 + decision-ledger writer + review-ticket file format + \
                 supply-chain-review CMDB sensor + auto-create from Layer 2 \
                 vigilance findings + CLI (sca-review create/list/resolve). \
                 Default weight 0.0 (advisory) per spec §16.4. v1 score model: \
                 100 - 10 × open_tickets. Calibration: E-SC-8."
            ),
        ),
    ];

    crate::cmdb::build_cmdb(
        "supply-chain-review",
        score_value as u8,
        cmdb_findings,
        Some(extras),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::super::ledger::PackageRef;
    use super::super::ticket::ReviewTicket;
    use super::*;
    use chrono::Utc;

    fn open_ticket(id: &str) -> ReviewTicket {
        ReviewTicket {
            id: id.to_string(),
            created_at: Utc::now(),
            package: PackageRef {
                name: "fakepkg".to_string(),
                ecosystem: "PyPI".to_string(),
                version_range: None,
            },
            from_version: None,
            to_version: None,
            triggering_signals: vec![],
            agent_findings: vec![],
            created_by: "auto".to_string(),
            creation_notes: None,
            resolved_at: None,
            resolution: None,
            resolved_by: None,
            resolution_notes: None,
            pending_ledger_ts: 1.0,
            schema_version: 1,
        }
    }

    fn closed_ticket(id: &str) -> ReviewTicket {
        let mut t = open_ticket(id);
        t.resolved_at = Some(Utc::now());
        t.resolution = Some("accept".to_string());
        t.resolved_by = Some("alice".to_string());
        t.resolution_notes = Some("ok".to_string());
        t
    }

    #[test]
    fn score_with_no_tickets_is_100() {
        assert_eq!(score(&[]), 100);
    }

    #[test]
    fn score_with_one_open_is_90() {
        let tickets = vec![open_ticket("t-1")];
        assert_eq!(score(&tickets), 90);
    }

    #[test]
    fn score_resolved_tickets_dont_deduct() {
        let tickets = vec![closed_ticket("t-1"), closed_ticket("t-2"), closed_ticket("t-3")];
        assert_eq!(score(&tickets), 100);
    }

    #[test]
    fn score_floor_at_0() {
        let tickets: Vec<ReviewTicket> =
            (0..15).map(|i| open_ticket(&format!("t-{}", i))).collect();
        assert_eq!(score(&tickets), 0);
    }

    #[test]
    fn score_mixed_only_counts_open() {
        let tickets = vec![
            open_ticket("t-1"),
            closed_ticket("t-2"),
            open_ticket("t-3"),
        ];
        // 2 open × 10 = 20 deduction → score 80.
        assert_eq!(score(&tickets), 80);
    }
}
