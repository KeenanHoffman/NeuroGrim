//! v4.3 S15-C-2 v2 — log-source readers for the Logs page.
//!
//! The Logs page aggregates events from multiple ledgers into a
//! single timeline. v1 covered publish-gates + approvals (already
//! exposed via `/api/brains/:id/publish-gates` and `/api/brains/
//! :id/approvals`). This module adds the reader for the third
//! source landing in v2: the invocation ledger.
//!
//! The reader is intentionally narrow — it returns the most recent
//! N entries flattened to a structured DTO. Per-skill aggregation
//! lives in `crate::skills` (Skills page); this is the raw timeline
//! view.
//!
//! Future v3 (deferred) sources:
//!
//! - `score-history.json` — diff snapshots into per-domain "score
//!   changed by Δ" entries (needs threshold tuning).
//! - `<project>/.claude/brain/services.jsonl` — service start/stop
//!   events. Requires a persistence layer in `crate::services`
//!   (today's registry is in-memory only).
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
}
