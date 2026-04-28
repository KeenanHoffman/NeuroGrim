//! Federated-patterns aggregator sensor — per-Brain federation observability.
//!
//! E-B2-7 C6 (2026-04-27). Pure file-read + JSONL-parse + count + JSON-output
//! validator (per Q9 source-level recursion-guard lock + the recursion guard
//! test in `tests/federated_patterns_sensor_behavior.rs::recursion_guard_no_command_in_validator_span`).
//!
//! Reads `<project_root>/.claude/brain/pattern-aggregation-ledger.jsonl` —
//! the FIRST Brains-2.0 ledger that captures BOTH inbound and outbound rows.
//! The reader recognizes TWO row kinds discriminated by the `entry_kind`
//! field per `pattern-aggregation-ledger-v1.schema.json`'s `oneOf`:
//!
//! - **`ReceivedEntry`** rows (`entry_kind == "received"`; written by
//!   `federated_pattern.rs`'s default-receive handler — C4): increment
//!   `received_count`, increment per-peer `from_brain_id` counter, track
//!   `last_seen` per peer, AND if `dropped_reason` is populated increment
//!   `dropped_count` + per-reason counter.
//! - **`EmittedEntry`** rows (`entry_kind == "emitted"`; written by the
//!   sender wiring BEFORE transmission — Q12 operator audit trail):
//!   increment `emitted_count`, increment per-peer `to_brain_id` counter,
//!   track `last_emit` per peer.
//!
//! The reader silently skips: missing file (returns advisory floor 100 with
//! `low_confidence: true`), empty lines, JSON-unparseable lines (one summary
//! `federated_patterns:malformed_lines` finding records the count, mirroring
//! E-B2-6 forgiveness discipline + `read_invocation_ledger` precedent), and
//! rows whose `entry_kind` is neither `received` nor `emitted` (forward-compat
//! tolerance per Q11 — closed-set discipline is the schema's job at write
//! time).
//!
//! # Score model (Q10 + Q17 lock)
//!
//! - `score = 100` ALWAYS. Federation is INFORMATION, not health — peer counts
//!   are observability, not gating. Per spec §16.6 "Score is advisory floor
//!   100 (federation is INFORMATION, not health)."
//! - `low_confidence = true` when `received_count + emitted_count == 0` over
//!   the last 7-day window. Surfaces the discoverability state through the
//!   exported variable + a `federated_patterns:low_confidence` finding.
//!
//! # Closed-set findings (Q17 lock)
//!
//! Four finding kinds — closed-set v1 vocabulary, mirrors the additive
//! discipline of `operator_calibration:*`, `trust_budget:*`,
//! `capability_hygiene:*`. New finding kinds require a spec change with an
//! explicit METHODOLOGY-EVOLUTION entry.
//!
//! - `federated_patterns:no_active_peers` — declared peers in
//!   `brain-registry.json:children` is empty (count zero) AND federation
//!   activity exists (received_count + emitted_count > 0). Drift signal:
//!   federation is happening but no declared peer to attribute it to.
//! - `federated_patterns:peer_inactive_30d:<peer-hash>` — per-peer; for each
//!   declared peer, neither side has spoken in 30 days (last_seen older than
//!   30 days AND last_emit older than 30 days, OR both are absent).
//! - `federated_patterns:high_drop_rate` — `dropped_count / received_count > 0.5`
//!   over the last 7-day window. Overall, not per-peer (per-peer is v2 per
//!   BACKLOG B-23).
//! - `federated_patterns:low_confidence` — see above.
//!
//! # Aggregation-only export (Q5 + E6-1 BR-5 mitigation; Q10 charter privacy
//! lock)
//!
//! The CMDB output emits ONLY aggregate totals + per-peer breakdowns +
//! per-pattern-kind breakdowns. NO per-message rows. NO per-row payload data.
//! The peer breakdown uses `peer_brain_id` which is the OPAQUE HASH (not the
//! source identity) — same anonymization contract as the wire-level
//! `anonymized_origin`. If a future change leaks per-row payload data into
//! the breakdown, the `aggregation_only_export_privacy_pin` test catches it.
//! Re-opening per-row export requires a charter-level BR-5 conversation, NOT
//! a Layer-2 lift.
//!
//! # Recursion-guard (Q9 source-level)
//!
//! This file MUST be pure file-read + JSONL-parse + count + JSON-output. No
//! shell-execution surfaces. Mirrors the recursion-guard discipline of
//! `trust_budget.rs` (E-B2-4 C3), `operator_calibration.rs` (E-B2-6 C4), and
//! `capability_hygiene.rs` hat-contract validator (E-B2-3 C5). The test
//! `tests/federated_patterns_sensor_behavior.rs::recursion_guard_no_command_in_validator_span`
//! reads the source file at test time and grep-checks for forbidden patterns
//! from a closed list. The wire-level recursion guard (origin_set membership
//! check) lives in `federated_pattern.rs` — Component 4.

use crate::cmdb::{build_cmdb, Finding};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// ── Embedded schema ─────────────────────────────────────────────────────────

/// Embedded pattern-aggregation-ledger schema (v1). Path is relative to this
/// source file (`src/federated_patterns.rs`); the build fails at compile time
/// if the schema is missing. Five hops: src → crate → crates → neurogrim →
/// NeuroGrim → ecosystem root.
///
/// We don't currently runtime-validate against this schema (the writer
/// `federated_pattern.rs` enforces shape at append time; the reader is
/// forgiving by design per Q11 forward-compat lock). Embedding keeps the
/// schema as a documentation anchor and ensures the v1 schema location stays
/// load-bearing — moving or renaming the schema breaks the build.
// v3.2.2: schema vendored into `data/schemas/` so it resolves in
// `cargo publish` tarballs. Canonical source remains
// `LSP-Brains/schemas/pattern-aggregation-ledger-v1.schema.json`;
// drift caught by schema-conformance tests.
#[allow(dead_code)]
const PATTERN_AGGREGATION_LEDGER_SCHEMA_JSON: &str = include_str!(
    "../data/schemas/pattern-aggregation-ledger-v1.schema.json"
);

// ── Closed-set finding kinds (Q17 lock) ─────────────────────────────────────

/// Documentation anchor for the four closed-set finding kinds emitted by this
/// sensor. The `federated_patterns:peer_inactive_30d` kind is a prefix; per-
/// peer findings emit `federated_patterns:peer_inactive_30d:<peer_hash>` so
/// each inactive peer surfaces as its own finding row. Drift is caught at
/// review time by the closed-set test in C7 (CLI rejects `^federated_patterns:`
/// pattern_kind values at parse time) and by spec §16.6 RFC 2119 prose.
#[allow(dead_code)]
const FINDING_KINDS: &[&str] = &[
    "federated_patterns:no_active_peers",
    "federated_patterns:peer_inactive_30d",
    "federated_patterns:high_drop_rate",
    "federated_patterns:low_confidence",
    // v3.1 E-V31-E E1.2 — emitted when ≥2 distinct anonymized origins
    // received `vigilance-pattern` findings within the 7-day window
    // sharing a feature_vector signature (severity_class +
    // observation_window_days). Multiple peers independently flagging
    // similar concerns is the operator-actionable signal that
    // federation-as-intelligence is supposed to surface. The finding's
    // detail payload is aggregate-only (peer count + severity + window)
    // — no per-peer hashes, no per-row data. Closed-set additivity per
    // Q17 lock; spec §16.6.1 amendment ships alongside this finding.
    "federated_patterns:cross_peer_co_occurrence",
];

// ── Locked thresholds ───────────────────────────────────────────────────────

/// Q10 lock: `low_confidence` flips true when the 7-day federation activity
/// window is empty. Charter prose: "The aggregator's score model is
/// intentionally DIFFERENT from prior epics — federation is INFORMATION,
/// not health" — so the threshold isn't a sample-size floor (no
/// statistical claim is being made), it's a recency window. v1 is
/// 7 days; v2 may revisit per BACKLOG B-23.
const LOW_CONFIDENCE_WINDOW_DAYS: i64 = 7;

/// Q17 lock: a peer is "inactive" if neither the receiver nor the sender
/// has talked to them in the last 30 days. The 30-day window mirrors the
/// `domain-calibration` last-triage-age threshold (§17.9) and gives
/// breathing room for project cadence variation.
const PEER_INACTIVE_WINDOW_DAYS: i64 = 30;

/// Q17 lock: emit `federated_patterns:high_drop_rate` when more than half
/// of received messages were dropped over the 7-day window. The 0.5
/// threshold is intentionally conservative — high drop rate is a signal
/// of either a misbehaving peer or a misconfigured opt-in posture; it
/// shouldn't fire on intermittent rate-limit hits.
const HIGH_DROP_RATE_THRESHOLD: f64 = 0.5;

/// Closed-set drop-reason vocabulary (mirrors the schema's
/// `DroppedReason.enum`). Documentation anchor for readers who don't want
/// to follow the `include_str!` chain. Drift is caught at write time by
/// the schema-conformance fixtures (C3 closed-set test).
#[allow(dead_code)]
const DROPPED_REASONS: &[&str] = &[
    "rate-limit-exceeded",
    "recursion-guard",
    "schema-validation-failed",
    "hop-limit-exceeded",
    "unknown-pattern-kind",
];

// ── Public analysis entry points ────────────────────────────────────────────

/// Analyze the federated-patterns aggregation ledger for a Brain at
/// `project_root`.
///
/// Mirrors `analyze_operator_calibration` shape — accepts `&str` to match the
/// existing CLI dispatch convention; the path is canonicalized internally
/// where possible.
///
/// Returns a CMDB envelope (cmdb-envelope-v1.schema.json) carrying the
/// federated-patterns score (advisory floor 100 always, weight 0.0), findings
/// list, and a `federated_patterns_breakdown` block with aggregate counters
/// + per-peer + per-pattern-kind breakdowns (NEVER per-row data; see
/// module-level docs).
pub async fn analyze_federated_patterns(project_root: &str) -> Value {
    let root_raw = PathBuf::from(project_root);
    let root = root_raw.canonicalize().unwrap_or(root_raw);
    analyze_federated_patterns_path(&root)
}

/// Path-typed implementation of `analyze_federated_patterns`. Separated out
/// so integration tests can exercise the path-typed surface without a UTF-8
/// round-trip through `&str`.
pub fn analyze_federated_patterns_path(root: &Path) -> Value {
    let mut findings: Vec<Finding> = Vec::new();
    let now = Utc::now();

    // ── Phase 1: locate the ledger ────────────────────────────────────
    let ledger_path = root
        .join(".claude")
        .join("brain")
        .join("pattern-aggregation-ledger.jsonl");

    let outcome = match std::fs::read_to_string(&ledger_path) {
        Ok(text) => {
            let counts = scan_ledger(&text, &mut findings);
            LedgerOutcome::Present(counts)
        }
        Err(_) => {
            // Missing file is the LEGITIMATE absence path: a Brain that
            // hasn't run any federation has no ledger. Per the brief +
            // Q10 lock, do NOT emit a hard error for absence; the
            // `low_confidence: true` exported variable surfaces the
            // state, and `no_active_peers` may surface separately if
            // the registry declares peers but the ledger is empty.
            LedgerOutcome::Absent
        }
    };

    // ── Phase 2: load the registry and count declared peers ───────────
    let declared_peers = load_declared_peers(root);
    let declared_peer_count = declared_peers.len();

    // ── Phase 3: compute window aggregates + emit findings (Q17) ──────
    let counts = outcome.counts();
    let window_aggs = window_aggregates(&counts, now, LOW_CONFIDENCE_WINDOW_DAYS);
    let low_confidence =
        window_aggs.received_in_window + window_aggs.emitted_in_window == 0;

    // Q17: federated_patterns:low_confidence
    if low_confidence {
        findings.push(Finding {
            name: "federated_patterns:low_confidence".to_string(),
            status: "neutral".to_string(),
            points: 0,
            detail: Some(format!(
                "No federated-pattern activity in the last {LOW_CONFIDENCE_WINDOW_DAYS} days \
                 (received_in_window={}, emitted_in_window={}). Federation is observability \
                 — empty windows are common during incubation. Per Q10 + Q17 lock, this is \
                 advisory only.",
                window_aggs.received_in_window, window_aggs.emitted_in_window
            )),
        });
    }

    // Q17: federated_patterns:no_active_peers — emit ONLY if declared peer
    // count is zero AND the ledger has activity (the drift signal: federation
    // is happening but the registry doesn't know about any peer to attribute
    // it to). Per the brief: "if NO declared peers in brain-registry.json
    // config (count: zero parent + zero children) AND received_count +
    // emitted_count > 0".
    if declared_peer_count == 0 && (counts.received_count + counts.emitted_count) > 0 {
        findings.push(Finding {
            name: "federated_patterns:no_active_peers".to_string(),
            status: "neutral".to_string(),
            points: 0,
            detail: Some(format!(
                "Federation activity observed (received_count={}, emitted_count={}) but \
                 brain-registry.json declares ZERO peers. Configuration drift signal — \
                 federation is happening without a declared peer to attribute it to. \
                 Investigate `.claude/brain-registry.json:config.children`.",
                counts.received_count, counts.emitted_count
            )),
        });
    }

    // Q17: federated_patterns:peer_inactive_30d — per-peer; only fires for
    // declared peers (otherwise we would surface noise about every transient
    // sender that pinged us once and went away).
    for peer_id in &declared_peers {
        let last_seen = counts.peers.get(peer_id).and_then(|p| p.last_seen);
        let last_emit = counts.peers.get(peer_id).and_then(|p| p.last_emit);
        let cutoff = now - Duration::days(PEER_INACTIVE_WINDOW_DAYS);

        let seen_recent = last_seen.map(|t| t > cutoff).unwrap_or(false);
        let emit_recent = last_emit.map(|t| t > cutoff).unwrap_or(false);

        if !seen_recent && !emit_recent {
            findings.push(Finding {
                name: format!(
                    "federated_patterns:peer_inactive_30d:{}",
                    short_peer_token(peer_id)
                ),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(format!(
                    "Declared peer `{peer_id}` has no federation activity in the last \
                     {PEER_INACTIVE_WINDOW_DAYS} days (last_seen={:?}, last_emit={:?}). \
                     Possible peer drift, network partition, or deactivation. Federation \
                     is observability-only — this is advisory.",
                    last_seen.map(|t| t.to_rfc3339()),
                    last_emit.map(|t| t.to_rfc3339())
                )),
            });
        }
    }

    // v3.1 E-V31-E: federated_patterns:cross_peer_co_occurrence —
    // when ≥2 distinct anonymized origins emit vigilance-pattern
    // findings sharing a feature_vector signature (severity_class +
    // observation_window_days) within the 7-day window. Multiple peers
    // independently flagging similar concerns is the operator-actionable
    // signal that federation-as-intelligence is meant to surface.
    // Aggregate-only export per Q5+E6-1 BR-5 lock — finding detail
    // carries peer count + severity + window, NEVER per-peer hashes.
    for detail in detect_cross_peer_co_occurrence(&counts, now, LOW_CONFIDENCE_WINDOW_DAYS) {
        findings.push(Finding {
            name: "federated_patterns:cross_peer_co_occurrence".to_string(),
            status: "neutral".to_string(),
            points: 0,
            detail: Some(detail),
        });
    }

    // Q17: federated_patterns:high_drop_rate — overall (not per-peer at v1).
    // Defense-in-depth divide-by-zero check; the predicate also guards on
    // received_in_window > 0.
    if window_aggs.received_in_window > 0 {
        let drop_ratio =
            window_aggs.dropped_in_window as f64 / window_aggs.received_in_window as f64;
        if drop_ratio > HIGH_DROP_RATE_THRESHOLD {
            findings.push(Finding {
                name: "federated_patterns:high_drop_rate".to_string(),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(format!(
                    "Drop rate {:.1}% over the last {LOW_CONFIDENCE_WINDOW_DAYS} days \
                     ({} dropped of {} received) exceeds the {:.0}% threshold. Investigate \
                     peer behavior or rate-limit configuration. Federation is observability — \
                     this is advisory.",
                    drop_ratio * 100.0,
                    window_aggs.dropped_in_window,
                    window_aggs.received_in_window,
                    HIGH_DROP_RATE_THRESHOLD * 100.0
                )),
            });
        }
    }

    // ── Phase 4: count active peers (peers seen at all) ───────────────
    // "Active" at v1 is "appears in the ledger" — different from the
    // peer_inactive_30d check which uses the recency window. v2 may make
    // this stricter (e.g., active in last 7 days).
    let active_peer_count = counts.peers.len();

    // ── Phase 5: build the breakdown payload ─────────────────────────
    let extras = vec![(
        "federated_patterns_breakdown",
        breakdown_json(
            &ledger_path,
            outcome.is_present(),
            &counts,
            low_confidence,
            declared_peer_count,
            active_peer_count,
        ),
    )];

    // ── Phase 6: build CMDB envelope. Score is ALWAYS 100 per Q10 ────
    build_cmdb(
        "federated-patterns",
        100,
        findings,
        Some(extras),
        None,
    )
}

// ── Ledger scanning ─────────────────────────────────────────────────────────

/// Per-peer aggregate state. Tracks both directions; `last_seen` / `last_emit`
/// are timestamps from the ledger's `ts` field, parsed once per row. Order
/// preserved via `BTreeMap` for stable JSON output.
#[derive(Debug, Default, Clone)]
struct PeerCounts {
    received_count: usize,
    emitted_count: usize,
    last_seen: Option<DateTime<Utc>>,
    last_emit: Option<DateTime<Utc>>,
}

/// Per-pattern-kind aggregate state. v1 only emits `vigilance-pattern` per
/// Q14 closed-set lock, but the breakdown dimension is left open for v2
/// expansion (per Q14 additive promotion). v1 readers tolerate unknown
/// pattern_kind values via Q11 forward-compat.
#[derive(Debug, Default, Clone)]
struct PatternKindCounts {
    received_count: usize,
    emitted_count: usize,
}

/// One feature-vector signature captured from a `ReceivedEntry` row.
/// Used internally for cross-peer co-occurrence detection — the
/// `(severity_class, observation_window_days)` pair is the signature
/// key, and per-row entries are grouped post-scan to detect when ≥2
/// distinct origins share the same signature.
///
/// **Privacy note:** these are TEMPORARY per-row records held only for
/// in-pass aggregation. They are NEVER written to the CMDB — the
/// emitted finding carries aggregate counts only (number of distinct
/// origins, severity class, window). The `aggregation_only_export_*`
/// privacy pin tests verify per-row data does not cross the CMDB
/// boundary.
#[derive(Debug, Clone)]
struct ReceivedSignature {
    ts: DateTime<Utc>,
    from_brain_id: String,
    severity_class: String,
    observation_window_days: u64,
}

/// Aggregate counters extracted from the ledger. The breakdown JSON only
/// reports aggregate totals + per-peer + per-pattern-kind groupings — never
/// per-row data (E6-1 BR-5 mitigation).
///
/// `received_ts` / `emitted_ts` carry the parsed timestamp of each row in
/// scan order — used to compute the 7-day windowed aggregates. We deliberately
/// keep the per-row timestamps as a Vec rather than per-row payload data
/// because timestamps are aggregate-grouping keys (counting how many rows
/// fall in a window), not message contents.
#[derive(Debug, Default, Clone)]
struct LedgerCounts {
    received_count: usize,
    emitted_count: usize,
    dropped_count: usize,
    /// Per-reason drop counters; sum equals `dropped_count` (closed set per
    /// the schema's DroppedReason enum). Rows with unknown drop reasons are
    /// counted toward `dropped_count` but NOT toward any per-reason counter
    /// (forward-compat tolerance — closed-set discipline is the schema's
    /// job at write time).
    dropped_by_reason: BTreeMap<String, usize>,
    /// Per-peer aggregates keyed by `peer_brain_id` (opaque hash from
    /// from_brain_id / to_brain_id depending on direction).
    peers: BTreeMap<String, PeerCounts>,
    /// Per-pattern-kind aggregates keyed by `payload.pattern_kind`.
    pattern_kinds: BTreeMap<String, PatternKindCounts>,
    /// Lines that failed JSON parsing entirely. Surfaced as a single summary
    /// finding mirroring the E-B2-6 forgiveness discipline.
    malformed_lines: usize,
    /// Parsed timestamps of received rows in scan order. Used for 7-day
    /// window aggregation. Aggregate-grouping key only — NOT per-row payload.
    received_ts: Vec<(DateTime<Utc>, bool)>, // (ts, was_dropped)
    /// Parsed timestamps of emitted rows in scan order.
    emitted_ts: Vec<DateTime<Utc>>,
    /// v3.1 E-V31-E: per-row feature-vector signatures from ReceivedEntry
    /// rows that successfully extracted ts + from_brain_id + severity +
    /// window. Used post-scan for cross-peer co-occurrence detection.
    /// In-memory only — NEVER exported to the CMDB.
    received_signatures: Vec<ReceivedSignature>,
}

enum LedgerOutcome {
    Absent,
    Present(LedgerCounts),
}

impl LedgerOutcome {
    fn counts(&self) -> LedgerCounts {
        match self {
            LedgerOutcome::Absent => LedgerCounts::default(),
            LedgerOutcome::Present(c) => c.clone(),
        }
    }

    fn is_present(&self) -> bool {
        matches!(self, LedgerOutcome::Present(_))
    }
}

/// 7-day-window aggregate slice of `LedgerCounts`. Computed once per scan
/// pass; the `low_confidence` + `high_drop_rate` predicates use these
/// counters rather than the lifetime totals so transient drops on an old
/// project don't flag a fresh activity window.
struct WindowAggregates {
    received_in_window: usize,
    emitted_in_window: usize,
    dropped_in_window: usize,
}

fn window_aggregates(
    counts: &LedgerCounts,
    now: DateTime<Utc>,
    window_days: i64,
) -> WindowAggregates {
    let cutoff = now - Duration::days(window_days);
    let mut received_in_window = 0usize;
    let mut emitted_in_window = 0usize;
    let mut dropped_in_window = 0usize;
    for (ts, was_dropped) in &counts.received_ts {
        if *ts > cutoff {
            received_in_window += 1;
            if *was_dropped {
                dropped_in_window += 1;
            }
        }
    }
    for ts in &counts.emitted_ts {
        if *ts > cutoff {
            emitted_in_window += 1;
        }
    }
    WindowAggregates {
        received_in_window,
        emitted_in_window,
        dropped_in_window,
    }
}

/// Scan a JSONL ledger body, classifying each row as `ReceivedEntry` or
/// `EmittedEntry` and incrementing the relevant counters. Lines that fail
/// JSON parsing are counted (malformed_lines) and surfaced via a single
/// summary finding; lines that parse but don't match either row kind are
/// silently skipped (forward-compat tolerance per Q11).
fn scan_ledger(text: &str, findings: &mut Vec<Finding>) -> LedgerCounts {
    let mut counts = LedgerCounts::default();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                // Malformed line — count toward summary finding; do not panic,
                // do not abort. Mirrors the silent-skip discipline of
                // `read_invocation_ledger` at `capability_hygiene.rs:1589-1592`
                // + `operator_calibration::scan_ledger:255`.
                counts.malformed_lines += 1;
                continue;
            }
        };

        // Discriminate row kind by `entry_kind` field (per the schema's
        // oneOf discriminator).
        let entry_kind = parsed.get("entry_kind").and_then(|v| v.as_str());
        match entry_kind {
            Some("received") => {
                count_received(&parsed, &mut counts);
            }
            Some("emitted") => {
                count_emitted(&parsed, &mut counts);
            }
            _ => {
                // Unknown / missing entry_kind. Forward-compat tolerance per
                // Q11 — closed-set discipline is the schema's job at write
                // time; the sensor doesn't crash on rows that slipped through
                // somehow.
            }
        }
    }

    if counts.malformed_lines > 0 {
        findings.push(Finding {
            name: "federated_patterns:malformed_lines".to_string(),
            status: "neutral".to_string(),
            points: 0,
            detail: Some(format!(
                "Skipped {} malformed JSONL line(s) in `.claude/brain/pattern-aggregation-ledger.jsonl`. \
                 Forgiveness discipline (Q11 forward-compat lock) — the sensor never crashes on \
                 bad input. Investigate the writer if the count is non-trivial.",
                counts.malformed_lines
            )),
        });
    }

    counts
}

/// Process a `ReceivedEntry` row. Increments `received_count`, per-peer
/// `received_count` keyed by `from_brain_id`, per-peer `last_seen`, and (if
/// `dropped_reason` is populated) `dropped_count` + per-reason counter.
fn count_received(parsed: &Value, counts: &mut LedgerCounts) {
    counts.received_count += 1;

    let ts = parse_ts(parsed);
    let was_dropped = parsed
        .get("dropped_reason")
        .and_then(|v| v.as_str())
        .is_some();

    if was_dropped {
        counts.dropped_count += 1;
        if let Some(reason) = parsed.get("dropped_reason").and_then(|v| v.as_str()) {
            *counts.dropped_by_reason.entry(reason.to_string()).or_insert(0) += 1;
        }
    }

    if let Some(peer_id) = parsed.get("from_brain_id").and_then(|v| v.as_str()) {
        let entry = counts
            .peers
            .entry(peer_id.to_string())
            .or_insert_with(PeerCounts::default);
        entry.received_count += 1;
        if let Some(t) = ts {
            entry.last_seen = match entry.last_seen {
                None => Some(t),
                Some(prev) if t > prev => Some(t),
                _ => entry.last_seen,
            };
        }
    }

    if let Some(kind) = parsed
        .get("payload")
        .and_then(|p| p.get("pattern_kind"))
        .and_then(|v| v.as_str())
    {
        let entry = counts
            .pattern_kinds
            .entry(kind.to_string())
            .or_insert_with(PatternKindCounts::default);
        entry.received_count += 1;
    }

    if let Some(t) = ts {
        counts.received_ts.push((t, was_dropped));
    }

    // v3.1 E-V31-E: capture the feature-vector signature for post-scan
    // cross-peer co-occurrence detection. Requires ts + from_brain_id +
    // severity_class + observation_window_days; rows missing any are
    // skipped (the per-row signature can't be aggregated without all
    // four). Privacy-safe: in-memory only, NEVER exported.
    let from_id = parsed.get("from_brain_id").and_then(|v| v.as_str());
    let severity = parsed
        .get("payload")
        .and_then(|p| p.get("feature_vector"))
        .and_then(|f| f.get("severity_class"))
        .and_then(|v| v.as_str());
    let window = parsed
        .get("payload")
        .and_then(|p| p.get("feature_vector"))
        .and_then(|f| f.get("observation_window_days"))
        .and_then(|v| v.as_u64());
    if let (Some(t), Some(fid), Some(sev), Some(w)) = (ts, from_id, severity, window) {
        counts.received_signatures.push(ReceivedSignature {
            ts: t,
            from_brain_id: fid.to_string(),
            severity_class: sev.to_string(),
            observation_window_days: w,
        });
    }
}

/// Process an `EmittedEntry` row. Increments `emitted_count`, per-peer
/// `emitted_count` keyed by `to_brain_id`, per-peer `last_emit`.
fn count_emitted(parsed: &Value, counts: &mut LedgerCounts) {
    counts.emitted_count += 1;

    let ts = parse_ts(parsed);

    if let Some(peer_id) = parsed.get("to_brain_id").and_then(|v| v.as_str()) {
        let entry = counts
            .peers
            .entry(peer_id.to_string())
            .or_insert_with(PeerCounts::default);
        entry.emitted_count += 1;
        if let Some(t) = ts {
            entry.last_emit = match entry.last_emit {
                None => Some(t),
                Some(prev) if t > prev => Some(t),
                _ => entry.last_emit,
            };
        }
    }

    if let Some(kind) = parsed
        .get("payload")
        .and_then(|p| p.get("pattern_kind"))
        .and_then(|v| v.as_str())
    {
        let entry = counts
            .pattern_kinds
            .entry(kind.to_string())
            .or_insert_with(PatternKindCounts::default);
        entry.emitted_count += 1;
    }

    if let Some(t) = ts {
        counts.emitted_ts.push(t);
    }
}

/// v3.1 E-V31-E: detect cross-peer co-occurrence in received vigilance
/// patterns. Groups received signatures within the rolling window by
/// `(severity_class, observation_window_days)`; when a group has ≥2
/// distinct `from_brain_id` values, emit one finding-detail string
/// per group describing the co-occurrence aggregately.
///
/// Returns a `Vec<String>` of finding-detail strings (zero-length
/// when no co-occurrences found). Caller wraps each into a Finding
/// with name `federated_patterns:cross_peer_co_occurrence`.
///
/// **Privacy contract:** the returned strings carry aggregate data
/// only — count of distinct peers, severity class, observation
/// window. NO per-peer hashes, NO per-row timestamps, NO finding
/// names from upstream peers. The `aggregation_only_export_*`
/// regression tests verify per-row data does not leak via the new
/// finding's detail field.
fn detect_cross_peer_co_occurrence(
    counts: &LedgerCounts,
    now: DateTime<Utc>,
    window_days: i64,
) -> Vec<String> {
    let cutoff = now - Duration::days(window_days);
    // Signature key: (severity_class, observation_window_days). Set of
    // distinct from_brain_id values per signature.
    let mut sig_to_peers: BTreeMap<(String, u64), BTreeSet<String>> = BTreeMap::new();
    for sig in &counts.received_signatures {
        if sig.ts > cutoff {
            sig_to_peers
                .entry((
                    sig.severity_class.clone(),
                    sig.observation_window_days,
                ))
                .or_insert_with(BTreeSet::new)
                .insert(sig.from_brain_id.clone());
        }
    }
    let mut details = Vec::new();
    for ((severity, window), peers) in sig_to_peers {
        if peers.len() >= 2 {
            details.push(format!(
                "Cross-peer co-occurrence: {peer_count} distinct anonymized origins emitted \
                 vigilance-pattern findings with severity_class={severity} and \
                 observation_window_days={window} within the last {window_days} \
                 days. Multiple peers independently flagged similar concerns. \
                 Federation is observability — this is advisory.",
                peer_count = peers.len(),
            ));
        }
    }
    details
}

/// Parse the row's `ts` field as ISO 8601 UTC. Returns `None` on missing /
/// unparseable — the row is still counted (lifetime totals); only the
/// windowed aggregates skip rows without a usable timestamp. This matches
/// the forgiveness discipline applied elsewhere in the sensor.
fn parse_ts(parsed: &Value) -> Option<DateTime<Utc>> {
    let raw = parsed.get("ts").and_then(|v| v.as_str())?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

// ── Registry loading (declared peers) ───────────────────────────────────────

/// Load the set of declared peer brain IDs from
/// `<root>/.claude/brain-registry.json`. v1 federation uses fractal-
/// composition `children` only (Q16 LOCAL lock per spec §16.6 + Q16 lock
/// in the Layer-2 plan). Sibling federation is OUT OF SCOPE for v1.
///
/// Returns a sorted `Vec<String>` of child brain identifiers (the keys of
/// the `config.children` object). Empty if the registry is missing,
/// malformed, or has no `children` block.
///
/// Note: the registry uses the human-readable child name (e.g.,
/// `"python-starter"`) as the key, NOT the opaque hash. Peers in the
/// ledger are identified by opaque hash. The `peer_inactive_30d` check
/// matches on the registry-key value as the peer-id token; this is a v1
/// approximation — the proper opaque-hash join is a v2 follow-on per
/// BACKLOG B-23 (the same anonymization-namespace concern as Q15). For
/// v1 the test surface confirms the finding fires when a declared peer
/// has no ledger activity, which is the operator-facing semantics.
fn load_declared_peers(root: &Path) -> Vec<String> {
    let path = root.join(".claude").join("brain-registry.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(parsed): Result<Value, _> = serde_json::from_str(&text) else {
        return Vec::new();
    };
    let children = parsed
        .get("config")
        .and_then(|c| c.get("children"))
        .and_then(|c| c.as_object());
    let Some(obj) = children else {
        return Vec::new();
    };
    let mut keys: Vec<String> = obj.keys().cloned().collect();
    keys.sort();
    keys
}

/// Short-token form of a peer id for embedding in a finding name (so the
/// `peer_inactive_30d` finding name is operator-friendly without
/// reproducing the full opaque hash). Truncates to the first 12 chars,
/// stripping any non-alphanumeric for safety inside a finding-name.
fn short_peer_token(peer_id: &str) -> String {
    let cleaned: String = peer_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    cleaned.chars().take(12).collect()
}

// ── CMDB extras / breakdown JSON ────────────────────────────────────────────

fn breakdown_json(
    ledger_path: &Path,
    ledger_present: bool,
    counts: &LedgerCounts,
    low_confidence: bool,
    declared_peer_count: usize,
    active_peer_count: usize,
) -> Value {
    let dropped_by_reason = {
        let mut obj = serde_json::Map::new();
        for (reason, count) in &counts.dropped_by_reason {
            obj.insert(reason.clone(), Value::from(*count));
        }
        Value::Object(obj)
    };

    let peer_breakdown: Vec<Value> = counts
        .peers
        .iter()
        .map(|(peer_brain_id, peer)| {
            json!({
                "peer_brain_id": peer_brain_id,
                "received_count": peer.received_count,
                "emitted_count": peer.emitted_count,
                "last_seen": peer.last_seen.map(|t| t.to_rfc3339()),
                "last_emit": peer.last_emit.map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    let pattern_kind_breakdown: Vec<Value> = counts
        .pattern_kinds
        .iter()
        .map(|(pattern_kind, pk)| {
            json!({
                "pattern_kind": pattern_kind,
                "received_count": pk.received_count,
                "emitted_count": pk.emitted_count,
            })
        })
        .collect();

    json!({
        "ledger_path": if ledger_present {
            Value::String(ledger_path.display().to_string())
        } else {
            Value::Null
        },
        "ledger_present": ledger_present,
        "total_received_count": counts.received_count,
        "total_emitted_count": counts.emitted_count,
        "total_dropped_count": counts.dropped_count,
        "dropped_by_reason": dropped_by_reason,
        "peer_breakdown": peer_breakdown,
        "pattern_kind_breakdown": pattern_kind_breakdown,
        "low_confidence": low_confidence,
        "declared_peer_count": declared_peer_count,
        "active_peer_count": active_peer_count,
    })
}

// ── Compile-time anchor for the schema embed ────────────────────────────────

#[cfg(test)]
mod schema_anchor_tests {
    use super::PATTERN_AGGREGATION_LEDGER_SCHEMA_JSON;

    /// Compile-time check that the embedded schema parses as JSON. If the
    /// schema file ever stops being valid JSON the build still succeeds
    /// (because `include_str!` is byte-level) but this test would catch
    /// the regression at test time.
    #[test]
    fn embedded_schema_parses_as_json() {
        let parsed: serde_json::Value =
            serde_json::from_str(PATTERN_AGGREGATION_LEDGER_SCHEMA_JSON)
                .expect("embedded pattern-aggregation-ledger schema must parse as JSON");
        assert_eq!(
            parsed.get("title").and_then(|v| v.as_str()),
            Some("Pattern Aggregation Ledger v1")
        );
    }
}
