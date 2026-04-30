//! v4.3 S15-C-2 v2 / S15-C-3 expansion — log-source readers for the
//! Logs page.
//!
//! The Logs page aggregates events from multiple ledgers into a
//! single timeline. v1 covered publish-gates + approvals (already
//! exposed via `/api/brains/:id/publish-gates` and `/api/brains/
//! :id/approvals`). v2 added the invocation ledger reader. The C-3
//! expansion (this module's score-history reader) adds the fourth
//! ledger source so operators see score deltas in the unified
//! "what happened" timeline alongside gate runs, approvals, and
//! invocations.
//!
//! Each reader is intentionally narrow — it returns the most recent
//! N entries flattened to a structured DTO. Per-skill aggregation
//! lives in `crate::skills` (Skills page); per-domain detail lives
//! in the Domains pages — the Logs reader returns only the unified
//! score per snapshot (with a delta from the prior snapshot) so the
//! timeline doesn't blow up at 17-entries-per-score-run.
//!
//! Future deferred sources:
//!
//! - `<project>/.claude/brain/services.jsonl` — service start/stop
//!   events. Requires a persistence layer in `crate::services`
//!   (today's registry is in-memory only). Out of scope for the
//!   C-3 expansion.
//!
//! The `_neurogrim/notifications` source uses the existing bus
//! endpoint at `/api/brains/:id/queues/_neurogrim/notifications`
//! — no new endpoint needed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;

/// Default cap on entries returned by `/api/brains/:id/logs/
/// invocation-ledger`. Picked to match the Logs page's typical
/// rendering window without loading the entire ledger into memory
/// for adopters with a multi-month invocation history.
pub const DEFAULT_LIMIT: usize = 50;

/// Hard upper bound on entries returned by the endpoint. Caps
/// pathological `?limit=` queries so a single request can't
/// exhaust the dashboard's response buffer.
pub const MAX_LIMIT: usize = 500;

/// One entry from the invocation ledger, surfaced as a Logs
/// timeline event.
///
/// Mirrors the on-disk JSONL line shape but typed + selectively
/// projected — the ledger may grow new fields (subtype, hook
/// emitter, etc.) over time and the wire format stays tight.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct InvocationLedgerEntry {
    /// RFC3339 timestamp from the ledger row's `ts` field.
    pub ts: String,
    /// Entry type — currently always `"skill"` per
    /// `record-skill-invocation.sh`. The schema reserves the field
    /// for future hook variants (mcp_call, command, etc.).
    pub entry_type: String,
    /// Skill (or hook) name. `None` if the ledger row was
    /// malformed; we keep the row visible rather than silently
    /// dropping it so operators can spot drift.
    pub name: Option<String>,
    /// Claude Code session id when present (post-2026-04-22 ledger
    /// entries carry it; older ones may not).
    pub session_id: Option<String>,
    /// Claude Code tool-use invocation id when present.
    pub invocation_id: Option<String>,
}

/// Response body of `GET /api/brains/:id/logs/invocation-ledger`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct InvocationLedgerResponse {
    /// Path the ledger was read from. Helps operators correlate
    /// the timeline with their on-disk audit trail.
    pub ledger_path: String,
    /// True when the file exists. False = empty timeline (the
    /// invocation hook isn't installed, or the project has zero
    /// recorded skill invocations).
    pub present: bool,
    /// Total non-empty lines parsed from the ledger, before the
    /// limit was applied. Surfaces "ledger has 12,408 entries; we
    /// returned the most recent 50" telemetry to the operator.
    pub total_entries: u32,
    /// Entries newest-first, capped at the request's limit.
    pub entries: Vec<InvocationLedgerEntry>,
}

/// Read the invocation ledger and return the most-recent `limit`
/// entries (newest-first).
///
/// Invariants:
///
/// - Missing file → `present: false`, empty entries, `total_entries: 0`.
/// - Malformed lines → silently skipped from the entries vector,
///   but `total_entries` reflects every non-empty line (helps
///   operators detect drift between ledger size and parseable
///   lines).
/// - `limit` clamped to `[1, MAX_LIMIT]`.
pub fn read_invocation_ledger(
    project_root: &Path,
    limit: usize,
) -> InvocationLedgerResponse {
    let ledger_path = project_root
        .join(".claude")
        .join("brain")
        .join("invocation-ledger.jsonl");

    let limit = limit.clamp(1, MAX_LIMIT);

    let text = match std::fs::read_to_string(&ledger_path) {
        Ok(t) => t,
        Err(_) => {
            return InvocationLedgerResponse {
                ledger_path: ledger_path.to_string_lossy().to_string(),
                present: false,
                total_entries: 0,
                entries: vec![],
            };
        }
    };

    let mut total_entries: u32 = 0;
    let mut parsed: Vec<(DateTime<Utc>, InvocationLedgerEntry)> = vec![];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        total_entries = total_entries.saturating_add(1);
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ts_str = match v.get("ts").and_then(|x| x.as_str()) {
            Some(t) => t,
            None => continue,
        };
        let ts = match DateTime::parse_from_rfc3339(ts_str) {
            Ok(t) => t.with_timezone(&Utc),
            Err(_) => continue,
        };
        let entry = InvocationLedgerEntry {
            ts: ts_str.to_string(),
            entry_type: v
                .get("type")
                .and_then(|x| x.as_str())
                .unwrap_or("skill")
                .to_string(),
            name: v
                .get("name")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
            session_id: v
                .get("session_id")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
            invocation_id: v
                .get("invocation_id")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
        };
        parsed.push((ts, entry));
    }

    // Newest-first.
    parsed.sort_by(|a, b| b.0.cmp(&a.0));
    parsed.truncate(limit);
    let entries: Vec<InvocationLedgerEntry> =
        parsed.into_iter().map(|(_, e)| e).collect();

    InvocationLedgerResponse {
        ledger_path: ledger_path.to_string_lossy().to_string(),
        present: true,
        total_entries,
        entries,
    }
}

// ── S15-C-3 expansion: score-history reader ──────────────────────────────

/// One snapshot from `<project>/.claude/brain/score-history.json`,
/// projected to a Logs-timeline shape: just the unified score plus
/// a delta against the prior snapshot. Per-domain detail lives in
/// the Domains pages — the Logs reader stays terse so a Brain
/// scored 50× per day doesn't drown the timeline.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct ScoreHistoryEntry {
    /// RFC3339 timestamp from the snapshot's `scored_at` field.
    pub scored_at: String,
    /// Unified score on that snapshot (0-100, or null when the
    /// Brain is fully advisory and the snapshot recorded `null`).
    pub score: Option<i32>,
    /// Delta vs. the previous snapshot in time-ordered sequence.
    /// `None` for the oldest snapshot in the returned window
    /// (no prior to diff against). `Some(0)` for unchanged scores.
    pub delta: Option<i32>,
}

/// Response body of `GET /api/brains/:id/logs/score-history`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ScoreHistoryResponse {
    /// Resolved on-disk path. Helps operators correlate the timeline
    /// with their score-history.json.
    pub history_path: String,
    /// True when the file exists. False = empty timeline (the Brain
    /// has never been scored).
    pub present: bool,
    /// Total snapshots parsed before the limit was applied. Surfaces
    /// "history has 700 snapshots; we returned the most recent 50"
    /// telemetry without forcing the operator to inspect the file.
    pub total_entries: u32,
    /// Entries newest-first, capped at the request's limit. The
    /// `delta` field is computed against the chronologically-prior
    /// snapshot (so the oldest entry in the returned slice has a
    /// real delta when there's at least one snapshot beyond the
    /// window; `None` when the slice covers the entire history).
    pub entries: Vec<ScoreHistoryEntry>,
}

/// Read the score history and return the most-recent `limit` snapshots
/// (newest-first), each annotated with the delta against its
/// chronologically-prior snapshot.
///
/// Invariants:
///
/// - Missing file → `present: false`, empty entries.
/// - Malformed file (not a JSON array, parse error) → `present:
///   true` (the file was found) with empty entries; operators see
///   the path in the response and can investigate. Drift signal
///   without aborting the whole page.
/// - `limit` clamped to `[1, MAX_LIMIT]`.
/// - Snapshot ordering: the on-disk history is append-only (oldest
///   first); we sort by `scored_at` defensively before computing
///   deltas.
pub fn read_score_history(
    project_root: &Path,
    limit: usize,
) -> ScoreHistoryResponse {
    let history_path = project_root
        .join(".claude")
        .join("brain")
        .join("score-history.json");

    let limit = limit.clamp(1, MAX_LIMIT);

    let text = match std::fs::read_to_string(&history_path) {
        Ok(t) => t,
        Err(_) => {
            return ScoreHistoryResponse {
                history_path: history_path.to_string_lossy().to_string(),
                present: false,
                total_entries: 0,
                entries: vec![],
            };
        }
    };

    // Parse to a flexible Value so unknown fields (per-domain
    // breakdowns, future schema additions) don't fail the read.
    let parsed: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => {
            return ScoreHistoryResponse {
                history_path: history_path.to_string_lossy().to_string(),
                present: true,
                total_entries: 0,
                entries: vec![],
            };
        }
    };

    let array = match parsed.as_array() {
        Some(a) => a,
        None => {
            return ScoreHistoryResponse {
                history_path: history_path.to_string_lossy().to_string(),
                present: true,
                total_entries: 0,
                entries: vec![],
            };
        }
    };

    // Project each snapshot to (scored_at, score). Skip entries
    // missing the timestamp; missing/null scores are kept as-is so
    // observers see "score: null" episodes in the timeline (e.g.,
    // all-advisory Brains).
    let mut snapshots: Vec<(DateTime<Utc>, String, Option<i32>)> = vec![];
    let mut total_entries: u32 = 0;
    for entry in array {
        total_entries = total_entries.saturating_add(1);
        let ts_str = match entry.get("scored_at").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => continue,
        };
        let ts = match DateTime::parse_from_rfc3339(ts_str) {
            Ok(t) => t.with_timezone(&Utc),
            Err(_) => continue,
        };
        let score = entry
            .get("score")
            .and_then(|v| v.as_i64())
            .map(|n| n as i32);
        snapshots.push((ts, ts_str.to_string(), score));
    }

    // Defensive sort by timestamp — score-history.json is append-
    // only oldest-first in practice, but treating the file as
    // ordered would mask corruption.
    snapshots.sort_by(|a, b| a.0.cmp(&b.0));

    // Compute deltas vs. the chronologically-prior snapshot. The
    // oldest snapshot in the *full* history has delta=None; later
    // snapshots have delta=Some(current - prior).
    let mut prior_score: Option<i32> = None;
    let mut with_deltas: Vec<(DateTime<Utc>, ScoreHistoryEntry)> = snapshots
        .into_iter()
        .map(|(ts, ts_str, score)| {
            let delta = match (prior_score, score) {
                (Some(p), Some(c)) => Some(c - p),
                _ => None,
            };
            prior_score = score;
            let entry = ScoreHistoryEntry {
                scored_at: ts_str,
                score,
                delta,
            };
            (ts, entry)
        })
        .collect();

    // Newest-first, take the requested window.
    with_deltas.sort_by(|a, b| b.0.cmp(&a.0));
    with_deltas.truncate(limit);
    let entries: Vec<ScoreHistoryEntry> =
        with_deltas.into_iter().map(|(_, e)| e).collect();

    ScoreHistoryResponse {
        history_path: history_path.to_string_lossy().to_string(),
        present: true,
        total_entries,
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().expect("tempdir");
        let project_root = dir.path().to_path_buf();
        let brain_dir = project_root.join(".claude").join("brain");
        fs::create_dir_all(&brain_dir).expect("brain dir");
        (dir, project_root)
    }

    fn write_ledger(project_root: &Path, lines: &[&str]) {
        let path = project_root
            .join(".claude")
            .join("brain")
            .join("invocation-ledger.jsonl");
        fs::write(&path, lines.join("\n") + "\n").expect("write ledger");
    }

    #[test]
    fn missing_ledger_returns_absent_response() {
        let (_dir, root) = setup();
        let resp = read_invocation_ledger(&root, 50);
        assert!(!resp.present);
        assert_eq!(resp.total_entries, 0);
        assert!(resp.entries.is_empty());
        assert!(resp.ledger_path.ends_with("invocation-ledger.jsonl"));
    }

    #[test]
    fn empty_ledger_returns_present_with_zero_entries() {
        let (_dir, root) = setup();
        write_ledger(&root, &[]);
        let resp = read_invocation_ledger(&root, 50);
        assert!(resp.present);
        assert_eq!(resp.total_entries, 0);
        assert!(resp.entries.is_empty());
    }

    #[test]
    fn parses_well_formed_entries() {
        let (_dir, root) = setup();
        write_ledger(
            &root,
            &[
                r#"{"schema_version":"1","ts":"2026-04-29T10:00:00Z","type":"skill","name":"hats","session_id":"s1","invocation_id":"i1"}"#,
                r#"{"schema_version":"1","ts":"2026-04-29T11:00:00Z","type":"skill","name":"plan-critic","session_id":"s2","invocation_id":"i2"}"#,
            ],
        );
        let resp = read_invocation_ledger(&root, 50);
        assert!(resp.present);
        assert_eq!(resp.total_entries, 2);
        assert_eq!(resp.entries.len(), 2);
        // Newest-first ordering.
        assert_eq!(resp.entries[0].name.as_deref(), Some("plan-critic"));
        assert_eq!(resp.entries[1].name.as_deref(), Some("hats"));
        assert_eq!(resp.entries[0].entry_type, "skill");
        assert_eq!(
            resp.entries[0].session_id.as_deref(),
            Some("s2"),
        );
    }

    #[test]
    fn skips_malformed_lines_but_counts_them_in_total() {
        let (_dir, root) = setup();
        write_ledger(
            &root,
            &[
                r#"{"ts":"2026-04-29T10:00:00Z","type":"skill","name":"good"}"#,
                r#"not-json-at-all"#,
                r#"{"missing":"ts-field","name":"bad"}"#,
                r#"{"ts":"not-a-date","name":"also-bad"}"#,
            ],
        );
        let resp = read_invocation_ledger(&root, 50);
        // Total counts every non-empty line — drift signal.
        assert_eq!(resp.total_entries, 4);
        // Only the first line is parseable.
        assert_eq!(resp.entries.len(), 1);
        assert_eq!(resp.entries[0].name.as_deref(), Some("good"));
    }

    #[test]
    fn applies_limit_to_entries_but_preserves_total() {
        let (_dir, root) = setup();
        let lines: Vec<String> = (0..10)
            .map(|i| {
                format!(
                    r#"{{"schema_version":"1","ts":"2026-04-29T{:02}:00:00Z","type":"skill","name":"skill-{}"}}"#,
                    i, i
                )
            })
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        write_ledger(&root, &line_refs);

        let resp = read_invocation_ledger(&root, 3);
        assert_eq!(resp.total_entries, 10);
        assert_eq!(resp.entries.len(), 3);
        // Newest-first.
        assert_eq!(resp.entries[0].name.as_deref(), Some("skill-9"));
        assert_eq!(resp.entries[1].name.as_deref(), Some("skill-8"));
        assert_eq!(resp.entries[2].name.as_deref(), Some("skill-7"));
    }

    #[test]
    fn clamps_limit_to_max() {
        let (_dir, root) = setup();
        // Just verify the clamp doesn't panic and returns sane shape.
        write_ledger(
            &root,
            &[r#"{"ts":"2026-04-29T10:00:00Z","type":"skill","name":"x"}"#],
        );
        let resp = read_invocation_ledger(&root, 99_999_999);
        assert_eq!(resp.entries.len(), 1);
    }

    #[test]
    fn clamps_limit_to_min_one() {
        let (_dir, root) = setup();
        write_ledger(
            &root,
            &[r#"{"ts":"2026-04-29T10:00:00Z","type":"skill","name":"x"}"#],
        );
        let resp = read_invocation_ledger(&root, 0);
        // limit of 0 clamps to 1, so one entry returns.
        assert_eq!(resp.entries.len(), 1);
    }

    #[test]
    fn missing_optional_fields_render_as_none() {
        let (_dir, root) = setup();
        write_ledger(
            &root,
            &[r#"{"ts":"2026-04-29T10:00:00Z","type":"skill","name":"only-required"}"#],
        );
        let resp = read_invocation_ledger(&root, 50);
        assert_eq!(resp.entries.len(), 1);
        assert_eq!(resp.entries[0].name.as_deref(), Some("only-required"));
        assert_eq!(resp.entries[0].session_id, None);
        assert_eq!(resp.entries[0].invocation_id, None);
    }

    #[test]
    fn entry_without_name_still_surfaces_for_drift_visibility() {
        // A row with no `name` field but a valid timestamp — we keep
        // it visible (with name=None) so operators can debug schema
        // drift. The invocation hook is supposed to always emit name,
        // so this scenario indicates something is wrong upstream.
        let (_dir, root) = setup();
        write_ledger(
            &root,
            &[r#"{"ts":"2026-04-29T10:00:00Z","type":"skill"}"#],
        );
        let resp = read_invocation_ledger(&root, 50);
        assert_eq!(resp.entries.len(), 1);
        assert_eq!(resp.entries[0].name, None);
    }

    // ── score-history reader (S15-C-3 expansion) ─────────────────

    fn write_score_history(project_root: &Path, json: &str) {
        let path = project_root
            .join(".claude")
            .join("brain")
            .join("score-history.json");
        fs::write(&path, json).expect("write score-history");
    }

    #[test]
    fn missing_score_history_returns_absent_response() {
        let (_dir, root) = setup();
        let resp = read_score_history(&root, 50);
        assert!(!resp.present);
        assert_eq!(resp.total_entries, 0);
        assert!(resp.entries.is_empty());
        assert!(resp.history_path.ends_with("score-history.json"));
    }

    #[test]
    fn empty_array_returns_present_with_zero_entries() {
        let (_dir, root) = setup();
        write_score_history(&root, "[]");
        let resp = read_score_history(&root, 50);
        assert!(resp.present);
        assert_eq!(resp.total_entries, 0);
        assert!(resp.entries.is_empty());
    }

    #[test]
    fn malformed_json_returns_present_with_empty_entries() {
        // Drift signal: file exists but corrupt → operator sees
        // present=true with 0 entries; they can investigate the
        // path the response surfaces.
        let (_dir, root) = setup();
        write_score_history(&root, "not json at all");
        let resp = read_score_history(&root, 50);
        assert!(resp.present);
        assert_eq!(resp.total_entries, 0);
        assert!(resp.entries.is_empty());
    }

    #[test]
    fn computes_delta_against_chronologically_prior_snapshot() {
        let (_dir, root) = setup();
        write_score_history(
            &root,
            r#"[
              {"scored_at":"2026-04-29T10:00:00Z","score":70,"domains":{}},
              {"scored_at":"2026-04-29T11:00:00Z","score":75,"domains":{}},
              {"scored_at":"2026-04-29T12:00:00Z","score":78,"domains":{}}
            ]"#,
        );
        let resp = read_score_history(&root, 50);
        assert_eq!(resp.total_entries, 3);
        // Newest-first.
        assert_eq!(resp.entries[0].scored_at, "2026-04-29T12:00:00Z");
        assert_eq!(resp.entries[0].score, Some(78));
        assert_eq!(resp.entries[0].delta, Some(3)); // 78 - 75
        assert_eq!(resp.entries[1].score, Some(75));
        assert_eq!(resp.entries[1].delta, Some(5)); // 75 - 70
        // Oldest in the full history → no prior to diff against.
        assert_eq!(resp.entries[2].score, Some(70));
        assert_eq!(resp.entries[2].delta, None);
    }

    #[test]
    fn delta_uses_full_history_not_just_returned_window() {
        // When the window is smaller than the history, the oldest
        // returned entry should still have a delta computed from
        // the snapshot prior to it (in the full history, not the
        // window). Validates that we sort + diff first, then
        // truncate.
        let (_dir, root) = setup();
        write_score_history(
            &root,
            r#"[
              {"scored_at":"2026-04-29T10:00:00Z","score":70,"domains":{}},
              {"scored_at":"2026-04-29T11:00:00Z","score":75,"domains":{}},
              {"scored_at":"2026-04-29T12:00:00Z","score":78,"domains":{}},
              {"scored_at":"2026-04-29T13:00:00Z","score":80,"domains":{}}
            ]"#,
        );
        // Limit = 2 → two newest entries, oldest of those (the
        // 12:00 snapshot) should have delta vs. 11:00 snapshot.
        let resp = read_score_history(&root, 2);
        assert_eq!(resp.total_entries, 4);
        assert_eq!(resp.entries.len(), 2);
        assert_eq!(resp.entries[0].scored_at, "2026-04-29T13:00:00Z");
        assert_eq!(resp.entries[0].delta, Some(2)); // 80 - 78
        assert_eq!(resp.entries[1].scored_at, "2026-04-29T12:00:00Z");
        assert_eq!(resp.entries[1].delta, Some(3)); // 78 - 75
    }

    #[test]
    fn null_score_is_preserved_with_no_delta() {
        // All-advisory Brains record `score: null`. Both the score
        // and any delta involving null should round-trip as null.
        let (_dir, root) = setup();
        write_score_history(
            &root,
            r#"[
              {"scored_at":"2026-04-29T10:00:00Z","score":null,"domains":{}},
              {"scored_at":"2026-04-29T11:00:00Z","score":75,"domains":{}}
            ]"#,
        );
        let resp = read_score_history(&root, 50);
        assert_eq!(resp.entries[0].score, Some(75));
        // Prior snapshot was null → no delta.
        assert_eq!(resp.entries[0].delta, None);
        assert_eq!(resp.entries[1].score, None);
        assert_eq!(resp.entries[1].delta, None);
    }

    #[test]
    fn unsorted_history_is_re_sorted_by_timestamp() {
        // Defensive: a corrupted on-disk file written out of order
        // should still produce a chronologically-sane delta sequence.
        let (_dir, root) = setup();
        write_score_history(
            &root,
            r#"[
              {"scored_at":"2026-04-29T12:00:00Z","score":78,"domains":{}},
              {"scored_at":"2026-04-29T10:00:00Z","score":70,"domains":{}},
              {"scored_at":"2026-04-29T11:00:00Z","score":75,"domains":{}}
            ]"#,
        );
        let resp = read_score_history(&root, 50);
        // Returned newest-first regardless of file order.
        assert_eq!(resp.entries[0].scored_at, "2026-04-29T12:00:00Z");
        assert_eq!(resp.entries[0].delta, Some(3));
        assert_eq!(resp.entries[1].scored_at, "2026-04-29T11:00:00Z");
        assert_eq!(resp.entries[1].delta, Some(5));
        assert_eq!(resp.entries[2].scored_at, "2026-04-29T10:00:00Z");
        assert_eq!(resp.entries[2].delta, None);
    }

    #[test]
    fn limit_clamps_to_max_and_min() {
        let (_dir, root) = setup();
        write_score_history(
            &root,
            r#"[{"scored_at":"2026-04-29T10:00:00Z","score":70,"domains":{}}]"#,
        );
        // Pathological limit clamps to MAX_LIMIT.
        let resp_max = read_score_history(&root, 99_999_999);
        assert_eq!(resp_max.entries.len(), 1);
        // Zero limit clamps to 1.
        let resp_zero = read_score_history(&root, 0);
        assert_eq!(resp_zero.entries.len(), 1);
    }

    #[test]
    fn entries_missing_timestamps_skipped_but_counted_in_total() {
        let (_dir, root) = setup();
        write_score_history(
            &root,
            r#"[
              {"scored_at":"2026-04-29T10:00:00Z","score":70,"domains":{}},
              {"score":75,"domains":{}},
              {"scored_at":"not-a-date","score":80,"domains":{}}
            ]"#,
        );
        let resp = read_score_history(&root, 50);
        // Total reflects every array element — drift signal.
        assert_eq!(resp.total_entries, 3);
        // Only the well-formed entry parses.
        assert_eq!(resp.entries.len(), 1);
    }

    #[test]
    fn non_array_root_returns_empty_entries() {
        // Defensive: if score-history.json was somehow written as a
        // single object (schema migration gone wrong, etc.), don't
        // crash — return empty.
        let (_dir, root) = setup();
        write_score_history(&root, r#"{"not": "an array"}"#);
        let resp = read_score_history(&root, 50);
        assert!(resp.present);
        assert_eq!(resp.total_entries, 0);
        assert!(resp.entries.is_empty());
    }
}
