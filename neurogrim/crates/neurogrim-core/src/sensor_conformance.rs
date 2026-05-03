//! Conformance suite for [`crate::sensor::Sensor`] impls and their
//! factories (V5-MOD-2 Phase 5, 2026-05-02).
//!
//! Third-party crates that ship a `Sensor` impl use this suite to
//! verify they honor the contract every built-in sensor satisfies.
//! The suite is intentionally **factory-shaped** — takes a
//! `&dyn SensorFactory`, builds sensors from it, runs cross-cutting
//! tests that don't depend on sensor-type-specific configuration.
//! Per-sensor happy-path tests (e.g., "GitHealthSensor against a
//! real git repo") still live in each sensor's own module.
//!
//! # Usage
//!
//! ```no_run
//! use neurogrim_core::sensor_conformance::run_factory_conformance;
//! # use neurogrim_core::sensor::{Sensor, SensorFactory};
//! # use std::path::Path;
//! # use serde_json::Value;
//! # use async_trait::async_trait;
//! # struct MyFactory;
//! # impl SensorFactory for MyFactory {
//! #     fn name(&self) -> &'static str { "my-sensor" }
//! #     fn build(&self) -> Box<dyn Sensor> { todo!() }
//! # }
//! # async fn example() {
//! let factory = MyFactory;
//! // Caller provides a tempdir path the suite uses for skeletal-input
//! // calls. Sensors must safely return Ok(degraded envelope) or Err
//! // when the project root is empty — the suite tests that.
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
//! # What the suite covers (10 cross-cutting tests)
//!
//! Cross-cutting (5 — port of V5-MOD-1's
//! `scoring_source_conformance` pattern, adapted for the
//! `Sensor::analyze -> Result<Value>` shape vs.
//! `ScoringSource::load -> Option<CmdbData>`):
//!
//! 1. **`factory_name_non_empty`** — wire-name must exist.
//! 2. **`factory_name_stable_across_calls`** — multiple calls
//!    return the same string (no per-call generation).
//! 3. **`factory_build_repeatable`** — calling `build()` 10 times
//!    succeeds; no global-state corruption.
//! 4. **`analyze_with_skeletal_project_root`** — empty tempdir;
//!    sensor must return `Ok(...)` or `Err(...)` within 30 seconds.
//!    Either category is acceptable (silent-degrade vs. fallible
//!    contract); the test only catches panics + deadlocks.
//! 5. **`analyze_is_concurrent_safe`** — 50 parallel `analyze()`
//!    calls don't deadlock or panic (proves `Send + Sync` honored
//!    at runtime, not just at compile time).
//!
//! Sensor-specific (5 — new for V5-MOD-2):
//!
//! 6. **`analyze_output_matches_cmdb_envelope_v1`** — the output
//!    JSON conforms to the load-bearing constraints of
//!    `cmdb-envelope-v1.schema.json`: required fields present,
//!    types correct, timestamps RFC3339-parseable. Hand-rolled
//!    structural check (no `jsonschema` dep — keeps the suite
//!    light for third-party authors).
//! 7. **`analyze_score_in_range_0_to_100`** — the top-level `score`
//!    field is an integer in `[0, 100]` (defense-in-depth alongside
//!    the schema check).
//! 8. **`analyze_meta_block_well_formed`** — `meta.schema_version
//!    == "1"`, `meta.updated_by` is non-empty, `meta.updated_at` is
//!    RFC3339-parseable.
//! 9. **`analyze_completes_within_30_seconds`** — wall-clock guard
//!    against pathologically slow sensors. Sensors that hit this on
//!    skeletal input have a contract violation (file reads should
//!    be µs; network calls should fast-fail).
//! 10. **`analyze_is_idempotent_on_identical_input`** — repeated
//!     `analyze()` calls return the same `Ok`/`Err` category. Score
//!     values may drift (timestamps are real-time) so we don't
//!     assert deep equality, only category parity.
//!
//! Per-sensor happy-path tests stay in the sensor's own module
//! (`git_health_runs_on_initialized_repo`, etc.). The conformance
//! suite is the universal *negative-path discipline* that protects
//! third-party sensor authors from forgetting to handle errors safely.
//!
//! # Why hand-rolled schema checks instead of full JSON Schema validation
//!
//! Full validation via `jsonschema` would catch more constraints
//! (`additionalProperties: false`, nested type unions, etc.) but
//! adds a transitive dep that every third-party `Sensor` author's
//! conformance test would pull in. The structural checks below
//! cover the load-bearing parts of `cmdb-envelope-v1`; full
//! schema validation can be added later via a feature flag if
//! third-party authors demand it. For now, the
//! `cargo xtask schema-drift-check` pattern (V5-MOD-2 Phase 0)
//! catches drift between the vendored copy and the canonical
//! LSP-Brains copy at the workspace level.

use crate::sensor::SensorFactory;
use chrono::DateTime;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// Per-test outcome inside a [`ConformanceReport`].
///
/// Note: this type duplicates `scoring_source_conformance::TestResult`
/// (V5-MOD-1 Phase 5) — same shape, same purpose. A future v5.5
/// refactor could hoist both into a shared `crate::conformance`
/// module; for now the duplication is small and the parallel keeps
/// each suite self-contained.
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

/// Per-sensor wall-clock ceiling. Sensors that take longer than
/// this on skeletal input have a contract violation.
const ANALYZE_TIMEOUT: Duration = Duration::from_secs(30);

/// Run the full factory-conformance suite. Returns the
/// [`ConformanceReport`] with one entry per test.
///
/// `project_root` is the path the suite passes to
/// `sensor.analyze(...)` calls. Tests use this as a skeletal input
/// (no fixture files); a well-behaved sensor must return safely
/// regardless of what the project root contains, so the directory's
/// actual contents don't matter — pass any existing path (typically
/// `tempfile::tempdir()` from the caller's dev-deps).
pub async fn run_factory_conformance(
    factory: &dyn SensorFactory,
    project_root: &Path,
) -> ConformanceReport {
    let mut report = ConformanceReport::new();

    // Cross-cutting (5)
    report.add(test_factory_name_non_empty(factory));
    report.add(test_factory_name_stable_across_calls(factory));
    report.add(test_factory_build_repeatable(factory));
    report.add(test_analyze_with_skeletal_project_root(factory, project_root).await);
    report.add(test_analyze_is_concurrent_safe(factory, project_root).await);

    // Sensor-specific (5)
    report.add(test_analyze_output_matches_cmdb_envelope_v1(factory, project_root).await);
    report.add(test_analyze_score_in_range_0_to_100(factory, project_root).await);
    report.add(test_analyze_meta_block_well_formed(factory, project_root).await);
    report.add(test_analyze_completes_within_30_seconds(factory, project_root).await);
    report.add(test_analyze_is_idempotent_on_identical_input(factory, project_root).await);

    report
}

// ────────────────────────────────────────────────────────────────
// T1-T3: factory contract (sync, no IO)
// ────────────────────────────────────────────────────────────────

fn test_factory_name_non_empty(factory: &dyn SensorFactory) -> TestResult {
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
    factory: &dyn SensorFactory,
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

fn test_factory_build_repeatable(factory: &dyn SensorFactory) -> TestResult {
    // Catch panic from build() across 10 invocations.
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
// T4-T5: analyze contract (async, with IO)
// ────────────────────────────────────────────────────────────────

async fn test_analyze_with_skeletal_project_root(
    factory: &dyn SensorFactory,
    project_root: &Path,
) -> TestResult {
    let sensor = factory.build();
    let project_root_str = project_root.to_string_lossy();
    let outcome = tokio::time::timeout(
        ANALYZE_TIMEOUT,
        sensor.analyze(&project_root_str),
    )
    .await;

    match outcome {
        Err(_) => TestResult::fail(
            "analyze_with_skeletal_project_root",
            format!(
                "sensor.analyze() exceeded {}-second timeout on skeletal \
                 input; contract requires fast-fail (file reads should \
                 be µs; network should timeout in seconds, not 30s)",
                ANALYZE_TIMEOUT.as_secs()
            ),
        ),
        Ok(Ok(_)) | Ok(Err(_)) => {
            // Either is acceptable — silent-degrade sensors return
            // Ok(degraded), fallible sensors return Err. The test
            // only catches panics + deadlocks.
            TestResult::pass("analyze_with_skeletal_project_root")
        }
    }
}

async fn test_analyze_is_concurrent_safe(
    factory: &dyn SensorFactory,
    project_root: &Path,
) -> TestResult {
    let project_root = project_root.to_path_buf();

    let mut handles = Vec::with_capacity(50);
    for _ in 0..50 {
        let sensor = factory.build();
        let project_root = project_root.clone();
        handles.push(tokio::spawn(async move {
            let project_root_str = project_root.to_string_lossy().to_string();
            tokio::time::timeout(
                ANALYZE_TIMEOUT,
                sensor.analyze(&project_root_str),
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
            Ok(Ok(_)) => {} // Ok or Err is fine
        }
    }

    if panicked {
        TestResult::fail(
            "analyze_is_concurrent_safe",
            "one or more concurrent analyze() calls panicked",
        )
    } else if deadlocked {
        TestResult::fail(
            "analyze_is_concurrent_safe",
            "one or more concurrent analyze() calls deadlocked or timed out",
        )
    } else {
        TestResult::pass("analyze_is_concurrent_safe")
    }
}

// ────────────────────────────────────────────────────────────────
// T6-T9: envelope shape conformance
// ────────────────────────────────────────────────────────────────

/// Hand-rolled check for `cmdb-envelope-v1.schema.json` constraints.
/// Returns `Ok(())` if the envelope conforms, `Err(reason)` otherwise.
///
/// This duplicates a subset of the JSON Schema constraints checked
/// at write time by sensors. The duplication is intentional — the
/// conformance suite must validate INDEPENDENTLY of the producer's
/// own logic, since third-party producers may have bugs.
fn check_cmdb_envelope_v1(value: &Value) -> Result<(), String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "envelope must be a JSON object".to_string())?;

    // Required top-level fields: meta, score, updated_at.
    let meta = obj
        .get("meta")
        .ok_or_else(|| "missing required field `meta`".to_string())?;
    let score = obj
        .get("score")
        .ok_or_else(|| "missing required field `score`".to_string())?;
    let updated_at = obj
        .get("updated_at")
        .ok_or_else(|| "missing required field `updated_at`".to_string())?;

    // meta block.
    let meta_obj = meta
        .as_object()
        .ok_or_else(|| "`meta` must be a JSON object".to_string())?;
    let sv = meta_obj
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing or non-string `meta.schema_version`".to_string())?;
    if sv != "1" {
        return Err(format!(
            "meta.schema_version must be \"1\", got {sv:?}"
        ));
    }
    let ub = meta_obj
        .get("updated_by")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing or non-string `meta.updated_by`".to_string())?;
    if ub.is_empty() {
        return Err("meta.updated_by is empty".to_string());
    }
    let mua = meta_obj
        .get("updated_at")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing or non-string `meta.updated_at`".to_string())?;
    DateTime::parse_from_rfc3339(mua)
        .map_err(|e| format!("meta.updated_at not RFC3339: {e}"))?;

    // score: integer in [0, 100].
    let score_n = score.as_u64().ok_or_else(|| {
        format!("score must be non-negative integer, got {score:?}")
    })?;
    if score_n > 100 {
        return Err(format!("score {score_n} out of range [0, 100]"));
    }

    // updated_at (top-level): RFC3339-parseable.
    let ua = updated_at
        .as_str()
        .ok_or_else(|| "updated_at must be string".to_string())?;
    DateTime::parse_from_rfc3339(ua)
        .map_err(|e| format!("updated_at not RFC3339: {e}"))?;

    Ok(())
}

async fn test_analyze_output_matches_cmdb_envelope_v1(
    factory: &dyn SensorFactory,
    project_root: &Path,
) -> TestResult {
    let sensor = factory.build();
    let project_root_str = project_root.to_string_lossy();
    let outcome = tokio::time::timeout(
        ANALYZE_TIMEOUT,
        sensor.analyze(&project_root_str),
    )
    .await;

    match outcome {
        Err(_) => TestResult::fail(
            "analyze_output_matches_cmdb_envelope_v1",
            "analyze() timed out before producing output",
        ),
        Ok(Err(e)) => {
            // Fallible sensors that return Err on skeletal input are
            // acceptable — there's no envelope to validate.
            TestResult::pass_with_note(
                "analyze_output_matches_cmdb_envelope_v1",
                format!(
                    "sensor returned Err on skeletal input ({e}); \
                     envelope-shape check skipped"
                ),
            )
        }
        Ok(Ok(envelope)) => match check_cmdb_envelope_v1(&envelope) {
            Ok(()) => TestResult::pass("analyze_output_matches_cmdb_envelope_v1"),
            Err(reason) => TestResult::fail(
                "analyze_output_matches_cmdb_envelope_v1",
                format!(
                    "envelope failed cmdb-envelope-v1 structural check: \
                     {reason}; envelope = {envelope:?}"
                ),
            ),
        },
    }
}

async fn test_analyze_score_in_range_0_to_100(
    factory: &dyn SensorFactory,
    project_root: &Path,
) -> TestResult {
    let sensor = factory.build();
    let project_root_str = project_root.to_string_lossy();
    let outcome = tokio::time::timeout(
        ANALYZE_TIMEOUT,
        sensor.analyze(&project_root_str),
    )
    .await;

    match outcome {
        Err(_) => TestResult::fail(
            "analyze_score_in_range_0_to_100",
            "analyze() timed out before producing output",
        ),
        Ok(Err(_)) => {
            // Fallible sensors returning Err — no envelope, nothing
            // to score-check. Pass.
            TestResult::pass("analyze_score_in_range_0_to_100")
        }
        Ok(Ok(envelope)) => match envelope.get("score").and_then(Value::as_u64) {
            Some(n) if n <= 100 => {
                TestResult::pass("analyze_score_in_range_0_to_100")
            }
            Some(n) => TestResult::fail(
                "analyze_score_in_range_0_to_100",
                format!("score {n} out of range [0, 100]"),
            ),
            None => TestResult::fail(
                "analyze_score_in_range_0_to_100",
                format!(
                    "missing or non-integer `score` field; envelope = {envelope:?}"
                ),
            ),
        },
    }
}

async fn test_analyze_meta_block_well_formed(
    factory: &dyn SensorFactory,
    project_root: &Path,
) -> TestResult {
    let sensor = factory.build();
    let project_root_str = project_root.to_string_lossy();
    let outcome = tokio::time::timeout(
        ANALYZE_TIMEOUT,
        sensor.analyze(&project_root_str),
    )
    .await;

    match outcome {
        Err(_) => TestResult::fail(
            "analyze_meta_block_well_formed",
            "analyze() timed out before producing output",
        ),
        Ok(Err(_)) => TestResult::pass("analyze_meta_block_well_formed"),
        Ok(Ok(envelope)) => {
            let meta = match envelope.get("meta").and_then(Value::as_object) {
                Some(m) => m,
                None => {
                    return TestResult::fail(
                        "analyze_meta_block_well_formed",
                        "missing or non-object `meta` block",
                    );
                }
            };
            let sv = meta.get("schema_version").and_then(Value::as_str);
            if sv != Some("1") {
                return TestResult::fail(
                    "analyze_meta_block_well_formed",
                    format!("meta.schema_version expected \"1\", got {sv:?}"),
                );
            }
            let ub = meta.get("updated_by").and_then(Value::as_str);
            match ub {
                Some(s) if !s.is_empty() => {}
                _ => {
                    return TestResult::fail(
                        "analyze_meta_block_well_formed",
                        format!("meta.updated_by must be non-empty string, got {ub:?}"),
                    );
                }
            }
            let mua = meta.get("updated_at").and_then(Value::as_str);
            match mua {
                Some(s) => match DateTime::parse_from_rfc3339(s) {
                    Ok(_) => TestResult::pass("analyze_meta_block_well_formed"),
                    Err(e) => TestResult::fail(
                        "analyze_meta_block_well_formed",
                        format!("meta.updated_at not RFC3339: {e}"),
                    ),
                },
                None => TestResult::fail(
                    "analyze_meta_block_well_formed",
                    "meta.updated_at missing or non-string",
                ),
            }
        }
    }
}

async fn test_analyze_completes_within_30_seconds(
    factory: &dyn SensorFactory,
    project_root: &Path,
) -> TestResult {
    let sensor = factory.build();
    let project_root_str = project_root.to_string_lossy();
    let outcome = tokio::time::timeout(
        ANALYZE_TIMEOUT,
        sensor.analyze(&project_root_str),
    )
    .await;

    match outcome {
        Err(_) => TestResult::fail(
            "analyze_completes_within_30_seconds",
            format!(
                "analyze() exceeded {}-second timeout on skeletal input; \
                 contract requires fast-fail",
                ANALYZE_TIMEOUT.as_secs()
            ),
        ),
        Ok(_) => TestResult::pass("analyze_completes_within_30_seconds"),
    }
}

async fn test_analyze_is_idempotent_on_identical_input(
    factory: &dyn SensorFactory,
    project_root: &Path,
) -> TestResult {
    let sensor = factory.build();
    let project_root_str = project_root.to_string_lossy();
    let r1 = tokio::time::timeout(
        ANALYZE_TIMEOUT,
        sensor.analyze(&project_root_str),
    )
    .await;
    let r2 = tokio::time::timeout(
        ANALYZE_TIMEOUT,
        sensor.analyze(&project_root_str),
    )
    .await;

    match (r1, r2) {
        (Err(_), _) | (_, Err(_)) => TestResult::fail(
            "analyze_is_idempotent_on_identical_input",
            "analyze() timed out on idempotency check",
        ),
        (Ok(o1), Ok(o2)) => {
            // Compare Ok/Err category parity. Inner envelope fields
            // may have drift (timestamps, freshness) but the
            // category must be stable.
            if o1.is_ok() == o2.is_ok() {
                TestResult::pass("analyze_is_idempotent_on_identical_input")
            } else {
                TestResult::fail(
                    "analyze_is_idempotent_on_identical_input",
                    format!(
                        "analyze() returned different categories on \
                         identical input: first={}, second={}",
                        if o1.is_ok() { "Ok(...)" } else { "Err(...)" },
                        if o2.is_ok() { "Ok(...)" } else { "Err(...)" }
                    ),
                )
            }
        }
    }
}

impl TestResult {
    /// Same as `pass`, but stores a `detail` note explaining
    /// degenerate-but-acceptable cases (e.g., "fallible sensor
    /// returned Err on skeletal input — envelope check skipped").
    /// Useful when the test "passes" only because there's nothing
    /// to check.
    fn pass_with_note(name: &'static str, detail: impl Into<String>) -> Self {
        TestResult {
            name,
            passed: true,
            detail: Some(detail.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensor::{Sensor, SensorFactory};
    use async_trait::async_trait;
    use serde_json::json;
    use tempfile::TempDir;

    /// A test-only sensor that always returns a well-formed envelope.
    /// Verifies the conformance suite passes a happy-path impl.
    struct GoodSensor;
    #[async_trait]
    impl Sensor for GoodSensor {
        async fn analyze(&self, _project_root: &str) -> anyhow::Result<Value> {
            Ok(json!({
                "meta": {
                    "schema_version": "1",
                    "updated_at": "2026-05-02T00:00:00Z",
                    "updated_by": "good-sensor"
                },
                "score": 100,
                "updated_at": "2026-05-02T00:00:00Z",
                "findings": []
            }))
        }
    }
    struct GoodFactory;
    impl SensorFactory for GoodFactory {
        fn name(&self) -> &'static str {
            "good"
        }
        fn build(&self) -> Box<dyn Sensor> {
            Box::new(GoodSensor)
        }
    }

    /// A test-only sensor that returns malformed envelopes.
    /// Verifies the conformance suite catches the violations.
    struct BadSensor;
    #[async_trait]
    impl Sensor for BadSensor {
        async fn analyze(&self, _project_root: &str) -> anyhow::Result<Value> {
            // Score out of range; missing meta.updated_by.
            Ok(json!({
                "meta": {
                    "schema_version": "1",
                    "updated_at": "2026-05-02T00:00:00Z"
                },
                "score": 250,
                "updated_at": "2026-05-02T00:00:00Z"
            }))
        }
    }
    struct BadFactory;
    impl SensorFactory for BadFactory {
        fn name(&self) -> &'static str {
            "bad"
        }
        fn build(&self) -> Box<dyn Sensor> {
            Box::new(BadSensor)
        }
    }

    /// A test-only fallible sensor — always returns Err. The
    /// conformance suite should pass everything because envelope
    /// checks are skipped (Err means no envelope to validate).
    struct ErrSensor;
    #[async_trait]
    impl Sensor for ErrSensor {
        async fn analyze(&self, _project_root: &str) -> anyhow::Result<Value> {
            Err(anyhow::anyhow!("simulated failure"))
        }
    }
    struct ErrFactory;
    impl SensorFactory for ErrFactory {
        fn name(&self) -> &'static str {
            "err"
        }
        fn build(&self) -> Box<dyn Sensor> {
            Box::new(ErrSensor)
        }
    }

    #[tokio::test]
    async fn good_sensor_passes_full_suite() {
        let dir = TempDir::new().unwrap();
        let report = run_factory_conformance(&GoodFactory, dir.path()).await;
        assert!(
            report.all_passed(),
            "{}/{} tests failed for GoodFactory: {:#?}",
            report.failures().len(),
            report.total(),
            report.failures()
        );
        assert!(
            report.total() >= 10,
            "suite must have ≥10 tests; got {}",
            report.total()
        );
    }

    #[tokio::test]
    async fn bad_sensor_fails_envelope_checks() {
        let dir = TempDir::new().unwrap();
        let report = run_factory_conformance(&BadFactory, dir.path()).await;
        assert!(
            !report.all_passed(),
            "BadFactory must fail envelope-shape checks; got: {:#?}",
            report.results
        );
        // T6 (envelope shape) must fail.
        let t6 = report
            .results
            .iter()
            .find(|r| r.name == "analyze_output_matches_cmdb_envelope_v1")
            .expect("T6 must run");
        assert!(!t6.passed, "T6 must fail; got {t6:?}");
        // T7 (score range) must fail.
        let t7 = report
            .results
            .iter()
            .find(|r| r.name == "analyze_score_in_range_0_to_100")
            .expect("T7 must run");
        assert!(!t7.passed, "T7 must fail; got {t7:?}");
    }

    #[tokio::test]
    async fn fallible_sensor_passes_via_skipped_envelope_checks() {
        let dir = TempDir::new().unwrap();
        let report = run_factory_conformance(&ErrFactory, dir.path()).await;
        assert!(
            report.all_passed(),
            "ErrFactory should pass (envelope checks skip on Err); got: {:#?}",
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
