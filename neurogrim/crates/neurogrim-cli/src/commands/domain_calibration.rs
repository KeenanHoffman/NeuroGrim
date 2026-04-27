//! `neurogrim domain-calibration` — operator-triage CLI for the
//! per-domain calibration ledger (LSP-Brains v2.8 §17).
//!
//! Three sub-commands:
//!
//! - **`list`** — print pending + recently triaged entries from one
//!   domain's ledger or all domain ledgers under
//!   `<project_root>/.claude/brain/`. Operators inspect this to find
//!   the `ts` of the pending entry they want to triage.
//! - **`triage`** — record a triage decision against an open pending
//!   entry. Appends a `triaged` ledger entry that supersedes the
//!   pending via `supersedes_ts`. The 4-class decision enum is
//!   validated at parse time so typos like `--decision yolo` fail
//!   fast.
//! - **`manual`** — append a new `pending` entry by operator
//!   initiative. The default path for domains whose
//!   `calibration_trigger` is `Manual` (the safe default for new
//!   domains; spec §17.3) — no automated triggers, all entries
//!   start here.
//!
//! All write paths require operator identity (per §17.6) via
//! `--operator <handle>` or `NEUROGRIM_OPERATOR`. Both unset → error,
//! not "unknown" fallback (the calibration-ledger writer rejects
//! empty operator strings — audit-rationale discipline).
//!
//! All write paths validate the `--domain` argument against
//! `<project_root>/.claude/brain-registry.json`'s
//! `domain_weights`, the authoritative domain enum (§17.2). Unknown
//! domain → error.

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use neurogrim_core::calibration_ledger::{
    self, default_ledger_path, fold, now_ts, read_all, resolve_operator,
    validate_domain_in_registry, DomainFamily, LedgerEntry, PendingEntry, TriageDecision,
    TriagedEntry,
};
use neurogrim_core::registry::BrainRegistry;
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug)]
pub enum DomainCalibrationCmd {
    /// List entries from one or all calibration ledgers.
    List {
        /// Project root path.
        #[arg(long, default_value = ".")]
        project_root: String,
        /// Limit to a single domain's ledger. When omitted, scans all
        /// `<project_root>/.claude/brain/*-calibration-ledger*.jsonl`.
        #[arg(long)]
        domain: Option<String>,
        /// Show only OPEN pending entries (default: show open + triaged).
        #[arg(long)]
        open_only: bool,
    },

    /// Record a triage decision against an open pending entry.
    Triage {
        /// Project root path.
        #[arg(long, default_value = ".")]
        project_root: String,
        /// Domain whose ledger contains the pending entry.
        #[arg(long)]
        domain: String,
        /// `ts` of the pending entry to triage (Unix seconds, fractional precision).
        /// Match exactly the value emitted by `list`.
        #[arg(long, value_name = "FLOAT")]
        pending_ts: f64,
        /// Triage decision (one of confirmed | mislabeled | gap | no-action).
        ///
        /// Validated at clap parse time via PossibleValuesParser so
        /// typos fail with clap's standard "invalid value" error
        /// instead of reaching the ledger writer.
        #[arg(
            long,
            value_parser = clap::builder::PossibleValuesParser::new([
                "confirmed", "mislabeled", "gap", "no-action",
            ]),
        )]
        decision: String,
        /// Verbatim audit rationale (required, non-empty per §17.5).
        #[arg(long)]
        note: String,
        /// Operator handle. Falls back to `$NEUROGRIM_OPERATOR`. Both
        /// unset → error per §17.6 (no "unknown" fallback).
        #[arg(long)]
        operator: Option<String>,
    },

    /// Append a new operator-initiated pending entry.
    ///
    /// Default path for domains whose `calibration_trigger` is
    /// `Manual` (the safe default for new domains; spec §17.3).
    Manual {
        /// Project root path.
        #[arg(long, default_value = ".")]
        project_root: String,
        /// Domain to log against. Validated against `brain-registry.json`'s
        /// `domain_weights` (§17.2 — registry is authoritative domain enum).
        #[arg(long)]
        domain: String,
        /// Trigger signal kind (e.g. "manual:operator-spotted-anomaly").
        /// Stored verbatim as `trigger_signal_kind` so audit trails
        /// reconstruct what the operator was responding to.
        #[arg(long, default_value = "manual")]
        signal: String,
        /// The effective_score that prompted the manual entry [0,100].
        #[arg(long)]
        actual_score: u8,
        /// Optional context note attached to the pending entry. Stored
        /// in `context_notes`.
        #[arg(long)]
        context: Option<String>,
        /// Operator handle. Falls back to `$NEUROGRIM_OPERATOR`. Both
        /// unset → error per §17.6 (audit-rationale discipline).
        #[arg(long)]
        operator: Option<String>,
    },
}

pub async fn run(subcommand: DomainCalibrationCmd) -> Result<()> {
    match subcommand {
        DomainCalibrationCmd::List {
            project_root,
            domain,
            open_only,
        } => cmd_list(&project_root, domain.as_deref(), open_only),
        DomainCalibrationCmd::Triage {
            project_root,
            domain,
            pending_ts,
            decision,
            note,
            operator,
        } => cmd_triage(
            &project_root,
            &domain,
            pending_ts,
            &decision,
            &note,
            operator.as_deref(),
        ),
        DomainCalibrationCmd::Manual {
            project_root,
            domain,
            signal,
            actual_score,
            context,
            operator,
        } => cmd_manual(
            &project_root,
            &domain,
            &signal,
            actual_score,
            context.as_deref(),
            operator.as_deref(),
        ),
    }
}

// ─── List ────────────────────────────────────────────────────────────

fn cmd_list(project_root: &str, domain: Option<&str>, open_only: bool) -> Result<()> {
    let root = Path::new(project_root);
    let entries = if let Some(d) = domain {
        // Single-domain path: read just that ledger file (rotation-aware
        // glob is the multi-domain path; this one assumes the operator
        // already knows which file).
        let path = default_ledger_path(root, d);
        read_all(&path).with_context(|| format!("read calibration ledger for {d}"))?
    } else {
        // Multi-domain path: scan .claude/brain/ for any
        // *-calibration-ledger*.jsonl (matches sensor's glob, including
        // §17.7 rotation form `<domain>-calibration-ledger-<year>.jsonl`).
        scan_all_ledgers(root)?
    };

    let folded = fold(&entries);

    if folded.open_pending.is_empty() && (open_only || folded.triaged.is_empty()) {
        println!(
            "{}",
            "(no entries — calibration ledger is clean)".dimmed()
        );
        return Ok(());
    }

    if !folded.open_pending.is_empty() {
        println!("{} ({}):", "OPEN PENDING".yellow().bold(), folded.open_pending.len());
        for p in &folded.open_pending {
            print_pending(p);
        }
    } else {
        println!("{}", "OPEN PENDING (0)".dimmed());
    }

    if !open_only && !folded.triaged.is_empty() {
        println!();
        println!("{} ({}):", "RECENTLY TRIAGED".green().bold(), folded.triaged.len());
        for t in &folded.triaged {
            print_triaged(t);
        }
    }

    Ok(())
}

fn print_pending(p: &PendingEntry) {
    println!(
        "  domain={}  ts={}  trigger={}  score={}",
        p.domain.cyan(),
        format!("{:.3}", p.ts),
        p.trigger_signal_kind,
        p.actual_score,
    );
    if let (Some(lo), Some(hi)) = (p.expected_score_lower, p.expected_score_upper) {
        println!("    expected_range=[{lo}, {hi}]");
    }
    if let Some(notes) = &p.context_notes {
        println!("    context: {notes}");
    }
}

fn print_triaged(t: &TriagedEntry) {
    println!(
        "  domain={}  ts={}  decision={}  operator={}  supersedes={}",
        t.domain.cyan(),
        format!("{:.3}", t.ts),
        t.triage_decision.as_str().green(),
        t.human_operator,
        format!("{:.3}", t.supersedes_ts),
    );
    println!("    note: {}", t.human_notes);
}

/// Walk `<project_root>/.claude/brain/` and read every
/// `*-calibration-ledger*.jsonl` file (handles §17.7 rotation form).
/// Aggregates entries across domains; returns empty Vec when the
/// brain directory doesn't exist.
fn scan_all_ledgers(project_root: &Path) -> Result<Vec<LedgerEntry>> {
    let brain_dir = project_root.join(".claude").join("brain");
    if !brain_dir.exists() {
        return Ok(Vec::new());
    }
    let mut all = Vec::new();
    for entry in std::fs::read_dir(&brain_dir)
        .with_context(|| format!("read_dir {}", brain_dir.display()))?
    {
        let entry = entry.context("read_dir entry")?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        // Match base form `<domain>-calibration-ledger.jsonl` and
        // rotation form `<domain>-calibration-ledger-<year>.jsonl`.
        if !name.contains("-calibration-ledger") || !name.ends_with(".jsonl") {
            continue;
        }
        let entries = read_all(&path)
            .with_context(|| format!("read calibration ledger {}", path.display()))?;
        all.extend(entries);
    }
    Ok(all)
}

// ─── Triage ──────────────────────────────────────────────────────────

fn cmd_triage(
    project_root: &str,
    domain: &str,
    pending_ts: f64,
    decision_str: &str,
    note: &str,
    operator_arg: Option<&str>,
) -> Result<()> {
    if note.trim().is_empty() {
        anyhow::bail!(
            "triage --note must be non-empty (audit-rationale discipline; spec §17.5)"
        );
    }
    let operator = resolve_operator(operator_arg)?;
    let registry = load_registry(project_root)?;
    validate_domain_in_registry(domain, &registry)?;

    let ledger_path = default_ledger_path(Path::new(project_root), domain);
    let existing = read_all(&ledger_path)
        .with_context(|| format!("read calibration ledger for {domain}"))?;
    let folded = fold(&existing);

    // The pending entry being triaged MUST exist AND still be open.
    // Use a small comparison tolerance for f64 round-trip from CLI
    // string → JSON-emit-and-reparse precision.
    const TS_EPS: f64 = 1e-6;
    let target = folded
        .open_pending
        .iter()
        .find(|p| (p.ts - pending_ts).abs() < TS_EPS)
        .with_context(|| {
            format!(
                "no OPEN pending entry with ts={} found for domain {} \
                 (run `neurogrim domain-calibration list --domain {}` to inspect)",
                pending_ts, domain, domain
            )
        })?;

    let decision = TriageDecision::from_str(decision_str).with_context(|| {
        format!(
            "unrecognized decision {:?}; expected one of: confirmed | mislabeled | gap | no-action",
            decision_str
        )
    })?;

    let triaged = TriagedEntry {
        ts: now_ts(),
        schema_version: "1".to_string(),
        domain: domain.to_string(),
        domain_family: DomainFamily::DomainCalibration,
        supersedes_ts: target.ts,
        triage_decision: decision,
        human_operator: operator.clone(),
        human_notes: note.to_string(),
        audit_artifacts: vec![],
    };
    calibration_ledger::append(&ledger_path, &LedgerEntry::Triaged(triaged))
        .context("append triaged entry")?;

    println!(
        "{} {} pending {} (domain={}) → {} by {}",
        "✓".green(),
        "triaged".green(),
        format!("{:.3}", target.ts),
        domain,
        decision.as_str().bold(),
        operator,
    );
    Ok(())
}

// ─── Manual ──────────────────────────────────────────────────────────

fn cmd_manual(
    project_root: &str,
    domain: &str,
    signal: &str,
    actual_score: u8,
    context_arg: Option<&str>,
    operator_arg: Option<&str>,
) -> Result<()> {
    // Manual entries still require operator identity per §17.6 — a
    // ledger entry with no audit trail is a bug not a feature.
    let operator = resolve_operator(operator_arg)?;
    let registry = load_registry(project_root)?;
    validate_domain_in_registry(domain, &registry)?;

    let pending = PendingEntry {
        ts: now_ts(),
        schema_version: "1".to_string(),
        domain: domain.to_string(),
        domain_family: DomainFamily::DomainCalibration,
        trigger_signal_kind: signal.to_string(),
        actual_score,
        expected_score_lower: None,
        expected_score_upper: None,
        context_notes: context_arg.map(|s| s.to_string()),
        context_artifacts: vec![],
    };
    let ledger_path = default_ledger_path(Path::new(project_root), domain);
    calibration_ledger::append(&ledger_path, &LedgerEntry::Pending(pending.clone()))
        .context("append manual pending entry")?;

    println!(
        "{} {} pending entry for domain={}: ts={}, signal={}, score={} (operator={})",
        "✓".green(),
        "appended".green(),
        domain.cyan(),
        format!("{:.3}", pending.ts),
        signal,
        actual_score,
        operator,
    );
    Ok(())
}

// ─── Registry loading ────────────────────────────────────────────────

/// Load `<project_root>/.claude/brain-registry.json` for domain
/// validation. Synchronous (CLI is sync; matches sca_review.rs
/// pattern).
fn load_registry(project_root: &str) -> Result<BrainRegistry> {
    let path: PathBuf = Path::new(project_root)
        .join(".claude")
        .join("brain-registry.json");
    let json = std::fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    BrainRegistry::from_json(&json)
        .with_context(|| format!("parse {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Wrapper for parse-only testing — clap requires a top-level
    /// derive(Parser) to drive arg-parsing tests for a Subcommand.
    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: DomainCalibrationCmd,
    }

    // ─── List parsing ─────────────────────────────────────────────

    #[test]
    fn clap_list_no_args_parses() {
        let parsed = TestCli::try_parse_from(["test", "list"]);
        assert!(parsed.is_ok(), "parse error: {:?}", parsed.err());
        match parsed.unwrap().cmd {
            DomainCalibrationCmd::List {
                domain, open_only, ..
            } => {
                assert!(domain.is_none());
                assert!(!open_only);
            }
            other => panic!("expected List, got {:?}", other),
        }
    }

    #[test]
    fn clap_list_with_domain_and_open_only() {
        let parsed = TestCli::try_parse_from(&[
            "test",
            "list",
            "--domain",
            "test-health",
            "--open-only",
        ]);
        assert!(parsed.is_ok(), "parse error: {:?}", parsed.err());
        match parsed.unwrap().cmd {
            DomainCalibrationCmd::List {
                domain, open_only, ..
            } => {
                assert_eq!(domain.as_deref(), Some("test-health"));
                assert!(open_only);
            }
            other => panic!("expected List, got {:?}", other),
        }
    }

    // ─── Triage parsing ───────────────────────────────────────────

    #[test]
    fn clap_accepts_each_valid_decision() {
        for decision in ["confirmed", "mislabeled", "gap", "no-action"] {
            let parsed = TestCli::try_parse_from(&[
                "test",
                "triage",
                "--domain",
                "test-health",
                "--pending-ts",
                "1745000000.123",
                "--decision",
                decision,
                "--note",
                "x",
            ]);
            assert!(
                parsed.is_ok(),
                "decision {decision:?} should parse; err: {:?}",
                parsed.err()
            );
        }
    }

    #[test]
    fn clap_rejects_invalid_decision() {
        // PossibleValuesParser regression guard: typo'd decisions
        // must error at parse time, before reaching the ledger writer.
        let parsed = TestCli::try_parse_from(&[
            "test",
            "triage",
            "--domain",
            "test-health",
            "--pending-ts",
            "1745000000.123",
            "--decision",
            "yolo",
            "--note",
            "x",
        ]);
        assert!(parsed.is_err(), "expected error for invalid decision");
        let err = parsed.unwrap_err().to_string();
        assert!(
            err.contains("yolo"),
            "error must name the bad value; got: {err}"
        );
        assert!(
            err.contains("confirmed") && err.contains("no-action"),
            "error must list the valid values; got: {err}"
        );
    }

    #[test]
    fn clap_rejects_triage_missing_required_args() {
        // --domain, --pending-ts, --decision, --note all required.
        let no_domain = TestCli::try_parse_from(&[
            "test",
            "triage",
            "--pending-ts",
            "1.0",
            "--decision",
            "confirmed",
            "--note",
            "x",
        ]);
        assert!(no_domain.is_err(), "missing --domain must error");
        let no_ts = TestCli::try_parse_from(&[
            "test",
            "triage",
            "--domain",
            "test-health",
            "--decision",
            "confirmed",
            "--note",
            "x",
        ]);
        assert!(no_ts.is_err(), "missing --pending-ts must error");
        let no_decision = TestCli::try_parse_from(&[
            "test",
            "triage",
            "--domain",
            "test-health",
            "--pending-ts",
            "1.0",
            "--note",
            "x",
        ]);
        assert!(no_decision.is_err(), "missing --decision must error");
        let no_note = TestCli::try_parse_from(&[
            "test",
            "triage",
            "--domain",
            "test-health",
            "--pending-ts",
            "1.0",
            "--decision",
            "confirmed",
        ]);
        assert!(no_note.is_err(), "missing --note must error");
    }

    // ─── Manual parsing ───────────────────────────────────────────

    #[test]
    fn clap_manual_default_signal_is_manual() {
        let parsed = TestCli::try_parse_from(&[
            "test",
            "manual",
            "--domain",
            "test-health",
            "--actual-score",
            "45",
        ]);
        assert!(parsed.is_ok(), "parse error: {:?}", parsed.err());
        match parsed.unwrap().cmd {
            DomainCalibrationCmd::Manual {
                domain,
                signal,
                actual_score,
                ..
            } => {
                assert_eq!(domain, "test-health");
                assert_eq!(signal, "manual"); // default value
                assert_eq!(actual_score, 45);
            }
            other => panic!("expected Manual, got {:?}", other),
        }
    }

    #[test]
    fn clap_manual_custom_signal_and_context() {
        let parsed = TestCli::try_parse_from(&[
            "test",
            "manual",
            "--domain",
            "test-health",
            "--signal",
            "manual:operator-spotted",
            "--actual-score",
            "30",
            "--context",
            "saw a regression in nightly",
        ]);
        assert!(parsed.is_ok(), "parse error: {:?}", parsed.err());
        match parsed.unwrap().cmd {
            DomainCalibrationCmd::Manual {
                signal,
                actual_score,
                context,
                ..
            } => {
                assert_eq!(signal, "manual:operator-spotted");
                assert_eq!(actual_score, 30);
                assert_eq!(context.as_deref(), Some("saw a regression in nightly"));
            }
            other => panic!("expected Manual, got {:?}", other),
        }
    }

    #[test]
    fn clap_manual_rejects_score_above_255() {
        // u8 ceiling: clap rejects ANY value > 255 at parse time.
        // Values 101..=255 still parse here (they're valid u8) but
        // are rejected by the ledger writer's validate() (>100).
        // Check the parse-time u8 ceiling:
        let parsed = TestCli::try_parse_from(&[
            "test",
            "manual",
            "--domain",
            "test-health",
            "--actual-score",
            "300",
        ]);
        assert!(parsed.is_err(), "score > 255 must fail u8 parse");
    }

    #[test]
    fn clap_rejects_manual_missing_required_args() {
        let no_domain = TestCli::try_parse_from(&[
            "test",
            "manual",
            "--actual-score",
            "30",
        ]);
        assert!(no_domain.is_err(), "missing --domain must error");
        let no_score = TestCli::try_parse_from(&[
            "test",
            "manual",
            "--domain",
            "test-health",
        ]);
        assert!(no_score.is_err(), "missing --actual-score must error");
    }
}
