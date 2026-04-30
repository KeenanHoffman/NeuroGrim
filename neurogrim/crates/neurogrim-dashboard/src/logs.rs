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
/// Reads from the `_neurogrim/skill-invocations` SQLite bus topic,
/// which is lazily caught up from the canonical
/// `invocation-ledger.jsonl` (the shell hook's append target) on
/// each call. See `neurogrim_core::skill_invocations` for the
/// hybrid pattern rationale.
///
/// Invariants:
///
/// - JSONL absent + SQLite empty → `present: false`, no entries.
/// - JSONL malformed lines → silently skipped from `entries`. The
///   `total_entries` count reflects what's parseable (i.e., what's
///   in SQLite), not the raw JSONL line count — drift visibility
///   moved to operator inspection of the JSONL via `cat`.
/// - `limit` clamped to `[1, MAX_LIMIT]`.
pub fn read_invocation_ledger(
    project_root: &Path,
    limit: usize,
) -> InvocationLedgerResponse {
    use neurogrim_core::queue_backend::QueueBackend;
    use neurogrim_core::skill_invocations;

    let limit = limit.clamp(1, MAX_LIMIT);
    let sqlite_path = skill_invocations::topic_sqlite_path(project_root);
    let jsonl_path = skill_invocations::jsonl_path(project_root);

    // If neither the SQLite topic nor the canonical JSONL exists,
    // the project has never recorded a skill invocation.
    if !sqlite_path.exists() && !jsonl_path.exists() {
        return InvocationLedgerResponse {
            ledger_path: jsonl_path.to_string_lossy().to_string(),
            present: false,
            total_entries: 0,
            entries: vec![],
        };
    }

    let backend = match skill_invocations::ingest_and_open(project_root) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("skill-invocations ingest_and_open failed: {e}");
            return InvocationLedgerResponse {
                ledger_path: jsonl_path.to_string_lossy().to_string(),
                present: true,
                total_entries: 0,
                entries: vec![],
            };
        }
    };

    let total = backend.len().unwrap_or(0);
    // Bounded read: take the last `limit` rows (SQLite ROWID is
    // monotonic from 1; if total > limit, start at total - limit + 1).
    let start_offset = if total > limit as u64 {
        total - limit as u64 + 1
    } else {
        1
    };
    let msgs = match backend.read_from(start_offset, limit) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("skill-invocations read_from failed: {e}");
            return InvocationLedgerResponse {
                ledger_path: jsonl_path.to_string_lossy().to_string(),
                present: true,
                total_entries: total as u32,
                entries: vec![],
            };
        }
    };

    let mut parsed: Vec<(DateTime<Utc>, InvocationLedgerEntry)> = msgs
        .iter()
        .filter_map(|m| {
            let v = &m.message.payload;
            let ts_str = v.get("ts").and_then(|x| x.as_str())?;
            let ts = DateTime::parse_from_rfc3339(ts_str)
                .ok()?
                .with_timezone(&Utc);
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
            Some((ts, entry))
        })
        .collect();

    // Newest-first.
    parsed.sort_by(|a, b| b.0.cmp(&a.0));
    parsed.truncate(limit);
    let entries: Vec<InvocationLedgerEntry> =
        parsed.into_iter().map(|(_, e)| e).collect();

    InvocationLedgerResponse {
        ledger_path: sqlite_path.to_string_lossy().to_string(),
        present: true,
        total_entries: total as u32,
        entries,
    }
}

// ── S15-C-3 expansion: score-history reader (migrated to bus SQLite) ─────

/// One snapshot from the `_neurogrim/score-snapshots` bus topic,
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
    /// Resolved on-disk path. Points to the SQLite bus topic file
    /// (`.claude/brain/queues/_neurogrim/score-snapshots.sqlite`)
    /// for projects running the current binary, or to the legacy
    /// `score-history.json` for projects that haven't scored yet
    /// under the new binary.
    pub history_path: String,
    /// True when the backing store exists. False = empty timeline
    /// (the Brain has never been scored).
    pub present: bool,
    /// Total snapshots in the store before the limit was applied.
    /// Surfaces "history has 700 snapshots; we returned the most
    /// recent 50" telemetry without forcing the operator to inspect
    /// the file.
    pub total_entries: u32,
    /// Entries newest-first, capped at the request's limit. The
    /// `delta` field is computed against the chronologically-prior
    /// snapshot (so the oldest entry in the returned slice has a
    /// real delta when there's at least one snapshot beyond the
    /// window; `None` when the slice covers the entire history).
    pub entries: Vec<ScoreHistoryEntry>,
}

/// Read the score history and return the most-recent `limit` snapshots
/// newest-first. Primary source is the `_neurogrim/score-snapshots`
/// SQLite bus topic; falls back to the legacy `score-history.json` for
/// projects that haven't yet run `neurogrim score` under the current
/// binary.
///
/// Reads `limit + 1` rows so the oldest entry in the returned window
/// has a real delta (computed against the row just before the window),
/// then drops the extra row from the response.
///
/// - Returns `present: false` when neither backing store exists.
/// - Returns `present: true, entries: []` for an empty or corrupt store.
/// - `limit` is clamped to `[1, MAX_LIMIT]`.
pub fn read_score_history(project_root: &Path, limit: usize) -> ScoreHistoryResponse {
    let limit = limit.clamp(1, MAX_LIMIT);
    let sqlite_path = project_root
        .join(".claude")
        .join("brain")
        .join("queues")
        .join("_neurogrim")
        .join("score-snapshots.sqlite");

    if sqlite_path.exists() {
        return read_score_history_sqlite(&sqlite_path, limit);
    }

    // Legacy fallback: project hasn't run `neurogrim score` under
    // the new binary yet — read the old JSON array.
    read_score_history_json(project_root, limit)
}

/// Read from the SQLite bus topic: efficient O(log N) seek + bounded
/// read regardless of total history size.
fn read_score_history_sqlite(sqlite_path: &Path, limit: usize) -> ScoreHistoryResponse {
    use neurogrim_core::queue_backend::{QueueBackend, SqliteBackend};

    let history_path = sqlite_path.to_string_lossy().to_string();

    let backend = match SqliteBackend::open(sqlite_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("score-snapshots SQLite open failed: {e}");
            return ScoreHistoryResponse {
                history_path,
                present: true,
                total_entries: 0,
                entries: vec![],
            };
        }
    };

    let total = match backend.len() {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("score-snapshots len() failed: {e}");
            return ScoreHistoryResponse {
                history_path,
                present: true,
                total_entries: 0,
                entries: vec![],
            };
        }
    };

    // Read limit+1 rows: the extra row gives us the delta prior for
    // the oldest entry in the display window. SQLite offsets are
    // 1-based ROWID values (AUTOINCREMENT). With N total rows and a
    // window of `limit`, start at row N-limit so we capture one row
    // before the window for the delta, then discard it from the
    // response.
    let read_count = limit + 1;
    let start_offset = if total > limit as u64 {
        total - limit as u64 // one row before the window
    } else {
        1
    };
    let msgs = match backend.read_from(start_offset, read_count) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("score-snapshots read_from({start_offset}, {read_count}) failed: {e}");
            return ScoreHistoryResponse {
                history_path,
                present: true,
                total_entries: total as u32,
                entries: vec![],
            };
        }
    };

    let now = Utc::now();
    // Project payloads to (timestamp, score_i32). Skip expired entries;
    // keep entries with null/missing score so advisory Brains appear
    // in the timeline.
    let mut snapshots: Vec<(DateTime<Utc>, String, Option<i32>)> = msgs
        .iter()
        .filter(|m| !m.message.is_expired(now))
        .filter_map(|m| {
            let ts_str = m.message.payload.get("scored_at")?.as_str()?;
            let ts = DateTime::parse_from_rfc3339(ts_str)
                .ok()?
                .with_timezone(&Utc);
            let score = m
                .message
                .payload
                .get("score")
                .and_then(|v| v.as_i64())
                .map(|n| n as i32);
            Some((ts, ts_str.to_string(), score))
        })
        .collect();

    // Messages come back ASC from read_from; defensive sort for any
    // clock skew in migrated historical data.
    snapshots.sort_by(|a, b| a.0.cmp(&b.0));

    // Compute deltas chronologically (oldest → newest).
    let mut prior_score: Option<i32> = None;
    let mut with_deltas: Vec<(DateTime<Utc>, ScoreHistoryEntry)> = snapshots
        .into_iter()
        .map(|(ts, ts_str, score)| {
            let delta = match (prior_score, score) {
                (Some(p), Some(c)) => Some(c - p),
                _ => None,
            };
            prior_score = score;
            (ts, ScoreHistoryEntry { scored_at: ts_str, score, delta })
        })
        .collect();

    // Return newest-first, capped at limit (drops the extra "prior"
    // row we fetched for delta computation).
    with_deltas.sort_by(|a, b| b.0.cmp(&a.0));
    with_deltas.truncate(limit);
    let entries: Vec<ScoreHistoryEntry> = with_deltas.into_iter().map(|(_, e)| e).collect();

    ScoreHistoryResponse {
        history_path,
        present: true,
        total_entries: total as u32,
        entries,
    }
}

/// Legacy reader: JSON array at `.claude/brain/score-history.json`.
/// Used as a fallback when the SQLite topic hasn't been created yet
/// (project hasn't scored under the new binary). Reads the entire
/// file — the O(N) path we're migrating away from.
fn read_score_history_json(project_root: &Path, limit: usize) -> ScoreHistoryResponse {
    let history_path = project_root
        .join(".claude")
        .join("brain")
        .join("score-history.json");
    let display_path = history_path.to_string_lossy().to_string();

    let text = match std::fs::read_to_string(&history_path) {
        Ok(t) => t,
        Err(_) => {
            return ScoreHistoryResponse {
                history_path: display_path,
                present: false,
                total_entries: 0,
                entries: vec![],
            };
        }
    };

    let parsed: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => {
            return ScoreHistoryResponse {
                history_path: display_path,
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
                history_path: display_path,
                present: true,
                total_entries: 0,
                entries: vec![],
            };
        }
    };

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

    snapshots.sort_by(|a, b| a.0.cmp(&b.0));

    let mut prior_score: Option<i32> = None;
    let mut with_deltas: Vec<(DateTime<Utc>, ScoreHistoryEntry)> = snapshots
        .into_iter()
        .map(|(ts, ts_str, score)| {
            let delta = match (prior_score, score) {
                (Some(p), Some(c)) => Some(c - p),
                _ => None,
            };
            prior_score = score;
            (ts, ScoreHistoryEntry { scored_at: ts_str, score, delta })
        })
        .collect();

    with_deltas.sort_by(|a, b| b.0.cmp(&a.0));
    with_deltas.truncate(limit);
    let entries: Vec<ScoreHistoryEntry> = with_deltas.into_iter().map(|(_, e)| e).collect();

    ScoreHistoryResponse {
        history_path: display_path,
        present: true,
        total_entries,
        entries,
    }
}

// ── S15-C-3 expansion follow-on: services.jsonl reader ──────────────────

/// Response body of `GET /api/brains/:id/logs/services`.
///
/// Backed by the `_neurogrim/services` SQLite bus topic since v4.4
/// (was `services.jsonl` previously). Falls back to the legacy JSONL
/// for projects that haven't yet emitted a service event under the
/// new binary.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ServicesLogResponse {
    pub log_path: String,
    pub present: bool,
    pub total_entries: u32,
    pub entries: Vec<crate::services::ServiceLogEntry>,
}

/// Read the services ledger and return the most-recent `limit`
/// entries (newest-first).
///
/// Primary source is the `_neurogrim/services` SQLite bus topic;
/// falls back to the legacy `services.jsonl` for projects that
/// haven't yet emitted a service event under the current binary.
///
/// Invariants:
/// - Missing store → `present: false`, empty entries.
/// - Malformed entries silently skipped from `entries` but counted in
///   `total_entries` for drift visibility.
/// - `limit` clamped to `[1, MAX_LIMIT]`.
pub fn read_services_log(project_root: &Path, limit: usize) -> ServicesLogResponse {
    let limit = limit.clamp(1, MAX_LIMIT);
    let sqlite_path = crate::services::services_topic_sqlite_path(project_root);

    if sqlite_path.exists() {
        return read_services_log_sqlite(&sqlite_path, limit);
    }

    // Legacy fallback: project hasn't emitted a service event under
    // the new binary yet — read the old JSONL.
    read_services_log_jsonl(project_root, limit)
}

fn read_services_log_sqlite(sqlite_path: &Path, limit: usize) -> ServicesLogResponse {
    use neurogrim_core::queue_backend::{QueueBackend, SqliteBackend};

    let log_path = sqlite_path.to_string_lossy().to_string();

    let backend = match SqliteBackend::open(sqlite_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("services topic SQLite open failed: {e}");
            return ServicesLogResponse {
                log_path,
                present: true,
                total_entries: 0,
                entries: vec![],
            };
        }
    };
    let total = backend.len().unwrap_or(0);

    // Bounded read of the most recent `limit` rows.
    let start_offset = if total > limit as u64 {
        total - limit as u64 + 1
    } else {
        1
    };
    let msgs = match backend.read_from(start_offset, limit) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("services topic read_from failed: {e}");
            return ServicesLogResponse {
                log_path,
                present: true,
                total_entries: total as u32,
                entries: vec![],
            };
        }
    };

    let mut parsed: Vec<(DateTime<Utc>, crate::services::ServiceLogEntry)> = msgs
        .iter()
        .filter_map(|m| {
            let entry: crate::services::ServiceLogEntry =
                serde_json::from_value(m.message.payload.clone()).ok()?;
            let ts = DateTime::parse_from_rfc3339(&entry.ts)
                .ok()?
                .with_timezone(&Utc);
            Some((ts, entry))
        })
        .collect();
    parsed.sort_by(|a, b| b.0.cmp(&a.0));
    parsed.truncate(limit);
    let entries: Vec<crate::services::ServiceLogEntry> =
        parsed.into_iter().map(|(_, e)| e).collect();

    ServicesLogResponse {
        log_path,
        present: true,
        total_entries: total as u32,
        entries,
    }
}

fn read_services_log_jsonl(project_root: &Path, limit: usize) -> ServicesLogResponse {
    let log_path = crate::services::services_log_path(project_root);
    let display_path = log_path.to_string_lossy().to_string();

    let text = match std::fs::read_to_string(&log_path) {
        Ok(t) => t,
        Err(_) => {
            return ServicesLogResponse {
                log_path: display_path,
                present: false,
                total_entries: 0,
                entries: vec![],
            };
        }
    };

    let mut total_entries: u32 = 0;
    let mut parsed: Vec<(DateTime<Utc>, crate::services::ServiceLogEntry)> = vec![];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        total_entries = total_entries.saturating_add(1);
        let entry: crate::services::ServiceLogEntry = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ts = match DateTime::parse_from_rfc3339(&entry.ts) {
            Ok(t) => t.with_timezone(&Utc),
            Err(_) => continue,
        };
        parsed.push((ts, entry));
    }
    parsed.sort_by(|a, b| b.0.cmp(&a.0));
    parsed.truncate(limit);
    let entries: Vec<crate::services::ServiceLogEntry> =
        parsed.into_iter().map(|(_, e)| e).collect();

    ServicesLogResponse {
        log_path: display_path,
        present: true,
        total_entries,
        entries,
    }
}

// ── S15-C-2 expansion: per-peer log tail reader ─────────────────────────

/// Maximum bytes read from the end of a peer log file. Caps memory +
/// response payload size regardless of how long the peer has been
/// running. 256 KB comfortably holds thousands of log lines while
/// keeping `GET /peers/:peer_name/log` cheap.
pub const PEER_LOG_TAIL_BYTES: u64 = 256 * 1024;

/// Hard upper bound on `?lines=N`. The on-disk log might have more
/// lines in the tail window than the operator wants to render — this
/// caps the count at a level that keeps the modal manageable.
pub const PEER_LOG_MAX_LINES: usize = 2000;

/// Default lines returned when `?lines=` is omitted.
pub const PEER_LOG_DEFAULT_LINES: usize = 200;

/// Response body of `GET /api/brains/:id/peers/:peer_name/log`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PeerLogResponse {
    /// Resolved on-disk path. Helps operators correlate the modal
    /// with what they'd see in `tail -f` outside the dashboard.
    pub log_path: String,
    pub present: bool,
    /// Total file size in bytes. `None` when `present: false`.
    /// Surfaces "the tail you see is the last 256 KB of a 12 MB
    /// file" telemetry without requiring a full read.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_size_bytes: Option<u64>,
    /// True when [`PEER_LOG_TAIL_BYTES`] truncation discarded older
    /// content. The frontend can hint that the operator is seeing
    /// only the most recent slice.
    pub truncated: bool,
    /// Lines, oldest-first within the returned slice. May contain a
    /// partial leading line stripped (we discard the first line
    /// when seeking into the middle of one) so all lines are whole.
    pub lines: Vec<String>,
}

/// Read the trailing portion of a peer log file as line-bounded
/// chunks. Reads only the last [`PEER_LOG_TAIL_BYTES`] from the
/// file regardless of total size, then returns the most-recent N
/// lines from that window.
///
/// Invariants:
/// - Missing file → `present: false`, empty lines.
/// - Read errors (permissions, etc.) → `present: false`, empty
///   lines. Partial-failure modes don't surface to the operator;
///   the path is included so manual investigation is one step away.
/// - Non-UTF-8 bytes → replaced with U+FFFD via
///   `String::from_utf8_lossy`. Operators get readable output even
///   when the peer emits raw binary stderr.
/// - `lines` clamped to `[1, PEER_LOG_MAX_LINES]`.
/// - When the file is larger than [`PEER_LOG_TAIL_BYTES`], the first
///   line in the read window is discarded (it might be a fragment),
///   and `truncated` is set so the UI can hint about the window.
pub fn read_peer_log_tail(log_path: &Path, lines: usize) -> PeerLogResponse {
    use std::io::{Read, Seek, SeekFrom};
    let lines = lines.clamp(1, PEER_LOG_MAX_LINES);
    let metadata = match std::fs::metadata(log_path) {
        Ok(m) => m,
        Err(_) => {
            return PeerLogResponse {
                log_path: log_path.to_string_lossy().to_string(),
                present: false,
                total_size_bytes: None,
                truncated: false,
                lines: vec![],
            };
        }
    };
    let total_size = metadata.len();
    let read_from = total_size.saturating_sub(PEER_LOG_TAIL_BYTES);
    let truncated = read_from > 0;

    let mut file = match std::fs::File::open(log_path) {
        Ok(f) => f,
        Err(_) => {
            return PeerLogResponse {
                log_path: log_path.to_string_lossy().to_string(),
                present: false,
                total_size_bytes: Some(total_size),
                truncated: false,
                lines: vec![],
            };
        }
    };
    if read_from > 0 {
        if file.seek(SeekFrom::Start(read_from)).is_err() {
            return PeerLogResponse {
                log_path: log_path.to_string_lossy().to_string(),
                present: false,
                total_size_bytes: Some(total_size),
                truncated: false,
                lines: vec![],
            };
        }
    }
    let mut buf = Vec::with_capacity(PEER_LOG_TAIL_BYTES.min(total_size) as usize);
    if file.read_to_end(&mut buf).is_err() {
        return PeerLogResponse {
            log_path: log_path.to_string_lossy().to_string(),
            present: false,
            total_size_bytes: Some(total_size),
            truncated: false,
            lines: vec![],
        };
    }
    // Lossy-decode so non-UTF-8 bytes don't fail the whole read.
    let text = String::from_utf8_lossy(&buf).into_owned();
    // Drop the partial first line when we seeked into the middle of
    // a line — otherwise the operator sees a fragment at the top of
    // the tail.
    let aligned = if truncated {
        match text.find('\n') {
            Some(idx) => text[idx + 1..].to_string(),
            None => String::new(), // entire window was a single partial line
        }
    } else {
        text
    };
    let all_lines: Vec<&str> = aligned.lines().collect();
    let start = all_lines.len().saturating_sub(lines);
    let tail: Vec<String> = all_lines[start..].iter().map(|s| s.to_string()).collect();

    PeerLogResponse {
        log_path: log_path.to_string_lossy().to_string(),
        present: true,
        total_size_bytes: Some(total_size),
        truncated,
        lines: tail,
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
        // Post-bus-migration: total_entries reflects rows in the
        // SQLite topic, which holds every line that parses as JSON
        // (including ones with bad/missing required fields). The
        // fully-malformed `not-json-at-all` line is dropped at the
        // ingest boundary, so total = 3 not 4. Operator drift signal
        // for fully-malformed lines moved to direct `wc -l` inspection
        // of the JSONL canonical store.
        assert_eq!(resp.total_entries, 3);
        // Only the first line is fully parseable as a ledger entry.
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

    // ── services.jsonl reader (S15-C-3 expansion follow-on) ─────

    fn write_services_log(project_root: &Path, lines: &[&str]) {
        let path = crate::services::services_log_path(project_root);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, lines.join("\n") + "\n").expect("write services log");
    }

    #[test]
    fn missing_services_log_returns_absent_response() {
        let (_dir, root) = setup();
        let resp = read_services_log(&root, 50);
        assert!(!resp.present);
        assert_eq!(resp.total_entries, 0);
        assert!(resp.entries.is_empty());
        assert!(resp.log_path.ends_with("services.jsonl"));
    }

    #[test]
    fn parses_started_failed_stopped_entries() {
        let (_dir, root) = setup();
        write_services_log(
            &root,
            &[
                r#"{"ts":"2026-04-30T10:00:00Z","kind":"started","peer_name":"alpha","pid":1234,"port":8421}"#,
                r#"{"ts":"2026-04-30T10:05:00Z","kind":"failed","peer_name":"beta","reason":"port-conflict: port 8422 already bound"}"#,
                r#"{"ts":"2026-04-30T10:10:00Z","kind":"stopped","peer_name":"alpha","pid":1234}"#,
            ],
        );
        let resp = read_services_log(&root, 50);
        assert_eq!(resp.total_entries, 3);
        assert_eq!(resp.entries.len(), 3);
        // Newest-first.
        assert_eq!(resp.entries[0].kind, "stopped");
        assert_eq!(resp.entries[0].peer_name, "alpha");
        assert_eq!(resp.entries[0].pid, Some(1234));
        assert_eq!(resp.entries[1].kind, "failed");
        assert_eq!(resp.entries[1].peer_name, "beta");
        assert!(resp.entries[1]
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("port-conflict"));
        assert_eq!(resp.entries[2].kind, "started");
        assert_eq!(resp.entries[2].port, Some(8421));
    }

    #[test]
    fn malformed_lines_skipped_but_counted_in_total() {
        let (_dir, root) = setup();
        write_services_log(
            &root,
            &[
                r#"{"ts":"2026-04-30T10:00:00Z","kind":"started","peer_name":"a","pid":1,"port":8421}"#,
                r#"not-json-at-all"#,
                r#"{"missing":"ts","kind":"failed","peer_name":"b"}"#,
                r#"{"ts":"not-a-date","kind":"stopped","peer_name":"c","pid":2}"#,
            ],
        );
        let resp = read_services_log(&root, 50);
        assert_eq!(resp.total_entries, 4);
        assert_eq!(resp.entries.len(), 1);
        assert_eq!(resp.entries[0].peer_name, "a");
    }

    #[test]
    fn services_log_limit_clamps_and_returns_newest_first() {
        let (_dir, root) = setup();
        let mut lines: Vec<String> = Vec::new();
        for i in 0..10u32 {
            lines.push(format!(
                r#"{{"ts":"2026-04-30T{:02}:00:00Z","kind":"started","peer_name":"peer-{}","pid":{},"port":{}}}"#,
                i,
                i,
                1000 + i,
                8421 + i
            ));
        }
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        write_services_log(&root, &line_refs);

        let resp = read_services_log(&root, 3);
        assert_eq!(resp.total_entries, 10);
        assert_eq!(resp.entries.len(), 3);
        assert_eq!(resp.entries[0].peer_name, "peer-9");
        assert_eq!(resp.entries[1].peer_name, "peer-8");
        assert_eq!(resp.entries[2].peer_name, "peer-7");
    }

    #[test]
    fn services_log_round_trip_through_append_helpers() {
        // Integration: the on-disk file the helpers write should be
        // readable by the reader without manual format coordination.
        let (_dir, root) = setup();
        crate::services::log_service_started(&root, "alpha", 1234, 8421);
        crate::services::log_service_failed(
            &root,
            "beta",
            "port-conflict: port 8422 already bound",
            None,
        );
        crate::services::log_service_stopped(&root, "alpha", 1234);
        let resp = read_services_log(&root, 50);
        assert_eq!(resp.total_entries, 3);
        assert_eq!(resp.entries.len(), 3);
        // Newest-first; the stopped event is last appended.
        assert_eq!(resp.entries[0].kind, "stopped");
        assert_eq!(resp.entries[0].peer_name, "alpha");
        assert_eq!(resp.entries[1].kind, "failed");
        assert_eq!(resp.entries[1].peer_name, "beta");
        // No PID on a port-conflict pre-spawn failure.
        assert_eq!(resp.entries[1].pid, None);
        assert_eq!(resp.entries[2].kind, "started");
        assert_eq!(resp.entries[2].port, Some(8421));
    }

    #[test]
    fn append_creates_parent_directories_when_absent() {
        // Regression guard: helpers should create
        // `<project>/.claude/brain/` if the operator has a fresh
        // project tree without those directories yet.
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        // No .claude/brain pre-existing.
        crate::services::log_service_started(&root, "gamma", 1, 8421);
        let resp = read_services_log(&root, 50);
        assert!(resp.present);
        assert_eq!(resp.entries.len(), 1);
        assert_eq!(resp.entries[0].peer_name, "gamma");
    }

    // ── peer log tail reader (S15-C-2 expansion) ─────────────────

    fn write_peer_log(
        dir: &TempDir,
        peer: &str,
        content: &str,
    ) -> std::path::PathBuf {
        let log_dir = dir.path().join(".claude").join("brain").join("logs");
        fs::create_dir_all(&log_dir).expect("log dir");
        let path = log_dir.join(format!("{peer}.log"));
        fs::write(&path, content).expect("write peer log");
        path
    }

    #[test]
    fn missing_peer_log_returns_absent_response() {
        let dir = TempDir::new().unwrap();
        let path = dir
            .path()
            .join(".claude")
            .join("brain")
            .join("logs")
            .join("alpha.log");
        let resp = read_peer_log_tail(&path, 200);
        assert!(!resp.present);
        assert_eq!(resp.total_size_bytes, None);
        assert!(!resp.truncated);
        assert!(resp.lines.is_empty());
        assert!(resp.log_path.ends_with("alpha.log"));
    }

    #[test]
    fn small_peer_log_returns_all_lines_untruncated() {
        let dir = TempDir::new().unwrap();
        let path = write_peer_log(&dir, "alpha", "line 1\nline 2\nline 3\n");
        let resp = read_peer_log_tail(&path, 200);
        assert!(resp.present);
        assert_eq!(resp.total_size_bytes, Some(21));
        assert!(!resp.truncated);
        assert_eq!(resp.lines, vec!["line 1", "line 2", "line 3"]);
    }

    #[test]
    fn lines_clamps_to_most_recent() {
        let dir = TempDir::new().unwrap();
        let mut content = String::new();
        for i in 0..50 {
            content.push_str(&format!("line {i}\n"));
        }
        let path = write_peer_log(&dir, "alpha", &content);
        let resp = read_peer_log_tail(&path, 5);
        assert_eq!(resp.lines.len(), 5);
        assert_eq!(resp.lines[0], "line 45");
        assert_eq!(resp.lines[4], "line 49");
    }

    #[test]
    fn lines_clamps_to_max_limit() {
        let dir = TempDir::new().unwrap();
        let path = write_peer_log(&dir, "alpha", "x\n");
        let resp = read_peer_log_tail(&path, 99_999_999);
        // Only one real line; clamp shouldn't error.
        assert_eq!(resp.lines, vec!["x"]);
    }

    #[test]
    fn lines_clamps_zero_to_one() {
        let dir = TempDir::new().unwrap();
        let path = write_peer_log(&dir, "alpha", "a\nb\nc\n");
        let resp = read_peer_log_tail(&path, 0);
        // 0 → 1; returns the single most-recent line.
        assert_eq!(resp.lines, vec!["c"]);
    }

    #[test]
    fn large_peer_log_truncates_and_drops_partial_first_line() {
        // Build a log that's larger than PEER_LOG_TAIL_BYTES.
        let dir = TempDir::new().unwrap();
        // ~512 KB total: ensures the read window of 256 KB starts
        // somewhere in the middle of a line.
        let mut content = String::with_capacity(600_000);
        let line_count = 12_000;
        for i in 0..line_count {
            // Long-ish lines (~50 bytes each) so the window cuts
            // through one.
            content.push_str(&format!(
                "line-{i:08}-{}\n",
                "x".repeat(40)
            ));
        }
        let path = write_peer_log(&dir, "alpha", &content);
        let total_size = std::fs::metadata(&path).unwrap().len();
        assert!(total_size > PEER_LOG_TAIL_BYTES);

        let resp = read_peer_log_tail(&path, 50);
        assert!(resp.present);
        assert_eq!(resp.total_size_bytes, Some(total_size));
        assert!(resp.truncated);
        assert_eq!(resp.lines.len(), 50);
        // Most recent line is the last one written.
        let last = format!("line-{:08}-{}", line_count - 1, "x".repeat(40));
        assert_eq!(resp.lines.last().unwrap(), &last);
        // Every returned line is whole — none of them start with an
        // incomplete fragment.
        for line in &resp.lines {
            assert!(
                line.starts_with("line-"),
                "line is a fragment, not whole: {line:?}"
            );
        }
    }

    #[test]
    fn non_utf8_bytes_lossy_decoded() {
        // A peer might emit raw bytes (binary stderr, rotated
        // buffers, etc.). The reader should not crash; bytes that
        // aren't valid UTF-8 become U+FFFD.
        let dir = TempDir::new().unwrap();
        let log_dir = dir.path().join(".claude").join("brain").join("logs");
        fs::create_dir_all(&log_dir).unwrap();
        let path = log_dir.join("alpha.log");
        // Valid UTF-8 line + invalid sequence + another valid line.
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"valid line\n");
        bytes.extend_from_slice(&[0xFF, 0xFE, 0xFD]); // invalid UTF-8
        bytes.extend_from_slice(b"\nrecovery line\n");
        std::fs::write(&path, &bytes).unwrap();

        let resp = read_peer_log_tail(&path, 200);
        assert!(resp.present);
        // 3 lines: valid, invalid-decoded, recovery.
        assert_eq!(resp.lines.len(), 3);
        assert_eq!(resp.lines[0], "valid line");
        assert_eq!(resp.lines[2], "recovery line");
        // The middle line decodes lossy — should contain the
        // replacement char rather than panicking.
        assert!(resp.lines[1].contains('\u{FFFD}'));
    }

    #[test]
    fn empty_peer_log_returns_present_with_no_lines() {
        let dir = TempDir::new().unwrap();
        let path = write_peer_log(&dir, "alpha", "");
        let resp = read_peer_log_tail(&path, 200);
        assert!(resp.present);
        assert_eq!(resp.total_size_bytes, Some(0));
        assert!(!resp.truncated);
        assert!(resp.lines.is_empty());
    }

    #[test]
    fn log_without_trailing_newline_returns_partial_last_line() {
        // Some peers don't flush a final newline; we should still
        // surface the trailing fragment.
        let dir = TempDir::new().unwrap();
        let path = write_peer_log(&dir, "alpha", "first\nsecond no newline");
        let resp = read_peer_log_tail(&path, 200);
        assert_eq!(resp.lines, vec!["first", "second no newline"]);
    }
}
