//! Append-only JSONL ledger for per-domain calibration entries
//! (LSP-Brains v2.8 §17, Brains-2.0 E-B2-2).
//!
//! Implements the writer + reader for `domain-calibration-ledger-
//! v1.schema.json`. Two entry kinds: `pending` (an automated
//! calibration trigger fired and produced an observation awaiting
//! human triage) and `triaged` (an operator reviewed the pending
//! entry and recorded a decision; supersedes the pending via
//! `supersedes_ts`).
//!
//! Same 2-phase Pending → Triaged shape as
//! `judge-integrity-ledger-v1` (§15.3),
//! `domain-promotion-ledger-v1` (§15.5), and
//! `supply-chain-decision-ledger-v1` (§16.4) — those three predate
//! v2.8 and are intentionally NOT migrated to this unified schema
//! (§17.10 carve-out).
//!
//! # Append-only discipline
//!
//! Per §17.2 MUST, ledger entries are NEVER edited in place.
//! Triage corrections are recorded as new `triaged` entries that
//! supersede a prior `pending` via `supersedes_ts`. Mirrors the
//! supply-chain-decision-ledger writer.
//!
//! # Atomic writes
//!
//! Each append is a single `OpenOptions::append`-write of one line +
//! newline. Writes ≤ PIPE_BUF (4KB) are POSIX-atomic.
//!
//! # v1 posture
//!
//! Most callers invoke this module via the operator-triage CLI
//! (lands in E-B2-2 C6) or the build_scorecard auto-trigger
//! (E-B2-2 C7). The auto-trigger path is gated by
//! `BrainConfig::enable_calibration_writes` (default false) AND
//! the per-domain `DomainDefinition::calibration_trigger` opt-in.
//! Both must be enabled for an entry to be written automatically.
//! See spec §17.3 for the trigger discriminated union.

use crate::registry::BrainRegistry;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

// ─── Top-level entry enum (mirrors schema's oneOf) ────────────────────

/// One calibration-ledger entry. Discriminated by `entry_kind` to
/// match `domain-calibration-ledger-v1.schema.json`'s `oneOf` shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "entry_kind", rename_all = "kebab-case")]
pub enum LedgerEntry {
    /// An automated trigger fired and produced an observation
    /// awaiting human triage (or an operator hand-created the entry
    /// via the CLI's --manual path).
    Pending(PendingEntry),
    /// An operator reviewed a pending entry and recorded a decision.
    /// Supersedes the pending via `supersedes_ts`.
    Triaged(TriagedEntry),
}

/// `domain_family` enum (matches schema's domain_family enum). v1
/// has a single value; future families add new variants here AND
/// per-family schema definitions dispatched via if/then/else
/// (§17.4).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DomainFamily {
    /// The domain-calibration sensor's own family (§17.9).
    DomainCalibration,
}

impl DomainFamily {
    /// Stable wire-name string, matching the schema enum.
    pub fn as_str(&self) -> &'static str {
        match self {
            DomainFamily::DomainCalibration => "domain-calibration",
        }
    }
}

/// 4-class triage decision enum (§17.5).
///
/// Coarse by design — finer categorization belongs in `human_notes`
/// (verbatim, auditable). The four classes are:
///
/// - `Confirmed` — signal is real and actionable
/// - `Mislabeled` — signal is false; sensor was wrong
/// - `Gap` — signal is real but no rubric mechanism exists to act on it
/// - `NoAction` — reviewed, no action warranted
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TriageDecision {
    Confirmed,
    Mislabeled,
    Gap,
    NoAction,
}

impl TriageDecision {
    /// Stable wire-name string, matching the schema enum.
    pub fn as_str(&self) -> &'static str {
        match self {
            TriageDecision::Confirmed => "confirmed",
            TriageDecision::Mislabeled => "mislabeled",
            TriageDecision::Gap => "gap",
            TriageDecision::NoAction => "no-action",
        }
    }

    /// Parse a wire string into the typed enum. Used by the CLI's
    /// `--decision` arg validation.
    pub fn from_str(s: &str) -> Option<TriageDecision> {
        match s {
            "confirmed" => Some(TriageDecision::Confirmed),
            "mislabeled" => Some(TriageDecision::Mislabeled),
            "gap" => Some(TriageDecision::Gap),
            "no-action" => Some(TriageDecision::NoAction),
            _ => None,
        }
    }
}

// ─── Pending + Triaged entry shapes (mirror schema definitions) ───────

/// Auto-created (or operator-manually-created) observation awaiting
/// triage. Snapshot of the score that triggered + the trigger reason.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingEntry {
    pub ts: f64,
    pub schema_version: String,
    pub domain: String,
    pub domain_family: DomainFamily,
    pub trigger_signal_kind: String,
    pub actual_score: u8,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expected_score_lower: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expected_score_upper: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub context_notes: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub context_artifacts: Vec<String>,
}

/// Operator's resolution of a pending entry. Supersedes via
/// `supersedes_ts`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriagedEntry {
    pub ts: f64,
    pub schema_version: String,
    pub domain: String,
    pub domain_family: DomainFamily,
    pub supersedes_ts: f64,
    pub triage_decision: TriageDecision,
    pub human_operator: String,
    pub human_notes: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub audit_artifacts: Vec<String>,
}

// ─── Calibration trigger discriminated union (config; §17.3) ──────────

/// Per-domain calibration trigger configuration. Lives on
/// `DomainDefinition::calibration_trigger`. The four variants cover
/// threshold-driven domains (`OutOfExpectedRange`), event-driven
/// domains (`SignalClassFired`), operator-only domains (`Manual`,
/// the safe default), and a v2 placeholder for §7-trajectory-based
/// triggers (`TrajectorySwing`, not exercised in v1).
///
/// JSON form uses the `kind` discriminator. Examples:
/// - `{"kind": "out-of-expected-range", "min": 70, "max": 100}`
/// - `{"kind": "signal-class-fired", "signal_kinds": ["pattern:..."]}`
/// - `{"kind": "manual"}`
/// - `{"kind": "trajectory-swing", "window_days": 14, "magnitude": 30}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CalibrationTrigger {
    /// Brain auto-creates `pending` when the domain's effective_score
    /// drops below `min` OR rises above `max`. Threshold-driven.
    /// Operators SHOULD configure this only after observing the
    /// domain's actual score distribution (otherwise it fires on
    /// legitimate signal collapses).
    OutOfExpectedRange {
        /// Inclusive lower bound; scores < this trigger.
        min: u8,
        /// Inclusive upper bound; scores > this trigger.
        max: u8,
    },
    /// Brain auto-creates `pending` when the domain emits a CMDB
    /// finding whose `name` matches one of `signal_kinds` OR an
    /// extras key matches. Event-driven.
    SignalClassFired { signal_kinds: Vec<String> },
    /// No automated triggers — operator-only entries via the CLI.
    /// **Default for new domains.**
    Manual,
    /// v2 placeholder. Δ-from-rolling-baseline using §7 trajectory
    /// primitive. Not exercised in v1; reserved variant so future
    /// configs round-trip cleanly.
    TrajectorySwing { window_days: u32, magnitude: u8 },
}

impl CalibrationTrigger {
    /// Returns true when this trigger SHOULD auto-fire entries on
    /// the build_scorecard hot path (E-B2-2 C7). Manual + v1's
    /// reserved TrajectorySwing return false.
    pub fn auto_fires(&self) -> bool {
        matches!(
            self,
            CalibrationTrigger::OutOfExpectedRange { .. } | CalibrationTrigger::SignalClassFired { .. }
        )
    }
}

// ─── Helper accessors on the LedgerEntry enum ─────────────────────────

impl LedgerEntry {
    /// Stable wire-name of the entry kind.
    pub fn kind(&self) -> &'static str {
        match self {
            LedgerEntry::Pending(_) => "pending",
            LedgerEntry::Triaged(_) => "triaged",
        }
    }

    /// Wall-clock seconds (Unix time, fractional precision).
    pub fn ts(&self) -> f64 {
        match self {
            LedgerEntry::Pending(e) => e.ts,
            LedgerEntry::Triaged(e) => e.ts,
        }
    }

    /// The domain this entry refers to.
    pub fn domain(&self) -> &str {
        match self {
            LedgerEntry::Pending(e) => &e.domain,
            LedgerEntry::Triaged(e) => &e.domain,
        }
    }

    /// Validate intrinsic schema requirements. Does NOT validate the
    /// entry's `domain` against a registry — that's a separate
    /// concern (see `validate_domain_in_registry`).
    pub fn validate(&self) -> Result<()> {
        match self {
            LedgerEntry::Pending(e) => {
                if e.schema_version != "1" {
                    bail!("schema_version must be \"1\"; got {:?}", e.schema_version);
                }
                if e.domain.trim().is_empty() {
                    bail!("domain must be non-empty");
                }
                if e.trigger_signal_kind.trim().is_empty() {
                    bail!("trigger_signal_kind must be non-empty");
                }
                if e.actual_score > 100 {
                    bail!("actual_score must be in [0, 100]; got {}", e.actual_score);
                }
                if let Some(lo) = e.expected_score_lower {
                    if lo > 100 {
                        bail!("expected_score_lower must be in [0, 100]; got {lo}");
                    }
                }
                if let Some(hi) = e.expected_score_upper {
                    if hi > 100 {
                        bail!("expected_score_upper must be in [0, 100]; got {hi}");
                    }
                }
            }
            LedgerEntry::Triaged(e) => {
                if e.schema_version != "1" {
                    bail!("schema_version must be \"1\"; got {:?}", e.schema_version);
                }
                if e.domain.trim().is_empty() {
                    bail!("domain must be non-empty");
                }
                if e.human_operator.trim().is_empty() {
                    bail!(
                        "human_operator must be non-empty (set NEUROGRIM_OPERATOR or pass --operator)"
                    );
                }
                if e.human_notes.trim().is_empty() {
                    bail!("human_notes must be non-empty (audit-rationale discipline; spec §17.5)");
                }
            }
        }
        Ok(())
    }
}

// ─── Public writer + reader API ───────────────────────────────────────

/// Append a new entry to the ledger file at `path`. Validates
/// schema-equivalent intrinsic constraints before writing; creates
/// the file (and parent directory) if missing.
///
/// Atomic write semantics: single OpenOptions::append-write of
/// `line + '\n'`. Writes ≤ PIPE_BUF (4KB) are POSIX-atomic.
///
/// **Does NOT validate the entry's `domain` against a registry.**
/// Callers that have a registry on hand SHOULD call
/// [`validate_domain_in_registry`] first.
pub fn append(path: &Path, entry: &LedgerEntry) -> Result<()> {
    entry.validate().context("calibration ledger entry validation")?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("calibration ledger: create parent dir")?;
    }

    let line = serde_json::to_string(entry).context("calibration ledger: serialize entry")?;
    if line.contains('\n') {
        bail!("calibration ledger entry must serialize to a single line; got multi-line JSON");
    }

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("calibration ledger: open {} for append", path.display()))?;
    writeln!(f, "{}", line).context("calibration ledger: write line")?;
    f.flush().context("calibration ledger: flush")?;
    Ok(())
}

/// Validate that `domain` is a known key in the operator's
/// `BrainRegistry`. Per §17.2, the registry is the authoritative
/// domain enum — entries referencing unknown domains are rejected
/// at write time.
pub fn validate_domain_in_registry(domain: &str, registry: &BrainRegistry) -> Result<()> {
    if registry.config.domain_weights.contains_key(domain) {
        Ok(())
    } else {
        bail!(
            "calibration ledger: domain '{domain}' is not declared in brain-registry.json's \
             domain_weights — the registry is the authoritative domain enum (§17.2). \
             Known domains: [{}]",
            registry
                .config
                .domain_weights
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Read the entire ledger from `path`. Returns empty Vec if the
/// file doesn't exist (first-run posture).
///
/// Malformed lines are logged + skipped, not propagated as errors —
/// the goal is to never block the sensor on a bad ledger entry.
/// Same posture as `supply_chain_review::ledger::read_all`.
pub fn read_all(path: &Path) -> Result<Vec<LedgerEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let f = File::open(path)
        .with_context(|| format!("calibration ledger: open {}", path.display()))?;
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for (i, line_result) in reader.lines().enumerate() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("calibration ledger: read error on line {}: {:#}", i + 1, e);
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<LedgerEntry>(&line) {
            Ok(entry) => out.push(entry),
            Err(e) => {
                tracing::warn!(
                    "calibration ledger: parse failed on line {} ({:#}); content: {}",
                    i + 1,
                    e,
                    line.chars().take(120).collect::<String>()
                );
            }
        }
    }
    Ok(out)
}

/// Read only the OPEN pending entries — those without a superseding
/// triaged entry. Convenience wrapper around `read_all` + `fold`.
///
/// Used by the CLI's `list --open-only` action.
pub fn read_open_only(path: &Path) -> Result<Vec<PendingEntry>> {
    let entries = read_all(path)?;
    Ok(fold(&entries).open_pending)
}

/// Folded ledger state — open pending entries (no superseding triaged)
/// + last-known triaged entries per domain.
#[derive(Debug, Default)]
pub struct CalibrationFold {
    /// Pending entries that have NOT been triaged yet.
    pub open_pending: Vec<PendingEntry>,
    /// All triaged entries, in chronological order.
    pub triaged: Vec<TriagedEntry>,
}

/// Fold a stream of ledger entries into the latest-state view.
/// A pending entry is "open" if no later triaged entry references
/// its `ts` via `supersedes_ts`.
pub fn fold(entries: &[LedgerEntry]) -> CalibrationFold {
    let mut pending_by_ts: std::collections::BTreeMap<u64, PendingEntry> =
        std::collections::BTreeMap::new();
    let mut superseded_ts: std::collections::BTreeSet<u64> =
        std::collections::BTreeSet::new();
    let mut triaged: Vec<TriagedEntry> = Vec::new();

    // Sort by ts ascending (ties broken by insertion order).
    let mut sorted: Vec<&LedgerEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.ts().partial_cmp(&b.ts()).unwrap_or(std::cmp::Ordering::Equal));

    for entry in sorted {
        match entry {
            LedgerEntry::Pending(p) => {
                pending_by_ts.insert(ts_key(p.ts), p.clone());
            }
            LedgerEntry::Triaged(t) => {
                superseded_ts.insert(ts_key(t.supersedes_ts));
                triaged.push(t.clone());
            }
        }
    }

    let open_pending: Vec<PendingEntry> = pending_by_ts
        .into_iter()
        .filter_map(|(k, v)| if superseded_ts.contains(&k) { None } else { Some(v) })
        .collect();

    CalibrationFold {
        open_pending,
        triaged,
    }
}

/// Convert an f64 ts to a stable u64 key for hash-map use (avoids
/// the f64-not-Hash problem). 1µs precision; matches supply-chain's
/// pattern at `supply_chain_review::ledger::fold`.
fn ts_key(ts: f64) -> u64 {
    (ts * 1e6) as u64
}

// ─── Operator identity guard ──────────────────────────────────────────

/// Resolve the operator handle. Per §17.6:
/// 1. `--operator <handle>` CLI arg (highest precedence)
/// 2. `NEUROGRIM_OPERATOR` env var
/// 3. Reject — operator identity is REQUIRED on triaged entries.
///
/// Returns `Ok(handle)` on success. On failure, the error message
/// names both the CLI arg and the env var so the operator knows
/// where to set the value.
pub fn resolve_operator(cli_arg: Option<&str>) -> Result<String> {
    if let Some(handle) = cli_arg {
        let trimmed = handle.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    match std::env::var("NEUROGRIM_OPERATOR") {
        Ok(v) if !v.trim().is_empty() => Ok(v.trim().to_string()),
        _ => bail!(
            "calibration ledger: operator identity REQUIRED on triaged entries (§17.6). \
             Set NEUROGRIM_OPERATOR env var or pass --operator <handle>."
        ),
    }
}

// ─── Path + timestamp helpers ─────────────────────────────────────────

/// Default ledger path for `domain` relative to `project_root`.
/// Pattern: `<project_root>/.claude/brain/<domain>-calibration-ledger.jsonl`.
pub fn default_ledger_path(project_root: &Path, domain: &str) -> PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join(format!("{domain}-calibration-ledger.jsonl"))
}

/// Construct the current Unix-seconds timestamp used by all entries.
pub fn now_ts() -> f64 {
    let now: DateTime<Utc> = Utc::now();
    now.timestamp() as f64 + (now.timestamp_subsec_nanos() as f64) / 1e9
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pending(ts: f64, domain: &str) -> PendingEntry {
        PendingEntry {
            ts,
            schema_version: "1".to_string(),
            domain: domain.to_string(),
            domain_family: DomainFamily::DomainCalibration,
            trigger_signal_kind: "out-of-range".to_string(),
            actual_score: 30,
            expected_score_lower: Some(70),
            expected_score_upper: Some(100),
            context_notes: Some("test fixture".to_string()),
            context_artifacts: vec![],
        }
    }

    fn make_triaged(ts: f64, supersedes_ts: f64, domain: &str) -> TriagedEntry {
        TriagedEntry {
            ts,
            schema_version: "1".to_string(),
            domain: domain.to_string(),
            domain_family: DomainFamily::DomainCalibration,
            supersedes_ts,
            triage_decision: TriageDecision::NoAction,
            human_operator: "test-operator".to_string(),
            human_notes: "Score drop was a deliberate test-suite restructure.".to_string(),
            audit_artifacts: vec![],
        }
    }

    // ─── Validation tests ──────────────────────────────────────────

    #[test]
    fn validate_pending_happy_path() {
        let entry = LedgerEntry::Pending(make_pending(1.0, "test-health"));
        assert!(entry.validate().is_ok());
    }

    #[test]
    fn validate_pending_rejects_empty_domain() {
        let entry = LedgerEntry::Pending(make_pending(1.0, ""));
        assert!(entry.validate().is_err());
    }

    #[test]
    fn validate_pending_rejects_empty_trigger_signal_kind() {
        let mut p = make_pending(1.0, "test-health");
        p.trigger_signal_kind = "".to_string();
        assert!(LedgerEntry::Pending(p).validate().is_err());
    }

    #[test]
    fn validate_pending_rejects_actual_score_above_100() {
        let mut p = make_pending(1.0, "test-health");
        p.actual_score = 150;
        assert!(LedgerEntry::Pending(p).validate().is_err());
    }

    #[test]
    fn validate_triaged_happy_path() {
        let entry = LedgerEntry::Triaged(make_triaged(2.0, 1.0, "test-health"));
        assert!(entry.validate().is_ok());
    }

    #[test]
    fn validate_triaged_rejects_empty_human_operator() {
        let mut t = make_triaged(2.0, 1.0, "test-health");
        t.human_operator = "".to_string();
        assert!(LedgerEntry::Triaged(t).validate().is_err());
    }

    #[test]
    fn validate_triaged_rejects_empty_human_notes() {
        let mut t = make_triaged(2.0, 1.0, "test-health");
        t.human_notes = "".to_string();
        assert!(LedgerEntry::Triaged(t).validate().is_err());
    }

    #[test]
    fn validate_rejects_wrong_schema_version() {
        let mut p = make_pending(1.0, "test-health");
        p.schema_version = "2".to_string();
        assert!(LedgerEntry::Pending(p).validate().is_err());
    }

    // ─── Append + read roundtrip ──────────────────────────────────

    #[test]
    fn append_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-health-calibration-ledger.jsonl");

        let pending = LedgerEntry::Pending(make_pending(1.0, "test-health"));
        append(&path, &pending).unwrap();

        let triaged = LedgerEntry::Triaged(make_triaged(2.0, 1.0, "test-health"));
        append(&path, &triaged).unwrap();

        let entries = read_all(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind(), "pending");
        assert_eq!(entries[1].kind(), "triaged");
        assert_eq!(entries[0].domain(), "test-health");
    }

    #[test]
    fn read_all_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.jsonl");
        let entries = read_all(&path).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn read_all_skips_malformed_lines() {
        // A bad line in the middle of good lines must not blow up
        // the reader. Same posture as the supply-chain ledger.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-health-calibration-ledger.jsonl");
        let pending = LedgerEntry::Pending(make_pending(1.0, "test-health"));
        append(&path, &pending).unwrap();

        // Write a malformed line directly.
        use std::io::Write;
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "this is not JSON {{").unwrap();
        f.flush().unwrap();

        let triaged = LedgerEntry::Triaged(make_triaged(3.0, 1.0, "test-health"));
        append(&path, &triaged).unwrap();

        let entries = read_all(&path).unwrap();
        // Two valid entries; the malformed line is silently skipped.
        assert_eq!(entries.len(), 2);
    }

    // ─── Fold + open-only ──────────────────────────────────────────

    #[test]
    fn fold_marks_pending_open_when_no_supersede() {
        let p = make_pending(1.0, "test-health");
        let entries = vec![LedgerEntry::Pending(p.clone())];
        let folded = fold(&entries);
        assert_eq!(folded.open_pending.len(), 1);
        assert_eq!(folded.triaged.len(), 0);
    }

    #[test]
    fn fold_marks_pending_closed_when_superseded() {
        let p = make_pending(1.0, "test-health");
        let t = make_triaged(2.0, 1.0, "test-health");
        let entries = vec![LedgerEntry::Pending(p), LedgerEntry::Triaged(t)];
        let folded = fold(&entries);
        assert_eq!(folded.open_pending.len(), 0);
        assert_eq!(folded.triaged.len(), 1);
    }

    #[test]
    fn fold_keeps_unsuperseded_pending_when_other_triaged() {
        // Two pendings, only one triaged: the un-triaged pending stays open.
        let p1 = make_pending(1.0, "test-health");
        let p2 = make_pending(3.0, "test-health");
        let t1 = make_triaged(2.0, 1.0, "test-health");
        let entries = vec![
            LedgerEntry::Pending(p1),
            LedgerEntry::Triaged(t1),
            LedgerEntry::Pending(p2),
        ];
        let folded = fold(&entries);
        assert_eq!(folded.open_pending.len(), 1);
        assert!((folded.open_pending[0].ts - 3.0).abs() < 1e-6);
    }

    #[test]
    fn read_open_only_filters_to_unsuperseded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-health-calibration-ledger.jsonl");
        let p1 = LedgerEntry::Pending(make_pending(1.0, "test-health"));
        let t1 = LedgerEntry::Triaged(make_triaged(2.0, 1.0, "test-health"));
        let p2 = LedgerEntry::Pending(make_pending(3.0, "test-health"));
        append(&path, &p1).unwrap();
        append(&path, &t1).unwrap();
        append(&path, &p2).unwrap();

        let open = read_open_only(&path).unwrap();
        assert_eq!(open.len(), 1);
        assert!((open[0].ts - 3.0).abs() < 1e-6);
    }

    // ─── Operator identity ─────────────────────────────────────────

    #[test]
    fn resolve_operator_prefers_cli_arg() {
        let result = resolve_operator(Some("alice"));
        assert_eq!(result.unwrap(), "alice");
    }

    #[test]
    fn resolve_operator_trims_cli_arg() {
        let result = resolve_operator(Some("  alice  "));
        assert_eq!(result.unwrap(), "alice");
    }

    #[test]
    fn resolve_operator_rejects_empty_cli_arg_then_falls_through() {
        // Empty CLI arg → fall through to env var (which is unset
        // in this test). Result must be Err.
        std::env::remove_var("NEUROGRIM_OPERATOR");
        let result = resolve_operator(Some(""));
        assert!(result.is_err());
    }

    #[test]
    fn resolve_operator_uses_env_var_when_no_cli_arg() {
        std::env::set_var("NEUROGRIM_OPERATOR", "bob");
        let result = resolve_operator(None);
        assert_eq!(result.unwrap(), "bob");
        std::env::remove_var("NEUROGRIM_OPERATOR");
    }

    #[test]
    fn resolve_operator_fails_when_neither_set() {
        std::env::remove_var("NEUROGRIM_OPERATOR");
        let result = resolve_operator(None);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("NEUROGRIM_OPERATOR"));
        assert!(msg.contains("--operator"));
    }

    // ─── CalibrationTrigger helpers ────────────────────────────────

    #[test]
    fn calibration_trigger_auto_fires_correctly() {
        assert!(CalibrationTrigger::OutOfExpectedRange { min: 70, max: 100 }.auto_fires());
        assert!(CalibrationTrigger::SignalClassFired {
            signal_kinds: vec!["test".to_string()]
        }
        .auto_fires());
        assert!(!CalibrationTrigger::Manual.auto_fires());
        assert!(!CalibrationTrigger::TrajectorySwing {
            window_days: 14,
            magnitude: 30
        }
        .auto_fires());
    }

    #[test]
    fn calibration_trigger_serde_roundtrip() {
        let triggers = vec![
            CalibrationTrigger::OutOfExpectedRange { min: 70, max: 100 },
            CalibrationTrigger::SignalClassFired {
                signal_kinds: vec!["pattern:foo".to_string()],
            },
            CalibrationTrigger::Manual,
            CalibrationTrigger::TrajectorySwing {
                window_days: 14,
                magnitude: 30,
            },
        ];
        for t in triggers {
            let json = serde_json::to_string(&t).unwrap();
            let parsed: CalibrationTrigger = serde_json::from_str(&json).unwrap();
            assert_eq!(t, parsed, "round-trip failed: {json}");
        }
    }

    // ─── TriageDecision helpers ────────────────────────────────────

    #[test]
    fn triage_decision_round_trip_via_str() {
        for d in [
            TriageDecision::Confirmed,
            TriageDecision::Mislabeled,
            TriageDecision::Gap,
            TriageDecision::NoAction,
        ] {
            let s = d.as_str();
            let parsed = TriageDecision::from_str(s).expect("known enum");
            assert_eq!(d, parsed);
        }
    }

    #[test]
    fn triage_decision_unknown_returns_none() {
        assert!(TriageDecision::from_str("escalate").is_none());
        assert!(TriageDecision::from_str("").is_none());
    }

    // ─── default_ledger_path ───────────────────────────────────────

    #[test]
    fn default_ledger_path_uses_brain_subdir() {
        let p = default_ledger_path(Path::new("/proj"), "test-health");
        let s = p.to_string_lossy().replace('\\', "/");
        assert!(s.ends_with("/proj/.claude/brain/test-health-calibration-ledger.jsonl"));
    }

    // ─── validate_domain_in_registry ───────────────────────────────

    #[test]
    fn validate_domain_in_registry_accepts_known() {
        // Registry's domain_weights is the authoritative domain enum.
        let registry_json = serde_json::json!({
            "meta": {
                "schema_version": "2",
                "description": "test fixture",
                "updated_by": "test"
            },
            "config": {
                "domain_weights": {
                    "test-health": 0.5,
                    "code-quality": 0.5
                },
                "advisory_domains": [],
                "principle_map": {},
                "domain_definitions": {}
            }
        });
        let registry: BrainRegistry = serde_json::from_value(registry_json).unwrap();
        assert!(validate_domain_in_registry("test-health", &registry).is_ok());
        assert!(validate_domain_in_registry("code-quality", &registry).is_ok());
    }

    #[test]
    fn validate_domain_in_registry_rejects_unknown() {
        // Q12 (Layer-2 plan): writer rejects unknown domain strings.
        // The registry IS the authoritative domain enum.
        let registry_json = serde_json::json!({
            "meta": {
                "schema_version": "2",
                "description": "test fixture",
                "updated_by": "test"
            },
            "config": {
                "domain_weights": {
                    "test-health": 1.0
                },
                "advisory_domains": [],
                "principle_map": {},
                "domain_definitions": {}
            }
        });
        let registry: BrainRegistry = serde_json::from_value(registry_json).unwrap();
        let result = validate_domain_in_registry("hallucinated-domain", &registry);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("hallucinated-domain"),
            "error message must name the rejected domain; got: {msg}"
        );
        assert!(
            msg.contains("test-health"),
            "error message should list known domains for the operator; got: {msg}"
        );
    }
}
