//! `neurogrim disposition` — operator-disposition CLI for the
//! invocation-ledger family (LSP-Brains v2.11 §17.12, E-B2-6 C3).
//!
//! Single sub-command at v1:
//!
//! - **`record`** — append a `DispositionEntry` row to the
//!   high-frequency append-only invocation ledger at
//!   `<project_root>/.claude/brain/invocation-ledger.jsonl`. The row
//!   captures the operator's judgment of a prior skill invocation by
//!   referencing its `invocation_id`.
//!
//! v1 ships `record` only — no `list`, no `triage`, no `delete`. The
//! invocation ledger is high-frequency append-only; corrections come
//! as new rows via `supersedes_invocation_id` in v2 (currently out of
//! scope per BACKLOG B-23). See spec §17.12 for the family semantics.
//!
//! Closed-set discipline (Q1 lock — spec §17.12.2):
//!
//! - `--kind` is validated at clap parse time via `PossibleValuesParser`
//!   against the 4-entry vocabulary `{accepted, rejected, modified,
//!   superseded}`. Typos like `--kind yolo` fail with clap's standard
//!   "invalid value" error before any I/O.
//!
//! Privacy contract (Q5 lock — spec §17.12.3):
//!
//! - NO `--note`, `--justification`, `--comment`, `--reason` flag is
//!   accepted. Free-text justification is forbidden at v1; the
//!   `additionalProperties: false` on `DispositionEntry` in
//!   `invocation-ledger-v1.schema.json` is the structural enforcement.
//!   Re-opening this surface requires a charter-level BR-5
//!   conversation; until then the `Args` struct below MUST contain no
//!   free-text field of any kind.
//!
//! Operator-identity discipline (Q6 echo — spec §17.6 / §17.12.5):
//!
//! - `--operator <handle>` flag → `NEUROGRIM_OPERATOR` env → reject.
//!   No "unknown" fallback. Mirrors `domain_calibration.rs` behavior.
//!
//! Recursion guard (Q6 lock — spec §17.12.5 MUST):
//!
//! - If `--invocation-id` starts with the literal prefix
//!   `operator_calibration:`, the CLI rejects the write before any
//!   file I/O. The carve-out closes the recursion loop (operator
//!   dispositions a sensor finding → re-counted in sensor's score)
//!   structurally. Mirrors §17.9 `Manual` calibration trigger
//!   discipline + E-B2-3 hat-contract recursion-guard pattern.
//!
//! Append-only writer:
//!
//! - Opens the ledger file with `OpenOptions::create(true).append(true)`
//!   and emits a single `\n`-terminated JSON line. POSIX guarantees
//!   atomicity for `O_APPEND` writes smaller than `PIPE_BUF` (4096
//!   bytes); a `DispositionEntry` row is well under that bound (~150
//!   bytes), so concurrent writers cannot interleave. On Windows,
//!   append-mode opens use `FILE_APPEND_DATA` access right which has
//!   the same atomicity guarantee for short writes. This is the same
//!   discipline used by `record-skill-invocation.sh` (the
//!   PostToolUse hook); cross-process safety follows from the same
//!   invariant.

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use clap::{Args as ClapArgs, Subcommand};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Recursion-guard prefix (Q6 lock — spec §17.12.5). Invocation IDs
/// that begin with this literal are findings emitted by the
/// operator-calibration sensor itself; dispositioning them would
/// create a circular feedback loop in the sensor's own score
/// denominator.
const RECURSION_GUARD_PREFIX: &str = "operator_calibration:";

/// Closed-set disposition vocabulary (Q1 lock — spec §17.12.2). This
/// const is the parse-time enum + the runtime serialization value;
/// the schema's `Disposition` enum mirrors it exactly. Adding a value
/// here without a corresponding spec change + schema bump would
/// silently break conformance.
const DISPOSITION_KINDS: &[&str] = &["accepted", "rejected", "modified", "superseded"];

/// Top-level args for `neurogrim disposition <subcommand>`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub subcommand: DispositionCmd,
}

#[derive(Subcommand, Debug)]
pub enum DispositionCmd {
    /// Append a DispositionEntry row to the invocation ledger.
    ///
    /// Records the operator's judgment of a prior skill invocation
    /// (referenced by `--invocation-id`) using one of the closed-set
    /// 4-entry vocabulary values (`accepted | rejected | modified |
    /// superseded`).
    ///
    /// **No `--note` flag at v1** (Q5 lock — spec §17.12.3). The
    /// privacy contract at `docs/invocation-ledger.md:26-39` forbids
    /// free-text justification on disposition records; the
    /// `additionalProperties: false` on `DispositionEntry` in
    /// `invocation-ledger-v1.schema.json` is the structural
    /// enforcement.
    Record {
        /// `invocation_id` of the SkillEntry being dispositioned.
        /// References must be authored by the operator from the
        /// existing ledger — the writer does not enforce referential
        /// integrity at v1 (the schema validates one row at a time).
        ///
        /// Recursion-guard: invocation IDs starting with
        /// `operator_calibration:` are rejected at parse time per
        /// spec §17.12.5 MUST.
        #[arg(long)]
        invocation_id: String,

        /// Disposition kind — one of accepted | rejected | modified |
        /// superseded (Q1 lock, spec §17.12.2).
        ///
        /// Validated at clap parse time via PossibleValuesParser so
        /// typos like `--kind yolo` fail with clap's standard
        /// "invalid value" error before any file I/O. Adding new
        /// values requires a spec change with an explicit
        /// METHODOLOGY-EVOLUTION entry.
        #[arg(
            long,
            value_parser = clap::builder::PossibleValuesParser::new(DISPOSITION_KINDS),
        )]
        kind: String,

        /// Operator handle. Falls back to `$NEUROGRIM_OPERATOR`. Both
        /// unset → error per spec §17.6 (no "unknown" fallback;
        /// mirrors `domain_calibration.rs`).
        #[arg(long)]
        operator: Option<String>,

        /// Project root path. The ledger lives at
        /// `<project_root>/.claude/brain/invocation-ledger.jsonl`;
        /// the `.claude/brain/` directory is created if absent.
        #[arg(long, default_value = ".")]
        project_root: String,
    },
}

/// On-disk shape of a `DispositionEntry` row. This struct is the
/// serialized contract — every field maps 1:1 to a required property
/// in `invocation-ledger-v1.schema.json` `DispositionEntry`. Adding
/// a free-text field here would violate the privacy contract (Q5
/// lock). Field order matches the schema for human-readable diffs.
///
/// `serde(rename = "type")` is NOT used — `entry_kind` is the actual
/// schema field name (the `type` field is reserved for `SkillEntry`).
#[derive(Serialize, Debug)]
struct DispositionEntry<'a> {
    schema_version: &'a str,
    ts: &'a str,
    entry_kind: &'a str,
    invocation_id: &'a str,
    disposition_kind: &'a str,
    human_operator: &'a str,
}

pub async fn run(args: Args) -> Result<()> {
    match args.subcommand {
        DispositionCmd::Record {
            invocation_id,
            kind,
            operator,
            project_root,
        } => cmd_record(&invocation_id, &kind, operator.as_deref(), &project_root),
    }
}

fn cmd_record(
    invocation_id: &str,
    kind: &str,
    operator_arg: Option<&str>,
    project_root: &str,
) -> Result<()> {
    // Recursion guard (Q6 lock, spec §17.12.5 MUST). Parse-time
    // rejection — no file I/O occurs if the guard fires. The case-
    // sensitive prefix match mirrors the sensor's finding-kind format
    // (`operator_calibration:low_confidence`,
    // `operator_calibration:no_dispositions_yet`).
    if invocation_id.starts_with(RECURSION_GUARD_PREFIX) {
        anyhow::bail!(
            "refusing to record disposition against an operator-calibration finding \
             (recursion guard, spec §17.12.5)"
        );
    }

    // Operator-identity discipline (spec §17.6). --operator wins;
    // NEUROGRIM_OPERATOR is the fallback; both unset → error. No
    // "unknown" sentinel — a ledger row with no audit trail is a bug
    // not a feature.
    let operator = resolve_operator(operator_arg)?;

    // ISO 8601 UTC timestamp with millisecond precision, trailing 'Z'
    // (e.g., "2026-04-27T20:00:00.000Z"). The `true` argument forces
    // the 'Z' suffix instead of the +00:00 form. Matches the existing
    // `record-skill-invocation.sh` style (which uses `date -u
    // +%Y-%m-%dT%H:%M:%SZ` — second-precision); millisecond precision
    // here keeps disposition rows monotonically distinguishable
    // within the same second when an operator dispositions multiple
    // invocations in rapid succession.
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);

    let entry = DispositionEntry {
        schema_version: "1",
        ts: &ts,
        entry_kind: "disposition",
        invocation_id,
        disposition_kind: kind,
        human_operator: &operator,
    };

    // Serialize to a single compact JSON line (no internal newlines).
    // `serde_json::to_string` emits {"a":1,...} with no whitespace,
    // matching the existing skill-record line style produced by
    // `record-skill-invocation.sh`. The schema's
    // `additionalProperties: false` discipline is enforced at the
    // struct definition above — there's no path to add a `note`
    // field without changing the struct + schema together.
    let line = serde_json::to_string(&entry).context("serialize disposition entry")?;

    let ledger_path = ledger_path(project_root);
    ensure_brain_dir(&ledger_path)?;

    // Append-only writer. `OpenOptions::create(true).append(true)`
    // gives `O_APPEND` semantics on POSIX and `FILE_APPEND_DATA` on
    // Windows — both atomic for writes < PIPE_BUF (4096 bytes). A
    // disposition row is ~150 bytes, well under the bound. Cross-
    // process safety follows from the same invariant the existing
    // PostToolUse hook (`record-skill-invocation.sh`) relies on.
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ledger_path)
        .with_context(|| format!("open ledger {}", ledger_path.display()))?;

    // Single `writeln!` emits the JSON line + '\n' atomically (one
    // syscall). The `\n` is required by the JSONL format used
    // throughout the Brain (one JSON object per line; trailing
    // newline on every row).
    writeln!(file, "{line}")
        .with_context(|| format!("append to ledger {}", ledger_path.display()))?;

    println!(
        "recorded disposition: invocation_id={} kind={} operator={} ts={}",
        invocation_id, kind, operator, ts
    );
    Ok(())
}

/// Resolve operator handle from `--operator` flag with
/// `NEUROGRIM_OPERATOR` env fallback. Returns an error if neither is
/// set (no "unknown" fallback per §17.6 audit-rationale discipline).
fn resolve_operator(operator_arg: Option<&str>) -> Result<String> {
    if let Some(op) = operator_arg {
        let trimmed = op.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Ok(env_val) = std::env::var("NEUROGRIM_OPERATOR") {
        let trimmed = env_val.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    anyhow::bail!(
        "operator identity required — set NEUROGRIM_OPERATOR env or pass --operator <handle>"
    )
}

/// Resolve the ledger path: `<project_root>/.claude/brain/invocation-ledger.jsonl`.
fn ledger_path(project_root: &str) -> PathBuf {
    Path::new(project_root)
        .join(".claude")
        .join("brain")
        .join("invocation-ledger.jsonl")
}

/// Ensure the parent `.claude/brain/` directory exists. Mirrors the
/// `mkdir -p .claude/brain` line in `record-skill-invocation.sh:34`
/// and the directory-creation behavior in `domain_calibration.rs`'s
/// ledger writer (via `calibration_ledger::append`).
fn ensure_brain_dir(ledger_path: &Path) -> Result<()> {
    if let Some(parent) = ledger_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("mkdir -p {}", parent.display()))?;
    }
    Ok(())
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
        cmd: DispositionCmd,
    }

    #[test]
    fn clap_accepts_each_valid_kind() {
        for kind in DISPOSITION_KINDS {
            let parsed = TestCli::try_parse_from([
                "test",
                "record",
                "--invocation-id",
                "tu_abc123",
                "--kind",
                kind,
            ]);
            assert!(
                parsed.is_ok(),
                "kind {kind:?} should parse; err: {:?}",
                parsed.err()
            );
        }
    }

    #[test]
    fn clap_rejects_invalid_kind() {
        // PossibleValuesParser regression guard: typo'd kinds must
        // error at parse time, before reaching the ledger writer.
        let parsed = TestCli::try_parse_from([
            "test",
            "record",
            "--invocation-id",
            "tu_abc123",
            "--kind",
            "yolo",
        ]);
        assert!(parsed.is_err(), "expected error for invalid kind");
        let err = parsed.unwrap_err().to_string();
        assert!(err.contains("yolo"), "error must name the bad value; got: {err}");
        assert!(
            err.contains("accepted") && err.contains("superseded"),
            "error must list the valid values; got: {err}"
        );
    }

    #[test]
    fn clap_requires_invocation_id() {
        let parsed = TestCli::try_parse_from([
            "test",
            "record",
            "--kind",
            "accepted",
        ]);
        assert!(parsed.is_err(), "missing --invocation-id must error");
    }

    #[test]
    fn clap_requires_kind() {
        let parsed = TestCli::try_parse_from([
            "test",
            "record",
            "--invocation-id",
            "tu_abc123",
        ]);
        assert!(parsed.is_err(), "missing --kind must error");
    }

    /// Q5 structural pin: there is NO `--note` flag at v1. If a future
    /// agent tries to add one, the closed-set parser below will fail
    /// because clap will treat `--note` as an unknown argument.
    #[test]
    fn clap_rejects_note_flag() {
        let parsed = TestCli::try_parse_from([
            "test",
            "record",
            "--invocation-id",
            "tu_abc123",
            "--kind",
            "accepted",
            "--note",
            "this should be forbidden",
        ]);
        assert!(
            parsed.is_err(),
            "free-text --note must be rejected (Q5 privacy lock)"
        );
    }

    /// Q5 structural pin: same for `--justification`.
    #[test]
    fn clap_rejects_justification_flag() {
        let parsed = TestCli::try_parse_from([
            "test",
            "record",
            "--invocation-id",
            "tu_abc123",
            "--kind",
            "accepted",
            "--justification",
            "this should be forbidden",
        ]);
        assert!(
            parsed.is_err(),
            "free-text --justification must be rejected (Q5 privacy lock)"
        );
    }
}
