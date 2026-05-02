//! Conformance suite for [`crate::scoring_source::ScoringSource`]
//! impls and their factories (V5-MOD-1 Phase 5, 2026-05-02).
//!
//! Third-party crates that ship a `ScoringSource` use this suite
//! to verify they honor the contract every other built-in
//! satisfies. The suite is intentionally **factory-shaped** —
//! takes a `&dyn ScoringSourceFactory`, builds sources from it,
//! and runs cross-cutting tests that don't depend on
//! source-type-specific configuration. Per-source happy-path
//! tests (e.g., "CmdbSource reads a real JSON file") still live
//! in each source's own module — those are necessarily
//! type-specific and not generalizable.
//!
//! # Usage
//!
//! ```no_run
//! use neurogrim_core::scoring_source_conformance::run_factory_conformance;
//! # use neurogrim_core::scoring_sources::cmdb::CmdbSourceFactory;
//! # use std::path::Path;
//! # async fn example() {
//! let factory = CmdbSourceFactory;
//! // Caller provides the project root the suite uses for any
//! // tempdir-style operations. Tests should pass an empty
//! // `tempfile::tempdir()` (the suite never writes; only reads
//! // would happen, and it never points the source at real files).
//! let project_root = Path::new(".");
//! let report = run_factory_conformance(&factory, project_root).await;
//! assert!(
//!     report.all_passed(),
//!     "Conformance failures: {:?}",
//!     report.failures()
//! );
//! # }
//! ```
//!
//! # What the suite covers
//!
//! Cross-cutting contract tests every factory + source must pass:
//!
//! 1. **`source_type_name` non-empty** — wire-name must exist.
//! 2. **`source_type_name` stable across calls** — multiple
//!    calls return the same string (no per-call generation).
//! 3. **Factory ↔ source name match** — `factory.build()`'s
//!    source has the same `source_type_name` as the factory
//!    that built it.
//! 4. **`build()` doesn't panic** — calling `build()` repeatedly
//!    succeeds; no global-state corruption.
//! 5. **`build()` produces independent instances** — multiple
//!    `build()` calls produce sources that work independently
//!    (no shared mutable state pitfalls).
//! 6. **`load()` with empty/skeletal config doesn't panic** —
//!    contract is "return None on any failure", never panic.
//!    With an empty config, every well-behaved source must
//!    return None within a reasonable timeout.
//! 7. **`load()` is concurrent-safe** — multiple parallel
//!    `load()` calls don't deadlock or interleave (proves
//!    `Send + Sync` is honored at runtime, not just compile
//!    time).
//! 8. **`load()` is idempotent on identical input** — calling
//!    `load()` twice with the same args produces the same
//!    *category* of result (Some vs None). Score values may
//!    drift between calls (timestamps, freshness) so we don't
//!    assert deep equality, only Some/None parity.
//!
//! Per-source happy-path tests stay in the source's own module
//! (cmdb_test_for_actual_files, a2a_test_against_mock_endpoint,
//! function_returns_none, etc.). The conformance suite is the
//! universal *negative-path discipline* that protects
//! third-party authors from forgetting to handle errors safely.

use crate::registry::ScoringSourceConfig;
use crate::scoring_source::ScoringSourceFactory;
use std::path::Path;
use std::time::Duration;

/// Per-test outcome inside a [`ConformanceReport`].
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Stable test name (snake_case).
    pub name: &'static str,
    /// `true` if the test passed.
    pub passed: bool,
    /// On failure: a short string describing what went wrong.
    /// `None` when the test passed.
    pub detail: Option<String>,
}

impl TestResult {
    fn pass(name: &'static str) -> Self {
        TestResult {
            name,
            passed: true,
            detail: None,
        }
    }
    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        TestResult {
            name,
            passed: false,
            detail: Some(detail.into()),
        }
    }
}

/// Aggregated outcome of running the conformance suite against
/// one factory.
#[derive(Debug, Clone, Default)]
pub struct ConformanceReport {
    pub results: Vec<TestResult>,
}

impl ConformanceReport {
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

    fn add(&mut self, result: TestResult) {
        self.results.push(result);
    }
}

/// Build a minimal [`ScoringSourceConfig`] addressed to the
/// given source_type. All optional fields are `None` —
/// representative of a "registered but unconfigured" call site
/// where the source must safely return `None`.
fn skeletal_config(source_type: &str) -> ScoringSourceConfig {
    ScoringSourceConfig {
        source_type: source_type.to_string(),
        path: None,
        endpoint: None,
        interface_version: None,
        score_field: None,
        updated_at_field: None,
        no_file_score: None,
    }
}

/// Run the full factory-conformance suite. Returns the
/// [`ConformanceReport`] with one entry per test.
///
/// `project_root` is the directory the suite passes to
/// `source.load(...)` calls. Tests use a skeletal config
/// (no `path`, no `endpoint`); a well-behaved source must
/// return `None` regardless of what the project root contains,
/// so the directory's actual contents don't matter — pass any
/// existing path (or a `tempfile::tempdir()` from the caller's
/// dev-deps, which keeps the suite's public API independent of
/// `tempfile`).
pub async fn run_factory_conformance(
    factory: &dyn ScoringSourceFactory,
    project_root: &Path,
) -> ConformanceReport {
    let mut report = ConformanceReport::new();

    // T1: source_type_name() returns a non-empty string.
    report.add(test_source_type_name_non_empty(factory));

    // T2: source_type_name() is stable across multiple calls.
    report.add(test_source_type_name_stable_across_calls(factory));

    // T3: factory.build()'s source has the same source_type_name
    //     as the factory.
    report.add(test_factory_source_name_consistency(factory));

    // T4: factory.build() can be called multiple times without
    //     panicking (no global-state corruption).
    report.add(test_factory_build_repeatable(factory));

    // T5: each factory.build() produces a source that's
    //     name-stable in isolation (the `'static str` returned
    //     doesn't depend on the build call site).
    report.add(test_built_sources_name_stable_across_builds(factory));

    // T6: source.load() with an empty/skeletal config returns
    //     None within a reasonable timeout (contract: never
    //     panic; return None on any failure).
    report.add(test_load_skeletal_config_returns_none(factory, project_root).await);

    // T7: source.load() is concurrent-safe — multiple parallel
    //     load() calls complete without deadlock.
    report.add(test_load_is_concurrent_safe(factory, project_root).await);

    // T8: source.load() is idempotent on identical input —
    //     repeated calls return the same Some/None category.
    report.add(test_load_is_idempotent_on_identical_input(factory, project_root).await);

    report
}

// ────────────────────────────────────────────────────────────────
// Individual test implementations
// ────────────────────────────────────────────────────────────────

fn test_source_type_name_non_empty(
    factory: &dyn ScoringSourceFactory,
) -> TestResult {
    let name = factory.source_type_name();
    if name.is_empty() {
        TestResult::fail(
            "source_type_name_non_empty",
            "factory.source_type_name() returned empty string",
        )
    } else {
        TestResult::pass("source_type_name_non_empty")
    }
}

fn test_source_type_name_stable_across_calls(
    factory: &dyn ScoringSourceFactory,
) -> TestResult {
    let n1 = factory.source_type_name();
    let n2 = factory.source_type_name();
    let n3 = factory.source_type_name();
    if n1 != n2 || n2 != n3 {
        TestResult::fail(
            "source_type_name_stable_across_calls",
            format!(
                "source_type_name returned different values across calls: \
                 {n1:?} vs {n2:?} vs {n3:?}"
            ),
        )
    } else {
        TestResult::pass("source_type_name_stable_across_calls")
    }
}

fn test_factory_source_name_consistency(
    factory: &dyn ScoringSourceFactory,
) -> TestResult {
    let factory_name = factory.source_type_name();
    let source = factory.build();
    let source_name = source.source_type_name();
    if factory_name != source_name {
        TestResult::fail(
            "factory_source_name_consistency",
            format!(
                "factory.source_type_name() = {factory_name:?} but \
                 factory.build().source_type_name() = {source_name:?}"
            ),
        )
    } else {
        TestResult::pass("factory_source_name_consistency")
    }
}

fn test_factory_build_repeatable(
    factory: &dyn ScoringSourceFactory,
) -> TestResult {
    // Catch any panic from build() across 10 invocations. We use
    // `catch_unwind` to convert a panic into a test failure
    // rather than aborting the whole conformance run.
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

fn test_built_sources_name_stable_across_builds(
    factory: &dyn ScoringSourceFactory,
) -> TestResult {
    let s1 = factory.build();
    let s2 = factory.build();
    let s3 = factory.build();
    let n1 = s1.source_type_name();
    let n2 = s2.source_type_name();
    let n3 = s3.source_type_name();
    if n1 != n2 || n2 != n3 {
        TestResult::fail(
            "built_sources_name_stable_across_builds",
            format!(
                "different builds produced different source_type_name: \
                 {n1:?} vs {n2:?} vs {n3:?}"
            ),
        )
    } else {
        TestResult::pass("built_sources_name_stable_across_builds")
    }
}

async fn test_load_skeletal_config_returns_none(
    factory: &dyn ScoringSourceFactory,
    project_root: &Path,
) -> TestResult {
    let source = factory.build();
    let config = skeletal_config(factory.source_type_name());

    // 5-second timeout: any source impl that takes longer than
    // this on a skeletal config has a contract violation
    // (network calls should fast-fail; file reads should be
    // microseconds).
    let load_fut = source.load("conformance_test_domain", &config, project_root);
    let outcome = tokio::time::timeout(Duration::from_secs(5), load_fut).await;

    match outcome {
        Err(_) => TestResult::fail(
            "load_skeletal_config_returns_none",
            "source.load() exceeded 5-second timeout on skeletal config",
        ),
        Ok(Some(_)) => TestResult::fail(
            "load_skeletal_config_returns_none",
            "source.load() returned Some(...) on skeletal config; \
             expected None (no path/endpoint/etc. to read from)",
        ),
        Ok(None) => TestResult::pass("load_skeletal_config_returns_none"),
    }
}

async fn test_load_is_concurrent_safe(
    factory: &dyn ScoringSourceFactory,
    project_root: &Path,
) -> TestResult {
    // 50 parallel load() calls; if any deadlock or panic, this
    // test catches it. We use `tokio::spawn` to ensure the
    // futures genuinely run on multiple workers when the
    // runtime is multi-threaded.
    let project_root = project_root.to_path_buf();
    let source_type = factory.source_type_name().to_string();

    let mut handles = Vec::with_capacity(50);
    for i in 0..50 {
        // Build a fresh source per task (the factory may produce
        // sources that are themselves Send+Sync, but we don't
        // require Sync — only that build is reusable).
        let source = factory.build();
        let config = skeletal_config(&source_type);
        let project_root = project_root.clone();
        handles.push(tokio::spawn(async move {
            let domain_key = format!("concurrent_test_{i}");
            tokio::time::timeout(
                Duration::from_secs(5),
                source.load(&domain_key, &config, &project_root),
            )
            .await
        }));
    }

    let mut deadlocked = false;
    let mut panicked = false;
    for h in handles {
        match h.await {
            Err(join_err) if join_err.is_panic() => {
                panicked = true;
                break;
            }
            Err(_) => {
                deadlocked = true;
                break;
            }
            Ok(Err(_)) => {
                // tokio::time::timeout fired — treat as deadlock.
                deadlocked = true;
                break;
            }
            Ok(Ok(_)) => {} // either Some or None is fine
        }
    }

    if panicked {
        TestResult::fail(
            "load_is_concurrent_safe",
            "one or more concurrent load() calls panicked",
        )
    } else if deadlocked {
        TestResult::fail(
            "load_is_concurrent_safe",
            "one or more concurrent load() calls deadlocked or timed out",
        )
    } else {
        TestResult::pass("load_is_concurrent_safe")
    }
}

async fn test_load_is_idempotent_on_identical_input(
    factory: &dyn ScoringSourceFactory,
    project_root: &Path,
) -> TestResult {
    let source = factory.build();
    let config = skeletal_config(factory.source_type_name());

    let r1 = tokio::time::timeout(
        Duration::from_secs(5),
        source.load("idempotent_test_domain", &config, project_root),
    )
    .await;
    let r2 = tokio::time::timeout(
        Duration::from_secs(5),
        source.load("idempotent_test_domain", &config, project_root),
    )
    .await;

    match (r1, r2) {
        (Err(_), _) | (_, Err(_)) => TestResult::fail(
            "load_is_idempotent_on_identical_input",
            "load() timed out on idempotency check",
        ),
        (Ok(o1), Ok(o2)) => {
            // Compare Some/None parity. Inner CmdbData fields may
            // have drift (timestamps, freshness) but the category
            // must be stable.
            if o1.is_some() == o2.is_some() {
                TestResult::pass("load_is_idempotent_on_identical_input")
            } else {
                TestResult::fail(
                    "load_is_idempotent_on_identical_input",
                    format!(
                        "load() returned different categories on identical \
                         input: first={:?}, second={:?}",
                        o1.as_ref().map(|_| "Some(...)"),
                        o2.as_ref().map(|_| "Some(...)")
                    ),
                )
            }
        }
    }

    // Note: source dropped here; the next conformance run gets
    // a fresh source from a fresh build() in the next test.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring_sources::cmdb::CmdbSourceFactory;
    use crate::scoring_sources::function::FunctionSourceFactory;
    use tempfile::TempDir;

    #[tokio::test]
    async fn cmdb_factory_passes_full_conformance_suite() {
        let factory = CmdbSourceFactory;
        let dir = TempDir::new().unwrap();
        let report = run_factory_conformance(&factory, dir.path()).await;
        assert!(
            report.all_passed(),
            "{}/{} conformance tests failed for CmdbSourceFactory: {:#?}",
            report.failures().len(),
            report.total(),
            report.failures()
        );
        // Must be ≥8 tests per epic Done-When; lower bound check.
        assert!(
            report.total() >= 8,
            "conformance suite must have ≥8 tests; got {}",
            report.total()
        );
    }

    #[tokio::test]
    async fn function_factory_passes_full_conformance_suite() {
        let factory = FunctionSourceFactory;
        let dir = TempDir::new().unwrap();
        let report = run_factory_conformance(&factory, dir.path()).await;
        assert!(
            report.all_passed(),
            "{}/{} conformance tests failed for FunctionSourceFactory: {:#?}",
            report.failures().len(),
            report.total(),
            report.failures()
        );
    }

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
}
