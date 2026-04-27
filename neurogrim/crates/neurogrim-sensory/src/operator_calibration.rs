//! Operator-calibration sensor — per-Brain operator-disposition rate observer.
//!
//! E-B2-6 C4 (2026-04-27). Pure file-read + JSONL-parse + count + JSON-output
//! validator (per Q6 hard rule + the recursion guard test in
//! `tests/operator_calibration_sensor_behavior.rs::recursion_guard_no_command_in_validator_span`).
//! Reads `<project_root>/.claude/brain/invocation-ledger.jsonl` — the SAME
//! file the existing `capability_hygiene::read_invocation_ledger` consumes
//! (P0-2) — but recognizes BOTH row kinds:
//!
//! - **`SkillEntry`** rows (written by `record-skill-invocation.sh`; existing
//!   contract): increment `total_invocations`. The existing reader continues
//!   to work unchanged because it only cares about these rows.
//! - **`DispositionEntry`** rows (NEW; written by `neurogrim disposition record`,
//!   E-B2-6 C3): increment `dispositioned_count` AND increment the per-kind
//!   counter (`accepted_count`, `rejected_count`, `modified_count`,
//!   `superseded_count`).
//!
//! The reader silently skips: missing file (returns advisory floor 100 with
//! `low_confidence: true`), empty lines, JSON-unparseable lines (one
//! summary `operator_calibration:malformed_lines` finding records the count
//! per Q3 forgiveness discipline + `read_invocation_ledger` precedent),
//! disposition rows with unknown `disposition_kind` (forward-compat
//! tolerance — closed-set discipline is the schema's job at write time).
//!
//! # Score model (Q4 lock)
//!
//! - `dispositioned_count == 0` → `score = 100` (advisory floor; no
//!   judgment yet means no negative signal). `low_confidence = true`.
//!   When `total_invocations > 50` we additionally emit
//!   `operator_calibration:no_dispositions_yet` as a gentle nudge that
//!   the operator hasn't engaged with the disposition CLI even after
//!   substantial usage. The 50-invocation threshold is a v1 calibration
//!   smell signal — operators with only a handful of invocations may be
//!   exploring, not yet operating.
//! - `dispositioned_count < N_MIN` → `score = null` per Q9 lock. JSON
//!   `null`, not 0 or 100. Below the meaningful-signal floor; consumers
//!   should read `low_confidence == true` and the dispositioned_count
//!   counter to understand the discoverability state. One
//!   `operator_calibration:low_confidence` advisory finding emitted.
//! - `dispositioned_count >= N_MIN` → `score = round(100.0 *
//!   accepted_count / dispositioned_count)`. `low_confidence = false`.
//!   No findings emitted (clean state).
//!
//! # Aggregation-only export (E6-1 BR-5 mitigation; MUST per spec §17.12.6)
//!
//! The CMDB output emits ONLY aggregate totals. NO per-invocation rows.
//! NO per-skill breakdown. NO per-session breakdown. Just the seven
//! counters listed in `operator_calibration_breakdown`. If a future
//! change leaks per-invocation data, the test
//! `aggregation_only_export_q5_privacy_pin` catches it. Re-opening
//! per-invocation export requires a charter-level BR-5 conversation,
//! NOT a Layer-2 lift.
//!
//! # Recursion-guard (Q6 hard rule)
//!
//! This file MUST be pure file-read + JSONL-parse + count + JSON-output.
//! No shell-execution surfaces. Mirrors the recursion-guard discipline
//! of `trust_budget.rs` (E-B2-4 C3) and `capability_hygiene.rs` hat
//! contract validator (E-B2-3 C5). The test
//! `tests/operator_calibration_sensor_behavior.rs::recursion_guard_no_command_in_validator_span`
//! reads the source file at test time and grep-checks for forbidden
//! patterns from a closed list (the patterns are deliberately not
//! enumerated in this comment so they don't trip the test against
//! itself; see the test source for the canonical list).

use crate::cmdb::{build_cmdb, Finding};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

// ── Embedded schema ─────────────────────────────────────────────────────────

/// Embedded invocation-ledger schema (v1). Path is relative to this source
/// file (`src/operator_calibration.rs`); the build fails at compile time if
/// the schema is missing. Five hops: src → crate → crates → neurogrim →
/// NeuroGrim → ecosystem root.
///
/// We don't currently runtime-validate against this schema (the writer
/// CLI in C3 enforces shape at append time; the reader is forgiving by
/// design per P0-2 / Q3 forgiveness lock). Embedding keeps the schema as
/// a documentation anchor and ensures the v1 schema location stays
/// load-bearing — moving or renaming the schema breaks the build.
#[allow(dead_code)]
const OPERATOR_CALIBRATION_SCHEMA_JSON: &str = include_str!(
    "../../../../../LSP-Brains/schemas/invocation-ledger-v1.schema.json"
);

// ── Locked thresholds ───────────────────────────────────────────────────────

/// Q9 lock; aligned with `LOW_CONFIDENCE_TOTAL_INVOCATIONS = 20` precedent in
/// `capability_hygiene.rs:150`. Below this dispositioned_count, the sensor
/// returns `score = null` (per Q9) — disposition-rate is not a meaningful
/// signal at small N. Charter's ≥50 dogfood gate at E-B2-8 is a separate
/// release-readiness threshold; this is the per-Brain sensor floor.
const N_MIN: usize = 20;

/// Threshold above which the sensor emits `operator_calibration:no_dispositions_yet`
/// when `dispositioned_count == 0`. v1 calibration smell signal — operators
/// with only a handful of invocations may be exploring, not yet operating.
/// Drawn from the Q4 plan-lock prose ("operator hasn't engaged with the
/// disposition CLI even after substantial usage").
const NO_DISPOSITIONS_YET_THRESHOLD: usize = 50;

/// Closed-set disposition vocabulary (mirrors the schema's
/// `Disposition.enum`). Documentation anchor for readers who don't want to
/// follow the `include_str!` chain. Drift is caught at write time by the
/// schema-conformance fixtures (C2 closed-set test).
#[allow(dead_code)]
const DISPOSITION_KINDS: &[&str] = &[
    "accepted",
    "rejected",
    "modified",
    "superseded",
];

// ── Public analysis entry points ────────────────────────────────────────────

/// Analyze the operator-calibration ledger for a Brain at `project_root`.
///
/// Mirrors `analyze_trust_budget` shape — accepts `&str` to match the
/// existing CLI dispatch convention; the path is canonicalized internally
/// where possible.
///
/// Returns a CMDB envelope (cmdb-envelope-v1.schema.json) carrying the
/// operator-calibration score (advisory, weight 0.0), findings list, and
/// an `operator_calibration_breakdown` block with aggregate counters
/// (NEVER per-invocation rows; see module-level docs).
pub async fn analyze_operator_calibration(project_root: &str) -> Value {
    let root_raw = PathBuf::from(project_root);
    let root = root_raw.canonicalize().unwrap_or(root_raw);
    analyze_operator_calibration_path(&root)
}

/// Path-typed implementation of `analyze_operator_calibration`. Separated
/// out so integration tests can exercise the path-typed surface without a
/// UTF-8 round-trip through `&str`.
pub fn analyze_operator_calibration_path(root: &Path) -> Value {
    let mut findings: Vec<Finding> = Vec::new();

    // ── Phase 1: locate the ledger ────────────────────────────────────
    let ledger_path = root
        .join(".claude")
        .join("brain")
        .join("invocation-ledger.jsonl");

    let counts = match std::fs::read_to_string(&ledger_path) {
        Ok(text) => {
            let counts = scan_ledger(&text, &mut findings);
            LedgerOutcome::Present(counts)
        }
        Err(_) => {
            // Missing file is the LEGITIMATE absence path: a Brain that
            // hasn't enabled the PostToolUse hook simply has no ledger.
            // Per Q4 + the brief: do NOT emit any finding for absence;
            // the `low_confidence: true` exported variable surfaces the
            // state.
            LedgerOutcome::Absent
        }
    };

    // ── Phase 2: compute score per Q4 model ───────────────────────────
    let breakdown = scoring_breakdown(&counts);
    let score_score = compute_score(&counts, &mut findings);

    // ── Phase 3: build CMDB envelope ──────────────────────────────────
    //
    // The envelope's `score` field is an integer in [0, 100] per the
    // CMDB envelope schema; `null` is encoded by emitting the envelope
    // with score = 0 sentinel and overlaying `score: null` post-build,
    // because `build_cmdb` takes `u8`. We use the post-build overlay
    // pattern so the rest of the envelope (meta, updated_at, findings,
    // breakdown) stays canonical.
    let extras = vec![(
        "operator_calibration_breakdown",
        breakdown_json(&ledger_path, &counts, &breakdown),
    )];
    let mut envelope = build_cmdb(
        "operator-calibration",
        score_score.unwrap_or(100),
        findings,
        Some(extras),
        None,
    );

    if score_score.is_none() {
        // Q9 lock — null score below N_MIN. Overwrite the integer score
        // with explicit JSON null so downstream consumers see the
        // discoverability signal directly.
        if let Some(obj) = envelope.as_object_mut() {
            obj.insert("score".to_string(), Value::Null);
        }
    }

    envelope
}

// ── Ledger scanning ─────────────────────────────────────────────────────────

/// Aggregate counters extracted from the ledger. The breakdown JSON only
/// reports aggregate totals (E6-1 BR-5 mitigation); never per-invocation,
/// never per-skill, never per-session.
#[derive(Debug, Default, Clone)]
struct LedgerCounts {
    /// Total `SkillEntry` rows seen in the ledger.
    total_invocations: usize,
    /// Total `DispositionEntry` rows seen, regardless of kind.
    dispositioned_count: usize,
    /// Per-kind counters; sum equals `dispositioned_count` (closed set
    /// per Q1; rows with unknown kinds are NOT counted here, just
    /// silently skipped per Q3 forgiveness).
    accepted_count: usize,
    rejected_count: usize,
    modified_count: usize,
    superseded_count: usize,
    /// Lines that failed JSON parsing entirely. Surfaced as a single
    /// summary finding per Q3 forgiveness lock.
    malformed_lines: usize,
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
}

/// Scan a JSONL ledger body, classifying each row as `SkillEntry` or
/// `DispositionEntry` and incrementing the relevant counter. Lines that
/// fail JSON parsing are counted (malformed_lines) and surfaced via a
/// single summary finding; lines that parse but don't match either row
/// kind are silently skipped (forward-compat tolerance).
fn scan_ledger(text: &str, findings: &mut Vec<Finding>) -> LedgerCounts {
    let mut counts = LedgerCounts::default();

    // Mirror `read_invocation_ledger` line iteration (split on `\n`,
    // .lines() handles \r\n implicitly; trim each line to be defensive
    // against trailing whitespace).
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                // Malformed line — count toward summary finding; do not
                // panic, do not abort. Mirrors the silent-skip discipline
                // of `read_invocation_ledger` at `capability_hygiene.rs:1589-1592`.
                counts.malformed_lines += 1;
                continue;
            }
        };

        // Discriminate row kind. Three cases:
        //
        //  1. `entry_kind == "disposition"` → DispositionEntry; classify
        //     by `disposition_kind`.
        //  2. `type == "skill"` (or `name` field present and no
        //     `entry_kind`) → SkillEntry; increment total_invocations.
        //  3. Anything else → silently skip (forward-compat tolerance).
        if parsed.get("entry_kind").and_then(|v| v.as_str()) == Some("disposition") {
            counts.dispositioned_count += 1;
            if let Some(kind) = parsed.get("disposition_kind").and_then(|v| v.as_str()) {
                match kind {
                    "accepted" => counts.accepted_count += 1,
                    "rejected" => counts.rejected_count += 1,
                    "modified" => counts.modified_count += 1,
                    "superseded" => counts.superseded_count += 1,
                    _ => {
                        // Unknown disposition_kind. Forward-compat
                        // tolerance: row counted toward
                        // `dispositioned_count` (it IS a disposition
                        // row by entry_kind) but NOT toward any of the
                        // closed-set per-kind counters. Closed-set
                        // discipline is the schema's job at write
                        // time; the sensor doesn't crash on rows that
                        // slipped through somehow.
                    }
                }
            }
        } else {
            // SkillEntry path. Recognize either the canonical
            // `type == "skill"` discriminator or — for tolerance with
            // the existing PostToolUse hook output that omits the
            // `entry_kind` field — any row that has a `name` field and
            // no `entry_kind`. This matches the existing
            // `read_invocation_ledger` shape.
            let is_skill = parsed.get("type").and_then(|v| v.as_str()) == Some("skill")
                || (parsed.get("name").is_some()
                    && parsed.get("entry_kind").is_none());
            if is_skill {
                counts.total_invocations += 1;
            }
            // else: silently skip — neither a recognized skill nor
            // disposition row. Forward-compat tolerance.
        }
    }

    if counts.malformed_lines > 0 {
        findings.push(Finding {
            name: "operator_calibration:malformed_lines".to_string(),
            status: "neutral".to_string(),
            points: 0,
            detail: Some(format!(
                "Skipped {} malformed JSONL line(s) in `.claude/brain/invocation-ledger.jsonl`. \
                 Forgiveness discipline (Q3 lock) — the sensor never crashes on bad input. \
                 Investigate the writer if the count is non-trivial.",
                counts.malformed_lines
            )),
        });
    }

    counts
}

// ── Score computation (Q4 lock) ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct ScoringBreakdown {
    low_confidence: bool,
    has_ever_dispositioned: bool,
}

fn scoring_breakdown(outcome: &LedgerOutcome) -> ScoringBreakdown {
    let counts = outcome.counts();
    ScoringBreakdown {
        low_confidence: counts.dispositioned_count < N_MIN,
        has_ever_dispositioned: counts.dispositioned_count > 0,
    }
}

/// Compute the score per Q4 lock. Returns `Some(score)` for the
/// non-low-confidence path and `Some(100)` for the no-dispositions-yet
/// advisory floor, OR `None` when the sample size is below N_MIN with
/// at least one disposition (the discoverability null state).
fn compute_score(outcome: &LedgerOutcome, findings: &mut Vec<Finding>) -> Option<u8> {
    let counts = outcome.counts();

    if counts.dispositioned_count == 0 {
        // Advisory floor: no judgment yet, so no negative signal.
        // Optionally emit `no_dispositions_yet` when total_invocations
        // crosses the calibration-smell threshold.
        if counts.total_invocations > NO_DISPOSITIONS_YET_THRESHOLD {
            findings.push(Finding {
                name: "operator_calibration:no_dispositions_yet".to_string(),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(format!(
                    "Observed {} skill invocations but ZERO disposition records. \
                     The operator hasn't engaged with `neurogrim disposition record` \
                     yet — calibration data is empty. Run `neurogrim disposition record \
                     --invocation-id <id> --kind <accepted|rejected|modified|superseded>` \
                     against a recent invocation to start populating the ledger.",
                    counts.total_invocations
                )),
            });
        }
        return Some(100);
    }

    if counts.dispositioned_count < N_MIN {
        // Q9 lock: discoverability null state. Score = null (signal
        // through the type system that the answer is "not enough data
        // yet"); emit one advisory finding to surface the state.
        findings.push(Finding {
            name: "operator_calibration:low_confidence".to_string(),
            status: "neutral".to_string(),
            points: 0,
            detail: Some(format!(
                "dispositioned_count={} below N_MIN={}; score deferred until sample size \
                 is meaningful. Per Q9 lock + LOW_CONFIDENCE_TOTAL_INVOCATIONS precedent in \
                 capability_hygiene.rs:150.",
                counts.dispositioned_count, N_MIN
            )),
        });
        return None;
    }

    // Meaningful-sample-size path. Score = accepted / dispositioned * 100,
    // rounded. The clamp at 100 is defense-in-depth — by construction
    // accepted_count <= dispositioned_count.
    let ratio = counts.accepted_count as f64 / counts.dispositioned_count as f64;
    let score = (ratio * 100.0).round().clamp(0.0, 100.0) as u8;
    Some(score)
}

// ── CMDB extras / breakdown JSON ────────────────────────────────────────────

fn breakdown_json(
    ledger_path: &Path,
    outcome: &LedgerOutcome,
    breakdown: &ScoringBreakdown,
) -> Value {
    let counts = outcome.counts();
    let present = matches!(outcome, LedgerOutcome::Present(_));
    json!({
        "ledger_path": if present {
            Value::String(ledger_path.display().to_string())
        } else {
            Value::Null
        },
        "ledger_present": present,
        "total_invocations": counts.total_invocations,
        "dispositioned_count": counts.dispositioned_count,
        "accepted_count": counts.accepted_count,
        "rejected_count": counts.rejected_count,
        "modified_count": counts.modified_count,
        "superseded_count": counts.superseded_count,
        "low_confidence": breakdown.low_confidence,
        "has_ever_dispositioned": breakdown.has_ever_dispositioned,
        "n_min": N_MIN,
    })
}

// ── Compile-time anchor for the schema embed ────────────────────────────────

#[cfg(test)]
mod schema_anchor_tests {
    use super::OPERATOR_CALIBRATION_SCHEMA_JSON;

    /// Compile-time check that the embedded schema parses as JSON. If
    /// the schema file ever stops being valid JSON the build still
    /// succeeds (because `include_str!` is byte-level) but this test
    /// would catch the regression at test time.
    #[test]
    fn embedded_schema_parses_as_json() {
        let parsed: serde_json::Value =
            serde_json::from_str(OPERATOR_CALIBRATION_SCHEMA_JSON)
                .expect("embedded invocation-ledger schema must parse as JSON");
        assert_eq!(
            parsed.get("title").and_then(|v| v.as_str()),
            Some("Invocation Ledger Entry v1")
        );
    }
}
