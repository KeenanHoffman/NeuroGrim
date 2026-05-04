//! `TestRunner` trait — pluggable contract for executing a workspace
//! test selection (V5-FOUND-4, 2026-05-04).
//!
//! Single-method trait. The `neurogrim test` wrapper dispatches
//! through `Box<dyn TestRunner>` internally so users can swap the
//! test runner without forking the wrapper. v5.0 ships one impl
//! (`NextestRunner` in `crate::test_runner_impls::nextest`);
//! AgentDrivenRunner is deferred to v5.5 (BACKLOG B-51) once the
//! agent-orchestration pattern has a real Rust LLM client to ride on
//! (cf. V5-FOUND-1.1 deferred-stub for the same Rust-LLM-client gap).
//!
//! # Reshape-rule alignment (v5-roadmap §A)
//!
//! V5-FOUND-4 clears the trait-extraction reshape rule via clause
//! **(iii)** — leaving NextestRunner concrete blocks V5-SDK-2's
//! promised conformance-suite re-export (deliverable 2). The trait
//! is the unblock; the second-impl AgentDrivenRunner is intentionally
//! NOT shipped at v5.0 to preserve methodology honesty (no aspirational
//! pluggability per proposed VISION principle #20).
//!
//! # Object-safety + async + Send
//!
//! `TestRunner` is `Send + Sync` and uses `#[async_trait]` (matches
//! the existing `Transport` / `ScoringSource` / `Sensor` / `QueueBackend`
//! workspace conventions). Object-safety is enforced at compile time
//! by the `_object_safety_check_test_runner` function below; `Send`
//! on `TestRunReport` is enforced by `_send_check_test_run_report`.
//! Both are zero-runtime-cost compile gates.
//!
//! # Forward-compatibility
//!
//! [`TestSelection`] and [`TestRunReport`] are `#[non_exhaustive]`.
//! v5.1's V5-FOUND-3 follow-up adds a `TestSelection::ByCoverage(...)`
//! variant (when the Windows coverage-toolchain gap unblocks).
//! Downstream `match selection` blocks must include `_ => ...` to
//! survive the variant addition non-breakingly.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// Which tests to run on a [`TestRunner::run`] invocation.
///
/// `#[non_exhaustive]` so future variants (e.g., V5-FOUND-3's
/// `ByCoverage`) extend without breaking downstream matches.
/// Pre-existing variants:
///
/// - `All` — run the entire workspace test set (default operator
///   experience: `neurogrim test` with no flags).
/// - `Names(Vec<String>)` — replay specific test names; corresponds
///   to libtest-compat `--exact <name>` (used by `--retry-failed`).
/// - `Packages(Vec<String>)` — restrict to specific cargo packages;
///   corresponds to `cargo nextest run -p X -p Y`. NextestRunner
///   v5.0 errors on this variant pending the v5.1 wiring.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestSelection {
    /// Run the entire workspace test set.
    All,
    /// Replay a specific list of test names (libtest `--exact` semantics).
    Names(Vec<String>),
    /// Restrict to specific cargo packages.
    Packages(Vec<String>),
}

/// Aggregate outcome of a single [`TestRunner::run`] invocation.
///
/// `#[non_exhaustive]` so future fields (e.g., per-binary coverage
/// data added by V5-FOUND-3 unblock) extend non-breakingly.
#[non_exhaustive]
#[derive(Debug, Clone, Default)]
pub struct TestRunReport {
    /// Number of tests that passed.
    pub passed: u32,
    /// Number of tests that failed (count, not detail).
    pub failed: u32,
    /// Number of tests skipped via `#[ignore]` or filterset exclusion.
    pub ignored: u32,
    /// Number of tests filtered out (selection narrower than total).
    pub filtered_out: u32,
    /// Wall-clock duration of the runner invocation.
    pub duration_ms: u64,
    /// Per-failure detail. Length should equal `failed` for
    /// well-behaved runners; consumers asserting this should rely
    /// on the impl's contract, not the trait's enforcement.
    pub failures: Vec<TestFailure>,
    /// Cargo / underlying-runner exit code, surfaced verbatim so
    /// the wrapper can preserve cargo's exit-code semantics
    /// (`0` = success, `1` = test failure, etc.).
    pub raw_exit_code: i32,
}

/// One test failure inside a [`TestRunReport`].
#[derive(Debug, Clone, Default)]
pub struct TestFailure {
    /// Full test path (e.g., `mod::path::to::test_name`).
    pub test_name: String,
    /// Test binary identifier (e.g., `neurogrim-core::lib`).
    pub binary: String,
    /// Panic / assertion detail, ANSI-stripped.
    pub detail: String,
}

/// Pluggable contract for executing a workspace test [`TestSelection`].
///
/// Implementations correspond 1:1 to the wire-name strings exposed
/// by [`TestRunnerFactory::name`]. Object-safe (`Box<dyn TestRunner>`);
/// `Send + Sync` for cross-thread dispatch via the wrapper's tracing
/// span flow.
///
/// # Contract
///
/// - **`run`** — execute the selection. `Err` is reserved for runner-
///   internal failures (cargo not found, parse error, host I/O fault).
///   Test failures are surfaced via [`TestRunReport::failures`] +
///   `failed` count, NOT via `Err`. A run with `failed > 0` is still
///   `Ok(report)` — the runner did its job, the *tests* failed.
///
/// # `#[non_exhaustive]` discipline
///
/// Implementations should match `selection` with an explicit `_`
/// arm — v5.1 adds `TestSelection::ByCoverage(...)` non-breakingly,
/// and a strict match without `_` would fail to compile against the
/// new variant.
#[async_trait]
pub trait TestRunner: Send + Sync {
    /// Execute the selection. See trait-level rustdoc for the
    /// `Err` vs. `Ok(report_with_failures)` contract.
    async fn run(&self, selection: &TestSelection) -> Result<TestRunReport>;
}

/// Factory producing a [`TestRunner`] for a given runner name.
///
/// Mirrors V5-MOD-2's `SensorFactory` pattern — the trait carries
/// the wire-name (`name()`); the runner trait itself does NOT have
/// a name method (deliberate, per plan-critic technical agent C4
/// alignment with `Sensor`).
pub trait TestRunnerFactory: Send + Sync {
    /// Stable wire-name (`"nextest"`, future: `"agent"`, etc.).
    /// MUST be stable across calls.
    fn name(&self) -> &'static str;

    /// Build a runner instance. May return distinct instances
    /// across calls; the trait does not require interior mutability.
    fn build(&self) -> Box<dyn TestRunner>;
}

/// Registry of [`TestRunnerFactory`] instances keyed by wire-name.
///
/// Mirrors V5-MOD-1's `ScoringSourceRegistry` and V5-MOD-2's
/// `SensorRegistry` patterns. Last-write-wins on duplicate names
/// (intentional — consumers can override built-ins for testing).
pub struct TestRunnerRegistry {
    factories: HashMap<&'static str, Box<dyn TestRunnerFactory>>,
}

impl TestRunnerRegistry {
    /// Empty registry. Caller registers factories explicitly.
    pub fn new() -> Self {
        TestRunnerRegistry {
            factories: HashMap::new(),
        }
    }

    /// Register a factory by its `name()`. Replaces any prior
    /// registration with the same name (last-write-wins).
    pub fn register(&mut self, factory: Box<dyn TestRunnerFactory>) {
        let name = factory.name();
        self.factories.insert(name, factory);
    }

    /// Look up a factory by wire-name. Returns `None` if no
    /// factory is registered for that name.
    pub fn get(&self, name: &str) -> Option<&dyn TestRunnerFactory> {
        self.factories.get(name).map(|f| f.as_ref())
    }

    /// Convenience: look up + immediately build. Returns `None`
    /// if no factory is registered for `name`.
    pub fn build(&self, name: &str) -> Option<Box<dyn TestRunner>> {
        self.get(name).map(|f| f.build())
    }

    /// Iterate over registered wire-names. Useful for diagnostics
    /// and the future v5.5 `--list-runners` flag (BACKLOG B-52).
    pub fn registered_names(&self) -> impl Iterator<Item = &&'static str> {
        self.factories.keys()
    }

    /// `true` iff a factory is registered for `name`.
    pub fn has(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }
}

impl Default for TestRunnerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ────────────────────────────────────────────────────────────────
// Compile-time safety guards (V5-FOUND-4 plan-critic technical
// agent S1 + C2). DO NOT REMOVE — these prevent silent
// regressions of object-safety and Send.
// ────────────────────────────────────────────────────────────────

/// If `TestRunner` ever loses object-safety, this fn fails to
/// compile. Mirrors the `_object_safety_check_scoring_source`
/// pattern at `scoring_source.rs:259`.
#[allow(dead_code)]
fn _object_safety_check_test_runner(_: Box<dyn TestRunner>) {}

/// If `TestRunReport` ever stops being `Send`, this fn fails to
/// compile. Required because `#[async_trait]` generates a `Box<dyn
/// Future<Output = ...> + Send>` only when the output type is `Send`.
#[allow(dead_code)]
fn _send_check_test_run_report(_: TestRunReport)
where
    TestRunReport: Send,
{
}

/// Belt-and-suspenders: factory must be `Send + Sync` for the
/// registry to hold `Box<dyn TestRunnerFactory>` across threads.
#[allow(dead_code)]
fn _send_sync_check_test_runner_factory(_: &dyn TestRunnerFactory) {}

// ────────────────────────────────────────────────────────────────
// Tests — registry round-trip + match-arm safety smoke
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct StubRunner;
    #[async_trait]
    impl TestRunner for StubRunner {
        async fn run(&self, _: &TestSelection) -> Result<TestRunReport> {
            Ok(TestRunReport::default())
        }
    }

    struct StubFactory;
    impl TestRunnerFactory for StubFactory {
        fn name(&self) -> &'static str {
            "stub"
        }
        fn build(&self) -> Box<dyn TestRunner> {
            Box::new(StubRunner)
        }
    }

    #[test]
    fn registry_register_and_get_round_trip() {
        let mut reg = TestRunnerRegistry::new();
        assert!(reg.get("stub").is_none());
        assert!(!reg.has("stub"));
        reg.register(Box::new(StubFactory));
        assert!(reg.get("stub").is_some());
        assert!(reg.has("stub"));
        assert_eq!(reg.registered_names().count(), 1);
    }

    #[test]
    fn registry_last_write_wins() {
        let mut reg = TestRunnerRegistry::new();
        reg.register(Box::new(StubFactory));
        reg.register(Box::new(StubFactory));
        assert_eq!(reg.registered_names().count(), 1);
    }

    #[test]
    fn registry_build_via_convenience() {
        let mut reg = TestRunnerRegistry::new();
        reg.register(Box::new(StubFactory));
        let runner = reg.build("stub");
        assert!(runner.is_some());
        let missing = reg.build("nonexistent");
        assert!(missing.is_none());
    }

    /// `#[non_exhaustive]` discipline demo: downstream `match`
    /// blocks MUST include `_ => ...` for forward-compat with
    /// future variants (v5.1 `ByCoverage`). From inside
    /// `neurogrim-core` the enum is treated as exhaustive, so the
    /// `_` arm is currently unreachable — we silence the warning
    /// to keep the example pedagogically useful for downstream
    /// readers who SHOULD include the arm.
    #[test]
    fn test_selection_match_arm_pattern_compiles() {
        let selections = vec![
            TestSelection::All,
            TestSelection::Names(vec!["a".into()]),
            TestSelection::Packages(vec!["b".into()]),
        ];
        for s in selections {
            #[allow(unreachable_patterns)]
            let _kind: &'static str = match &s {
                TestSelection::All => "all",
                TestSelection::Names(_) => "names",
                TestSelection::Packages(_) => "packages",
                _ => "future-variant",
            };
        }
    }
}
