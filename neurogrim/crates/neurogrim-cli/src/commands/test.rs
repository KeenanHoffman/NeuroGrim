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

    /// Project root containing `.claude/brain/`. Default: cwd.
    #[arg(long, default_value = ".")]
    pub project_root: String,
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
    let ledger_path = ledger_path(project_root);
    let archive_path = project_root.join(".claude/brain").join(ARCHIVE_FILENAME);

    // Determine which tests to run.
    let cargo_args = if args.retry_failed {
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
        let mut v: Vec<String> = vec!["test".into(), "--workspace".into(), "--all-targets".into(), "--color".into(), "never".into(), "--".into(), "--exact".into()];
        // Each name passed positionally with --exact; cargo's libtest
        // honors them as OR filters.
        for name in &to_retry {
            v.push(name.clone());
        }
        v
    } else {
        let mut v: Vec<String> = vec!["test".into(), "--workspace".into(), "--all-targets".into(), "--color".into(), "never".into()];
        if args.slow {
            v.push("--".into());
            v.push("--include-ignored".into());
        }
        v
    };

    // If verbose, just exec cargo and bail — let cargo speak.
    if args.verbose {
        let status = Command::new("cargo")
            .args(&cargo_args)
            .status()
            .with_context(|| "failed to spawn cargo")?;
        std::process::exit(status.code().unwrap_or(1));
    }

    // Run cargo with output captured.
    eprintln!("✦ neurogrim test — running workspace tests…");
    let output = Command::new("cargo")
        .args(&cargo_args)
        .output()
        .with_context(|| "failed to spawn cargo")?;

    let stdout_text = String::from_utf8_lossy(&output.stdout);
    let stderr_text = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout_text}\n{stderr_text}");

    let parsed = parse_cargo_output(&combined);

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

    // Mirror cargo's exit code.
    std::process::exit(output.status.code().unwrap_or(1));
}

/// Path to `<project>/.claude/brain/test-failures.jsonl`.
pub fn ledger_path(project_root: &Path) -> PathBuf {
    project_root.join(".claude/brain").join(LEDGER_FILENAME)
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
}
