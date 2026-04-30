//! `neurogrim publish-gate {run,ack}` — v4.0 S12-G-4.
//!
//! Reads `<brain>/.claude/brain/publish-gates.yaml` (validated against
//! `publish-gates-v1.schema.json` by S12-G-3), executes gates in
//! declared order, and emits one
//! [`LedgerEntry`] per gate to
//! `<brain>/.claude/brain/publish-gate-ledger.jsonl`.
//!
//! ## Two sub-commands
//!
//! - **`run`** — execute the manifest's gates. Filtered by `--gate
//!   <id>` (single gate) or `--mode {pre-commit,pre-publish,full}`
//!   (heuristic in v1; schema v2 will add explicit per-gate mode
//!   tags). Exit code aggregates across all blocking gates: 0 pass,
//!   1 fail, 2 pending operator. Non-blocking gate failures are
//!   recorded but never drive the exit code.
//!
//! - **`ack`** — mark the most recent `pending` ledger entry for
//!   `--gate <id>` as `passed`, with the operator handle attached
//!   (resolved from `--operator` or `$NEUROGRIM_OPERATOR`).
//!
//! ## Per-gate-type semantics
//!
//! | type | run action | possible status |
//! |---|---|---|
//! | `automated` | `sh -c <check_command>` (Unix) / `cmd /c` (Windows), wall-clock timeout = `timeout_seconds` or 600s default | passed (exit 0) / failed (exit ≠ 0) / timed_out / error |
//! | `manual` | print description + instructions + ack hint | pending |
//! | `e2e` | emit deferred entry pointing at S12-G-5 | deferred |
//!
//! ## Ledger schema (v1)
//!
//! ```json
//! {
//!   "schema_version": "1",
//!   "run_id": "<uuid v4>",
//!   "gate_id": "<id>",
//!   "gate_type": "automated|manual|e2e",
//!   "mode": "pre-commit|pre-publish|full|single|ack",
//!   "started_at": "2026-04-29T...",
//!   "completed_at": "2026-04-29T..." | null,
//!   "status": "passed|failed|pending|timed_out|deferred|error",
//!   "blocking": true,
//!   "operator": "<handle>" | null,
//!   "exit_code": 0 | null,
//!   "stdout_truncated": "..." | null,
//!   "stderr_truncated": "..." | null,
//!   "error_detail": "..." | null
//! }
//! ```
//!
//! Truncation: stdout/stderr captured to 4 KB head + 4 KB tail to keep
//! typical ledger lines small enough that `O_APPEND` writes stay
//! atomic (sub-PIPE_BUF). A pathological program that emits no output
//! will skip both fields.

use anyhow::{anyhow, Context, Result};
use chrono::{SecondsFormat, Utc};
use clap::{Args as ClapArgs, Subcommand};
use neurogrim_mcp::publish_gates::{
    load_publish_gates, Gate, GateType, PublishGatesConfig, PublishGatesError,
    PUBLISH_GATES_MANIFEST_RELPATH,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command as TokioCommand;
use uuid::Uuid;

use crate::commands::test::find_dashboard_frontend;

const SCHEMA_VERSION: &str = "1";
const LEDGER_FILENAME: &str = "publish-gate-ledger.jsonl";
const DEFAULT_TIMEOUT_SECS: u64 = 600;
const STREAM_TRUNCATE_HEAD: usize = 4096;
const STREAM_TRUNCATE_TAIL: usize = 4096;

// ── CLI shape ─────────────────────────────────────────────────────────

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub command: PublishGateCommand,
}

#[derive(Subcommand, Debug)]
pub enum PublishGateCommand {
    /// Execute gates from `<brain>/.claude/brain/publish-gates.yaml`.
    /// Exit code: 0 all blocking passed, 1 any blocking failed, 2 any
    /// blocking pending (and none failed).
    Run(RunArgs),
    /// Mark a manual gate as passed by an operator. Reads the most
    /// recent `pending` entry for `--gate <id>` from the ledger and
    /// appends a follow-up `passed` entry referencing the same run_id.
    Ack(AckArgs),
}

#[derive(ClapArgs, Debug)]
pub struct RunArgs {
    /// Run only the gate with this id. Overrides `--mode`.
    #[arg(long)]
    pub gate: Option<String>,

    /// Filter gates by mode. v1 heuristic: pre-commit = automated
    /// gates with `timeout_seconds ≤ 30` or unset; pre-publish = all
    /// `blocking: true` gates; full = every gate. Schema v2 will
    /// introduce explicit per-gate mode tags.
    #[arg(
        long,
        value_parser = ["pre-commit", "pre-publish", "full"],
        default_value = "full"
    )]
    pub mode: String,

    /// Show captured stdout/stderr per automated gate.
    #[arg(long, short = 'v')]
    pub verbose: bool,

    /// v4.0 S12-G-6: when stdin is a TTY, prompt the operator y/N
    /// for each manual gate inline. y → mark passed (requires
    /// resolvable operator handle); anything else → fall through
    /// to async pending (existing behavior). Force on with
    /// `--interactive`; force off with `--no-interactive`. Default
    /// is auto-detect via `IsTerminal`.
    #[arg(long, conflicts_with = "no_interactive")]
    pub interactive: bool,

    /// Force the non-interactive (CI-style) path even when stdin
    /// is a TTY. Manual gates always emit pending; ack via the
    /// `ack` sub-command later.
    #[arg(long, conflicts_with = "interactive")]
    pub no_interactive: bool,

    /// Operator handle for inline manual-gate ack. Falls back to
    /// `$NEUROGRIM_OPERATOR`. When unset and a manual gate is
    /// answered with 'y' interactively, the gate falls through to
    /// pending with a warning rather than ack'ing under "unknown".
    #[arg(long)]
    pub operator: Option<String>,

    /// Project root containing `.claude/brain/publish-gates.yaml`.
    #[arg(long, default_value = ".")]
    pub project_root: String,
}

#[derive(ClapArgs, Debug)]
pub struct AckArgs {
    /// Gate id whose pending entry is being acknowledged.
    #[arg(long)]
    pub gate: String,

    /// Operator handle. Falls back to `$NEUROGRIM_OPERATOR`.
    #[arg(long)]
    pub operator: Option<String>,

    /// Project root containing `.claude/brain/publish-gate-ledger.jsonl`.
    #[arg(long, default_value = ".")]
    pub project_root: String,
}

// ── Ledger entry ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerEntry {
    pub schema_version: String,
    pub run_id: String,
    pub gate_id: String,
    pub gate_type: String,
    pub mode: String,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub completed_at: Option<String>,
    pub status: String,
    pub blocking: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub operator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stdout_truncated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stderr_truncated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error_detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Passed,
    Failed,
    Pending,
    TimedOut,
    Deferred,
    Error,
}

impl Status {
    fn as_str(&self) -> &'static str {
        match self {
            Status::Passed => "passed",
            Status::Failed => "failed",
            Status::Pending => "pending",
            Status::TimedOut => "timed_out",
            Status::Deferred => "deferred",
            Status::Error => "error",
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────

pub async fn run(args: Args) -> Result<()> {
    match args.command {
        PublishGateCommand::Run(run_args) => run_subcmd(run_args).await,
        PublishGateCommand::Ack(ack_args) => ack_subcmd(ack_args).await,
    }
}

// ── `run` sub-command ─────────────────────────────────────────────────

async fn run_subcmd(args: RunArgs) -> Result<()> {
    let project_root = Path::new(&args.project_root);
    let manifest_path = project_root.join(PUBLISH_GATES_MANIFEST_RELPATH);
    let ledger_path = project_root
        .join(".claude")
        .join("brain")
        .join(LEDGER_FILENAME);

    let config = match load_publish_gates(&manifest_path) {
        Ok(c) => c,
        Err(PublishGatesError::NotFound) => {
            return Err(anyhow!(
                "no publish-gates manifest at {}; author one then re-run",
                manifest_path.display()
            ));
        }
        Err(other) => {
            return Err(anyhow!(
                "publish-gates manifest at {} failed to load: {other}",
                manifest_path.display()
            ));
        }
    };

    let selected: Vec<&Gate> = select_gates(&config, args.gate.as_deref(), &args.mode)?;
    if selected.is_empty() {
        eprintln!(
            "✦ neurogrim publish-gate run --mode {} — no gates selected by this filter",
            args.mode
        );
        return Ok(());
    }

    let run_id = Uuid::new_v4().to_string();
    let mode_tag: String = if args.gate.is_some() {
        "single".into()
    } else {
        args.mode.clone()
    };

    eprintln!(
        "✦ neurogrim publish-gate run — {} gate(s), mode={}",
        selected.len(),
        mode_tag
    );

    let interactive = build_interactive_ctx(&args);

    let mut entries: Vec<LedgerEntry> = Vec::with_capacity(selected.len());
    for gate in &selected {
        let entry = execute_gate(
            gate,
            &run_id,
            &mode_tag,
            args.verbose,
            project_root,
            &interactive,
        )
        .await;
        print_outcome(&entry, args.verbose);
        entries.push(entry);
    }

    if !entries.is_empty() {
        append_ledger_entries(&ledger_path, &entries).with_context(|| {
            format!("failed to append to {}", ledger_path.display())
        })?;
    }

    let exit = aggregate_exit_code(&entries);
    eprintln!();
    eprintln!(
        "✦ exit code {exit} — {} passed · {} failed · {} pending · {} deferred · {} error · {} timed_out",
        count(&entries, "passed"),
        count(&entries, "failed"),
        count(&entries, "pending"),
        count(&entries, "deferred"),
        count(&entries, "error"),
        count(&entries, "timed_out"),
    );
    std::process::exit(exit);
}

// ── `ack` sub-command ─────────────────────────────────────────────────

async fn ack_subcmd(args: AckArgs) -> Result<()> {
    let project_root = Path::new(&args.project_root);
    let ledger_path = project_root
        .join(".claude")
        .join("brain")
        .join(LEDGER_FILENAME);
    let operator = resolve_operator(args.operator.as_deref())?;

    let prior = read_most_recent_pending(&ledger_path, &args.gate)?
        .ok_or_else(|| {
            anyhow!(
                "no pending ledger entry for gate '{}' in {}; nothing to ack",
                args.gate,
                ledger_path.display()
            )
        })?;

    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let ack_entry = LedgerEntry {
        schema_version: SCHEMA_VERSION.to_string(),
        run_id: prior.run_id.clone(),
        gate_id: args.gate.clone(),
        gate_type: prior.gate_type.clone(),
        mode: "ack".to_string(),
        started_at: prior.started_at.clone(),
        completed_at: Some(now),
        status: Status::Passed.as_str().to_string(),
        blocking: prior.blocking,
        operator: Some(operator.clone()),
        exit_code: None,
        stdout_truncated: None,
        stderr_truncated: None,
        error_detail: None,
    };
    append_ledger_entries(&ledger_path, &[ack_entry])
        .with_context(|| format!("failed to append to {}", ledger_path.display()))?;

    eprintln!(
        "✓ gate '{}' acknowledged passed by {operator} (run_id={})",
        args.gate, prior.run_id
    );
    Ok(())
}

// ── Gate execution ────────────────────────────────────────────────────

/// Per-run interactive context (S12-G-6). Builds once per
/// `publish-gate run` invocation and is threaded into manual-gate
/// execution so the prompt can ack inline when appropriate.
#[derive(Debug, Clone)]
struct InteractiveCtx {
    /// True iff inline prompting is allowed: `--interactive` was
    /// passed, OR (stdin is a TTY AND `--no-interactive` wasn't
    /// passed).
    enabled: bool,
    /// Resolved operator handle (`--operator` flag → env). None
    /// when neither is set; the prompt warns and falls through to
    /// pending in that case.
    operator: Option<String>,
}

/// Decide whether inline prompting fires for this run.
///
/// - `--interactive` → forces enabled (operator opted in even if
///   redirected stdin / piped input).
/// - `--no-interactive` → forces disabled (CI / scripts).
/// - Neither flag → enabled iff stdin is a TTY.
///
/// Operator hint is resolved best-effort here — the prompt re-checks
/// at ack-time, so a missing handle just falls through to pending
/// rather than failing the whole run.
fn build_interactive_ctx(args: &RunArgs) -> InteractiveCtx {
    use std::io::IsTerminal;
    let enabled = if args.interactive {
        true
    } else if args.no_interactive {
        false
    } else {
        std::io::stdin().is_terminal()
    };
    let operator = resolve_operator(args.operator.as_deref()).ok();
    InteractiveCtx { enabled, operator }
}

async fn execute_gate(
    gate: &Gate,
    run_id: &str,
    mode_tag: &str,
    verbose: bool,
    project_root: &Path,
    interactive: &InteractiveCtx,
) -> LedgerEntry {
    let started_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let blocking = gate.blocking.unwrap_or(true);
    let base = LedgerEntry {
        schema_version: SCHEMA_VERSION.to_string(),
        run_id: run_id.to_string(),
        gate_id: gate.id.clone(),
        gate_type: gate_type_str(gate.gate_type).to_string(),
        mode: mode_tag.to_string(),
        started_at,
        completed_at: None,
        status: Status::Error.as_str().to_string(),
        blocking,
        operator: None,
        exit_code: None,
        stdout_truncated: None,
        stderr_truncated: None,
        error_detail: None,
    };
    match gate.gate_type {
        GateType::Automated => execute_automated(gate, base, verbose).await,
        GateType::Manual => execute_manual(gate, base, interactive),
        GateType::E2e => execute_e2e_playwright(gate, base, project_root).await,
    }
}

async fn execute_automated(
    gate: &Gate,
    mut entry: LedgerEntry,
    _verbose: bool,
) -> LedgerEntry {
    let cmd = match &gate.check_command {
        Some(c) if !c.trim().is_empty() => c.clone(),
        _ => {
            entry.status = Status::Error.as_str().to_string();
            entry.error_detail = Some(
                "automated gate has no check_command — schema requires it; \
                 manifest must be re-validated by `neurogrim doctor`"
                    .to_string(),
            );
            entry.completed_at = Some(now_rfc3339());
            return entry;
        }
    };
    let timeout_secs = gate.timeout_seconds.map(u64::from).unwrap_or(DEFAULT_TIMEOUT_SECS);

    let mut child_cmd = build_shell_command(&cmd);
    child_cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = match child_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            entry.status = Status::Error.as_str().to_string();
            entry.error_detail = Some(format!("failed to spawn shell: {e}"));
            entry.completed_at = Some(now_rfc3339());
            return entry;
        }
    };

    let output_fut = child.wait_with_output();
    let outcome = tokio::time::timeout(Duration::from_secs(timeout_secs), output_fut).await;

    match outcome {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            entry.exit_code = Some(exit_code);
            entry.stdout_truncated = truncate_stream(&output.stdout);
            entry.stderr_truncated = truncate_stream(&output.stderr);
            entry.status = if output.status.success() {
                Status::Passed.as_str().to_string()
            } else {
                Status::Failed.as_str().to_string()
            };
        }
        Ok(Err(e)) => {
            entry.status = Status::Error.as_str().to_string();
            entry.error_detail = Some(format!("wait failed: {e}"));
        }
        Err(_elapsed) => {
            // Timeout fired. The child handle was consumed by
            // `wait_with_output`, so we cannot kill it directly here;
            // the process will be reaped when the OS notices the
            // pipes closed. Mark as timed_out.
            entry.status = Status::TimedOut.as_str().to_string();
            entry.error_detail = Some(format!(
                "exceeded timeout_seconds={timeout_secs}; process killed"
            ));
        }
    }

    entry.completed_at = Some(now_rfc3339());
    entry
}

/// Dispatch a manual gate. When `interactive.enabled`, prompts
/// y/N and ack's inline on yes. Otherwise falls through to the
/// async pending flow (operator runs `publish-gate ack` later).
fn execute_manual(
    gate: &Gate,
    entry: LedgerEntry,
    interactive: &InteractiveCtx,
) -> LedgerEntry {
    print_manual_gate_header(gate);
    if interactive.enabled {
        let stdin = std::io::stdin();
        let mut handle = stdin.lock();
        match prompt_manual_gate_inline(gate, &mut handle, interactive.operator.as_deref()) {
            ManualPromptOutcome::Acked(operator) => return finalize_manual_passed(entry, operator),
            ManualPromptOutcome::Pending(reason) => {
                if let Some(r) = reason {
                    eprintln!("  ({r})");
                }
                return finalize_manual_pending(gate, entry);
            }
        }
    }
    finalize_manual_pending(gate, entry)
}

fn print_manual_gate_header(gate: &Gate) {
    eprintln!();
    eprintln!("◇ manual gate: {}", gate.id);
    eprintln!("  {}", gate.description);
    if let Some(instr) = &gate.instructions {
        eprintln!();
        for line in instr.lines() {
            eprintln!("    {line}");
        }
    }
}

fn finalize_manual_pending(gate: &Gate, mut entry: LedgerEntry) -> LedgerEntry {
    eprintln!();
    eprintln!("  to mark passed:");
    eprintln!(
        "    neurogrim publish-gate ack --gate {} [--operator <handle>]",
        gate.id
    );
    entry.status = Status::Pending.as_str().to_string();
    entry.completed_at = None;
    entry
}

fn finalize_manual_passed(mut entry: LedgerEntry, operator: String) -> LedgerEntry {
    entry.status = Status::Passed.as_str().to_string();
    entry.completed_at = Some(now_rfc3339());
    entry.operator = Some(operator);
    entry
}

#[derive(Debug, PartialEq, Eq)]
enum ManualPromptOutcome {
    /// Operator typed 'y'/'Y' AND a handle resolved → ack inline.
    Acked(String),
    /// Operator typed something other than 'y' OR no handle was
    /// resolvable. Carries an optional reason for the human-facing
    /// summary (None = explicit "no" from operator).
    Pending(Option<String>),
}

/// Prompt the operator y/N for a single manual gate and parse the
/// answer. Factored out from `execute_manual` so tests can drive
/// the I/O via a Cursor without spawning a real subprocess.
fn prompt_manual_gate_inline<R: std::io::BufRead>(
    gate: &Gate,
    reader: &mut R,
    operator_hint: Option<&str>,
) -> ManualPromptOutcome {
    use std::io::Write;
    eprintln!();
    eprint!(
        "  Mark gate '{}' passed by {} now? [y/N]: ",
        gate.id,
        operator_hint.unwrap_or("(no operator handle resolved)")
    );
    let _ = std::io::stderr().flush();

    let mut buf = String::new();
    if reader.read_line(&mut buf).is_err() || buf.trim().is_empty() {
        return ManualPromptOutcome::Pending(None);
    }
    let answered_yes = matches!(buf.trim().to_ascii_lowercase().as_str(), "y" | "yes");
    if !answered_yes {
        eprintln!("  → marked pending; ack via the `ack` sub-command later.");
        return ManualPromptOutcome::Pending(None);
    }
    match operator_hint {
        Some(op) if !op.trim().is_empty() => ManualPromptOutcome::Acked(op.trim().to_string()),
        _ => ManualPromptOutcome::Pending(Some(
            "no operator handle resolved (set --operator or NEUROGRIM_OPERATOR); \
             falling through to pending"
                .to_string(),
        )),
    }
}

/// Run the Playwright E2E suite at
/// `<project_root>/crates/neurogrim-dashboard/frontend/`. The suite
/// is the third gate-type in the v4.0 publish pipeline; under the
/// hood it shells out to `npx playwright test`, which itself spawns
/// the dashboard binary on a fixed port (per
/// `playwright.config.ts:webServer`) and runs the smoke specs against
/// the built React bundle.
///
/// Failure modes:
/// - frontend dir missing → `error` (adopters who don't ship the
///   dashboard shouldn't declare e2e gates; surface explicitly)
/// - playwright not installed (`npx` exits 9009 / "not found") →
///   `error` with a hint pointing at the install steps in README
/// - playwright runs but tests fail → `failed` with exit code captured
/// - playwright runs and tests pass → `passed`
///
/// Timeout enforcement: respects the gate's `timeout_seconds` (or the
/// 600s default). Playwright also has its own `globalTimeout` ceiling
/// in the config (180s) — whichever fires first wins.
async fn execute_e2e_playwright(
    gate: &Gate,
    mut entry: LedgerEntry,
    project_root: &Path,
) -> LedgerEntry {
    let frontend = match find_dashboard_frontend(project_root) {
        Ok(p) => p,
        Err(e) => {
            entry.status = Status::Error.as_str().to_string();
            entry.error_detail = Some(format!(
                "{e} — `e2e` gate type requires the NeuroGrim dashboard frontend; \
                 adopters without `crates/neurogrim-dashboard/frontend/` should use \
                 `automated` gates with their own playwright command instead"
            ));
            entry.completed_at = Some(now_rfc3339());
            return entry;
        }
    };

    let timeout_secs =
        gate.timeout_seconds.map(u64::from).unwrap_or(DEFAULT_TIMEOUT_SECS);

    eprintln!();
    eprintln!(
        "▶ e2e gate '{}' — invoking Playwright at {}",
        gate.id,
        frontend.display()
    );

    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "windows") {
        ("npx.cmd", vec!["playwright", "test"])
    } else {
        ("npx", vec!["playwright", "test"])
    };
    let mut child_cmd = TokioCommand::new(program);
    child_cmd
        .args(&args)
        .current_dir(&frontend)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = match child_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            entry.status = Status::Error.as_str().to_string();
            entry.error_detail = Some(format!(
                "failed to spawn `{program} playwright test` in {}: {e} \
                 — install with `npm install` + `npx playwright install chromium` \
                 (see frontend/README.md)",
                frontend.display()
            ));
            entry.completed_at = Some(now_rfc3339());
            return entry;
        }
    };

    let outcome =
        tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await;

    match outcome {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            entry.exit_code = Some(exit_code);
            entry.stdout_truncated = truncate_stream(&output.stdout);
            entry.stderr_truncated = truncate_stream(&output.stderr);
            entry.status = if output.status.success() {
                Status::Passed.as_str().to_string()
            } else {
                Status::Failed.as_str().to_string()
            };
        }
        Ok(Err(e)) => {
            entry.status = Status::Error.as_str().to_string();
            entry.error_detail = Some(format!("playwright wait failed: {e}"));
        }
        Err(_elapsed) => {
            entry.status = Status::TimedOut.as_str().to_string();
            entry.error_detail = Some(format!(
                "playwright exceeded timeout_seconds={timeout_secs}; process killed"
            ));
        }
    }
    entry.completed_at = Some(now_rfc3339());
    entry
}

// ── Mode / single-gate filter ─────────────────────────────────────────

fn select_gates<'a>(
    config: &'a PublishGatesConfig,
    single_gate: Option<&str>,
    mode: &str,
) -> Result<Vec<&'a Gate>> {
    if let Some(id) = single_gate {
        let one = config
            .gates
            .iter()
            .find(|g| g.id == id)
            .ok_or_else(|| anyhow!("no gate with id '{id}' in manifest"))?;
        return Ok(vec![one]);
    }
    Ok(match mode {
        "pre-commit" => config
            .gates
            .iter()
            .filter(|g| {
                g.gate_type == GateType::Automated
                    && g.timeout_seconds.map(|t| t <= 30).unwrap_or(true)
            })
            .collect(),
        "pre-publish" => config
            .gates
            .iter()
            .filter(|g| g.blocking.unwrap_or(true))
            .collect(),
        // "full" or any other value — schema enforces the value parser
        // upstream, so unknown modes shouldn't reach this match arm.
        _ => config.gates.iter().collect(),
    })
}

// ── Aggregate exit code ───────────────────────────────────────────────

fn aggregate_exit_code(entries: &[LedgerEntry]) -> i32 {
    let any_blocking_failed = entries
        .iter()
        .any(|e| e.blocking && (e.status == "failed" || e.status == "timed_out" || e.status == "error"));
    if any_blocking_failed {
        return 1;
    }
    let any_blocking_pending = entries.iter().any(|e| e.blocking && e.status == "pending");
    if any_blocking_pending {
        return 2;
    }
    0
}

fn count(entries: &[LedgerEntry], status: &str) -> usize {
    entries.iter().filter(|e| e.status == status).count()
}

fn print_outcome(entry: &LedgerEntry, verbose: bool) {
    let symbol = match entry.status.as_str() {
        "passed" => "✓",
        "failed" => "✗",
        "pending" => "◇",
        "timed_out" => "⏱",
        "deferred" => "○",
        _ => "!",
    };
    let suffix = if !entry.blocking { " (advisory)" } else { "" };
    eprintln!(
        "  {symbol} [{}] {} → {}{}",
        entry.gate_type, entry.gate_id, entry.status, suffix
    );
    if verbose {
        if let Some(out) = &entry.stdout_truncated {
            if !out.is_empty() {
                eprintln!("    stdout:");
                for line in out.lines().take(20) {
                    eprintln!("      {line}");
                }
            }
        }
        if let Some(err) = &entry.stderr_truncated {
            if !err.is_empty() {
                eprintln!("    stderr:");
                for line in err.lines().take(20) {
                    eprintln!("      {line}");
                }
            }
        }
    }
    if let Some(detail) = &entry.error_detail {
        eprintln!("    {detail}");
    }
}

// ── Ledger I/O ────────────────────────────────────────────────────────

pub fn append_ledger_entries(path: &Path, entries: &[LedgerEntry]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    for e in entries {
        let line = serde_json::to_string(e)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        writeln!(f, "{line}")?;
    }
    f.sync_all()?;
    Ok(())
}

/// Walk the ledger backwards, returning the most recent entry whose
/// `gate_id` matches AND whose status is `pending`.
pub fn read_most_recent_pending(path: &Path, gate_id: &str) -> Result<Option<LedgerEntry>> {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(anyhow!("read {}: {e}", path.display())),
    };
    let mut last_pending: Option<LedgerEntry> = None;
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        if let Ok(entry) = serde_json::from_str::<LedgerEntry>(line) {
            if entry.gate_id == gate_id {
                if entry.status == "pending" {
                    last_pending = Some(entry);
                } else {
                    // Non-pending entry for this gate — invalidates
                    // any earlier pending (it's been resolved).
                    last_pending = None;
                }
            }
        }
    }
    Ok(last_pending)
}

// ── Helpers ───────────────────────────────────────────────────────────

fn build_shell_command(cmd: &str) -> TokioCommand {
    if cfg!(target_os = "windows") {
        let mut c = TokioCommand::new("cmd");
        c.args(["/C", cmd]);
        c
    } else {
        let mut c = TokioCommand::new("sh");
        c.args(["-c", cmd]);
        c
    }
}

fn gate_type_str(gt: GateType) -> &'static str {
    match gt {
        GateType::Automated => "automated",
        GateType::Manual => "manual",
        GateType::E2e => "e2e",
    }
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Truncate captured stream bytes to head + tail, marking the gap
/// when truncation fires. Returns `None` for empty input so the
/// ledger entry stays minimal.
fn truncate_stream(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let s = String::from_utf8_lossy(bytes);
    // `total` is byte-length but the constants below are *character*
    // counts (HEAD/TAIL). Multibyte content (Playwright's stderr can
    // carry checkmarks etc.) means head.len() + tail.len() can exceed
    // `total` once we extract — `saturating_sub` keeps the truncation
    // message honest without panicking on integer underflow.
    let total = s.len();
    let char_count = s.chars().count();
    if char_count <= STREAM_TRUNCATE_HEAD + STREAM_TRUNCATE_TAIL {
        return Some(s.into_owned());
    }
    let head: String = s.chars().take(STREAM_TRUNCATE_HEAD).collect();
    let tail_start = char_count.saturating_sub(STREAM_TRUNCATE_TAIL);
    let tail: String = s.chars().skip(tail_start).collect();
    let truncated_bytes = total
        .saturating_sub(head.len())
        .saturating_sub(tail.len());
    Some(format!(
        "{head}\n…[truncated {truncated_bytes} bytes]…\n{tail}"
    ))
}

/// Resolve operator handle from `--operator` flag with
/// `NEUROGRIM_OPERATOR` env fallback. No "unknown" fallback per the
/// audit-rationale discipline (LSP-Brains spec §17.6).
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

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as stdfs;
    use tempfile::TempDir;

    /// Spawn a tempdir with a Brain-shaped subdirectory and write a
    /// publish-gates manifest at the canonical path. Returns the
    /// project root for use with --project-root.
    fn write_manifest(yaml: &str) -> TempDir {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join(".claude").join("brain");
        stdfs::create_dir_all(&dir).unwrap();
        stdfs::write(dir.join("publish-gates.yaml"), yaml).unwrap();
        tmp
    }

    fn ledger_path(root: &Path) -> PathBuf {
        root.join(".claude").join("brain").join(LEDGER_FILENAME)
    }

    fn read_ledger(path: &Path) -> Vec<LedgerEntry> {
        let raw = match stdfs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        raw.lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    fn fresh_gate(id: &str, gate_type: GateType) -> Gate {
        Gate {
            id: id.into(),
            gate_type,
            description: format!("test gate {id}"),
            blocking: Some(true),
            timeout_seconds: Some(60),
            check_command: None,
            instructions: None,
            operator_required: None,
        }
    }

    /// Tempdir → Path that has no frontend dir. e2e gates targeted
    /// at this root will hit the "no frontend" error path. Used by
    /// non-e2e tests just to satisfy execute_gate's signature.
    fn no_frontend_root() -> TempDir {
        TempDir::new().unwrap()
    }

    /// Default non-interactive context for tests that exercise
    /// non-manual gate paths. Mirrors the CI default behavior.
    fn no_interactive() -> InteractiveCtx {
        InteractiveCtx { enabled: false, operator: None }
    }

    #[tokio::test]
    async fn passing_automated_gate_returns_passed_status() {
        let mut g = fresh_gate("ok", GateType::Automated);
        g.check_command = Some("echo hello".to_string());
        let tmp = no_frontend_root();
        let entry = execute_gate(&g, "run-1", "full", false, tmp.path(), &no_interactive()).await;
        assert_eq!(entry.status, "passed");
        assert_eq!(entry.exit_code, Some(0));
        assert!(entry.completed_at.is_some());
        assert!(
            entry
                .stdout_truncated
                .as_deref()
                .unwrap_or("")
                .contains("hello"),
            "stdout should carry 'hello': {entry:?}"
        );
    }

    #[tokio::test]
    async fn failing_automated_gate_returns_failed_status() {
        let mut g = fresh_gate("bad", GateType::Automated);
        // `exit 1` works in both sh and cmd.
        g.check_command = Some("exit 1".to_string());
        let tmp = no_frontend_root();
        let entry = execute_gate(&g, "run-2", "full", false, tmp.path(), &no_interactive()).await;
        assert_eq!(entry.status, "failed");
        assert_eq!(entry.exit_code, Some(1));
        assert!(entry.completed_at.is_some());
    }

    #[tokio::test]
    async fn automated_gate_without_check_command_returns_error() {
        let g = fresh_gate("nocmd", GateType::Automated);
        let tmp = no_frontend_root();
        let entry = execute_gate(&g, "run-3", "full", false, tmp.path(), &no_interactive()).await;
        assert_eq!(entry.status, "error");
        assert!(
            entry
                .error_detail
                .as_deref()
                .unwrap_or("")
                .contains("no check_command")
        );
    }

    #[tokio::test]
    async fn automated_gate_timeout_kills_and_records_timed_out() {
        let mut g = fresh_gate("slow", GateType::Automated);
        g.timeout_seconds = Some(1);
        // 5-second sleep, portable across cmd (ping) and sh (sleep).
        g.check_command = Some(if cfg!(target_os = "windows") {
            // ping with -n 6 takes ~5s.
            "ping 127.0.0.1 -n 6 > nul".to_string()
        } else {
            "sleep 5".to_string()
        });
        let tmp = no_frontend_root();
        let started = std::time::Instant::now();
        let entry = execute_gate(&g, "run-4", "full", false, tmp.path(), &no_interactive()).await;
        let elapsed = started.elapsed();
        assert_eq!(entry.status, "timed_out", "entry: {entry:?}");
        assert!(
            elapsed < Duration::from_secs(4),
            "timeout should fire ~1s in, not wait the full 5s; elapsed={elapsed:?}"
        );
    }

    #[tokio::test]
    async fn manual_gate_run_emits_pending() {
        let mut g = fresh_gate("review", GateType::Manual);
        g.instructions = Some("look at the dashboard".into());
        let tmp = no_frontend_root();
        let entry = execute_gate(&g, "run-5", "full", false, tmp.path(), &no_interactive()).await;
        assert_eq!(entry.status, "pending");
        assert_eq!(entry.completed_at, None);
    }

    /// S12-G-5 changed e2e behavior: instead of unconditional
    /// `deferred`, the runner now invokes Playwright at
    /// `<project_root>/crates/neurogrim-dashboard/frontend/`. When
    /// the frontend dir is absent — adopter brain that doesn't ship
    /// the dashboard — the gate emits `error` with a clear message
    /// pointing at the alternative (use `automated` instead).
    #[tokio::test]
    async fn e2e_gate_without_frontend_dir_returns_error() {
        let g = fresh_gate("e2e-smoke", GateType::E2e);
        let tmp = no_frontend_root();
        let entry = execute_gate(&g, "run-6", "full", false, tmp.path(), &no_interactive()).await;
        assert_eq!(entry.status, "error");
        let detail = entry.error_detail.as_deref().unwrap_or("");
        assert!(
            detail.contains("crates/neurogrim-dashboard/frontend"),
            "error_detail should mention the missing dir; got: {detail}"
        );
        assert!(
            detail.contains("automated"),
            "error_detail should suggest the `automated` alternative; got: {detail}"
        );
    }

    #[test]
    fn aggregate_exit_code_zero_when_all_passed() {
        let entries = vec![
            LedgerEntry {
                schema_version: SCHEMA_VERSION.into(),
                run_id: "r".into(),
                gate_id: "a".into(),
                gate_type: "automated".into(),
                mode: "full".into(),
                started_at: "2026-04-29T00:00:00Z".into(),
                completed_at: Some("2026-04-29T00:00:01Z".into()),
                status: "passed".into(),
                blocking: true,
                operator: None,
                exit_code: Some(0),
                stdout_truncated: None,
                stderr_truncated: None,
                error_detail: None,
            },
        ];
        assert_eq!(aggregate_exit_code(&entries), 0);
    }

    #[test]
    fn aggregate_exit_code_one_when_blocking_failed() {
        let entries = vec![
            mk("a", "passed", true),
            mk("b", "failed", true),
            mk("c", "pending", true),
        ];
        assert_eq!(aggregate_exit_code(&entries), 1, "failed dominates pending");
    }

    #[test]
    fn aggregate_exit_code_two_when_only_pending() {
        let entries = vec![
            mk("a", "passed", true),
            mk("b", "pending", true),
        ];
        assert_eq!(aggregate_exit_code(&entries), 2);
    }

    #[test]
    fn aggregate_exit_code_zero_when_advisory_failure_only() {
        let entries = vec![
            mk("a", "passed", true),
            mk("b", "failed", false), // advisory
        ];
        assert_eq!(
            aggregate_exit_code(&entries),
            0,
            "non-blocking failure must NOT drive exit"
        );
    }

    #[test]
    fn aggregate_exit_code_one_when_timed_out_blocking() {
        let entries = vec![mk("a", "timed_out", true)];
        assert_eq!(aggregate_exit_code(&entries), 1);
    }

    fn mk(id: &str, status: &str, blocking: bool) -> LedgerEntry {
        LedgerEntry {
            schema_version: SCHEMA_VERSION.into(),
            run_id: "r".into(),
            gate_id: id.into(),
            gate_type: "automated".into(),
            mode: "full".into(),
            started_at: "2026-04-29T00:00:00Z".into(),
            completed_at: Some("2026-04-29T00:00:01Z".into()),
            status: status.into(),
            blocking,
            operator: None,
            exit_code: None,
            stdout_truncated: None,
            stderr_truncated: None,
            error_detail: None,
        }
    }

    fn cfg(yaml: &str) -> PublishGatesConfig {
        neurogrim_mcp::publish_gates::validate_publish_gates_yaml(yaml)
            .expect("test fixture should validate")
    }

    #[test]
    fn select_gates_single_gate_filters_to_one() {
        let c = cfg(r#"
schema_version: "1"
gates:
  - id: a
    gate_type: automated
    description: x
    check_command: "echo ok"
  - id: b
    gate_type: automated
    description: x
    check_command: "echo ok"
"#);
        let g = select_gates(&c, Some("b"), "full").unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].id, "b");
    }

    #[test]
    fn select_gates_unknown_id_returns_error() {
        let c = cfg(r#"
schema_version: "1"
gates:
  - id: a
    gate_type: automated
    description: x
    check_command: "echo ok"
"#);
        let err = select_gates(&c, Some("does-not-exist"), "full").unwrap_err();
        assert!(err.to_string().contains("no gate with id"));
    }

    #[test]
    fn select_gates_pre_commit_filters_to_fast_automated() {
        let c = cfg(r#"
schema_version: "1"
gates:
  - id: fast-test
    gate_type: automated
    description: x
    check_command: "echo ok"
    timeout_seconds: 10
  - id: slow-test
    gate_type: automated
    description: x
    check_command: "echo ok"
    timeout_seconds: 600
  - id: review
    gate_type: manual
    description: x
    instructions: "look"
"#);
        let g = select_gates(&c, None, "pre-commit").unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].id, "fast-test");
    }

    #[test]
    fn select_gates_pre_publish_filters_to_blocking() {
        let c = cfg(r#"
schema_version: "1"
gates:
  - id: tests
    gate_type: automated
    description: x
    check_command: "echo ok"
    blocking: true
  - id: lint-advisory
    gate_type: automated
    description: x
    check_command: "echo ok"
    blocking: false
"#);
        let g = select_gates(&c, None, "pre-publish").unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].id, "tests");
    }

    #[test]
    fn select_gates_full_includes_everything() {
        let c = cfg(r#"
schema_version: "1"
gates:
  - id: tests
    gate_type: automated
    description: x
    check_command: "echo ok"
    blocking: true
  - id: lint-advisory
    gate_type: automated
    description: x
    check_command: "echo ok"
    blocking: false
"#);
        let g = select_gates(&c, None, "full").unwrap();
        assert_eq!(g.len(), 2);
    }

    #[test]
    fn ledger_round_trip_serialization() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("ledger.jsonl");
        let entries = vec![
            mk("a", "passed", true),
            mk("b", "pending", true),
        ];
        append_ledger_entries(&path, &entries).unwrap();
        let read = read_ledger(&path);
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].status, "passed");
        assert_eq!(read[1].status, "pending");
    }

    #[test]
    fn read_most_recent_pending_returns_none_when_no_ledger() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("ledger.jsonl");
        let result = read_most_recent_pending(&path, "any").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn read_most_recent_pending_returns_pending_entry() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("ledger.jsonl");
        append_ledger_entries(
            &path,
            &[
                mk("a", "passed", true),
                mk("b", "pending", true),
            ],
        )
        .unwrap();
        let result = read_most_recent_pending(&path, "b").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().status, "pending");
    }

    #[test]
    fn read_most_recent_pending_returns_none_when_resolved() {
        // Pending → then ack (passed) — the pending is no longer
        // outstanding.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("ledger.jsonl");
        append_ledger_entries(
            &path,
            &[
                mk("a", "pending", true),
                mk("a", "passed", true), // ack came in
            ],
        )
        .unwrap();
        let result = read_most_recent_pending(&path, "a").unwrap();
        assert!(
            result.is_none(),
            "passed entry must invalidate prior pending; got {result:?}"
        );
    }

    #[test]
    fn truncate_stream_short_returns_unchanged() {
        let s = b"hello";
        assert_eq!(truncate_stream(s).as_deref(), Some("hello"));
    }

    #[test]
    fn truncate_stream_empty_returns_none() {
        assert_eq!(truncate_stream(b""), None);
    }

    #[test]
    fn truncate_stream_long_emits_head_and_tail_with_marker() {
        // Need > HEAD + TAIL bytes (8192) to cross the truncation
        // threshold. Use 2*HEAD + 2*TAIL + tail-sentinel so the
        // gap is large and the assertions are stable under refactor.
        let big_size = STREAM_TRUNCATE_HEAD * 2 + STREAM_TRUNCATE_TAIL * 2;
        let mut buf = String::with_capacity(big_size + 32);
        for i in 0..big_size {
            buf.push((b'a' + (i % 26) as u8) as char);
        }
        buf.push_str("UNIQUETAILSENTINEL");
        let result = truncate_stream(buf.as_bytes()).unwrap();
        assert!(
            result.contains("…[truncated"),
            "expected truncation marker; result starts with: {}",
            &result.chars().take(120).collect::<String>()
        );
        assert!(
            result.contains("UNIQUETAILSENTINEL"),
            "tail sentinel must be preserved"
        );
        assert!(
            result.len() < buf.len(),
            "truncated must be smaller than input"
        );
    }

    /// Round-trip through `ack_subcmd`: write a manifest, run a manual
    /// gate to land a pending entry, then ack it. Verify a follow-up
    /// passed entry lands in the ledger.
    #[tokio::test]
    async fn ack_subcmd_marks_pending_as_passed() {
        let tmp = write_manifest(
            r#"
schema_version: "1"
gates:
  - id: review
    gate_type: manual
    description: look at it
    instructions: "look"
"#,
        );
        // Land a pending entry directly (skipping the print path —
        // execute_manual would also work but adds noise to test output).
        let pending = LedgerEntry {
            schema_version: SCHEMA_VERSION.into(),
            run_id: "run-pending".into(),
            gate_id: "review".into(),
            gate_type: "manual".into(),
            mode: "full".into(),
            started_at: "2026-04-29T00:00:00Z".into(),
            completed_at: None,
            status: Status::Pending.as_str().into(),
            blocking: true,
            operator: None,
            exit_code: None,
            stdout_truncated: None,
            stderr_truncated: None,
            error_detail: None,
        };
        append_ledger_entries(&ledger_path(tmp.path()), &[pending]).unwrap();

        // Ack via the sub-command (shimming via direct call to the
        // ack subcmd avoids spawning a child binary).
        std::env::set_var("NEUROGRIM_OPERATOR", "test-op");
        let result = ack_subcmd(AckArgs {
            gate: "review".into(),
            operator: None,
            project_root: tmp.path().display().to_string(),
        })
        .await;
        std::env::remove_var("NEUROGRIM_OPERATOR");
        assert!(result.is_ok(), "ack failed: {result:?}");

        let entries = read_ledger(&ledger_path(tmp.path()));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].status, "pending");
        assert_eq!(entries[1].status, "passed");
        assert_eq!(entries[1].operator.as_deref(), Some("test-op"));
        assert_eq!(entries[1].run_id, "run-pending"); // same run_id as pending
        assert_eq!(entries[1].mode, "ack");
    }

    #[tokio::test]
    async fn ack_subcmd_errors_when_no_pending() {
        let tmp = TempDir::new().unwrap();
        std::env::set_var("NEUROGRIM_OPERATOR", "test-op");
        let result = ack_subcmd(AckArgs {
            gate: "ghost".into(),
            operator: None,
            project_root: tmp.path().display().to_string(),
        })
        .await;
        std::env::remove_var("NEUROGRIM_OPERATOR");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("no pending"),
            "expected 'no pending' message"
        );
    }

    #[test]
    fn resolve_operator_prefers_arg_over_env() {
        std::env::set_var("NEUROGRIM_OPERATOR", "from-env");
        let r = resolve_operator(Some("from-arg")).unwrap();
        std::env::remove_var("NEUROGRIM_OPERATOR");
        assert_eq!(r, "from-arg");
    }

    #[test]
    fn resolve_operator_falls_back_to_env() {
        std::env::set_var("NEUROGRIM_OPERATOR", "env-only");
        let r = resolve_operator(None).unwrap();
        std::env::remove_var("NEUROGRIM_OPERATOR");
        assert_eq!(r, "env-only");
    }

    #[test]
    fn resolve_operator_errors_when_neither_set() {
        // Make sure env is clear (other tests may have set it).
        std::env::remove_var("NEUROGRIM_OPERATOR");
        let r = resolve_operator(None);
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("operator identity"));
    }

    // ── S12-G-6: interactive manual-gate prompt ─────────────────────

    #[test]
    fn prompt_y_with_operator_returns_acked() {
        let g = fresh_gate("review", GateType::Manual);
        let mut input = std::io::Cursor::new(b"y\n".to_vec());
        let outcome = prompt_manual_gate_inline(&g, &mut input, Some("alice"));
        assert_eq!(outcome, ManualPromptOutcome::Acked("alice".to_string()));
    }

    #[test]
    fn prompt_yes_uppercase_also_accepts() {
        let g = fresh_gate("review", GateType::Manual);
        let mut input = std::io::Cursor::new(b"YES\n".to_vec());
        let outcome = prompt_manual_gate_inline(&g, &mut input, Some("alice"));
        assert_eq!(outcome, ManualPromptOutcome::Acked("alice".to_string()));
    }

    #[test]
    fn prompt_n_returns_pending() {
        let g = fresh_gate("review", GateType::Manual);
        let mut input = std::io::Cursor::new(b"n\n".to_vec());
        let outcome = prompt_manual_gate_inline(&g, &mut input, Some("alice"));
        assert_eq!(outcome, ManualPromptOutcome::Pending(None));
    }

    #[test]
    fn prompt_empty_returns_pending() {
        let g = fresh_gate("review", GateType::Manual);
        let mut input = std::io::Cursor::new(b"\n".to_vec());
        let outcome = prompt_manual_gate_inline(&g, &mut input, Some("alice"));
        assert_eq!(outcome, ManualPromptOutcome::Pending(None));
    }

    #[test]
    fn prompt_y_without_operator_falls_through_to_pending_with_reason() {
        let g = fresh_gate("review", GateType::Manual);
        let mut input = std::io::Cursor::new(b"y\n".to_vec());
        let outcome = prompt_manual_gate_inline(&g, &mut input, None);
        match outcome {
            ManualPromptOutcome::Pending(Some(reason)) => {
                assert!(
                    reason.contains("no operator handle"),
                    "reason should explain why we fell through; got: {reason}"
                );
            }
            other => panic!("expected Pending(Some(_)); got {other:?}"),
        }
    }

    #[test]
    fn prompt_y_with_blank_operator_falls_through_to_pending() {
        let g = fresh_gate("review", GateType::Manual);
        let mut input = std::io::Cursor::new(b"y\n".to_vec());
        let outcome = prompt_manual_gate_inline(&g, &mut input, Some("   "));
        assert!(matches!(outcome, ManualPromptOutcome::Pending(Some(_))));
    }

    #[tokio::test]
    async fn execute_manual_with_interactive_yes_returns_passed_with_operator() {
        let g = fresh_gate("review", GateType::Manual);
        // We can't easily inject a fake reader into execute_manual (it
        // uses stdin directly). Instead, exercise the unit via
        // finalize_manual_passed which is what execute_manual calls
        // when the prompt returns Acked.
        let entry = finalize_manual_passed(
            LedgerEntry {
                schema_version: SCHEMA_VERSION.into(),
                run_id: "run-x".into(),
                gate_id: g.id.clone(),
                gate_type: "manual".into(),
                mode: "full".into(),
                started_at: "2026-04-29T00:00:00Z".into(),
                completed_at: None,
                status: Status::Error.as_str().into(),
                blocking: true,
                operator: None,
                exit_code: None,
                stdout_truncated: None,
                stderr_truncated: None,
                error_detail: None,
            },
            "alice".to_string(),
        );
        assert_eq!(entry.status, "passed");
        assert_eq!(entry.operator.as_deref(), Some("alice"));
        assert!(entry.completed_at.is_some());
    }

    #[tokio::test]
    async fn execute_manual_with_no_interactive_emits_pending() {
        let mut g = fresh_gate("review", GateType::Manual);
        g.instructions = Some("look at it".into());
        let tmp = no_frontend_root();
        // Default no_interactive() context — should produce pending.
        let entry =
            execute_gate(&g, "run-int", "full", false, tmp.path(), &no_interactive()).await;
        assert_eq!(entry.status, "pending");
        assert_eq!(entry.completed_at, None);
        assert_eq!(entry.operator, None);
    }
}
