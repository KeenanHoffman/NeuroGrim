//! Supply-chain Layer 3 — agent-assisted human review framework.
//!
//! Implements LSP-Brains v2.6 §16.4. Five components:
//!
//! 1. **`supply-chain-auditor` hat** — declared in
//!    `.claude/skills/hats/SKILL.md`.
//! 2. **Decision ledger** (`ledger.rs`) — append-only JSONL at
//!    `.claude/supply-chain-decision-ledger.jsonl` matching
//!    `supply-chain-decision-ledger-v1.schema.json`.
//! 3. **Review tickets** (`ticket.rs`) — JSON files at
//!    `.claude/brain/supply-chain-tickets/<id>.json` holding open
//!    review work.
//! 4. **`supply-chain-review` CMDB sensor** (this module's
//!    orchestrator) — reports score 100-(10 × open_tickets), capped 0.
//! 5. **CLI subcommands** (`neurogrim sca-review create | list |
//!    resolve`) — operator-driven ticket lifecycle.
//!
//! # Auto-create from Layer 2 vigilance findings
//!
//! When `supply_chain_vigilance::scan` produces findings,
//! `auto_create_from_vigilance` in this module ingests them and
//! creates one ticket per `(ecosystem, package_name, finding_kind)`
//! that doesn't already have an open ticket. Repeated scans across
//! days don't multiply tickets — the dedup key is stable.
//!
//! # Spec §16.4 conformance
//!
//! - **Read-only static analysis MUST**: this framework never
//!   downloads or extracts package source. Tickets carry diffs +
//!   excerpts only when an agent-review step (out of v1 scope)
//!   produces them.
//! - **Decisions in append-only ledger MUST**: `ledger::append` is
//!   the only write path; entries are validated against the §16.7
//!   schema before commit.
//! - **Human decision is the gate**: the `resolve` CLI requires
//!   `--operator` (or env-default) + non-empty `--note`. No
//!   automation in v1 makes resolution decisions.
//! - **Append-only discipline**: triage is recorded as a
//!   `review-triaged` entry referencing the prior `review-pending`
//!   via `supersedes_ts`. Originals are never edited.

pub mod ledger;
pub mod scoring;
pub mod ticket;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

// =========================================================================
// MCP server wrapper (mirrors supply_chain_sca / supply_chain_vigilance)
// =========================================================================

#[derive(Debug, Clone)]
pub struct SupplyChainReviewServer {
    tool_router: ToolRouter<Self>,
}

impl SupplyChainReviewServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for SupplyChainReviewServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckSupplyChainReviewParams {
    /// Filesystem path to the project's root directory. Looks for
    /// `.claude/brain/supply-chain-tickets/` and
    /// `.claude/supply-chain-decision-ledger.jsonl`.
    pub project_root: String,
}

#[tool_router]
impl SupplyChainReviewServer {
    #[tool(
        description = "Run native-Rust supply-chain Layer 3 review CMDB sensor \
        (LSP-Brains v2.6 §16.4). Reads the operator-curated review tickets at \
        .claude/brain/supply-chain-tickets/ and the append-only decision ledger \
        at .claude/supply-chain-decision-ledger.jsonl. Score model (v1): \
        100 - 10 × open_tickets, floor 0. Default weight 0.0 (advisory) per \
        §16.4 — promotion past advisory requires §15.5-equivalent calibration \
        evidence (E-SC-8). NO LLM invocation in v1; agent_findings are \
        operator-edited via `neurogrim sca-review resolve` CLI. Tickets are \
        auto-created from Layer 2 vigilance findings (dedup by ecosystem / \
        package / finding_kind)."
    )]
    async fn check_supply_chain_review(
        &self,
        Parameters(p): Parameters<CheckSupplyChainReviewParams>,
    ) -> String {
        serde_json::to_string_pretty(&analyze_supply_chain_review(&p.project_root))
            .unwrap_or_default()
    }
}

impl ServerHandler for SupplyChainReviewServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Native-Rust supply-chain Layer 3 review framework. \
                 Hat + decision-ledger + review-ticket files + CMDB sensor + \
                 auto-create from Layer 2. Advisory weight by default. \
                 Read-only static analysis only — no LLM invocation in v1."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

/// Primary sensor entry point. Reads tickets + ledger + emits CMDB.
pub fn analyze_supply_chain_review(project_root: &str) -> Value {
    let root = Path::new(project_root);
    // Fall back to parent if .claude/ isn't at project_root (mirrors
    // supply_chain_sca's workspace-subdir handling).
    let claude_root = if root.join(".claude").is_dir() {
        root.to_path_buf()
    } else if root.parent().map(|p| p.join(".claude").is_dir()).unwrap_or(false) {
        root.parent().unwrap().to_path_buf()
    } else {
        root.to_path_buf()
    };

    let tickets_dir = ticket::default_tickets_dir(&claude_root);
    let ledger_path = ledger::default_ledger_path(&claude_root);

    let tickets = ticket::read_all(&tickets_dir).unwrap_or_else(|e| {
        tracing::warn!("supply-chain-review: read tickets failed: {:#}", e);
        Vec::new()
    });
    let entries = ledger::read_all(&ledger_path).unwrap_or_else(|e| {
        tracing::warn!("supply-chain-review: read ledger failed: {:#}", e);
        Vec::new()
    });
    let folded = ledger::fold(&entries);
    let score_value = scoring::score(&tickets);
    scoring::build_cmdb_envelope(score_value, &tickets, &entries, &folded)
}

// =========================================================================
// Auto-create bridge from Layer 2 vigilance
// =========================================================================

use crate::supply_chain_vigilance::scoring::VigilanceFinding;

/// For each Layer 2 vigilance finding, ensure there's an open
/// ticket for `(ecosystem, package, finding_kind)`. New tickets
/// auto-created with `created_by: "auto"`. Pre-existing open
/// tickets are NOT mutated (dedup); resolved tickets matching the
/// dedup key DO produce a fresh ticket (the operator already
/// decided once; if the signal recurs, that's a new event worth a
/// fresh review).
///
/// Returns the count of NEWLY created tickets so the orchestrator
/// can surface it in the vigilance CMDB extras.
pub fn auto_create_from_vigilance(
    findings: &[VigilanceFinding],
    project_root: &Path,
) -> Result<usize, anyhow::Error> {
    if findings.is_empty() {
        return Ok(0);
    }
    let claude_root = if project_root.join(".claude").is_dir() {
        project_root.to_path_buf()
    } else if project_root
        .parent()
        .map(|p| p.join(".claude").is_dir())
        .unwrap_or(false)
    {
        project_root.parent().unwrap().to_path_buf()
    } else {
        project_root.to_path_buf()
    };
    let tickets_dir = ticket::default_tickets_dir(&claude_root);
    let ledger_path = ledger::default_ledger_path(&claude_root);

    let existing_tickets = ticket::read_all(&tickets_dir).unwrap_or_default();
    let mut created = 0usize;

    for f in findings {
        // Skip the informational sensor-degradation kind.
        if matches!(
            f.kind,
            crate::supply_chain_vigilance::scoring::VigilanceKind::SensorDegradation
        ) {
            continue;
        }
        let signal_kind = format!("vigilance:{}", f.kind.as_str());

        // Dedup against open tickets.
        if ticket::find_open_by_dedup_key(
            &existing_tickets,
            f.package.ecosystem,
            &f.package.name,
            &signal_kind,
        )
        .is_some()
        {
            continue;
        }

        let now = chrono::Utc::now();
        let id = ticket::next_ticket_id(&tickets_dir, now)?;
        let triggering_signal = ledger::TriggeringSignal {
            layer: "2".to_string(),
            signal_kind: signal_kind.clone(),
            advisory_id: None,
            source_uri: None,
            confidence: Some(f.confidence as f64),
        };
        let pending_ts = ledger::now_ts();
        let pending_entry = ledger::LedgerEntry::ReviewPending(ledger::ReviewPendingEntry {
            ts: pending_ts,
            schema_version: "1".to_string(),
            package: ledger::PackageRef {
                name: f.package.name.clone(),
                ecosystem: f.package.ecosystem.to_string(),
                version_range: None,
            },
            from_version: Some(f.package.version.clone()),
            to_version: None,
            triggering_signals: vec![triggering_signal.clone()],
            agent_findings: vec![],
            human_operator: Some("auto".to_string()),
            human_notes: Some(format!("Auto-created from Layer 2 vigilance: {}", f.summary)),
            audit_reports: vec![],
            review_ticket_id: Some(id.clone()),
        });
        ledger::append(&ledger_path, &pending_entry)?;

        let ticket_obj = ticket::ReviewTicket {
            id: id.clone(),
            created_at: now,
            package: ledger::PackageRef {
                name: f.package.name.clone(),
                ecosystem: f.package.ecosystem.to_string(),
                version_range: None,
            },
            from_version: Some(f.package.version.clone()),
            to_version: None,
            triggering_signals: vec![triggering_signal],
            agent_findings: vec![],
            created_by: "auto".to_string(),
            creation_notes: Some(f.summary.clone()),
            resolved_at: None,
            resolution: None,
            resolved_by: None,
            resolution_notes: None,
            pending_ledger_ts: pending_ts,
            schema_version: 1,
        };
        ticket::write_one(&tickets_dir, &ticket_obj)?;
        created += 1;
    }

    Ok(created)
}

// =========================================================================
// CLI helpers (called from neurogrim-cli)
// =========================================================================

/// Operator-driven ticket creation (`neurogrim sca-review create`).
pub fn cli_create(
    project_root: &Path,
    package_ecosystem: &str,
    package_name: &str,
    package_version: Option<&str>,
    signal_kind: &str,
    note: &str,
    operator: &str,
) -> Result<String, anyhow::Error> {
    let claude_root = resolve_claude_root(project_root);
    let tickets_dir = ticket::default_tickets_dir(&claude_root);
    let ledger_path = ledger::default_ledger_path(&claude_root);
    let now = chrono::Utc::now();
    let id = ticket::next_ticket_id(&tickets_dir, now)?;
    let triggering = ledger::TriggeringSignal {
        layer: "3".to_string(),
        signal_kind: signal_kind.to_string(),
        advisory_id: None,
        source_uri: None,
        confidence: None,
    };
    let pending_ts = ledger::now_ts();
    let pending = ledger::LedgerEntry::ReviewPending(ledger::ReviewPendingEntry {
        ts: pending_ts,
        schema_version: "1".to_string(),
        package: ledger::PackageRef {
            name: package_name.to_string(),
            ecosystem: package_ecosystem.to_string(),
            version_range: None,
        },
        from_version: package_version.map(String::from),
        to_version: None,
        triggering_signals: vec![triggering.clone()],
        agent_findings: vec![],
        human_operator: Some(operator.to_string()),
        human_notes: Some(note.to_string()),
        audit_reports: vec![],
        review_ticket_id: Some(id.clone()),
    });
    ledger::append(&ledger_path, &pending)?;

    let t = ticket::ReviewTicket {
        id: id.clone(),
        created_at: now,
        package: ledger::PackageRef {
            name: package_name.to_string(),
            ecosystem: package_ecosystem.to_string(),
            version_range: None,
        },
        from_version: package_version.map(String::from),
        to_version: None,
        triggering_signals: vec![triggering],
        agent_findings: vec![],
        created_by: operator.to_string(),
        creation_notes: Some(note.to_string()),
        resolved_at: None,
        resolution: None,
        resolved_by: None,
        resolution_notes: None,
        pending_ledger_ts: pending_ts,
        schema_version: 1,
    };
    ticket::write_one(&tickets_dir, &t)?;
    Ok(id)
}

/// `neurogrim sca-review list` — print tickets to a writer.
pub fn cli_list(
    project_root: &Path,
    only_open: bool,
    out: &mut dyn std::io::Write,
) -> Result<usize, anyhow::Error> {
    let claude_root = resolve_claude_root(project_root);
    let tickets_dir = ticket::default_tickets_dir(&claude_root);
    let tickets = ticket::read_all(&tickets_dir)?;
    let filtered: Vec<&ticket::ReviewTicket> = if only_open {
        tickets.iter().filter(|t| t.is_open()).collect()
    } else {
        tickets.iter().collect()
    };
    if filtered.is_empty() {
        writeln!(out, "(no {} tickets)", if only_open { "open" } else { "" }.trim())?;
        return Ok(0);
    }
    writeln!(
        out,
        "{:<22} {:<10} {:<32} {:<10} {:<10} {}",
        "ID", "STATUS", "PACKAGE", "ECO", "OPENED", "SIGNALS"
    )?;
    for t in &filtered {
        let signals: Vec<&str> = t
            .triggering_signals
            .iter()
            .map(|s| s.signal_kind.as_str())
            .collect();
        writeln!(
            out,
            "{:<22} {:<10} {:<32} {:<10} {:<10} {}",
            t.id,
            if t.is_open() { "OPEN" } else { "RESOLVED" },
            t.package.name,
            t.package.ecosystem,
            t.created_at.format("%Y-%m-%d"),
            signals.join(", "),
        )?;
    }
    Ok(filtered.len())
}

/// `neurogrim sca-review resolve` — close an open ticket.
#[allow(clippy::too_many_arguments)]
pub fn cli_resolve(
    project_root: &Path,
    ticket_id: &str,
    decision: &str,
    note: &str,
    operator: &str,
    from_version: Option<&str>,
    to_version: Option<&str>,
) -> Result<(), anyhow::Error> {
    if !matches!(decision, "accept" | "reject" | "pin-to-last-good" | "no-action") {
        anyhow::bail!(
            "decision must be one of: accept | reject | pin-to-last-good | no-action; got {decision:?}"
        );
    }
    if note.trim().is_empty() {
        anyhow::bail!("--note must be non-empty (operator must document the rationale)");
    }
    let claude_root = resolve_claude_root(project_root);
    let tickets_dir = ticket::default_tickets_dir(&claude_root);
    let ledger_path = ledger::default_ledger_path(&claude_root);

    // Load the ticket.
    let path = tickets_dir.join(format!("{}.json", ticket_id));
    let mut t = ticket::read_one(&path)
        .with_context(|| format!("ticket {} not found at {}", ticket_id, path.display()))?;
    if !t.is_open() {
        anyhow::bail!(
            "ticket {} already resolved at {} (resolution: {})",
            t.id,
            t.resolved_at
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            t.resolution.as_deref().unwrap_or("?")
        );
    }
    let now = chrono::Utc::now();
    let triaged = ledger::LedgerEntry::ReviewTriaged(ledger::ReviewTriagedEntry {
        ts: ledger::now_ts(),
        schema_version: "1".to_string(),
        package: t.package.clone(),
        from_version: from_version.map(String::from).or_else(|| t.from_version.clone()),
        to_version: to_version.map(String::from).or_else(|| t.to_version.clone()),
        supersedes_ts: t.pending_ledger_ts,
        resolution: decision.to_string(),
        triggering_signals: t.triggering_signals.clone(),
        agent_findings: t.agent_findings.clone(),
        human_operator: operator.to_string(),
        human_notes: note.to_string(),
        audit_reports: vec![],
        review_ticket_id: Some(t.id.clone()),
    });
    ledger::append(&ledger_path, &triaged)?;

    // Update the ticket file.
    t.resolved_at = Some(now);
    t.resolution = Some(decision.to_string());
    t.resolved_by = Some(operator.to_string());
    t.resolution_notes = Some(note.to_string());
    if let Some(fv) = from_version {
        t.from_version = Some(fv.to_string());
    }
    if let Some(tv) = to_version {
        t.to_version = Some(tv.to_string());
    }
    ticket::write_one(&tickets_dir, &t)?;
    Ok(())
}

fn resolve_claude_root(project_root: &Path) -> std::path::PathBuf {
    if project_root.join(".claude").is_dir() {
        project_root.to_path_buf()
    } else if project_root
        .parent()
        .map(|p| p.join(".claude").is_dir())
        .unwrap_or(false)
    {
        project_root.parent().unwrap().to_path_buf()
    } else {
        project_root.to_path_buf()
    }
}

use anyhow::Context as _;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supply_chain_sca::Package;
    use crate::supply_chain_vigilance::scoring::{VigilanceFinding, VigilanceKind};

    #[test]
    fn analyze_empty_returns_score_100() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let result = analyze_supply_chain_review(dir.path().to_str().unwrap());
        assert_eq!(result.get("score").and_then(|v| v.as_u64()), Some(100));
    }

    #[test]
    fn cli_create_then_list_open() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let id = cli_create(
            dir.path(),
            "PyPI",
            "litellm",
            Some("1.82.7"),
            "manual:operator-spotted",
            "high-base64-payload count noticed in upstream diff",
            "alice",
        )
        .unwrap();
        let mut buf: Vec<u8> = Vec::new();
        let count = cli_list(dir.path(), true, &mut buf).unwrap();
        assert_eq!(count, 1);
        let s = String::from_utf8_lossy(&buf).to_string();
        assert!(s.contains(&id), "ID should appear in list output: {s}");
        assert!(s.contains("OPEN"), "status should be OPEN: {s}");
    }

    #[test]
    fn cli_resolve_closes_ticket() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let id = cli_create(
            dir.path(),
            "PyPI",
            "litellm",
            Some("1.82.7"),
            "manual:test",
            "test note",
            "alice",
        )
        .unwrap();
        cli_resolve(
            dir.path(),
            &id,
            "pin-to-last-good",
            "Pinned to 1.82.6 pending upstream context",
            "alice",
            Some("1.82.7"),
            Some("1.82.6"),
        )
        .unwrap();
        let mut buf: Vec<u8> = Vec::new();
        let open_count = cli_list(dir.path(), true, &mut buf).unwrap();
        assert_eq!(open_count, 0);
        let mut buf: Vec<u8> = Vec::new();
        let all_count = cli_list(dir.path(), false, &mut buf).unwrap();
        assert_eq!(all_count, 1);
    }

    #[test]
    fn cli_resolve_rejects_invalid_decision() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let id = cli_create(
            dir.path(),
            "PyPI",
            "litellm",
            Some("1.82.7"),
            "manual:test",
            "test note",
            "alice",
        )
        .unwrap();
        let r = cli_resolve(
            dir.path(),
            &id,
            "auto-yolo",
            "n/a",
            "alice",
            None,
            None,
        );
        assert!(r.is_err());
    }

    #[test]
    fn cli_resolve_requires_non_empty_note() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let id = cli_create(
            dir.path(),
            "PyPI",
            "litellm",
            Some("1.82.7"),
            "manual:test",
            "test note",
            "alice",
        )
        .unwrap();
        let r = cli_resolve(dir.path(), &id, "accept", "", "alice", None, None);
        assert!(r.is_err());
    }

    #[test]
    fn cli_resolve_rejects_already_resolved() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let id = cli_create(
            dir.path(),
            "PyPI",
            "litellm",
            Some("1.82.7"),
            "manual:test",
            "test note",
            "alice",
        )
        .unwrap();
        cli_resolve(dir.path(), &id, "accept", "ok", "alice", None, None).unwrap();
        let r = cli_resolve(dir.path(), &id, "accept", "ok again", "alice", None, None);
        assert!(r.is_err());
    }

    #[test]
    fn auto_create_creates_then_dedups() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let finding = VigilanceFinding {
            kind: VigilanceKind::TyposquatProximity,
            package: Package::pypi("litelm", "1.0.0"),
            summary: "name is 1 edit from 'litellm'".to_string(),
            evidence: None,
            confidence: 0.7,
        };
        let n1 = auto_create_from_vigilance(&[finding.clone()], dir.path()).unwrap();
        assert_eq!(n1, 1);
        // Second call with same finding: dedup → 0 new tickets.
        let n2 = auto_create_from_vigilance(&[finding], dir.path()).unwrap();
        assert_eq!(n2, 0);
    }

    #[test]
    fn auto_create_skips_sensor_degradation() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let finding = VigilanceFinding {
            kind: VigilanceKind::SensorDegradation,
            package: Package::pypi("anything", "1.0.0"),
            summary: "info".to_string(),
            evidence: None,
            confidence: 0.0,
        };
        let n = auto_create_from_vigilance(&[finding], dir.path()).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn auto_create_after_resolve_creates_fresh_ticket() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let finding = VigilanceFinding {
            kind: VigilanceKind::TyposquatProximity,
            package: Package::pypi("litelm", "1.0.0"),
            summary: "test".to_string(),
            evidence: None,
            confidence: 0.7,
        };
        let n1 = auto_create_from_vigilance(&[finding.clone()], dir.path()).unwrap();
        assert_eq!(n1, 1);
        // Resolve all open tickets.
        let claude_root = resolve_claude_root(dir.path());
        let tickets_dir = ticket::default_tickets_dir(&claude_root);
        let tickets = ticket::read_all(&tickets_dir).unwrap();
        for t in &tickets {
            cli_resolve(
                dir.path(),
                &t.id,
                "no-action",
                "fp",
                "alice",
                None,
                None,
            )
            .unwrap();
        }
        // Same finding fires again → fresh ticket (operator already
        // decided once; recurrence is a new event).
        let n2 = auto_create_from_vigilance(&[finding], dir.path()).unwrap();
        assert_eq!(n2, 1);
    }
}
