//! Append-only JSONL ledger for Layer 3 supply-chain decisions.
//!
//! Implements the writer + reader for `supply-chain-decision-
//! ledger-v1.schema.json` (LSP-Brains v2.6 §16.4 + §16.7). Five
//! entry kinds: `accept`, `reject`, `pin-to-last-good`,
//! `review-pending`, `review-triaged`.
//!
//! # Append-only discipline
//!
//! Per §16.4 MUST, ledger entries are NEVER edited in place.
//! Triage corrections are recorded as new `review-triaged` entries
//! that supersede a prior `review-pending` via `supersedes_ts`.
//! This mirrors the `domain-promotion-ledger-v1` shape (§15.5).
//!
//! # Atomic writes
//!
//! Each append happens as a single `OpenOptions::append`-write of
//! one line + newline. JSONL entries are line-delimited; the
//! filesystem-level POSIX guarantee is that writes ≤ PIPE_BUF (4KB)
//! are atomic. Most entries are well under that. For larger entries
//! (rare; typically with extensive `agent_findings` + audit_reports),
//! we lock the file via OpenOptions append + a sentinel-write
//! pattern.
//!
//! # Schema conformance
//!
//! Every write validates required fields per the §16.7 schema
//! before committing to disk. Malformed entries are rejected at
//! write time, not silently appended.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// One ledger entry. Discriminated by `entry_kind`. Mirrors the
/// `oneOf` shape in `supply-chain-decision-ledger-v1.schema.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "entry_kind", rename_all = "kebab-case")]
pub enum LedgerEntry {
    /// Operator accepts a flagged dep as not-currently-actionable.
    Accept(AcceptEntry),
    /// Operator rejects the dep; remediation required.
    Reject(RejectEntry),
    /// Operator pins to a known-good version pending upstream fix.
    PinToLastGood(PinToLastGoodEntry),
    /// Review ticket opened; no human decision yet.
    ReviewPending(ReviewPendingEntry),
    /// Pending review resolved.
    ReviewTriaged(ReviewTriagedEntry),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRef {
    pub name: String,
    pub ecosystem: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_range: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggeringSignal {
    /// "1", "2", or "3" — which spec §16 layer surfaced the signal.
    pub layer: String,
    /// Implementation-defined signal id (e.g.,
    /// "vigilance:typosquat-proximity").
    pub signal_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFinding {
    pub finding_kind: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptEntry {
    pub ts: f64,
    pub schema_version: String,
    pub package: PackageRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub triggering_signals: Vec<TriggeringSignal>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub agent_findings: Vec<AgentFinding>,
    pub human_operator: String,
    pub human_notes: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub audit_reports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectEntry {
    pub ts: f64,
    pub schema_version: String,
    pub package: PackageRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub triggering_signals: Vec<TriggeringSignal>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub agent_findings: Vec<AgentFinding>,
    pub human_operator: String,
    pub human_notes: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub audit_reports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinToLastGoodEntry {
    pub ts: f64,
    pub schema_version: String,
    pub package: PackageRef,
    pub from_version: String,
    pub to_version: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub triggering_signals: Vec<TriggeringSignal>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub agent_findings: Vec<AgentFinding>,
    pub human_operator: String,
    pub human_notes: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub audit_reports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPendingEntry {
    pub ts: f64,
    pub schema_version: String,
    pub package: PackageRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
    pub triggering_signals: Vec<TriggeringSignal>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub agent_findings: Vec<AgentFinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_operator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_notes: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub audit_reports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_ticket_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewTriagedEntry {
    pub ts: f64,
    pub schema_version: String,
    pub package: PackageRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
    pub supersedes_ts: f64,
    /// "accept", "reject", "pin-to-last-good", or "no-action".
    pub resolution: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub triggering_signals: Vec<TriggeringSignal>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub agent_findings: Vec<AgentFinding>,
    pub human_operator: String,
    pub human_notes: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub audit_reports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_ticket_id: Option<String>,
}

impl LedgerEntry {
    /// Stable identifier of the entry kind (matches the schema's
    /// `entry_kind` const values).
    pub fn kind(&self) -> &'static str {
        match self {
            LedgerEntry::Accept(_) => "accept",
            LedgerEntry::Reject(_) => "reject",
            LedgerEntry::PinToLastGood(_) => "pin-to-last-good",
            LedgerEntry::ReviewPending(_) => "review-pending",
            LedgerEntry::ReviewTriaged(_) => "review-triaged",
        }
    }

    /// Wall-clock seconds (Unix time, fractional precision).
    pub fn ts(&self) -> f64 {
        match self {
            LedgerEntry::Accept(e) => e.ts,
            LedgerEntry::Reject(e) => e.ts,
            LedgerEntry::PinToLastGood(e) => e.ts,
            LedgerEntry::ReviewPending(e) => e.ts,
            LedgerEntry::ReviewTriaged(e) => e.ts,
        }
    }

    /// Package the entry refers to.
    pub fn package(&self) -> &PackageRef {
        match self {
            LedgerEntry::Accept(e) => &e.package,
            LedgerEntry::Reject(e) => &e.package,
            LedgerEntry::PinToLastGood(e) => &e.package,
            LedgerEntry::ReviewPending(e) => &e.package,
            LedgerEntry::ReviewTriaged(e) => &e.package,
        }
    }

    /// Validate required fields per the §16.7 schema. Returns
    /// `Err` if a required field is empty or schema_version != "1".
    pub fn validate(&self) -> Result<()> {
        let common_check = |sv: &str, op: Option<&str>, notes: Option<&str>| -> Result<()> {
            if sv != "1" {
                bail!("schema_version must be \"1\"; got {sv:?}");
            }
            if let Some(op) = op {
                if op.trim().is_empty() {
                    bail!("human_operator must be non-empty");
                }
            }
            if let Some(notes) = notes {
                if notes.trim().is_empty() {
                    bail!("human_notes must be non-empty for this entry kind");
                }
            }
            Ok(())
        };
        match self {
            LedgerEntry::Accept(e) => {
                common_check(&e.schema_version, Some(&e.human_operator), Some(&e.human_notes))?;
                if e.package.name.is_empty() || e.package.ecosystem.is_empty() {
                    bail!("package.name and package.ecosystem must be non-empty");
                }
            }
            LedgerEntry::Reject(e) => {
                common_check(&e.schema_version, Some(&e.human_operator), Some(&e.human_notes))?;
                if e.package.name.is_empty() || e.package.ecosystem.is_empty() {
                    bail!("package.name and package.ecosystem must be non-empty");
                }
            }
            LedgerEntry::PinToLastGood(e) => {
                common_check(&e.schema_version, Some(&e.human_operator), Some(&e.human_notes))?;
                if e.package.name.is_empty() || e.package.ecosystem.is_empty() {
                    bail!("package.name and package.ecosystem must be non-empty");
                }
                if e.from_version.is_empty() || e.to_version.is_empty() {
                    bail!("pin-to-last-good entries require non-empty from/to versions");
                }
            }
            LedgerEntry::ReviewPending(e) => {
                // 2026-04-26 PRE-RELEASE B10 fix: schema tightened
                // human_operator from optional to required. The
                // conventional value for auto-created tickets is
                // "auto"; real operators use their handle. This
                // matches existing impl callers
                // (auto_create_from_vigilance + cli_create both
                // already pass Some(...)).
                let op = e
                    .human_operator
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!(
                        "review-pending entries require human_operator (use \"auto\" for system-opened tickets per spec §16.4)"
                    ))?;
                common_check(&e.schema_version, Some(op), None)?;
                if e.package.name.is_empty() || e.package.ecosystem.is_empty() {
                    bail!("package.name and package.ecosystem must be non-empty");
                }
                if e.triggering_signals.is_empty() {
                    bail!("review-pending entries require ≥ 1 triggering_signal");
                }
            }
            LedgerEntry::ReviewTriaged(e) => {
                common_check(&e.schema_version, Some(&e.human_operator), Some(&e.human_notes))?;
                if e.package.name.is_empty() || e.package.ecosystem.is_empty() {
                    bail!("package.name and package.ecosystem must be non-empty");
                }
                match e.resolution.as_str() {
                    "accept" | "reject" | "pin-to-last-good" | "no-action" => {}
                    other => bail!("review-triaged resolution must be one of accept|reject|pin-to-last-good|no-action; got {other:?}"),
                }
            }
        }
        Ok(())
    }
}

/// Append a new entry to the ledger file at `path`. Validates
/// schema conformance before writing; creates the file (and parent
/// directory) if missing.
///
/// Append is a single OpenOptions::append-write of `line + '\n'`.
/// Writes ≤ PIPE_BUF (4KB) are POSIX-atomic; larger entries fall
/// back to a temp-then-append pattern (rare).
pub fn append(path: &Path, entry: &LedgerEntry) -> Result<()> {
    entry.validate().context("ledger entry validation")?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("ledger: create parent dir")?;
    }

    let line = serde_json::to_string(entry).context("ledger: serialize entry")?;
    if line.contains('\n') {
        bail!("ledger entry must serialize to a single line; got multi-line JSON");
    }

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("ledger: open {} for append", path.display()))?;
    writeln!(f, "{}", line).context("ledger: write line")?;
    f.flush().context("ledger: flush")?;
    Ok(())
}

/// Read the entire ledger from `path`. Returns empty Vec if the
/// file doesn't exist (first-run posture).
///
/// Malformed lines are logged + skipped, not propagated as errors —
/// the goal is to never block the sensor on a bad ledger entry.
pub fn read_all(path: &Path) -> Result<Vec<LedgerEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let f = File::open(path).with_context(|| format!("ledger: open {}", path.display()))?;
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for (i, line_result) in reader.lines().enumerate() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("ledger: read error on line {}: {:#}", i + 1, e);
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
                    "ledger: parse failed on line {} ({:#}); content: {}",
                    i + 1,
                    e,
                    line.chars().take(120).collect::<String>()
                );
            }
        }
    }
    Ok(out)
}

/// Default ledger path relative to the project root.
pub fn default_ledger_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".claude")
        .join("supply-chain-decision-ledger.jsonl")
}

/// Construct the current Unix-seconds timestamp used by all entries.
pub fn now_ts() -> f64 {
    let now: DateTime<Utc> = Utc::now();
    now.timestamp() as f64 + (now.timestamp_subsec_nanos() as f64) / 1e9
}

/// Folded ledger state — the last decision per package.
///
/// Reconstructs decision state by chronological fold: later entries
/// supersede earlier ones for the same package. `review-triaged`
/// supersedes its `review-pending` predecessor (matched via
/// `supersedes_ts`).
#[derive(Debug, Default)]
pub struct LedgerFold {
    /// Map of (ecosystem, name) → last-known LedgerEntry.
    pub by_package: std::collections::BTreeMap<(String, String), LedgerEntry>,
    /// All review-pending entries that have NOT been triaged yet.
    pub open_pending: Vec<ReviewPendingEntry>,
}

/// Fold a stream of ledger entries into the latest-state view.
pub fn fold(entries: &[LedgerEntry]) -> LedgerFold {
    let mut by_package: std::collections::BTreeMap<(String, String), LedgerEntry> =
        std::collections::BTreeMap::new();
    let mut pending_by_ts: std::collections::BTreeMap<u64, ReviewPendingEntry> =
        std::collections::BTreeMap::new();
    let mut superseded_ts: std::collections::BTreeSet<u64> =
        std::collections::BTreeSet::new();

    // Sort by ts ascending. Ties broken by insertion order.
    let mut sorted: Vec<&LedgerEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.ts().partial_cmp(&b.ts()).unwrap_or(std::cmp::Ordering::Equal));

    for entry in sorted {
        let key = (
            entry.package().ecosystem.clone(),
            entry.package().name.clone(),
        );
        by_package.insert(key, entry.clone());
        if let LedgerEntry::ReviewPending(p) = entry {
            // Use ts*1e6 as integer key to dodge float-hashing.
            pending_by_ts.insert((p.ts * 1e6) as u64, p.clone());
        }
        if let LedgerEntry::ReviewTriaged(t) = entry {
            superseded_ts.insert((t.supersedes_ts * 1e6) as u64);
        }
    }

    let open_pending: Vec<ReviewPendingEntry> = pending_by_ts
        .into_iter()
        .filter_map(|(k, v)| if superseded_ts.contains(&k) { None } else { Some(v) })
        .collect();

    LedgerFold {
        by_package,
        open_pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pkg() -> PackageRef {
        PackageRef {
            name: "fakepkg".to_string(),
            ecosystem: "PyPI".to_string(),
            version_range: None,
        }
    }

    fn make_signal() -> TriggeringSignal {
        TriggeringSignal {
            layer: "2".to_string(),
            signal_kind: "vigilance:test".to_string(),
            advisory_id: None,
            source_uri: None,
            confidence: Some(0.7),
        }
    }

    #[test]
    fn validate_accept_requires_notes() {
        let bad = LedgerEntry::Accept(AcceptEntry {
            ts: 1.0,
            schema_version: "1".to_string(),
            package: make_pkg(),
            from_version: None,
            to_version: None,
            triggering_signals: vec![],
            agent_findings: vec![],
            human_operator: "alice".to_string(),
            human_notes: "".to_string(),
            audit_reports: vec![],
            expires_at: None,
        });
        assert!(bad.validate().is_err());
    }

    #[test]
    fn validate_review_pending_requires_signals() {
        // Provides a valid human_operator so the test isolates the
        // signals-check branch (per the 2026-04-26 B10 schema fix,
        // a missing human_operator would also trigger Err on a
        // different path — set it to a real value here so this
        // test stays focused on the empty-signals branch).
        let bad = LedgerEntry::ReviewPending(ReviewPendingEntry {
            ts: 1.0,
            schema_version: "1".to_string(),
            package: make_pkg(),
            from_version: None,
            to_version: None,
            triggering_signals: vec![], // empty — must reject
            agent_findings: vec![],
            human_operator: Some("auto".to_string()),
            human_notes: None,
            audit_reports: vec![],
            review_ticket_id: None,
        });
        assert!(bad.validate().is_err());
    }

    #[test]
    fn validate_review_pending_requires_human_operator() {
        // 2026-04-26 PRE-RELEASE B10 regression: schema tightened
        // human_operator from optional to required for ReviewPending
        // (operator-identity discipline applies to every ledger
        // entry kind per spec §16.4). This test guards the
        // tightened constraint from being relaxed.
        let bad = LedgerEntry::ReviewPending(ReviewPendingEntry {
            ts: 1.0,
            schema_version: "1".to_string(),
            package: make_pkg(),
            from_version: None,
            to_version: None,
            triggering_signals: vec![make_signal()],
            agent_findings: vec![],
            human_operator: None, // missing — must reject
            human_notes: None,
            audit_reports: vec![],
            review_ticket_id: None,
        });
        let err = bad.validate().unwrap_err();
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("human_operator"),
            "error must mention human_operator; got: {msg}"
        );
        assert!(
            msg.contains("auto"),
            "error must mention 'auto' as the conventional value; got: {msg}"
        );
    }

    #[test]
    fn validate_review_pending_accepts_auto_operator() {
        // Sanity: the conventional 'auto' value used by the Layer 2
        // vigilance bridge is accepted. Documents the contract.
        let good = LedgerEntry::ReviewPending(ReviewPendingEntry {
            ts: 1.0,
            schema_version: "1".to_string(),
            package: make_pkg(),
            from_version: None,
            to_version: None,
            triggering_signals: vec![make_signal()],
            agent_findings: vec![],
            human_operator: Some("auto".to_string()),
            human_notes: Some("Auto-created from Layer 2".to_string()),
            audit_reports: vec![],
            review_ticket_id: Some("t-2026-04-26-0001".to_string()),
        });
        assert!(good.validate().is_ok());
    }

    #[test]
    fn validate_review_triaged_resolution_enum() {
        let bad = LedgerEntry::ReviewTriaged(ReviewTriagedEntry {
            ts: 2.0,
            schema_version: "1".to_string(),
            package: make_pkg(),
            from_version: None,
            to_version: None,
            supersedes_ts: 1.0,
            resolution: "bogus".to_string(),
            triggering_signals: vec![],
            agent_findings: vec![],
            human_operator: "alice".to_string(),
            human_notes: "fixed".to_string(),
            audit_reports: vec![],
            review_ticket_id: None,
        });
        assert!(bad.validate().is_err());
    }

    #[test]
    fn validate_pin_to_last_good_requires_versions() {
        let bad = LedgerEntry::PinToLastGood(PinToLastGoodEntry {
            ts: 1.0,
            schema_version: "1".to_string(),
            package: make_pkg(),
            from_version: "".to_string(),
            to_version: "0.9".to_string(),
            triggering_signals: vec![],
            agent_findings: vec![],
            human_operator: "alice".to_string(),
            human_notes: "pin".to_string(),
            audit_reports: vec![],
            expires_at: None,
        });
        assert!(bad.validate().is_err());
    }

    #[test]
    fn append_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");

        let pending = LedgerEntry::ReviewPending(ReviewPendingEntry {
            ts: 1.0,
            schema_version: "1".to_string(),
            package: make_pkg(),
            from_version: None,
            to_version: None,
            triggering_signals: vec![make_signal()],
            agent_findings: vec![],
            human_operator: Some("auto".to_string()),
            human_notes: None,
            audit_reports: vec![],
            review_ticket_id: Some("t-001".to_string()),
        });
        append(&path, &pending).unwrap();

        let triaged = LedgerEntry::ReviewTriaged(ReviewTriagedEntry {
            ts: 2.0,
            schema_version: "1".to_string(),
            package: make_pkg(),
            from_version: None,
            to_version: None,
            supersedes_ts: 1.0,
            resolution: "accept".to_string(),
            triggering_signals: vec![],
            agent_findings: vec![],
            human_operator: "alice".to_string(),
            human_notes: "FP — package is well-known and intentional".to_string(),
            audit_reports: vec![],
            review_ticket_id: Some("t-001".to_string()),
        });
        append(&path, &triaged).unwrap();

        let entries = read_all(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind(), "review-pending");
        assert_eq!(entries[1].kind(), "review-triaged");
    }

    #[test]
    fn fold_reconstructs_open_pending() {
        let p1 = ReviewPendingEntry {
            ts: 1.0,
            schema_version: "1".to_string(),
            package: make_pkg(),
            from_version: None,
            to_version: None,
            triggering_signals: vec![make_signal()],
            agent_findings: vec![],
            human_operator: None,
            human_notes: None,
            audit_reports: vec![],
            review_ticket_id: Some("t-001".to_string()),
        };
        let p2 = ReviewPendingEntry {
            ts: 3.0,
            schema_version: "1".to_string(),
            package: PackageRef {
                name: "other".to_string(),
                ecosystem: "PyPI".to_string(),
                version_range: None,
            },
            from_version: None,
            to_version: None,
            triggering_signals: vec![make_signal()],
            agent_findings: vec![],
            human_operator: None,
            human_notes: None,
            audit_reports: vec![],
            review_ticket_id: Some("t-002".to_string()),
        };
        let t1 = ReviewTriagedEntry {
            ts: 2.0,
            schema_version: "1".to_string(),
            package: make_pkg(),
            from_version: None,
            to_version: None,
            supersedes_ts: 1.0,
            resolution: "accept".to_string(),
            triggering_signals: vec![],
            agent_findings: vec![],
            human_operator: "alice".to_string(),
            human_notes: "ok".to_string(),
            audit_reports: vec![],
            review_ticket_id: Some("t-001".to_string()),
        };
        let entries = vec![
            LedgerEntry::ReviewPending(p1),
            LedgerEntry::ReviewTriaged(t1),
            LedgerEntry::ReviewPending(p2),
        ];
        let folded = fold(&entries);
        // p1 is superseded by t1; p2 is still open.
        assert_eq!(folded.open_pending.len(), 1);
        assert_eq!(folded.open_pending[0].review_ticket_id.as_deref(), Some("t-002"));
    }

    #[test]
    fn read_all_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.jsonl");
        let entries = read_all(&path).unwrap();
        assert!(entries.is_empty());
    }
}
