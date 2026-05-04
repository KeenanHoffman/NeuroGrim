//! Conformance suite for [`crate::test_runner::TestRunner`] impls
//! and their factories (V5-FOUND-4 Phase 1, 2026-05-04).
//!
//! Third-party crates that ship a `TestRunner` impl use this suite
//! to verify they honor the contract every built-in runner satisfies.
//! The suite is intentionally **factory-shaped** — takes a
//! `&dyn TestRunnerFactory`, builds runners from it, runs cross-cutting
//! tests that don't depend on runner-type-specific configuration.
//!
//! # Usage
//!
//! ```no_run
//! use neurogrim_core::test_runner_conformance::run_factory_conformance;
//! # use neurogrim_core::test_runner::{TestRunner, TestRunnerFactory, TestSelection, TestRunReport};
//! # use async_trait::async_trait;
//! # struct MyFactory;
//! # struct MyRunner;
//! # #[async_trait]
//! # impl TestRunner for MyRunner {
//! #     async fn run(&self, _: &TestSelection) -> anyhow::Result<TestRunReport> { todo!() }
//! # }
//! # impl TestRunnerFactory for MyFactory {
//! #     fn name(&self) -> &'static str { "my-runner" }
//! #     fn build(&self) -> Box<dyn TestRunner> { Box::new(MyRunner) }
//! # }
//! # async fn example() {
//! let factory = MyFactory;
//! let report = run_factory_conformance(&factory).await;
//! assert!(
//!     report.all_passed(),
//!     "Conformance failures: {:?}",
//!     report.failures()
//! );
//! # }
//! ```
//!
//! # What the suite covers (4 cross-cutting tests)
//!
//! Plan-critic v1 (2026-05-04) reduced the suite from 6 → 4 tests.
//! The dropped 2 tests (`run_with_empty_selection_completes` and
//! `run_is_concurrent_safe`) were not honestly testable for runners
//! that spawn subprocesses (cargo lockfile contention, undefined
//! cargo-with-zero-args behavior). Object-safety + Send guarantees
//! are now enforced at compile time in `crate::test_runner` instead.
//!
//! 1. **`factory_name_non_empty`** — wire-name must exist.
//! 2. **`factory_name_stable_across_calls`** — multiple `name()`
//!    calls return the same string (no per-call generation).
//! 3. **`factory_build_repeatable`** — calling `build()` 10 times
//!    succeeds; no global-state corruption.
//! 4. **`run_with_malformed_selection_returns_ok_or_err_no_panic`** —
//!    `TestSelection::Names(vec!["::::nonexistent::::".into()])`
//!    returns `Ok(report_with_zero_matches)` OR `Err(...)` within
//!    60s — must not panic. Catches contract violations where a
//!    runner panics on inputs that no test name matches.
//!
//! # Type re-exports (V5-SDK-1 Phase 1.5 hoist — Fork F1)
//!
//! `ConformanceReport` and `TestResult` are the canonical shared
//! types from [`crate::conformance`]; re-exported here so existing
//! consumers' import paths keep working. SDK consumers writing
//! multiple plugin types (e.g., a sensor + a runner) get one nominal
//! `ConformanceReport` across all 4 V5 conformance suites.

use crate::test_runner::{TestRunnerFactory, TestSelection};
use std::time::Duration;

pub use crate::conformance::{ConformanceReport, TestResult};

/// Per-`run()` wall-clock ceiling. Runners that take longer than
/// this on a malformed-selection input have a contract violation
/// (or are doing something egregious like cold-building the entire
/// universe). 60s is generous to accommodate cold cargo builds.
const RUN_TIMEOUT: Duration = Duration::from_secs(60);

/// Run the full factory-conformance suite. Returns the
/// [`ConformanceReport`] with one entry per test (4 entries total).
pub async fn run_factory_conformance(
    factory: &dyn TestRunnerFactory,
) -> ConformanceReport {
    let mut report = ConformanceReport::new();

    // Cross-cutting (3 — synchronous; mirror V5-MOD-2's pattern)
    report.add(test_factory_name_non_empty(factory));
    report.add(test_factory_name_stable_across_calls(factory));
    report.add(test_factory_build_repeatable(factory));

    // Cross-cutting (1 — async; spawns the runner against a
    // deliberately-malformed selection)
    report.add(test_run_with_malformed_selection(factory).await);

    report
}

// ────────────────────────────────────────────────────────────────
// T1-T3: factory contract (sync, no IO)
// ────────────────────────────────────────────────────────────────

fn test_factory_name_non_empty(factory: &dyn TestRunnerFactory) -> TestResult {
    let name = factory.name();
    if name.is_empty() {
        TestResult::fail(
            "factory_name_non_empty",
            "factory.name() returned empty string",
        )
    } else {
        TestResult::pass("factory_name_non_empty")
    }
}

fn test_factory_name_stable_across_calls(
    factory: &dyn TestRunnerFactory,
) -> TestResult {
    let n1 = factory.name();
    let n2 = factory.name();
    let n3 = factory.name();
    if n1 != n2 || n2 != n3 {
        TestResult::fail(
            "factory_name_stable_across_calls",
            format!(
                "factory.name() returned different values across calls: \
                 {n1:?} vs {n2:?} vs {n3:?}"
            ),
        )
    } else {
        TestResult::pass("factory_name_stable_across_calls")
    }
}

fn test_factory_build_repeatable(factory: &dyn TestRunnerFactory) -> TestResult {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for _ in 0..10 {
            let _ = factory.build();
        }
    }));
    if result.is_err() {
        TestResult::fail(
            "factory_build_repeatable",
            "factory.build() panicked across 10 invocations",
        )
    } else {
        TestResult::pass("factory_build_repeatable")
    }
}

// ────────────────────────────────────────────────────────────────
// T4: run contract (async — invokes the runner)
// ────────────────────────────────────────────────────────────────

async fn test_run_with_malformed_selection(
    factory: &dyn TestRunnerFactory,
) -> TestResult {
    let runner = factory.build();
    let selection = TestSelection::Names(vec!["::::nonexistent::::".into()]);
    let result = tokio::time::timeout(RUN_TIMEOUT, runner.run(&selection)).await;
    match result {
        Err(_) => TestResult::fail(
            "run_with_malformed_selection_returns_ok_or_err_no_panic",
            format!("runner.run() exceeded {RUN_TIMEOUT:?} on a malformed selection"),
        ),
        Ok(Err(_)) => TestResult::pass_with_note(
            "run_with_malformed_selection_returns_ok_or_err_no_panic",
            "fallible: returned Err on malformed selection",
        ),
        Ok(Ok(_)) => TestResult::pass(
            "run_with_malformed_selection_returns_ok_or_err_no_panic",
        ),
    }
}

// ────────────────────────────────────────────────────────────────
// Tests — verify the suite itself is sound by running it against
// a stub runner with deterministic Ok behavior.
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_runner::{TestRunReport, TestRunner};
    use anyhow::Result;
    use async_trait::async_trait;

    struct MockSuccessRunner;

    #[async_trait]
    impl TestRunner for MockSuccessRunner {
        async fn run(&self, _: &TestSelection) -> Result<TestRunReport> {
            Ok(TestRunReport::default())
        }
    }

    struct MockSuccessFactory;

    impl TestRunnerFactory for MockSuccessFactory {
        fn name(&self) -> &'static str {
            "mock-success"
        }
        fn build(&self) -> Box<dyn TestRunner> {
            Box::new(MockSuccessRunner)
        }
    }

    /// V5-FOUND-4 Phase 1: a deterministic-Ok runner passes every
    /// test in the suite. Validates the suite itself is sound.
    #[tokio::test]
    async fn mock_success_factory_passes_full_conformance_suite() {
        let factory = MockSuccessFactory;
        let report = run_factory_conformance(&factory).await;
        assert!(
            report.all_passed(),
            "{}/{} failed: {:#?}",
            report.failures().len(),
            report.total(),
            report.failures()
        );
        assert_eq!(report.total(), 4, "must run exactly 4 conformance tests");
    }

    // ── Negative-path coverage: factories that violate the contract ──

    struct MockEmptyNameFactory;
    impl TestRunnerFactory for MockEmptyNameFactory {
        fn name(&self) -> &'static str {
            ""
        }
        fn build(&self) -> Box<dyn TestRunner> {
            Box::new(MockSuccessRunner)
        }
    }

    #[tokio::test]
    async fn empty_name_factory_fails_conformance() {
        let factory = MockEmptyNameFactory;
        let report = run_factory_conformance(&factory).await;
        assert!(
            !report.all_passed(),
            "empty-name factory should fail conformance"
        );
        let names: Vec<_> = report.failures().iter().map(|r| r.name).collect();
        assert!(
            names.contains(&"factory_name_non_empty"),
            "expected factory_name_non_empty failure; got {names:?}"
        );
    }

    /// Explicit Err return is a documented "fallible" pass, not a
    /// failure — the contract permits Err on malformed selection.
    struct MockFallibleRunner;
    #[async_trait]
    impl TestRunner for MockFallibleRunner {
        async fn run(&self, _: &TestSelection) -> Result<TestRunReport> {
            anyhow::bail!("fallible test — refused to run malformed selection")
        }
    }

    struct MockFallibleFactory;
    impl TestRunnerFactory for MockFallibleFactory {
        fn name(&self) -> &'static str {
            "mock-fallible"
        }
        fn build(&self) -> Box<dyn TestRunner> {
            Box::new(MockFallibleRunner)
        }
    }

    #[tokio::test]
    async fn fallible_factory_still_passes_conformance() {
        let factory = MockFallibleFactory;
        let report = run_factory_conformance(&factory).await;
        assert!(
            report.all_passed(),
            "fallible factory should pass conformance (Err is acceptable on malformed selection): {:#?}",
            report.failures()
        );
    }
}
