//! Shared `ConformanceReport` + `TestResult` types for the V5
//! conformance suites (V5-SDK-1 Phase 1.5, 2026-05-02 — Fork F1).
//!
//! V5-MOD-1 (`scoring_source_conformance`), V5-MOD-2
//! (`sensor_conformance`), and V5-MOD-3 (`queue_backend_conformance`)
//! each shipped their own copy of `ConformanceReport` + `TestResult`
//! with structurally-identical shapes. The duplication was
//! intentional during Theme B (each story self-contained for
//! independent shipping), with each module's rustdoc carrying a
//! TODO acknowledging the future hoist.
//!
//! V5-SDK-1's plan-critic round (2026-05-02) caught a real
//! consumer-side problem: third-party authors writing **multiple**
//! plugin types (e.g., a sensor + a queue backend) would import
//! `ConformanceReport` from two different SDK module paths. Even
//! though structurally identical, they're different **nominal
//! types** to the compiler; cross-assignment fails with
//! confusing `expected sensor_conformance::ConformanceReport,
//! found queue_backend_conformance::ConformanceReport` errors.
//!
//! Hoisting **before** SDK 0.1.0 ships is much cheaper than
//! after consumers depend on the duplicated names — V5-SDK-1
//! Fork F1 does it now.
//!
//! # Public API
//!
//! - [`TestResult`] — one test outcome (name + passed + optional
//!   detail).
//! - [`ConformanceReport`] — aggregate of test results with
//!   convenience accessors (`all_passed`, `failures`, `total`,
//!   `passed_count`).
//!
//! Both types are re-exported from each conformance module
//! (`scoring_source_conformance::*`, `sensor_conformance::*`,
//! `queue_backend_conformance::*`) so existing consumers' import
//! paths keep working. The SDK exposes them via
//! `neurogrim_sdk::conformance::*`.

/// Per-test outcome inside a [`ConformanceReport`].
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Stable test name (snake_case).
    pub name: &'static str,
    /// `true` if the test passed.
    pub passed: bool,
    /// On failure: a short string describing what went wrong.
    /// On a passing test that elects to record a note (e.g.,
    /// "skipped because the fallible sensor returned Err"),
    /// carries the note. Otherwise `None`.
    pub detail: Option<String>,
}

impl TestResult {
    /// Construct a passing result with no detail.
    pub fn pass(name: &'static str) -> Self {
        TestResult {
            name,
            passed: true,
            detail: None,
        }
    }

    /// Construct a failing result with a human-readable detail.
    pub fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        TestResult {
            name,
            passed: false,
            detail: Some(detail.into()),
        }
    }

    /// Construct a passing result with an explanatory note (e.g.,
    /// "test skipped — fallible sensor returned Err on skeletal
    /// input"). Useful for tests that "pass" only because there's
    /// nothing to check on this branch.
    pub fn pass_with_note(name: &'static str, detail: impl Into<String>) -> Self {
        TestResult {
            name,
            passed: true,
            detail: Some(detail.into()),
        }
    }
}

/// Aggregated outcome of running a conformance suite against one
/// factory.
#[derive(Debug, Clone, Default)]
pub struct ConformanceReport {
    pub results: Vec<TestResult>,
}

impl ConformanceReport {
    /// Construct an empty report.
    pub fn new() -> Self {
        ConformanceReport {
            results: Vec::new(),
        }
    }

    /// `true` iff every test passed.
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }

    /// Number of tests that passed.
    pub fn passed_count(&self) -> usize {
        self.results.iter().filter(|r| r.passed).count()
    }

    /// Total tests run.
    pub fn total(&self) -> usize {
        self.results.len()
    }

    /// Just the failures — useful for assertion messages.
    pub fn failures(&self) -> Vec<&TestResult> {
        self.results.iter().filter(|r| !r.passed).collect()
    }

    /// Append a test result. Used by conformance-suite runner
    /// functions to build the report incrementally.
    pub fn add(&mut self, result: TestResult) {
        self.results.push(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_methods_work() {
        let mut r = ConformanceReport::new();
        r.add(TestResult::pass("a"));
        r.add(TestResult::fail("b", "broke"));
        assert_eq!(r.total(), 2);
        assert_eq!(r.passed_count(), 1);
        assert_eq!(r.failures().len(), 1);
        assert!(!r.all_passed());

        let mut r2 = ConformanceReport::new();
        r2.add(TestResult::pass("a"));
        r2.add(TestResult::pass("b"));
        assert!(r2.all_passed());
    }

    #[test]
    fn pass_with_note_records_detail() {
        let r = TestResult::pass_with_note("x", "skipped because Y");
        assert!(r.passed);
        assert_eq!(r.detail.as_deref(), Some("skipped because Y"));
    }
}
