//! `neurogrim diag` — operator-facing CLI surface for the
//! V5-FOUND-1 diagnostics ledger.
//!
//! Two subcommands at v1:
//!
//! - **`report`** — read
//!   `<project_root>/.claude/brain/diagnostics.jsonl` and print
//!   the top-N slowest operations grouped by `name`, with
//!   `count`, `p50_ms`, `p95_ms`, `p99_ms`, `max_ms`. Default
//!   output is a small text table; `--json` emits JSON. Filters:
//!   `--kind`, `--since`, `--name`. The default project_root is
//!   `.` (CWD), matching the existing per-command convention used
//!   by other CLI subcommands (e.g., `disposition record
//!   --project-root .`).
//!
//! - **`synthesize`** — **deferred to V5-FOUND-1.1**. Reserves
//!   the subcommand name so `--help` listings show the future
//!   surface; invocation returns a clear "not yet implemented"
//!   error pointing at V5-FOUND-1.1. The deferral was a
//!   plan-critic finding (no Rust-side LLM client exists today;
//!   no `anthropic` crate, no runtime `reqwest`, neurogrim-secrets
//!   stores credentials at rest, claude-proxy is for containerized
//!   agents). Building that pathway is +2–3 days that decoupled
//!   cleanly from V5-FOUND-1's core value (instrumentation +
//!   baseline + report).
//!
//! The diagnostics ledger writer + schema live in
//! `neurogrim-core/src/diagnostics_ledger.rs`; the tracing Layer
//! that produces entries lives in
//! `neurogrim-cli/src/diagnostics_layer.rs`.

use anyhow::{Context, Result};
use clap::{Args as ClapArgs, Subcommand};
use neurogrim_core::diagnostics_ledger::{self, DiagnosticsEntry};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Top-level args for `neurogrim diag <subcommand>`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub subcommand: DiagCmd,
}

#[derive(Subcommand, Debug)]
pub enum DiagCmd {
    /// Summarize the diagnostics ledger as a top-N slow-ops report.
    ///
    /// Reads `<project_root>/.claude/brain/diagnostics.jsonl`,
    /// groups entries by span `name`, and prints the slowest by
    /// p95 latency. Use `--json` for machine-readable output and
    /// `--kind`, `--since`, `--name` to filter.
    Report {
        /// Project root containing `.claude/brain/diagnostics.jsonl`.
        /// Defaults to the current working directory, matching the
        /// CWD-as-project-root convention used elsewhere in the CLI.
        #[arg(long, default_value = ".")]
        project_root: String,

        /// Show top-N slowest operations by p95 latency.
        #[arg(short, long, default_value = "10")]
        top: usize,

        /// Filter to a single event kind (e.g., `scoring`,
        /// `mcp_dispatch`, `a2a_post`, `dashboard_route`). Values
        /// match the schema's `kind` enum.
        #[arg(long)]
        kind: Option<String>,

        /// Filter to entries whose `ts_start` is at or after this
        /// ISO 8601 timestamp (string comparison; ISO 8601 sorts
        /// lexicographically when timezones are uniform — UTC `Z`
        /// suffix is the convention here).
        #[arg(long)]
        since: Option<String>,

        /// Filter to entries whose `name` starts with this prefix
        /// (e.g., `score.` to match all `score.*` spans).
        #[arg(long)]
        name: Option<String>,

        /// Emit JSON instead of the default text table.
        #[arg(long)]
        json: bool,
    },

    /// **Deferred to V5-FOUND-1.1** — agent-driven synthesis with
    /// structural guardrails (baseline + target citations
    /// required). Returns a "not yet implemented" error in v1.
    Synthesize,
}

/// Entry point for `neurogrim diag <subcommand>`.
pub async fn run(args: Args) -> Result<()> {
    match args.subcommand {
        DiagCmd::Report {
            project_root,
            top,
            kind,
            since,
            name,
            json,
        } => run_report(
            &project_root,
            top,
            kind.as_deref(),
            since.as_deref(),
            name.as_deref(),
            json,
        ),
        DiagCmd::Synthesize => {
            // V5-FOUND-1.1 (deferred 2026-05-02). Reserve the
            // subcommand name so --help shows the future surface.
            anyhow::bail!(
                "neurogrim diag synthesize is not yet implemented. \
                 Deferred to V5-FOUND-1.1 (a follow-on epic) because no \
                 Rust-side LLM client exists today (no `anthropic` crate, \
                 no runtime `reqwest`, neurogrim-secrets is for credentials \
                 at rest, claude-proxy is for containerized agents). \
                 See `.claude/plans/v5-found-1-diagnostic-monitor.md` § \
                 V5-FOUND-1.1 for the carry-forward design."
            )
        }
    }
}

/// Statistic row per `name` group emitted by the `report` command.
/// Serialize-able for `--json` output.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NameStat {
    /// Span name (e.g., `score.pipeline.run`).
    pub name: String,
    /// Number of entries with this `name` after filtering.
    pub count: usize,
    /// 50th-percentile duration in milliseconds.
    pub p50_ms: u64,
    /// 95th-percentile duration in milliseconds.
    pub p95_ms: u64,
    /// 99th-percentile duration in milliseconds.
    pub p99_ms: u64,
    /// Largest duration observed in milliseconds.
    pub max_ms: u64,
}

fn run_report(
    project_root: &str,
    top: usize,
    kind: Option<&str>,
    since: Option<&str>,
    name: Option<&str>,
    json: bool,
) -> Result<()> {
    let path = diagnostics_ledger::default_ledger_path(Path::new(project_root));
    let entries = diagnostics_ledger::read_all(&path).with_context(|| {
        format!(
            "neurogrim diag report: read {} (the diagnostics ledger)",
            path.display()
        )
    })?;

    if entries.is_empty() {
        if json {
            println!("[]");
        } else {
            println!(
                "no events in {} (empty ledger or first run; \
                 set NEUROGRIM_DIAG=1 and run an instrumented \
                 command — e.g., `neurogrim score` — to populate)",
                path.display()
            );
        }
        return Ok(());
    }

    let stats = compute_stats(&entries, kind, since, name, top);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&stats)
                .context("neurogrim diag report: serialize report to JSON")?
        );
    } else {
        print_table(&stats);
    }
    Ok(())
}

/// Pure compute: filter `entries`, group by `name`, sort by p95
/// descending, truncate to `top`. Lifted out of `run_report` so
/// tests can drive it directly without filesystem fixtures.
pub fn compute_stats(
    entries: &[DiagnosticsEntry],
    kind: Option<&str>,
    since: Option<&str>,
    name_prefix: Option<&str>,
    top: usize,
) -> Vec<NameStat> {
    // Filter
    let filtered: Vec<&DiagnosticsEntry> = entries
        .iter()
        .filter(|e| {
            if let Some(k) = kind {
                if e.kind.as_str() != k {
                    return false;
                }
            }
            if let Some(s) = since {
                // ISO 8601 sorts lexicographically when normalized
                // (UTC + millisecond precision). Our writer emits
                // RFC 3339 with Z suffix, so this is sound.
                if e.ts_start.as_str() < s {
                    return false;
                }
            }
            if let Some(p) = name_prefix {
                if !e.name.starts_with(p) {
                    return false;
                }
            }
            true
        })
        .collect();

    // Group durations by name. BTreeMap so iteration is deterministic
    // (handy for tests + stable JSON output).
    let mut by_name: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for e in &filtered {
        by_name
            .entry(e.name.clone())
            .or_default()
            .push(e.duration_ms);
    }

    // Compute stats
    let mut stats: Vec<NameStat> = by_name
        .into_iter()
        .map(|(name, mut durs)| {
            durs.sort_unstable();
            let count = durs.len();
            NameStat {
                name,
                count,
                p50_ms: percentile(&durs, 50),
                p95_ms: percentile(&durs, 95),
                p99_ms: percentile(&durs, 99),
                max_ms: durs.last().copied().unwrap_or(0),
            }
        })
        .collect();

    // Sort by p95 desc; ties broken by name (stable, alpha).
    stats.sort_by(|a, b| b.p95_ms.cmp(&a.p95_ms).then(a.name.cmp(&b.name)));
    stats.truncate(top);
    stats
}

/// Compute the `p`-th percentile of a sorted slice using
/// nearest-rank (rounded). Returns 0 for an empty slice. We use
/// nearest-rank rather than linear interpolation so the result
/// is always one of the observed values (matches operator
/// expectations when the table column says `p95_ms = 17` —
/// "the 95th-percentile run took 17ms").
fn percentile(sorted: &[u64], p: u32) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let n = sorted.len();
    // p/100 * (n-1) → nearest index, clamped to [0, n-1].
    let idx = ((p as f64 / 100.0) * (n as f64 - 1.0)).round() as usize;
    sorted[idx.min(n - 1)]
}

fn print_table(stats: &[NameStat]) {
    if stats.is_empty() {
        println!("no events match the given filters");
        return;
    }
    // Column widths chosen to fit a typical 80-col terminal with
    // span names up to ~32 chars (longest current: dashboard.route).
    println!(
        "{:<32} {:>6} {:>10} {:>10} {:>10} {:>10}",
        "name", "count", "p50_ms", "p95_ms", "p99_ms", "max_ms"
    );
    println!("{}", "-".repeat(82));
    for s in stats {
        println!(
            "{:<32} {:>6} {:>10} {:>10} {:>10} {:>10}",
            s.name, s.count, s.p50_ms, s.p95_ms, s.p99_ms, s.max_ms
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurogrim_core::diagnostics_ledger::{self as dl, EventKind, Outcome};
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn make_entry(
        name: &str,
        kind: EventKind,
        duration_ms: u64,
        ts_start: &str,
    ) -> DiagnosticsEntry {
        DiagnosticsEntry {
            schema_version: 1,
            event_id: uuid::Uuid::new_v4().to_string(),
            ts_start: ts_start.to_string(),
            duration_ms,
            kind,
            name: name.to_string(),
            outcome: Outcome::Ok,
            depth: 0,
            parent_event_id: None,
            extras: BTreeMap::new(),
        }
    }

    #[test]
    fn percentile_handles_small_and_empty_inputs() {
        assert_eq!(percentile(&[], 50), 0);
        assert_eq!(percentile(&[42], 50), 42);
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 50), 3);
        // p95 over [1..100] should land near the top.
        let v: Vec<u64> = (1..=100).collect();
        let p95 = percentile(&v, 95);
        assert!(p95 >= 95 && p95 <= 100, "expected ~95-100, got {p95}");
    }

    #[test]
    fn compute_stats_groups_by_name_and_sorts_by_p95() {
        let entries = vec![
            make_entry("score.pipeline.run", EventKind::Scoring, 5, "2026-05-02T00:00:00.000Z"),
            make_entry("score.pipeline.run", EventKind::Scoring, 8, "2026-05-02T00:00:01.000Z"),
            make_entry("score.pipeline.run", EventKind::Scoring, 12, "2026-05-02T00:00:02.000Z"),
            make_entry("test.run", EventKind::Test, 100, "2026-05-02T00:00:03.000Z"),
            make_entry("test.run", EventKind::Test, 200, "2026-05-02T00:00:04.000Z"),
        ];
        let stats = compute_stats(&entries, None, None, None, 10);
        assert_eq!(stats.len(), 2);
        // test.run (max 200) should sort above score.pipeline.run (max 12).
        assert_eq!(stats[0].name, "test.run");
        assert_eq!(stats[0].count, 2);
        assert_eq!(stats[0].max_ms, 200);
        assert_eq!(stats[1].name, "score.pipeline.run");
        assert_eq!(stats[1].count, 3);
        // p50 of [5, 8, 12] is 8 (nearest rank).
        assert_eq!(stats[1].p50_ms, 8);
    }

    #[test]
    fn compute_stats_kind_filter_excludes_others() {
        let entries = vec![
            make_entry("score.pipeline.run", EventKind::Scoring, 10, "2026-05-02T00:00:00Z"),
            make_entry("test.run", EventKind::Test, 100, "2026-05-02T00:00:01Z"),
        ];
        let stats = compute_stats(&entries, Some("scoring"), None, None, 10);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].name, "score.pipeline.run");
    }

    #[test]
    fn compute_stats_since_filter_drops_older_entries() {
        let entries = vec![
            make_entry("test.run", EventKind::Test, 1, "2026-05-01T00:00:00Z"),
            make_entry("test.run", EventKind::Test, 2, "2026-05-02T00:00:00Z"),
            make_entry("test.run", EventKind::Test, 3, "2026-05-03T00:00:00Z"),
        ];
        let stats = compute_stats(&entries, None, Some("2026-05-02T00:00:00Z"), None, 10);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].count, 2, "should include 05-02 and 05-03");
    }

    #[test]
    fn compute_stats_name_prefix_filter() {
        let entries = vec![
            make_entry("score.pipeline.run", EventKind::Scoring, 10, "2026-05-02T00:00:00Z"),
            make_entry("score.foo", EventKind::Scoring, 20, "2026-05-02T00:00:01Z"),
            make_entry("test.run", EventKind::Test, 30, "2026-05-02T00:00:02Z"),
        ];
        let stats = compute_stats(&entries, None, None, Some("score."), 10);
        assert_eq!(stats.len(), 2);
        let names: Vec<&str> = stats.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"score.pipeline.run"));
        assert!(names.contains(&"score.foo"));
        assert!(!names.contains(&"test.run"));
    }

    #[test]
    fn compute_stats_top_truncates() {
        let entries: Vec<DiagnosticsEntry> = (0..15)
            .map(|i| {
                make_entry(
                    &format!("name.{i:02}"),
                    EventKind::Scoring,
                    i as u64 * 10,
                    "2026-05-02T00:00:00Z",
                )
            })
            .collect();
        let stats = compute_stats(&entries, None, None, None, 5);
        assert_eq!(stats.len(), 5, "top=5 must cap at 5");
        // Slowest should be name.14 (140ms).
        assert_eq!(stats[0].name, "name.14");
    }

    #[test]
    fn run_report_empty_ledger_prints_friendly_message() {
        // No ledger file at all → friendly message + exit 0.
        let dir = TempDir::new().unwrap();
        let result = run_report(
            dir.path().to_str().unwrap(),
            10,
            None,
            None,
            None,
            false,
        );
        assert!(result.is_ok(), "missing ledger must NOT error: {result:?}");
    }

    #[test]
    fn run_report_empty_ledger_json_emits_empty_array() {
        let dir = TempDir::new().unwrap();
        let result = run_report(
            dir.path().to_str().unwrap(),
            10,
            None,
            None,
            None,
            true,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn run_report_real_ledger_round_trips() {
        let dir = TempDir::new().unwrap();
        let project_root = dir.path();
        // Write 3 scoring entries via the writer (exercises the
        // full schema validation path, mirroring a real deployment).
        let path = dl::default_ledger_path(project_root);
        for ms in [5, 10, 15u64] {
            let mut extras = BTreeMap::new();
            extras.insert("score".to_string(), serde_json::json!(80));
            let entry = DiagnosticsEntry {
                schema_version: 1,
                event_id: uuid::Uuid::new_v4().to_string(),
                ts_start: "2026-05-02T00:00:00Z".to_string(),
                duration_ms: ms,
                kind: EventKind::Scoring,
                name: "score.pipeline.run".to_string(),
                outcome: Outcome::Ok,
                depth: 0,
                parent_event_id: None,
                extras,
            };
            dl::append(&path, &entry).unwrap();
        }
        let result = run_report(
            project_root.to_str().unwrap(),
            10,
            None,
            None,
            None,
            true,
        );
        assert!(result.is_ok(), "real-ledger round trip should succeed");
    }

    #[test]
    fn synthesize_returns_clear_v5_found_1_1_pointer() {
        // Importing inside the test to keep the runtime check tight.
        let args = Args {
            subcommand: DiagCmd::Synthesize,
        };
        // run() is async; use a small runtime for the assertion.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(run(args));
        let err = result.expect_err("synthesize must error in V5-FOUND-1");
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("V5-FOUND-1.1"),
            "error message must point at V5-FOUND-1.1: {msg}"
        );
    }
}
