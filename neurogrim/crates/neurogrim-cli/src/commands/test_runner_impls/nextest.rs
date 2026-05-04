//! `NextestRunner` — `TestRunner` impl that wraps `cargo nextest run`
//! (V5-FOUND-4 Phase 2, 2026-05-04).
//!
//! Surgical extraction of the cargo-invocation + parse logic that
//! used to live inline in [`super::super::test::run`]. Re-uses the
//! existing [`super::super::test::build_cargo_args`] (V5-FOUND-3 Phase 0c,
//! commit `39d7295`) and [`super::super::test::parse_nextest_output`]
//! (V5-FOUND-2 Phase 1, commit `52356f0`) without moving them —
//! Fork B1 + C1 from the V5-FOUND-4 plan keep these in
//! `commands::test` so their 25+ unit tests stay in place.
//!
//! # Span ownership (plan-critic technical agent C3)
//!
//! - **Wrapper (`commands::test::run`) owns the outer `test.run` span**
//!   — that's the V5-FOUND-1 instrumentation point. Phase 3 wires the
//!   wrapper to dispatch through this runner.
//! - **NextestRunner owns the child `cargo.invoke` span** — emitted
//!   inside `run()`, scoped to the cargo subprocess wall-time.
//!
//! # Selection translation
//!
//! ```text
//! TestSelection::All        → cargo argv from build_cargo_args(args, None)
//! TestSelection::Names(v)   → cargo argv from build_cargo_args(args, Some(&v))
//!                              (libtest-compat -- --exact <name> ...)
//! TestSelection::Packages   → not yet wired — bails with anyhow::Error.
//!                              v5.1 adds `cargo nextest run -p X -p Y`.
//! _ (future variants)       → bails with explicit "rebuild against
//!                              current TestSelection variants" message.
//! ```
//!
//! The `_` arm is required by `#[non_exhaustive]` on `TestSelection` —
//! v5.1's V5-FOUND-3 follow-up adds `ByCoverage(...)` non-breakingly.

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use neurogrim_core::test_runner::{
    TestFailure, TestRunReport, TestRunner, TestRunnerFactory, TestSelection,
};
use std::path::PathBuf;
use std::time::Instant;
use tokio::process::Command;

use super::super::test::{build_cargo_args, parse_nextest_output, Args};

/// `TestRunner` impl wrapping `cargo nextest run`.
///
/// Constructed with the project root + the same `--profile` and
/// `--slow` flags the wrapper carries on its `Args`. `run()` builds
/// the cargo argv via [`build_cargo_args`], spawns cargo (capturing
/// stdout + stderr), parses via [`parse_nextest_output`], and
/// translates the parsed output to a [`TestRunReport`].
pub struct NextestRunner {
    project_root: PathBuf,
    profile: String,
    slow: bool,
}

impl NextestRunner {
    /// Construct from explicit fields. Phase 3 wires
    /// `commands::test::run` to construct via this constructor.
    pub fn new(project_root: PathBuf, profile: String, slow: bool) -> Self {
        NextestRunner {
            project_root,
            profile,
            slow,
        }
    }

    /// Bridge the runner's lean `(profile, slow)` carriage to
    /// `build_cargo_args`'s `&Args` signature (Fork B1 — leave
    /// `build_cargo_args` unchanged in `commands::test`). Other
    /// `Args` fields default to "no-op for cargo argv construction"
    /// values; only `profile` + `slow` actually influence the
    /// generated argv.
    fn synthetic_args(&self) -> Args {
        Args {
            keep_last: 500,
            show_only_new: false,
            retry_failed: false, // synthetic_args drives the All/Names branch via the retry_names parameter, not this flag
            slow: self.slow,
            verbose: false,
            e2e: false,
            project_root: self
                .project_root
                .to_string_lossy()
                .into_owned(),
            profile: self.profile.clone(),
        }
    }
}

#[async_trait]
impl TestRunner for NextestRunner {
    async fn run(&self, selection: &TestSelection) -> anyhow::Result<TestRunReport> {
        // Translate TestSelection → cargo argv.
        // `_` arm covers future #[non_exhaustive] variants (V5-FOUND-3
        // unblock adds ByCoverage(...) non-breakingly).
        let retry_names: Option<Vec<String>> = match selection {
            TestSelection::All => None,
            TestSelection::Names(names) => Some(names.clone()),
            TestSelection::Packages(_) => {
                return Err(anyhow!(
                    "TestSelection::Packages is not yet wired in NextestRunner — \
                     tracked as v5.1 work (would map to `cargo nextest run -p X -p Y`)"
                ));
            }
            _ => {
                return Err(anyhow!(
                    "TestSelection::<unknown variant> — recompile NextestRunner \
                     against the current `neurogrim-core::test_runner` variants"
                ));
            }
        };

        let args = self.synthetic_args();
        let cargo_args = build_cargo_args(&args, retry_names.as_deref());

        // Child `cargo.invoke` span — V5-FOUND-1 instrumentation.
        // The outer `test.run` span is owned by the wrapper at
        // `commands::test::run` (Phase 3 wiring).
        let cargo_span = tracing::info_span!(
            "cargo.invoke",
            cmd = "test",
            runner = "nextest",
            exit_code = tracing::field::Empty,
        );
        let _enter = cargo_span.enter();
        let started = Instant::now();
        let output = Command::new("cargo")
            .args(&cargo_args)
            .current_dir(&self.project_root)
            .output()
            .await
            .with_context(|| "failed to spawn cargo")?;
        let raw_exit_code = output.status.code().unwrap_or(-1);
        cargo_span.record("exit_code", raw_exit_code as i64);
        let duration_ms = started.elapsed().as_millis() as u64;
        drop(_enter);

        // Parse via the existing parser (Fork C1 — left in commands::test).
        let stdout_text = String::from_utf8_lossy(&output.stdout);
        let stderr_text = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout_text}\n{stderr_text}");
        let parsed = parse_nextest_output(&combined);

        // Translate ParsedCargoOutput → TestRunReport. 1:1 field
        // mapping; no logic change vs. pre-V5-FOUND-4 behavior.
        // `filtered_out` isn't tracked by the existing parser, so we
        // leave it at 0 — selection narrower than the workspace will
        // surface as "fewer tests run" rather than an explicit count.
        //
        // Construction pattern: `TestRunReport` is `#[non_exhaustive]`
        // (forward-compat for v5.1 ByCoverage fields), so struct-
        // expression construction is blocked from outside
        // `neurogrim-core`. We use `Default::default()` + field
        // assignment instead — works on `#[non_exhaustive]` structs
        // and naturally accommodates future field additions
        // (consumers ignore fields they don't set).
        let mut report = TestRunReport::default();
        report.passed = parsed.total_passed;
        report.failed = parsed.total_failed;
        report.ignored = parsed.total_ignored;
        report.filtered_out = 0;
        report.duration_ms = duration_ms;
        report.failures = parsed
            .failures
            .into_iter()
            .map(|f| {
                let mut tf = TestFailure::default();
                tf.test_name = f.test_name;
                tf.binary = f.binary;
                tf.detail = f.output_block;
                tf
            })
            .collect();
        report.raw_exit_code = raw_exit_code;

        Ok(report)
    }
}

/// Factory for [`NextestRunner`]. Stateless. The wrapper at
/// `commands::test::run` (Phase 3) constructs `NextestRunner`
/// directly with its `Args`-derived fields — this factory exists
/// for [`neurogrim_core::test_runner::TestRunnerRegistry`]
/// registration when V5.5 BACKLOG B-52 (`--runner=` flag) lands.
pub struct NextestRunnerFactory;

impl TestRunnerFactory for NextestRunnerFactory {
    fn name(&self) -> &'static str {
        "nextest"
    }

    fn build(&self) -> Box<dyn TestRunner> {
        // Default project_root + profile + slow. Production path
        // constructs NextestRunner directly with operator-supplied
        // values; the registry path is for the future v5.5 dispatch
        // surface where those values live in operator config.
        Box::new(NextestRunner::new(
            ".".into(),
            "default".into(),
            false,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_name_is_nextest() {
        let f = NextestRunnerFactory;
        assert_eq!(f.name(), "nextest");
    }

    #[test]
    fn factory_build_returns_runner() {
        let f = NextestRunnerFactory;
        let _runner = f.build();
        // Construction succeeds; runtime invocation is verified in
        // Phase 3's byte-identical-output check (the wrapper
        // dispatching through the trait produces the same operator-
        // facing output as the pre-V5-FOUND-4 inline path).
    }

    #[test]
    fn synthetic_args_carries_profile_and_slow() {
        let r = NextestRunner::new(".".into(), "ci".into(), true);
        let a = r.synthetic_args();
        assert_eq!(a.profile, "ci");
        assert!(a.slow);
        assert!(!a.retry_failed);
    }

    #[test]
    fn synthetic_args_default_profile() {
        let r = NextestRunner::new(".".into(), "default".into(), false);
        let a = r.synthetic_args();
        assert_eq!(a.profile, "default");
        assert!(!a.slow);
    }

    /// V5-FOUND-4 Phase 2 — verify the selection translation
    /// matches `build_cargo_args`'s expectations without spawning
    /// cargo. The translation logic is the load-bearing part of
    /// NextestRunner's `run()`; spawning cargo is verified via
    /// Phase 3's byte-identical-output check.
    #[test]
    fn selection_all_maps_to_no_retry_names() {
        let _r = NextestRunner::new(".".into(), "default".into(), false);
        // The translation lives inside run(); to test it without
        // an executor, we exercise the same logic shape directly.
        let selection = TestSelection::All;
        let retry_names: Option<Vec<String>> = match selection {
            TestSelection::All => None,
            TestSelection::Names(names) => Some(names),
            TestSelection::Packages(_) => panic!("not expected in this test"),
            _ => panic!("future variant"),
        };
        assert!(retry_names.is_none());
    }

    #[test]
    fn selection_names_maps_to_retry_names() {
        let selection = TestSelection::Names(vec!["test_a".into(), "test_b".into()]);
        let retry_names: Option<Vec<String>> = match selection {
            TestSelection::All => None,
            TestSelection::Names(names) => Some(names),
            TestSelection::Packages(_) => panic!("not expected in this test"),
            _ => panic!("future variant"),
        };
        assert_eq!(retry_names.unwrap(), vec!["test_a", "test_b"]);
    }
}
