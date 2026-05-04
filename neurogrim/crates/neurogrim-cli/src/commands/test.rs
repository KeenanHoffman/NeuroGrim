//! `neurogrim test` — quiet test wrapper with persisted failure ledger.
//!
//! S12-G-2 (v4.0 publish-gate foundation). Wraps
//! `cargo test --workspace --all-targets`, suppresses the per-test
//! success spam, prints failures inline, and appends one ledger
//! entry per failure to
//! `<project_root>/.claude/brain/test-failures.jsonl`.
//!
//! Exits with code 0 when all tests pass, 1 when any fail — mirrors
//! cargo's exit code so the wrapper drops into CI / pre-publish gate
//! pipelines without translation.
//!
//! ## Flags
//!
//! - `--keep-last N` (default 500): after the run, ledger entries
//!   beyond the N most-recent are rotated to
//!   `test-failures.archive.jsonl`.
//! - `--show-only-new`: compare the current run's failures against
//!   the most recent prior run; only print failures whose test names
//!   didn't appear last time.
//! - `--retry-failed`: skip the workspace run; instead, run
//!   `cargo test --workspace -- --exact <name>` for each test name
//!   in the most recent failure batch.
//! - `--slow`: pass `--include-ignored` to cargo. Runs the
//!   `#[ignore]`-marked benchmarks (S12-G-1 moved
//!   `b10_phase1_*` here).
//! - `--verbose`: bypass the quiet wrapper; stream cargo's output
//!   unchanged. Useful when the parser misclassifies output during
//!   debug.
//!
//! ## Ledger schema (v1)
//!
//! ```json
//! {
//!   "schema_version": "1",
//!   "run_id": "<uuid v4>",
//!   "ts": "2026-04-29T13:42:18Z",
//!   "test_name": "<full test path>",
//!   "binary": "<test-binary identifier>",
//!   "outcome": "failed",
//!   "output": "<panic / assertion detail, ANSI-stripped>",
//!   "commit": "<git rev or 'unknown'>"
//! }
//! ```
//!
//! Append-only via `OpenOptions::create(true).append(true)`. Same
//! discipline as the disposition + calibration ledgers — POSIX +
//! Windows guarantee atomicity for sub-`PIPE_BUF` (~4 KB) writes,
//! and a `FailureEntry` is comfortably under that. Concurrent
//! `neurogrim test` invocations cannot interleave entries.
//!
//! ## Caveats
//!
//! - `--show-only-new` compares against the immediately-prior run
//!   regardless of `run_kind`. If the prior run was
//!   `--retry-failed`, the baseline is just those tests — not the
//!   full workspace. v1 limitation; documented.
//! - The cargo output parser handles libtest's standard format
//!   (one binary at a time, "test result: ..." per binary). Custom
//!   harnesses may parse differently; fall back to `--verbose`.

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use clap::Args as ClapArgs;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

const SCHEMA_VERSION: &str = "1";
const ARCHIVE_FILENAME: &str = "test-failures.archive.jsonl";
const LEDGER_FILENAME: &str = "test-failures.jsonl";

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Maximum failure entries kept in the active ledger.
    /// Excess entries rotated to `test-failures.archive.jsonl`.
    #[arg(long, default_value_t = 500)]
    pub keep_last: u32,

    /// Show only failures whose test names didn't appear in the
    /// previous run.
    #[arg(long)]
    pub show_only_new: bool,

    /// Skip the workspace run; replay only the tests in the most
    /// recent failure batch.
    #[arg(long)]
    pub retry_failed: bool,

    /// Include `#[ignore]`d tests. Passes `--include-ignored` to
    /// cargo. Useful for the v3.5 / v4.0 slow benchmarks.
    #[arg(long)]
    pub slow: bool,

    /// Bypass the quiet wrapper; stream cargo's output unchanged.
    #[arg(long, short = 'v')]
    pub verbose: bool,

    /// v4.0 S12-G-5 — invoke the Playwright E2E suite at
    /// `<project_root>/crates/neurogrim-dashboard/frontend/`
    /// instead of cargo. Skips cargo entirely; mirrors playwright's
    /// exit code. Requires the operator to have run
    /// `npm install` + `npx playwright install chromium` (one-time)
    /// AND `cargo build --bin neurogrim` + `npm run build` (every
    /// time the dashboard or frontend changes). README documents
    /// the build sequence.
    #[arg(long)]
    pub e2e: bool,

    /// Project root containing `.claude/brain/`. Default: cwd.
    #[arg(long, default_value = ".")]
    pub project_root: String,

    /// V5-FOUND-2 Phase 1 (2026-05-03) — nextest profile to use.
    /// Profiles defined in `<workspace>/.config/nextest.toml`:
    /// - `default` (developer-friendly: no fail-fast, no retries)
    /// - `ci` (strict: fail-fast, retries=2, `flaky-result = "fail"`)
    ///
    /// Pass-through to `cargo nextest run --profile <name>`. Operator
    /// rarely needs to override; CI uses `--profile ci` directly via
    /// `.github/workflows/ci.yml`. Ignored when `--verbose` is set
    /// (the libtest passthrough doesn't honor nextest profiles).
    #[arg(long, default_value = "default")]
    pub profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailureEntry {
    pub schema_version: String,
    pub run_id: String,
    pub ts: String,
    pub test_name: String,
    pub binary: String,
    pub outcome: String,
    pub output: String,
    pub commit: String,
}

/// Entry point for `neurogrim test`.
pub async fn run(args: Args) -> Result<()> {
    let project_root = Path::new(&args.project_root);

    // S12-G-5: --e2e diverts entirely from the cargo path. Spawn
    // Playwright in the dashboard frontend dir, inherit its stdio so
    // the operator sees real-time test progress, mirror its exit code.
    if args.e2e {
        let frontend = find_dashboard_frontend(project_root).with_context(|| {
            "could not locate `crates/neurogrim-dashboard/frontend/` from \
             --project-root; --e2e is only meaningful inside the NeuroGrim \
             workspace where the dashboard lives"
        })?;
        eprintln!(
            "✦ neurogrim test --e2e — invoking Playwright at {}",
            frontend.display()
        );
        let status = spawn_playwright_inherit(&frontend)
            .with_context(|| "failed to spawn playwright")?;
        std::process::exit(status.code().unwrap_or(1));
    }

    let ledger_path = ledger_path(project_root);
    let archive_path = project_root.join(".claude/brain").join(ARCHIVE_FILENAME);

    // V5-FOUND-2 Phase 1 (2026-05-03): main path now invokes
    // `cargo nextest run` instead of `cargo test`. The libtest path
    // remains accessible via `--verbose` (raw cargo passthrough).
    //
    // Nextest invocation differences from the libtest path:
    // - Subcommand `nextest run` instead of `test`
    // - `--profile <name>` flag (configured at .config/nextest.toml)
    // - `-- --exact <name>` retry pattern is libtest-compat (per V5-FOUND-2
    //   plan Fork F1; nextest filterset DSL deferred to v5.5)
    // - `--include-ignored` for `--slow` works via the same libtest-compat
    //   `-- --include-ignored` arg passthrough
    let cargo_args: Vec<String> = if args.retry_failed {
        let to_retry = read_most_recent_run_failure_names(&ledger_path);
        if to_retry.is_empty() {
            eprintln!(
                "✦ neurogrim test --retry-failed: no failures in the ledger at {} — nothing to retry.",
                ledger_path.display()
            );
            return Ok(());
        }
        eprintln!(
            "✦ neurogrim test --retry-failed: re-running {} prior failure(s)",
            to_retry.len()
        );
        build_cargo_args(&args, Some(&to_retry))
    } else {
        build_cargo_args(&args, None)
    };

    // V5-FOUND-1 Phase 3 step 2: wrap the main test path in a
    // `test.run` parent span; the cargo invocation is a child
    // `cargo.invoke` span (same subprocess boundary, per plan-critic
    // — cargo timing folds into test rather than being its own
    // surface). Block scope so the spans drop and the diagnostics
    // Layer's on_close fires BEFORE std::process::exit at the
    // bottom (exit doesn't run drop). The --verbose / --e2e paths
    // exit too quickly to instrument cleanly; their cargo invocation
    // gets a `cargo.invoke` span only.

    // If verbose, just exec cargo and bail — let cargo speak.
    if args.verbose {
        let status = {
            let cargo_span = tracing::info_span!(
                "cargo.invoke",
                cmd = "test",
                exit_code = tracing::field::Empty,
            );
            let _ce = cargo_span.enter();
            let s = Command::new("cargo")
                .args(&cargo_args)
                .status()
                .with_context(|| "failed to spawn cargo")?;
            cargo_span.record("exit_code", s.code().unwrap_or(-1) as i64);
            s
        };
        std::process::exit(status.code().unwrap_or(1));
    }

    // Main path: parent test.run span + child cargo.invoke span,
    // both block-scoped so spans drop (and the Layer emits) before
    // process::exit at the bottom of the function.
    let test_run_span = tracing::info_span!(
        "test.run",
        test_count = tracing::field::Empty,
        fail_count = tracing::field::Empty,
        ignored_count = tracing::field::Empty,
    );
    let _test_entered = test_run_span.enter();

    // Run cargo with output captured (child span).
    eprintln!("✦ neurogrim test — running workspace tests…");
    let output = {
        let cargo_span = tracing::info_span!(
            "cargo.invoke",
            cmd = "test",
            exit_code = tracing::field::Empty,
        );
        let _ce = cargo_span.enter();
        let o = Command::new("cargo")
            .args(&cargo_args)
            .output()
            .with_context(|| "failed to spawn cargo")?;
        cargo_span.record("exit_code", o.status.code().unwrap_or(-1) as i64);
        o
    };

    let stdout_text = String::from_utf8_lossy(&output.stdout);
    let stderr_text = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout_text}\n{stderr_text}");

    // V5-FOUND-2 Phase 1: nextest stdout has a different shape from
    // libtest's. The new parser handles `PASS/FAIL/FLAKY` lines,
    // `--- STDERR/STDOUT:` failure detail blocks, and the `Summary`
    // total line.
    let parsed = parse_nextest_output(&combined);

    // Build failure entries for this run.
    let run_id = Uuid::new_v4().to_string();
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let commit = current_git_rev(project_root);
    let entries: Vec<FailureEntry> = parsed
        .failures
        .iter()
        .map(|f| FailureEntry {
            schema_version: SCHEMA_VERSION.to_string(),
            run_id: run_id.clone(),
            ts: ts.clone(),
            test_name: f.test_name.clone(),
            binary: f.binary.clone(),
            outcome: "failed".to_string(),
            output: f.output_block.clone(),
            commit: commit.clone(),
        })
        .collect();

    // For --show-only-new, derive what's new vs the previous run.
    let prior_names = read_most_recent_run_failure_names(&ledger_path)
        .into_iter()
        .collect::<HashSet<_>>();

    let to_print: Vec<&FailureEntry> = if args.show_only_new {
        entries.iter().filter(|e| !prior_names.contains(&e.test_name)).collect()
    } else {
        entries.iter().collect()
    };

    // Print operator-facing summary.
    print_summary(&parsed, &to_print, args.show_only_new);

    // Persist failures to the ledger.
    if !entries.is_empty() {
        append_failures(&ledger_path, &entries)
            .with_context(|| format!("failed to append to {}", ledger_path.display()))?;
    }

    // Rotate ledger if it's grown past `keep_last`.
    rotate_ledger_if_needed(&ledger_path, &archive_path, args.keep_last)
        .with_context(|| "failed to rotate ledger")?;

    // V5-FOUND-1 Phase 3 step 2: record final test counts before the
    // span drops. The Layer's on_close composes the entry from these.
    test_run_span.record("test_count", parsed.total_tests as i64);
    test_run_span.record("fail_count", parsed.total_failed as i64);
    test_run_span.record("ignored_count", parsed.total_ignored as i64);

    // Capture exit code, then drop the entered guard + the span so
    // the Layer's on_close fires before std::process::exit (exit
    // does NOT run Drop, so without explicit drops here the span
    // would never close and no ledger entry would be written).
    let exit_code = output.status.code().unwrap_or(1);
    drop(_test_entered);
    drop(test_run_span);
    std::process::exit(exit_code);
}

/// Path to `<project>/.claude/brain/test-failures.jsonl`.
pub fn ledger_path(project_root: &Path) -> PathBuf {
    project_root.join(".claude/brain").join(LEDGER_FILENAME)
}

/// Build the cargo subcommand arg vector for a `neurogrim test` run.
///
/// V5-FOUND-3 Phase 0 (2026-05-03) — extracted from `run()` so that:
/// 1. The two branches (workspace run / `--retry-failed` replay) share
///    a single arg-construction code path. Previously both branches
///    duplicated the prefix and the `--retry-failed` branch silently
///    dropped `--include-ignored` when `--slow` was set (the libtest-
///    args section was hardcoded to `-- --exact <names>` with no slot
///    for other libtest flags). This function fixes that bug.
/// 2. Future flags (V5-FOUND-3 Phase 3 `--instrument-coverage`, Phase 4
///    `--select-by-coverage`) extend a single function rather than
///    re-duplicating prefix construction.
///
/// `retry_names` carries the failure-ledger names when invoked via
/// `--retry-failed`; `None` means a normal workspace run.
pub fn build_cargo_args(args: &Args, retry_names: Option<&[String]>) -> Vec<String> {
    let mut v: Vec<String> = vec![
        "nextest".into(),
        "run".into(),
        "--workspace".into(),
        "--all-targets".into(),
        "--profile".into(),
        args.profile.clone(),
        "--color".into(),
        "never".into(),
    ];

    // Libtest-compat arg section (after `--`). Currently used by:
    //   - `--slow`         → `--include-ignored`
    //   - `--retry-failed` → `--exact <name1> <name2> ...`
    //
    // Both can apply simultaneously (slow retry of a prior `--slow`
    // run); the `--` separator goes once, with both flag sets inside.
    let needs_libtest_section = args.slow || retry_names.is_some();
    if needs_libtest_section {
        v.push("--".into());
        if args.slow {
            v.push("--include-ignored".into());
        }
        if let Some(names) = retry_names {
            v.push("--exact".into());
            for name in names {
                v.push(name.clone());
            }
        }
    }
    v
}

/// Stripped-down summary of one cargo binary's parsed output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFailure {
    pub test_name: String,
    pub binary: String,
    pub output_block: String,
}

/// Parser result.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ParsedCargoOutput {
    pub total_tests: u32,
    pub total_failed: u32,
    pub total_passed: u32,
    pub total_ignored: u32,
    pub failures: Vec<ParsedFailure>,
    /// Whether ANY binary reported a non-zero failure count. We
    /// can't fully trust cargo's exit code (some harnesses panic
    /// out without writing test result lines); this is a parser-
    /// derived second opinion.
    pub any_failure_observed: bool,
}

/// Parse cargo's combined stdout+stderr into the tests + failures
/// structure. Handles multiple test binaries (one per crate's lib +
/// per-integration-test) by pre-extracting `Running … <binary>` lines
/// and advancing the active binary on each libtest `running N tests`
/// marker.
///
/// Why pre-extract: cargo writes `Running ...` to stderr while test
/// output (`running N tests`, failure detail, `test result:`) goes to
/// stdout. When the wrapper captures the two streams via
/// `Command::output()`, they arrive as separate buffers; the wrapper
/// concatenates stdout+stderr so all `Running ...` lines arrive AFTER
/// all test output. We can't rely on line-order interleaving, so we
/// take an ordered snapshot of the binary names from the `Running ...`
/// lines and advance through it as we encounter the `running N tests`
/// stdout markers (which DO interleave with the rest of stdout).
pub fn parse_cargo_output(text: &str) -> ParsedCargoOutput {
    let mut out = ParsedCargoOutput::default();

    // Pre-pass: ordered list of binary IDs from `Running …` lines.
    let binaries: Vec<String> = text
        .lines()
        .filter_map(|raw| {
            let line = strip_ansi(raw);
            let trimmed = line.trim();
            trimmed
                .strip_prefix("Running ")
                .and_then(|rest| extract_binary_id(rest))
        })
        .collect();

    let mut current_binary = String::from("(unknown)");
    let mut binary_idx: usize = 0;
    let mut in_failures_block = false;
    let mut failure_blocks: Vec<(String, String)> = Vec::new(); // (test_name, output)
    let mut current_failure_name: Option<String> = None;
    let mut current_failure_buffer: String = String::new();
    let mut named_failures: Vec<(String, String)> = Vec::new(); // (test_name, binary)

    for raw_line in text.lines() {
        let line = strip_ansi(raw_line);
        let trimmed = line.trim();

        // libtest emits `running N tests` (or `running 1 test`) at the
        // start of each binary's test block — this is the reliable
        // delimiter for "we are now in binary K". Advance through the
        // pre-extracted binary list on each occurrence.
        if is_running_n_tests_line(trimmed) {
            if let Some(name) = binaries.get(binary_idx) {
                current_binary = name.clone();
            }
            binary_idx += 1;
            in_failures_block = false;
            current_failure_name = None;
            current_failure_buffer.clear();
            continue;
        }

        // Skip the stderr `Running …` lines themselves — they were
        // consumed in the pre-pass and would otherwise reset state at
        // the wrong moment when stdout/stderr are concatenated.
        if trimmed.starts_with("Running ") {
            continue;
        }

        // Per-binary failure-detail block opener:
        //   ---- <test_name> stdout ----
        if let Some(name) = parse_failure_detail_header(&line) {
            // Flush any prior buffered detail block.
            if let Some(prev) = current_failure_name.take() {
                failure_blocks.push((prev, std::mem::take(&mut current_failure_buffer)));
            }
            current_failure_name = Some(name);
            current_failure_buffer.clear();
            continue;
        }

        // While inside a failure-detail block, buffer the lines until
        // the next sentinel (another `----` header, or the
        // `failures:` summary block).
        if let Some(_) = &current_failure_name {
            if trimmed == "failures:" || trimmed.starts_with("test result:") {
                if let Some(prev) = current_failure_name.take() {
                    failure_blocks.push((prev, std::mem::take(&mut current_failure_buffer)));
                }
                // Don't `continue` — fall through so the line is also
                // matched as a summary marker below.
            } else {
                current_failure_buffer.push_str(&line);
                current_failure_buffer.push('\n');
                continue;
            }
        }

        // The libtest "failures:" block lists the test names that
        // failed in this binary (one per indented line, no `test ` prefix).
        if trimmed == "failures:" {
            in_failures_block = true;
            continue;
        }
        if in_failures_block {
            // Block ends when we hit `test result:`.
            if trimmed.starts_with("test result:") {
                in_failures_block = false;
                // Fall through to result parsing below.
            } else if !trimmed.is_empty() {
                named_failures.push((trimmed.to_string(), current_binary.clone()));
                continue;
            } else {
                continue;
            }
        }

        // Per-binary result summary line.
        if let Some(rest) = trimmed.strip_prefix("test result: ") {
            update_totals_from_summary(&mut out, rest);
        }
    }

    // Flush any trailing failure-detail buffer.
    if let Some(prev) = current_failure_name.take() {
        failure_blocks.push((prev, current_failure_buffer));
    }

    // Stitch named_failures (binary attribution) with failure_blocks
    // (panic detail). Match by test_name; default the output to a
    // placeholder when no detail block was emitted.
    let detail_map: std::collections::HashMap<String, String> =
        failure_blocks.into_iter().collect();
    for (test_name, binary) in named_failures {
        let output_block = detail_map
            .get(&test_name)
            .cloned()
            .unwrap_or_else(|| "(no panic detail captured)".to_string());
        out.failures.push(ParsedFailure {
            test_name,
            binary,
            output_block,
        });
    }

    out.any_failure_observed = !out.failures.is_empty() || out.total_failed > 0;
    out
}

/// V5-FOUND-2 Phase 1 (2026-05-03) — parse `cargo nextest run` stdout.
///
/// Nextest's output format differs from libtest's in three ways:
/// 1. **Per-test status lines** are unified — one line per test with
///    `PASS|FAIL|FLAKY|SKIP|TIMEOUT|LEAK|SLOW` prefix, duration,
///    `(current/total)` counter, `<binary>` (crate::test_target form),
///    and the test function name. No multi-binary "running N tests"
///    delimiters — nextest interleaves output across binaries.
/// 2. **Failure detail blocks** are framed as `--- STDERR: <binary>
///    <test> ---` followed by panic output, then `--- STDOUT: <binary>
///    <test> ---` and stdout output. The block ends at the next status
///    line, the next `---` framing line, or the `Summary` line.
/// 3. **The summary line** uses `Summary [<dur>] N tests run: P
///    passed[, F failed][, S skipped]` rather than libtest's per-binary
///    `test result: ok. ...`.
///
/// **`FLAKY` semantic** (nextest 0.9.131+): a test that failed on
/// attempt 1 but passed on retry. With `flaky-result = "fail"` in the
/// CI profile, FLAKY counts as a failure for the run's exit code, but
/// nextest still emits a separate `FLAKY` status line. We treat FLAKY
/// as a failure for the ledger so operators can see retry-rescued
/// tests in the failure history.
///
/// Returns the same `ParsedCargoOutput` shape as `parse_cargo_output`
/// so the rest of the wrapper (ledger writer, summary printer) is
/// runner-agnostic.
pub fn parse_nextest_output(text: &str) -> ParsedCargoOutput {
    let mut out = ParsedCargoOutput::default();

    // Real nextest output for a failing test:
    //
    //         FAIL [   0.018s] (1/1) <binary> <test_name>
    //   stdout ───
    //
    //       running 1 test
    //       test <test_name> ... FAILED
    //       ...
    //
    //   stderr ───
    //
    //       thread '<test_name>' panicked at <file>:<line>:<col>:
    //       <msg>
    //       ...
    //
    // ────────────
    //      Summary [   0.021s] 1 test run: 0 passed, 1 failed
    //         FAIL [   0.018s] (1/1) <binary> <test_name>      ← echoed after summary
    //
    // The `────────────` divider closes a detail block; the `FAIL`
    // line is emitted twice (once during the run, once echoed after
    // the summary), so we dedup failures by (test_name, binary).
    let mut named_failures: Vec<(String, String)> = Vec::new();
    let mut seen_failure_keys: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut detail_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut current_failure_test: Option<String> = None;
    let mut current_buffer: String = String::new();

    let flush_buffer = |test: &mut Option<String>,
                        buf: &mut String,
                        map: &mut std::collections::HashMap<String, String>| {
        if let Some(t) = test.take() {
            // Only insert if the buffer has real content; FLAKY/TIMEOUT/
            // LEAK can fail without a stdout/stderr block, in which case
            // the placeholder is used at stitch time.
            if !buf.trim().is_empty() {
                map.entry(t).or_insert_with(|| std::mem::take(buf));
            }
            buf.clear();
        }
    };

    for raw_line in text.lines() {
        let line = strip_ansi(raw_line);
        let trimmed = line.trim();

        // `────────────` divider (4+ U+2500 box-drawing chars, nothing
        // else). Closes any in-progress detail buffer. Also appears
        // BEFORE the first test line (section divider before the
        // run header) — harmless since no buffer is open yet.
        if !trimmed.is_empty() && trimmed.chars().all(|c| c == '─') {
            flush_buffer(&mut current_failure_test, &mut current_buffer, &mut detail_map);
            continue;
        }

        // Per-test status line.
        if let Some((status, binary, test_name)) = parse_nextest_status_line(&line) {
            // A new status line ends any prior detail buffer.
            flush_buffer(&mut current_failure_test, &mut current_buffer, &mut detail_map);

            match status.as_str() {
                "PASS" => out.total_passed += 1,
                "FAIL" | "TIMEOUT" | "LEAK" | "FLAKY" => {
                    let key = (test_name.clone(), binary.clone());
                    if seen_failure_keys.insert(key) {
                        out.total_failed += 1;
                        named_failures.push((test_name.clone(), binary.clone()));
                    }
                    // The detail block (if any) follows this line and
                    // is closed by the next `────────────` divider.
                    current_failure_test = Some(test_name.clone());
                }
                "SKIP" => out.total_ignored += 1,
                "SLOW" => {
                    // Informational only; the final PASS/FAIL line for
                    // the same test arrives later.
                }
                _ => {}
            }
            continue;
        }

        // Summary line: `     Summary [   1.5s] N tests run: P passed[, F failed][, S skipped]`
        if let Some(rest) = trimmed.strip_prefix("Summary [") {
            flush_buffer(&mut current_failure_test, &mut current_buffer, &mut detail_map);
            update_totals_from_nextest_summary(&mut out, rest);
            continue;
        }

        // Inside a detail block: append the raw line (preserve formatting
        // since panic backtraces have their own indentation, and the
        // `stdout ───` / `stderr ───` sub-headers are part of the block).
        if current_failure_test.is_some() {
            current_buffer.push_str(raw_line);
            current_buffer.push('\n');
        }
    }

    // Final flush.
    flush_buffer(&mut current_failure_test, &mut current_buffer, &mut detail_map);

    // Stitch named_failures with detail_map.
    for (test_name, binary) in named_failures {
        let output_block = detail_map
            .get(&test_name)
            .cloned()
            .map(|s| s.trim_end_matches('\n').to_string())
            .unwrap_or_else(|| "(no panic detail captured)".to_string());
        out.failures.push(ParsedFailure {
            test_name,
            binary,
            output_block,
        });
    }

    out.any_failure_observed = !out.failures.is_empty() || out.total_failed > 0;
    out
}

/// Parse a nextest per-test status line.
/// Format: `        STATUS [   0.029s] (1/7) <binary> <test_name>`
/// where leading whitespace varies and STATUS is one of
/// PASS/FAIL/FLAKY/SKIP/TIMEOUT/LEAK/SLOW.
fn parse_nextest_status_line(line: &str) -> Option<(String, String, String)> {
    let trimmed = line.trim_start();
    // Status word
    let (status, rest) = trimmed.split_once(' ')?;
    let status = status.trim();
    if !matches!(
        status,
        "PASS" | "FAIL" | "FLAKY" | "SKIP" | "TIMEOUT" | "LEAK" | "SLOW"
    ) {
        return None;
    }
    // Skip the `[   N.Ns]` duration block
    let rest = rest.trim_start();
    if !rest.starts_with('[') {
        return None;
    }
    let rest = rest.split_once(']')?.1.trim_start();
    // Skip the `(current/total)` counter
    if !rest.starts_with('(') {
        return None;
    }
    let rest = rest.split_once(')')?.1.trim_start();
    // Now `<binary> <test_name>` remain
    let (binary, test_name) = rest.split_once(' ')?;
    Some((
        status.to_string(),
        binary.trim().to_string(),
        test_name.trim().to_string(),
    ))
}

/// Parse `<binary> <test_name>` from a `--- STDERR:` / `--- STDOUT:`
/// header. The block format is:
///   `--- STDERR:              <binary> <test_name> ---`
/// where the trailing `---` was already trimmed by the caller. The
/// inner content has variable whitespace between binary and test name.
fn parse_failure_block_header(inner: &str) -> Option<(String, String)> {
    // Whitespace-collapse: split into 2 tokens
    let mut parts = inner.split_whitespace();
    let binary = parts.next()?.to_string();
    let test_name = parts.next()?.to_string();
    Some((binary, test_name))
}

/// Update parser totals from a nextest summary line.
/// Format (after the `Summary [` prefix has been stripped):
///   `   1.5s] 12 tests run: 11 passed, 1 failed, 0 skipped`
/// Both `failed` and `skipped` segments are optional in nextest output.
fn update_totals_from_nextest_summary(out: &mut ParsedCargoOutput, rest: &str) {
    // Skip the duration prefix `   N.Ns]`
    let rest = match rest.split_once(']') {
        Some((_, after)) => after.trim_start(),
        None => return,
    };
    // `12 tests run: 11 passed[, 1 failed][, 0 skipped]`
    let (total_str, after_total) = match rest.split_once(" tests run:") {
        Some((t, after)) => (t.trim(), after.trim_start()),
        None => return,
    };
    if let Ok(total) = total_str.parse::<u32>() {
        out.total_tests = total;
    }
    // Each segment ends with a verb: passed/failed/skipped. The summary
    // is comma-separated. Parse each segment.
    for segment in after_total.split(',') {
        let seg = segment.trim();
        if let Some(n_str) = seg.strip_suffix(" passed") {
            if let Ok(n) = n_str.trim().parse::<u32>() {
                out.total_passed = n;
            }
        } else if let Some(n_str) = seg.strip_suffix(" failed") {
            if let Ok(n) = n_str.trim().parse::<u32>() {
                // Trust the summary's total_failed over our line-by-line
                // count; the per-line count includes FLAKY rescues.
                if n > 0 {
                    out.total_failed = n;
                }
            }
        } else if let Some(n_str) = seg.strip_suffix(" skipped") {
            if let Ok(n) = n_str.trim().parse::<u32>() {
                out.total_ignored = n;
            }
        }
    }
}

/// Extract a stable binary identifier from `Running ...` lines like:
///   "unittests src/lib.rs (target/debug/deps/neurogrim_core-abc123.exe)"
/// Returns the basename without hash + extension when possible.
fn extract_binary_id(rest: &str) -> Option<String> {
    let path = rest.rsplit_once('(')?.1.trim_end_matches(')').trim();
    let basename = path.rsplit_once(['/', '\\']).map(|(_, b)| b).unwrap_or(path);
    let without_ext = basename.trim_end_matches(".exe");
    // Strip the trailing `-<hash>` segment (16 hex chars).
    Some(
        without_ext
            .rsplit_once('-')
            .map(|(stem, _)| stem.to_string())
            .unwrap_or_else(|| without_ext.to_string()),
    )
}

/// True iff `s` is a libtest binary-block opener: `running N tests`
/// or (when N == 1) `running 1 test`. Used as the delimiter for
/// "binary K's test block starts now."
fn is_running_n_tests_line(s: &str) -> bool {
    if let Some(rest) = s.strip_prefix("running ") {
        let mut parts = rest.split_whitespace();
        let n_ok = parts.next().and_then(|n| n.parse::<u32>().ok()).is_some();
        let suffix = parts.next();
        n_ok && (suffix == Some("test") || suffix == Some("tests"))
    } else {
        false
    }
}

/// `---- foo::bar stdout ----` → `Some("foo::bar")`.
/// Also handles `---- foo::bar stderr ----`.
fn parse_failure_detail_header(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("---- ") {
        return None;
    }
    // Strip leading `---- ` and trailing ` stdout ----` or ` stderr ----`.
    let inner = trimmed.trim_start_matches("---- ");
    let suffix_start = inner.rfind(" stdout ----").or_else(|| inner.rfind(" stderr ----"))?;
    Some(inner[..suffix_start].to_string())
}

/// "ok. 5 passed; 1 failed; 0 ignored; 0 measured; ..."
fn update_totals_from_summary(out: &mut ParsedCargoOutput, rest: &str) {
    // Format examples:
    //   "ok. 5 passed; 0 failed; 0 ignored; 0 measured; ..."
    //   "FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; ..."
    //   "ok. 0 passed; 0 failed; 1 ignored; 0 measured; ..."
    let is_failed = rest.starts_with("FAILED");
    if is_failed {
        out.any_failure_observed = true;
    }
    // Pull number-token pairs.
    let parts: Vec<&str> = rest.split(';').collect();
    for part in parts {
        let p = part.trim().trim_start_matches(|c: char| !c.is_ascii_digit());
        let mut split = p.split_whitespace();
        let n_str = split.next().unwrap_or("0");
        let label = split.next().unwrap_or("");
        let n: u32 = n_str.parse().unwrap_or(0);
        match label {
            "passed" => {
                out.total_passed += n;
                out.total_tests += n;
            }
            "failed" => {
                out.total_failed += n;
                out.total_tests += n;
            }
            "ignored" => {
                out.total_ignored += n;
                out.total_tests += n;
            }
            _ => {}
        }
    }
}

/// Strip ANSI escape sequences from a line.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC [ ... letter ; or ESC ] ... BEL.
            if let Some(&next) = chars.peek() {
                chars.next();
                if next == '[' {
                    while let Some(&n) = chars.peek() {
                        chars.next();
                        if n.is_ascii_alphabetic() {
                            break;
                        }
                    }
                } else if next == ']' {
                    while let Some(&n) = chars.peek() {
                        chars.next();
                        if n == '\x07' {
                            break;
                        }
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Print a tight summary to stderr.
fn print_summary(parsed: &ParsedCargoOutput, to_print: &[&FailureEntry], show_only_new: bool) {
    eprintln!();
    eprintln!(
        "✦ {} test(s) total · {} passed · {} failed · {} ignored",
        parsed.total_tests, parsed.total_passed, parsed.total_failed, parsed.total_ignored
    );

    if to_print.is_empty() && !parsed.failures.is_empty() && show_only_new {
        eprintln!("  No NEW failures (all already in the previous run's batch).");
        return;
    }
    if to_print.is_empty() {
        eprintln!("  All tests passed ✓");
        return;
    }

    eprintln!();
    eprintln!("  Failures{}:", if show_only_new { " (new only)" } else { "" });
    for entry in to_print {
        eprintln!("    ✗ {} ({})", entry.test_name, entry.binary);
        // Print the detail block, indented for visual grouping.
        for line in entry.output.lines().take(20) {
            eprintln!("      {line}");
        }
        if entry.output.lines().count() > 20 {
            eprintln!("      … (truncated; full detail in {LEDGER_FILENAME})");
        }
    }
}

/// Append a batch of failure entries to the ledger.
pub fn append_failures(path: &Path, entries: &[FailureEntry]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    for entry in entries {
        let line = serde_json::to_string(entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writeln!(f, "{line}")?;
    }
    f.sync_all()?;
    Ok(())
}

/// Read the failure entries belonging to the most recent `run_id`.
pub fn read_most_recent_run_failures(path: &Path) -> Vec<FailureEntry> {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    // Iterate from the end backwards: collect entries whose run_id
    // matches the last entry's. Stop once a different run_id appears.
    let mut entries: Vec<FailureEntry> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<FailureEntry>(l).ok())
        .collect();
    let last_run_id = match entries.last().map(|e| e.run_id.clone()) {
        Some(id) => id,
        None => return Vec::new(),
    };
    entries.retain(|e| e.run_id == last_run_id);
    entries
}

/// Return the deduplicated test names from the most recent run's
/// failure batch. Used by `--retry-failed` and `--show-only-new`.
pub fn read_most_recent_run_failure_names(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = read_most_recent_run_failures(path)
        .into_iter()
        .map(|e| e.test_name)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// If the ledger has more than `keep_last` entries, move the oldest
/// excess to the archive file.
fn rotate_ledger_if_needed(
    ledger: &Path,
    archive: &Path,
    keep_last: u32,
) -> std::io::Result<()> {
    let raw = match fs::read_to_string(ledger) {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
    if (lines.len() as u32) <= keep_last {
        return Ok(());
    }
    let split_at = lines.len() - keep_last as usize;
    let (to_archive, to_keep) = lines.split_at(split_at);
    if !to_archive.is_empty() {
        if let Some(parent) = archive.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = OpenOptions::new().create(true).append(true).open(archive)?;
        for l in to_archive {
            writeln!(f, "{l}")?;
        }
        f.sync_all()?;
    }
    fs::write(ledger, to_keep.join("\n") + "\n")?;
    Ok(())
}

// ── S12-G-5: Playwright E2E launcher ────────────────────────────────

/// Locate the dashboard frontend directory containing
/// `playwright.config.ts`. Walks upward from `start` for a few levels,
/// trying both layouts on the way up:
///   - `<p>/crates/neurogrim-dashboard/frontend` (Cargo workspace root)
///   - `<p>/neurogrim/crates/neurogrim-dashboard/frontend` (repo root,
///     where the Brain's `.claude/` lives one level above the workspace)
///
/// Supports invocation from either the workspace root (`/d/Brains/NeuroGrim/neurogrim/`)
/// or the repo root (`/d/Brains/NeuroGrim/`) without per-call config.
pub fn find_dashboard_frontend(start: &Path) -> Result<PathBuf> {
    let canonical = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let mut cur: Option<&Path> = Some(&canonical);
    for _ in 0..6 {
        let p = match cur {
            Some(p) => p,
            None => break,
        };
        for prefix in [None, Some("neurogrim")] {
            let mut candidate = p.to_path_buf();
            if let Some(pref) = prefix {
                candidate.push(pref);
            }
            candidate.push("crates");
            candidate.push("neurogrim-dashboard");
            candidate.push("frontend");
            if candidate.join("playwright.config.ts").is_file() {
                return Ok(candidate);
            }
        }
        cur = p.parent();
    }
    anyhow::bail!(
        "no `crates/neurogrim-dashboard/frontend/playwright.config.ts` found by \
         walking up from {} (tried both `<p>/crates/...` and \
         `<p>/neurogrim/crates/...` at each level)",
        start.display()
    )
}

/// Run `npx playwright test` in `frontend_dir`, inheriting stdio so
/// the operator sees real-time test progress. Returns the child's
/// exit status (caller decides whether to mirror it via
/// `std::process::exit`).
///
/// Used by `neurogrim test --e2e` and (with capture instead of
/// inherit) by `neurogrim publish-gate run` for `e2e` gate types.
pub fn spawn_playwright_inherit(frontend_dir: &Path) -> Result<std::process::ExitStatus> {
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "windows") {
        ("npx.cmd", vec!["playwright", "test"])
    } else {
        ("npx", vec!["playwright", "test"])
    };
    let status = Command::new(program)
        .args(&args)
        .current_dir(frontend_dir)
        .status()
        .with_context(|| format!("failed to spawn `{program} playwright test`"))?;
    Ok(status)
}

/// Best-effort `git rev-parse --short HEAD`. Returns "unknown" when
/// not in a git repo or git is missing.
fn current_git_rev(project_root: &Path) -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(project_root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Sample cargo output: one passing test in lib, one failing test
    /// in an integration binary.
    const SAMPLE_OK_AND_FAIL: &str = "
   Compiling neurogrim-cli v3.5.0 (D:/Brains/NeuroGrim/neurogrim/crates/neurogrim-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.23s
     Running unittests src/lib.rs (target/debug/deps/example_lib-abcdef1234567890.exe)

running 1 test
test tests::it_works ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running tests/cli.rs (target/debug/deps/cli-fedcba9876543210.exe)

running 2 tests
test smoke ... ok
test broken ... FAILED

failures:

---- broken stdout ----
thread 'broken' panicked at tests/cli.rs:13:5:
assertion `left == right` failed
  left: 1
 right: 2
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    broken

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
";

    const SAMPLE_ALL_OK: &str = "
     Running unittests src/lib.rs (target/debug/deps/foo-aaa.exe)

running 3 tests
test a ... ok
test b ... ok
test c ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
";

    #[test]
    fn parse_cargo_output_no_failures() {
        let out = parse_cargo_output(SAMPLE_ALL_OK);
        assert_eq!(out.total_passed, 3);
        assert_eq!(out.total_failed, 0);
        assert_eq!(out.failures.len(), 0);
        assert!(!out.any_failure_observed);
    }

    #[test]
    fn parse_cargo_output_one_failure_one_binary() {
        let out = parse_cargo_output(SAMPLE_OK_AND_FAIL);
        assert_eq!(out.total_passed, 2);
        assert_eq!(out.total_failed, 1);
        assert_eq!(out.failures.len(), 1);
        assert_eq!(out.failures[0].test_name, "broken");
        assert!(out.failures[0].binary.contains("cli"));
        assert!(out.failures[0].output_block.contains("assertion `left == right` failed"));
        assert!(out.any_failure_observed);
    }

    /// Real-cargo case: `Command::output()` returns stdout and stderr
    /// as separate buffers; concatenating them places ALL stderr
    /// `Running ...` lines after the entire stdout test output.
    /// Binary attribution must still be correct.
    #[test]
    fn parse_cargo_output_handles_stderr_appended_after_stdout() {
        let stdout = "\
running 1 test
test tests::it_works ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 2 tests
test smoke ... ok
test broken ... FAILED

failures:

---- broken stdout ----
thread 'broken' panicked at tests/cli.rs:13:5:
boom


failures:
    broken

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
";
        let stderr = "\
   Compiling neurogrim-cli v3.5.0 (.)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.23s
     Running unittests src/lib.rs (target/debug/deps/example_lib-abcdef1234567890.exe)
     Running tests/cli.rs (target/debug/deps/cli-fedcba9876543210.exe)
";
        let combined = format!("{stdout}\n{stderr}");
        let out = parse_cargo_output(&combined);
        assert_eq!(out.total_passed, 2);
        assert_eq!(out.total_failed, 1);
        assert_eq!(out.failures.len(), 1);
        assert_eq!(out.failures[0].test_name, "broken");
        assert_eq!(
            out.failures[0].binary, "cli",
            "binary should be the SECOND pre-extracted Running line, not '(unknown)' \
             or the first binary"
        );
    }

    #[test]
    fn parse_cargo_output_strips_ansi_codes() {
        let with_ansi = "\u{1b}[32mtest \u{1b}[0mfoo \u{1b}[31m... FAILED\u{1b}[0m\n\n\
                         failures:\n\n\
                         ---- foo stdout ----\n\
                         panic\n\n\
                         failures:\n    foo\n\n\
                         test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";
        let out = parse_cargo_output(with_ansi);
        assert_eq!(out.total_failed, 1);
        assert_eq!(out.failures[0].test_name, "foo");
    }

    #[test]
    fn append_failures_round_trip_and_read_recent() {
        let tmp = TempDir::new().unwrap();
        let ledger = tmp.path().join(".claude/brain/test-failures.jsonl");

        let entries = vec![
            FailureEntry {
                schema_version: "1".into(),
                run_id: "run-1".into(),
                ts: "2026-04-29T12:00:00Z".into(),
                test_name: "tests::a".into(),
                binary: "neurogrim_core".into(),
                outcome: "failed".into(),
                output: "boom".into(),
                commit: "abc123".into(),
            },
            FailureEntry {
                schema_version: "1".into(),
                run_id: "run-1".into(),
                ts: "2026-04-29T12:00:00Z".into(),
                test_name: "tests::b".into(),
                binary: "neurogrim_core".into(),
                outcome: "failed".into(),
                output: "kaboom".into(),
                commit: "abc123".into(),
            },
        ];
        append_failures(&ledger, &entries).unwrap();

        let recent = read_most_recent_run_failures(&ledger);
        assert_eq!(recent.len(), 2);
        let names = read_most_recent_run_failure_names(&ledger);
        assert_eq!(names, vec!["tests::a".to_string(), "tests::b".to_string()]);
    }

    #[test]
    fn read_most_recent_run_returns_only_last_batch() {
        let tmp = TempDir::new().unwrap();
        let ledger = tmp.path().join(".claude/brain/test-failures.jsonl");

        // Run 1 — 2 failures.
        let r1 = vec![
            FailureEntry {
                schema_version: "1".into(),
                run_id: "run-1".into(),
                ts: "2026-04-29T12:00:00Z".into(),
                test_name: "tests::a".into(),
                binary: "x".into(),
                outcome: "failed".into(),
                output: "".into(),
                commit: "abc".into(),
            },
            FailureEntry {
                schema_version: "1".into(),
                run_id: "run-1".into(),
                ts: "2026-04-29T12:00:00Z".into(),
                test_name: "tests::b".into(),
                binary: "x".into(),
                outcome: "failed".into(),
                output: "".into(),
                commit: "abc".into(),
            },
        ];
        append_failures(&ledger, &r1).unwrap();

        // Run 2 — 1 failure with a different run_id.
        let r2 = vec![FailureEntry {
            schema_version: "1".into(),
            run_id: "run-2".into(),
            ts: "2026-04-29T13:00:00Z".into(),
            test_name: "tests::c".into(),
            binary: "x".into(),
            outcome: "failed".into(),
            output: "".into(),
            commit: "def".into(),
        }];
        append_failures(&ledger, &r2).unwrap();

        let recent = read_most_recent_run_failures(&ledger);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].test_name, "tests::c");
    }

    #[test]
    fn rotate_ledger_moves_oldest_excess_to_archive() {
        let tmp = TempDir::new().unwrap();
        let ledger = tmp.path().join(".claude/brain/test-failures.jsonl");
        let archive = tmp.path().join(".claude/brain/test-failures.archive.jsonl");

        // Append 5 entries.
        let entries: Vec<FailureEntry> = (0..5)
            .map(|i| FailureEntry {
                schema_version: "1".into(),
                run_id: format!("run-{i}"),
                ts: "2026-04-29T12:00:00Z".into(),
                test_name: format!("tests::t{i}"),
                binary: "x".into(),
                outcome: "failed".into(),
                output: "".into(),
                commit: "abc".into(),
            })
            .collect();
        append_failures(&ledger, &entries).unwrap();

        // Keep only the last 2; oldest 3 should go to archive.
        rotate_ledger_if_needed(&ledger, &archive, 2).unwrap();

        let kept = fs::read_to_string(&ledger).unwrap();
        let archived = fs::read_to_string(&archive).unwrap();
        let kept_lines: Vec<&str> = kept.lines().filter(|l| !l.trim().is_empty()).collect();
        let archived_lines: Vec<&str> = archived.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(kept_lines.len(), 2);
        assert_eq!(archived_lines.len(), 3);
        // Archive holds t0/t1/t2; ledger holds t3/t4.
        assert!(kept_lines[0].contains("tests::t3"));
        assert!(archived_lines[0].contains("tests::t0"));
    }

    #[test]
    fn rotate_ledger_no_op_when_under_threshold() {
        let tmp = TempDir::new().unwrap();
        let ledger = tmp.path().join(".claude/brain/test-failures.jsonl");
        let archive = tmp.path().join(".claude/brain/test-failures.archive.jsonl");

        let entries: Vec<FailureEntry> = (0..3)
            .map(|i| FailureEntry {
                schema_version: "1".into(),
                run_id: format!("run-{i}"),
                ts: "2026-04-29T12:00:00Z".into(),
                test_name: format!("tests::t{i}"),
                binary: "x".into(),
                outcome: "failed".into(),
                output: "".into(),
                commit: "abc".into(),
            })
            .collect();
        append_failures(&ledger, &entries).unwrap();
        rotate_ledger_if_needed(&ledger, &archive, 500).unwrap();

        let kept_lines: Vec<String> = fs::read_to_string(&ledger)
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(String::from)
            .collect();
        assert_eq!(kept_lines.len(), 3);
        assert!(!archive.exists());
    }

    #[test]
    fn parse_failure_detail_header_handles_stdout_and_stderr() {
        assert_eq!(
            parse_failure_detail_header("---- foo::bar stdout ----"),
            Some("foo::bar".to_string())
        );
        assert_eq!(
            parse_failure_detail_header("---- foo::bar stderr ----"),
            Some("foo::bar".to_string())
        );
        assert_eq!(parse_failure_detail_header("test result: ok"), None);
        assert_eq!(parse_failure_detail_header("---- not a header"), None);
    }

    #[test]
    fn extract_binary_id_strips_hash_and_extension() {
        assert_eq!(
            extract_binary_id(
                "unittests src/lib.rs (target/debug/deps/neurogrim_core-abc123def456.exe)"
            ),
            Some("neurogrim_core".to_string())
        );
        assert_eq!(
            extract_binary_id("tests/cli.rs (target/debug/deps/cli-1234567890abcdef)"),
            Some("cli".to_string())
        );
    }

    // ── V5-FOUND-2 Phase 1 (2026-05-03) — nextest parser tests ──────────

    /// Sample nextest stdout for a clean run (all tests pass).
    /// Captured from `cargo nextest run -p neurogrim-sdk` 2026-05-03.
    const NEXTEST_SAMPLE_ALL_OK: &str = "
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.39s
────────────
 Nextest run ID 32a284ed-98c9-45b7-9737-8f71fa41910c with nextest profile: default
    Starting 7 tests across 3 binaries
        PASS [   0.029s] (1/7) neurogrim-sdk::compile_test_re_exports theme_b_traits_are_object_safe_via_sdk
        PASS [   0.041s] (2/7) neurogrim-sdk::compile_test_re_exports queue_built_in_factories_reachable
        PASS [   0.052s] (3/7) neurogrim-sdk::compile_test_re_exports registries_constructible_via_sdk
        PASS [   0.068s] (4/7) neurogrim-sdk::sdk_surface_assertion sdk_surface_signatures_unchanged
        PASS [   0.095s] (5/7) neurogrim-sdk::compile_test_re_exports core_types_reachable_via_sdk
        PASS [   0.109s] (6/7) neurogrim-sdk::compile_test_re_exports conformance_types_unified_across_suites
        PASS [   0.129s] (7/7) neurogrim-sdk::compile_test_re_exports adjacent_stable_traits_reachable
────────────
     Summary [   0.135s] 7 tests run: 7 passed, 0 skipped
";

    /// Sample nextest stdout with one failing test + one panic detail block.
    /// Format mirrors real nextest 0.9.133 output (captured 2026-05-03):
    /// detail block follows the FAIL line, terminated by `────────────`,
    /// and the FAIL line is echoed once after the Summary (must dedup).
    const NEXTEST_SAMPLE_ONE_FAILURE: &str = "
────────────
 Nextest run ID test-abc with nextest profile: default
    Starting 3 tests across 1 binaries
        PASS [   0.012s] (1/3) my_crate::tests passing_test
        FAIL [   0.025s] (2/3) my_crate::tests broken_test
  stdout ───

    running 1 test
    test broken_test ... FAILED

  stderr ───

    thread 'broken_test' panicked at tests/foo.rs:13:5:
    assertion `left == right` failed
      left: 1
     right: 2
    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

────────────
        PASS [   0.018s] (3/3) my_crate::tests another_passing
────────────
     Summary [   0.060s] 3 tests run: 2 passed, 1 failed, 0 skipped
        FAIL [   0.025s] (2/3) my_crate::tests broken_test
";

    /// Sample with FLAKY (test failed on attempt 1, passed on retry).
    /// With `flaky-result = "fail"` in the ci profile, this counts as
    /// a run failure for the ledger.
    const NEXTEST_SAMPLE_FLAKY: &str = "
────────────
 Nextest run ID test-flake with nextest profile: ci
    Starting 2 tests across 1 binaries
        PASS [   0.008s] (1/2) my_crate::tests stable_test
        FLAKY [   0.150s] (2/2) my_crate::tests flaky_test
────────────
     Summary [   0.165s] 2 tests run: 1 passed, 1 failed, 0 skipped
";

    /// Sample with SKIP (test was filtered out / `#[ignore]`d).
    const NEXTEST_SAMPLE_WITH_SKIP: &str = "
────────────
 Nextest run ID test-skip with nextest profile: default
    Starting 3 tests across 1 binaries
        PASS [   0.005s] (1/3) my_crate::tests fast_test
        SKIP [   0.000s] (2/3) my_crate::tests ignored_test
        PASS [   0.012s] (3/3) my_crate::tests another_test
────────────
     Summary [   0.020s] 3 tests run: 2 passed, 1 skipped
";

    /// Sample with TIMEOUT (test exceeded slow-timeout's terminate-after).
    const NEXTEST_SAMPLE_TIMEOUT: &str = "
────────────
 Nextest run ID test-timeout with nextest profile: default
    Starting 2 tests across 1 binaries
        PASS [   0.005s] (1/2) my_crate::tests fast_test
        TIMEOUT [ 120.001s] (2/2) my_crate::tests hanging_test
────────────
     Summary [ 120.010s] 2 tests run: 1 passed, 1 failed, 0 skipped
";

    /// Sample with ANSI color codes (nextest emits color by default; we
    /// pass --color never to strip, but this test verifies graceful
    /// handling if color sneaks through).
    const NEXTEST_SAMPLE_WITH_ANSI: &str = "
────────────
 \x1b[1mNextest run ID test-ansi\x1b[0m with nextest profile: default
    Starting 1 tests across 1 binaries
        \x1b[32mPASS\x1b[0m [   0.005s] (1/1) my_crate::tests colored_test
────────────
     Summary [   0.010s] 1 tests run: 1 passed, 0 skipped
";

    #[test]
    fn parse_nextest_no_failures() {
        let out = parse_nextest_output(NEXTEST_SAMPLE_ALL_OK);
        assert_eq!(out.total_passed, 7, "expected 7 PASS lines counted");
        assert_eq!(out.total_failed, 0, "expected zero failures");
        assert_eq!(out.total_ignored, 0, "expected zero skipped");
        assert_eq!(out.total_tests, 7, "summary total_tests");
        assert!(out.failures.is_empty(), "failures vec must be empty");
        assert!(!out.any_failure_observed, "any_failure_observed must be false");
    }

    #[test]
    fn parse_nextest_one_failure_with_detail() {
        let out = parse_nextest_output(NEXTEST_SAMPLE_ONE_FAILURE);
        assert_eq!(out.total_passed, 2);
        assert_eq!(out.total_failed, 1);
        assert_eq!(out.total_tests, 3);
        assert_eq!(out.failures.len(), 1);
        let f = &out.failures[0];
        assert_eq!(f.test_name, "broken_test");
        assert_eq!(f.binary, "my_crate::tests");
        assert!(
            f.output_block.contains("assertion `left == right` failed"),
            "output_block should contain panic detail; got: {}",
            f.output_block
        );
        assert!(out.any_failure_observed);
    }

    #[test]
    fn parse_nextest_flaky_counts_as_failure() {
        // FLAKY = passed-on-retry. With `flaky-result = "fail"` in the
        // ci profile, this should count as a failure for ledger purposes.
        let out = parse_nextest_output(NEXTEST_SAMPLE_FLAKY);
        assert_eq!(out.total_passed, 1, "stable_test passed");
        assert_eq!(out.total_failed, 1, "flaky_test counts as failure");
        assert_eq!(out.failures.len(), 1);
        let f = &out.failures[0];
        assert_eq!(f.test_name, "flaky_test");
        // No panic detail block in this sample; placeholder expected.
        assert!(
            f.output_block.contains("no panic detail captured"),
            "FLAKY without detail block should use placeholder; got: {}",
            f.output_block
        );
    }

    #[test]
    fn parse_nextest_skip_counted_as_ignored() {
        let out = parse_nextest_output(NEXTEST_SAMPLE_WITH_SKIP);
        assert_eq!(out.total_passed, 2);
        assert_eq!(out.total_failed, 0);
        assert_eq!(out.total_ignored, 1, "SKIP increments ignored");
        assert!(out.failures.is_empty());
    }

    #[test]
    fn parse_nextest_timeout_counted_as_failure() {
        let out = parse_nextest_output(NEXTEST_SAMPLE_TIMEOUT);
        assert_eq!(out.total_passed, 1);
        assert_eq!(out.total_failed, 1, "TIMEOUT counts as failure");
        assert_eq!(out.failures.len(), 1);
        let f = &out.failures[0];
        assert_eq!(f.test_name, "hanging_test");
        // No panic detail block (test was killed); placeholder expected.
        assert!(f.output_block.contains("no panic detail captured"));
    }

    #[test]
    fn parse_nextest_strips_ansi_codes() {
        let out = parse_nextest_output(NEXTEST_SAMPLE_WITH_ANSI);
        // The ANSI-decorated PASS line should still be recognized.
        assert_eq!(out.total_passed, 1);
        assert_eq!(out.total_failed, 0);
        assert_eq!(out.total_tests, 1);
    }

    #[test]
    fn parse_nextest_status_line_extracts_components() {
        // Direct unit test of the helper.
        let line = "        PASS [   0.029s] (1/7) neurogrim-sdk::compile_test_re_exports theme_b_traits_are_object_safe_via_sdk";
        let (status, binary, test) = parse_nextest_status_line(line).expect("must parse");
        assert_eq!(status, "PASS");
        assert_eq!(binary, "neurogrim-sdk::compile_test_re_exports");
        assert_eq!(test, "theme_b_traits_are_object_safe_via_sdk");
    }

    #[test]
    fn parse_nextest_status_line_rejects_non_status_lines() {
        // Lines that look similar but aren't status lines.
        assert!(parse_nextest_status_line("    Finished `test` profile").is_none());
        assert!(parse_nextest_status_line("     Summary [   0.135s] 7 tests run").is_none());
        assert!(parse_nextest_status_line("--- STDERR:    foo bar ---").is_none());
        assert!(parse_nextest_status_line("").is_none());
    }

    // ── V5-FOUND-3 Phase 0 (2026-05-03) — build_cargo_args tests ──────────

    /// Default Args fixture for build_cargo_args tests; tweak per-test.
    fn args_fixture() -> Args {
        Args {
            keep_last: 500,
            show_only_new: false,
            retry_failed: false,
            slow: false,
            verbose: false,
            e2e: false,
            project_root: ".".into(),
            profile: "default".into(),
        }
    }

    #[test]
    fn build_cargo_args_default_workspace_run() {
        let v = build_cargo_args(&args_fixture(), None);
        assert_eq!(
            v,
            vec![
                "nextest", "run", "--workspace", "--all-targets",
                "--profile", "default", "--color", "never",
            ]
        );
        // No `--` separator when neither --slow nor --retry-failed apply.
        assert!(!v.iter().any(|s| s == "--"), "no libtest section expected");
    }

    #[test]
    fn build_cargo_args_slow_only() {
        let mut a = args_fixture();
        a.slow = true;
        let v = build_cargo_args(&a, None);
        assert!(v.contains(&"--".to_string()));
        assert!(v.contains(&"--include-ignored".to_string()));
        assert!(!v.contains(&"--exact".to_string()));
    }

    #[test]
    fn build_cargo_args_retry_only() {
        let names = vec!["test_a".to_string(), "test_b".to_string()];
        let v = build_cargo_args(&args_fixture(), Some(&names));
        assert!(v.contains(&"--".to_string()));
        assert!(v.contains(&"--exact".to_string()));
        assert!(v.contains(&"test_a".to_string()));
        assert!(v.contains(&"test_b".to_string()));
        assert!(
            !v.contains(&"--include-ignored".to_string()),
            "no --include-ignored without --slow"
        );
    }

    /// V5-FOUND-3 Phase 0 bug-fix regression: prior to this commit, the
    /// `--retry-failed` branch hardcoded `-- --exact <names>` and left
    /// no slot for `--include-ignored`, so `--retry-failed --slow`
    /// silently dropped the slow flag. This asserts both flags now
    /// propagate together.
    #[test]
    fn build_cargo_args_retry_and_slow_propagates_include_ignored() {
        let mut a = args_fixture();
        a.slow = true;
        let names = vec!["my_test".to_string()];
        let v = build_cargo_args(&a, Some(&names));

        // Find the `--` index; everything after must contain BOTH
        // --include-ignored AND --exact <name>.
        let dash_dash_idx = v.iter().position(|s| s == "--").expect("-- present");
        let after = &v[dash_dash_idx + 1..];
        assert!(
            after.contains(&"--include-ignored".to_string()),
            "--include-ignored MUST appear in libtest section under --retry-failed --slow; got: {after:?}"
        );
        assert!(
            after.contains(&"--exact".to_string()),
            "--exact still present"
        );
        assert!(
            after.contains(&"my_test".to_string()),
            "test name still present"
        );
    }

    #[test]
    fn build_cargo_args_honors_profile_arg() {
        let mut a = args_fixture();
        a.profile = "ci".into();
        let v = build_cargo_args(&a, None);
        let profile_idx = v.iter().position(|s| s == "--profile").expect("present");
        assert_eq!(v[profile_idx + 1], "ci");
    }

    #[test]
    fn build_cargo_args_retry_with_empty_names_still_emits_libtest_section() {
        // Empty-list edge case: this branch shouldn't be reached at
        // call-site (run() short-circuits when to_retry is empty), but
        // the function should remain well-defined for an empty slice.
        let names: Vec<String> = vec![];
        let v = build_cargo_args(&args_fixture(), Some(&names));
        let dash_dash_idx = v.iter().position(|s| s == "--").expect("-- present");
        // After --, we expect --exact with no positional names following.
        assert_eq!(v[dash_dash_idx + 1], "--exact");
        assert_eq!(v.len(), dash_dash_idx + 2, "no positional names");
    }
}
