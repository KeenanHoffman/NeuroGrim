//! Review-ticket file format + reader/writer.
//!
//! A **review ticket** is a JSON file at
//! `.claude/brain/supply-chain-tickets/<id>.json` representing one
//! Layer 3 review work item. Tickets are open work; the durable
//! record lives in the decision-ledger JSONL.
//!
//! Ticket lifecycle:
//!
//! 1. Created (open):
//!    - Triggered by a Layer 2 vigilance finding (auto-create), OR
//!    - Operator-initiated (CLI: `neurogrim sca-review create ...`).
//!    - At creation, a `review-pending` entry is appended to the
//!      ledger; `review_ticket_id` correlates them.
//! 2. (Optional) Agent review fills in `agent_findings`. v1: this
//!    is operator-driven; the field may stay empty.
//! 3. Resolved:
//!    - Operator runs `neurogrim sca-review resolve --id <id> --decision <kind>`.
//!    - The ticket gains `resolution` + `resolved_at` + `resolution_notes`.
//!    - A `review-triaged` ledger entry is appended that supersedes
//!      the `review-pending` predecessor.
//!
//! The `(ecosystem, package_name, finding_kind)` triple is the
//! dedup key for AUTO-CREATED tickets — repeated Layer 2 scans
//! against the same dep+kind don't open multiple tickets.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::ledger::{PackageRef, TriggeringSignal};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewTicket {
    /// Human-friendly stable id (e.g., `t-2026-04-26-0001` or a
    /// hash-prefix). Used as the filename.
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub package: PackageRef,
    /// Lockfile-resolved version that triggered the review. Optional;
    /// some tickets are about a package globally (not a specific
    /// version).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_version: Option<String>,
    /// Operator-target version (e.g., for pin-to-last-good).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
    /// All Layer 1 / Layer 2 / Layer 3 signals that motivated the
    /// ticket. At creation time at least one signal is present;
    /// later scans may append more (deduped by signal_kind).
    pub triggering_signals: Vec<TriggeringSignal>,
    /// Filled in by the agent-review step (if/when invoked).
    /// v1 in framework-only mode: operator-edited.
    #[serde(default)]
    pub agent_findings: Vec<super::ledger::AgentFinding>,
    /// Operator who opened the ticket. `"auto"` for auto-created
    /// tickets; a real operator handle for CLI-created.
    pub created_by: String,
    /// Optional notes captured at creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creation_notes: Option<String>,
    /// Set on resolve. `None` while ticket is open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    /// One of "accept" | "reject" | "pin-to-last-good" | "no-action".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    /// Operator who resolved (none if still open).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<String>,
    /// Resolution rationale (required at resolve-time).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_notes: Option<String>,
    /// `ts` of the corresponding `review-pending` ledger entry.
    /// Used to construct the matching `review-triaged` entry's
    /// `supersedes_ts` on resolve.
    pub pending_ledger_ts: f64,
    /// Schema version for forward compatibility.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

fn default_schema_version() -> u32 {
    1
}

impl ReviewTicket {
    pub fn is_open(&self) -> bool {
        self.resolved_at.is_none()
    }

    /// Dedup key for auto-create: `(ecosystem, name, kind)`. Two
    /// findings with the same key don't create separate tickets;
    /// the second updates the first's signals list.
    pub fn dedup_key(&self) -> Option<(String, String, String)> {
        let kind = self
            .triggering_signals
            .first()
            .map(|s| s.signal_kind.clone())?;
        Some((self.package.ecosystem.clone(), self.package.name.clone(), kind))
    }
}

/// Default tickets directory relative to the project root.
pub fn default_tickets_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("supply-chain-tickets")
}

/// Load all tickets in the tickets directory. Returns empty Vec
/// if the directory doesn't exist (first-run posture).
pub fn read_all(tickets_dir: &Path) -> Result<Vec<ReviewTicket>> {
    if !tickets_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry_result in fs::read_dir(tickets_dir).context("ticket: read_dir")? {
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("ticket: read_dir entry error: {:#}", e);
                continue;
            }
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        match read_one(&path) {
            Ok(t) => out.push(t),
            Err(e) => {
                tracing::warn!(
                    "ticket: parse failed for {} ({:#})",
                    path.display(),
                    e
                );
            }
        }
    }
    out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(out)
}

pub fn read_one(path: &Path) -> Result<ReviewTicket> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("ticket: read {}", path.display()))?;
    let t: ReviewTicket = serde_json::from_str(&raw).context("ticket: parse JSON")?;
    Ok(t)
}

/// Write a ticket to disk. Atomic via temp-then-rename.
pub fn write_one(tickets_dir: &Path, ticket: &ReviewTicket) -> Result<()> {
    fs::create_dir_all(tickets_dir).context("ticket: mkdir")?;
    let path = tickets_dir.join(format!("{}.json", ticket.id));
    let json = serde_json::to_string_pretty(ticket).context("ticket: serialize")?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).context("ticket: write tmp")?;
    fs::rename(&tmp, &path).context("ticket: rename tmp into place")?;
    Ok(())
}

/// Generate a ticket id of the form `t-YYYY-MM-DD-NNNN` where NNNN
/// is the next available index for the day.
pub fn next_ticket_id(tickets_dir: &Path, now: DateTime<Utc>) -> Result<String> {
    let prefix = format!("t-{}-", now.format("%Y-%m-%d"));
    if !tickets_dir.is_dir() {
        return Ok(format!("{}0001", prefix));
    }
    let mut max_existing: u32 = 0;
    for entry_result in fs::read_dir(tickets_dir).context("ticket: read_dir")? {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = match entry.file_name().to_str().map(|s| s.to_string()) {
            Some(n) => n,
            None => continue,
        };
        if !name.starts_with(&prefix) {
            continue;
        }
        let suffix_with_ext = &name[prefix.len()..];
        let suffix = suffix_with_ext.trim_end_matches(".json");
        if let Ok(n) = suffix.parse::<u32>() {
            if n > max_existing {
                max_existing = n;
            }
        }
    }
    Ok(format!("{}{:04}", prefix, max_existing + 1))
}

/// Find an open ticket matching `(ecosystem, name, finding_kind)`,
/// if any. Used by auto-create to dedup repeated findings.
pub fn find_open_by_dedup_key<'a>(
    tickets: &'a [ReviewTicket],
    ecosystem: &str,
    name: &str,
    finding_kind: &str,
) -> Option<&'a ReviewTicket> {
    tickets.iter().find(|t| {
        t.is_open()
            && t.package.ecosystem == ecosystem
            && t.package.name == name
            && t.triggering_signals
                .iter()
                .any(|s| s.signal_kind == finding_kind)
    })
}

#[cfg(test)]
mod tests {
    use super::super::ledger::PackageRef;
    use super::*;

    fn pkg() -> PackageRef {
        PackageRef {
            name: "fakepkg".to_string(),
            ecosystem: "PyPI".to_string(),
            version_range: None,
        }
    }

    fn signal(kind: &str) -> TriggeringSignal {
        TriggeringSignal {
            layer: "2".to_string(),
            signal_kind: kind.to_string(),
            advisory_id: None,
            source_uri: None,
            confidence: Some(0.7),
        }
    }

    fn open_ticket(id: &str, kind: &str) -> ReviewTicket {
        ReviewTicket {
            id: id.to_string(),
            created_at: Utc::now(),
            package: pkg(),
            from_version: None,
            to_version: None,
            triggering_signals: vec![signal(kind)],
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

    #[test]
    fn write_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let t = open_ticket("t-2026-04-26-0001", "vigilance:typosquat-proximity");
        write_one(dir.path(), &t).unwrap();
        let loaded = read_all(dir.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "t-2026-04-26-0001");
    }

    #[test]
    fn next_id_starts_at_0001() {
        let dir = tempfile::tempdir().unwrap();
        let now = Utc::now();
        let id = next_ticket_id(dir.path(), now).unwrap();
        assert!(id.ends_with("-0001"));
    }

    #[test]
    fn next_id_increments() {
        let dir = tempfile::tempdir().unwrap();
        let now = Utc::now();
        let id1 = next_ticket_id(dir.path(), now).unwrap();
        let mut t = open_ticket(&id1, "vigilance:typosquat-proximity");
        write_one(dir.path(), &t).unwrap();
        let id2 = next_ticket_id(dir.path(), now).unwrap();
        assert_ne!(id1, id2);
        assert!(id2.ends_with("-0002"));
        t.id = id2.clone();
        write_one(dir.path(), &t).unwrap();
        let id3 = next_ticket_id(dir.path(), now).unwrap();
        assert!(id3.ends_with("-0003"));
    }

    #[test]
    fn dedup_finds_open_match() {
        let t1 = open_ticket("t-001", "vigilance:typosquat-proximity");
        let mut t2 = open_ticket("t-002", "vigilance:typosquat-proximity");
        t2.resolved_at = Some(Utc::now());
        t2.resolution = Some("accept".to_string());
        let tickets = vec![t1.clone(), t2];
        let found = find_open_by_dedup_key(
            &tickets,
            "PyPI",
            "fakepkg",
            "vigilance:typosquat-proximity",
        );
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "t-001");
    }

    #[test]
    fn dedup_skips_resolved() {
        let mut t = open_ticket("t-001", "vigilance:typosquat-proximity");
        t.resolved_at = Some(Utc::now());
        t.resolution = Some("accept".to_string());
        let tickets = vec![t];
        let found = find_open_by_dedup_key(
            &tickets,
            "PyPI",
            "fakepkg",
            "vigilance:typosquat-proximity",
        );
        assert!(found.is_none());
    }

    #[test]
    fn dedup_distinguishes_kind() {
        let t = open_ticket("t-001", "vigilance:typosquat-proximity");
        let tickets = vec![t];
        let found_other = find_open_by_dedup_key(
            &tickets,
            "PyPI",
            "fakepkg",
            "vigilance:exfil-indicator",
        );
        assert!(found_other.is_none());
    }
}
