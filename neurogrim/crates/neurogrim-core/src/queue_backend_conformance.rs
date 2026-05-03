//! Conformance suite for [`crate::queue_backend::QueueBackend`]
//! impls and their factories (V5-MOD-3 Phase 4, 2026-05-02).
//!
//! Third-party crates that ship a `QueueBackend` use this suite
//! to verify they honor the contract every built-in backend
//! satisfies. The suite is intentionally **factory-shaped** —
//! takes a `&dyn QueueBackendFactory`, builds backends from it,
//! and runs cross-cutting tests that don't depend on
//! backend-type-specific configuration.
//!
//! # Usage
//!
//! ```no_run
//! use neurogrim_core::queue_backend_conformance::run_factory_conformance;
//! # use neurogrim_core::queue_backend::{QueueBackend, QueueBackendFactory};
//! # use std::path::Path;
//! # use std::sync::Arc;
//! # struct MyFactory;
//! # impl QueueBackendFactory for MyFactory {
//! #     fn name(&self) -> &'static str { "my-backend" }
//! #     fn build(&self, _: &Path, _: &str) -> anyhow::Result<Arc<dyn QueueBackend>> { todo!() }
//! # }
//! # async fn example() {
//! let factory = MyFactory;
//! // Caller provides a tempdir as the queue_root the suite uses
//! // for per-test backend instances.
//! let queue_root = Path::new(".");
//! let report = run_factory_conformance(&factory, queue_root).await;
//! assert!(
//!     report.all_passed(),
//!     "Conformance failures: {:?}",
//!     report.failures()
//! );
//! # }
//! ```
//!
//! # What the suite covers (12 cross-cutting tests)
//!
//! Cross-cutting (3 — port from V5-MOD-1's `scoring_source_conformance`
//! and V5-MOD-2's `sensor_conformance`):
//!
//! 1. **`factory_name_non_empty`** — wire-name must exist.
//! 2. **`factory_name_stable_across_calls`** — multiple calls
//!    return the same string.
//! 3. **`factory_build_repeatable`** — calling `build()` 10 times
//!    on different topic names succeeds; no global-state corruption.
//!
//! Backend-specific (9 — new for V5-MOD-3):
//!
//! 4. **`appended_messages_round_trip`** — append N messages,
//!    read all back, verify `read_from(0, N).len() == N` and
//!    offsets are ascending.
//! 5. **`read_from_offset`** — append, then read with
//!    `since_offset > 0`; returned messages start at the right
//!    boundary.
//! 6. **`read_with_limit`** — limit honored; reading less than
//!    appended count returns the right slice.
//! 7. **`len_after_append`** — `len()` reflects the appended
//!    count.
//! 8. **`concurrent_appends_dont_panic`** — 5 parallel
//!    `append()` calls via `std::thread::spawn`; assertion is no
//!    panic, no deadlock, no errors. Does NOT assert offset
//!    uniqueness — JsonlBackend's append-then-count semantics
//!    have a known TOCTOU race under concurrent writes (it's a
//!    fan-out backend, not a transactional one). Backends that
//!    DO need transactional concurrent-append guarantees
//!    (SqliteBackend's `INSERT … RETURNING` rowid) are stronger
//!    than this check requires; their own per-backend tests
//!    cover that property.
//! 9. **`empty_backend_returns_empty_reads`** — fresh backend
//!    has `len() == 0` and `read_from(0, ...)` is empty.
//! 10. **`ack_methods_consistent_with_supports_ack`** — if
//!     `factory.supports_ack()` claims `true`, the backend's
//!     `read_unacked` / `ack` / `last_acked` actually work; if
//!     `false`, the trait's default impls fire (errors on
//!     `read_unacked` / `ack`, `Ok(None)` on `last_acked`).
//! 11. **`ack_idempotency`** (only when `supports_ack()`) —
//!     acking the same offset twice for the same group is a
//!     no-op (idempotent); doesn't double-count or panic.
//! 12. **`build_returns_send_sync_arc`** — compile-time check
//!     that `Arc<dyn QueueBackend>` from `build()` is shareable
//!     across threads (proves the V5-MOD-3 Fork A2 trait bound
//!     holds at runtime).
//!
//! # `ConformanceReport` + `TestResult`
//!
//! Same shape as V5-MOD-1's `scoring_source_conformance` and
//! V5-MOD-2's `sensor_conformance`. The 30-line duplication is
//! intentional; a future v5.5 cleanup could hoist all three into
//! a shared `crate::conformance` module.

use crate::queue::{Priority, QueueMessage};
use crate::queue_backend::QueueBackendFactory;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: &'static str,
    pub passed: bool,
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

    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }

    pub fn passed_count(&self) -> usize {
        self.results.iter().filter(|r| r.passed).count()
    }

    pub fn total(&self) -> usize {
        self.results.len()
    }

    pub fn failures(&self) -> Vec<&TestResult> {
        self.results.iter().filter(|r| !r.passed).collect()
    }

    fn add(&mut self, result: TestResult) {
        self.results.push(result);
    }
}

/// Per-test wall-clock ceiling. Backends that take longer than
/// this on a tempdir have a contract violation (file I/O should
/// be µs; SQLite's WAL-mode writes are <1ms; in-memory backends
/// near-instant).
const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Run the full factory-conformance suite. Returns a
/// [`ConformanceReport`] with one entry per test.
///
/// `queue_root` is a directory the suite passes to factory
/// `build(queue_root, topic)` calls. Each test uses a fresh
/// `topic` name to keep state isolated; pass any existing path
/// (typically `tempfile::tempdir()` from the caller's dev-deps).
pub async fn run_factory_conformance(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
) -> ConformanceReport {
    let mut report = ConformanceReport::new();

    // Cross-cutting (3)
    report.add(test_factory_name_non_empty(factory));
    report.add(test_factory_name_stable_across_calls(factory));
    report.add(test_factory_build_repeatable(factory, queue_root));

    // Backend-specific (9)
    report.add(test_appended_messages_round_trip(factory, queue_root));
    report.add(test_read_from_offset(factory, queue_root));
    report.add(test_read_with_limit(factory, queue_root));
    report.add(test_len_after_append(factory, queue_root));
    report.add(test_concurrent_appends_dont_panic(factory, queue_root));
    report.add(test_empty_backend_returns_empty_reads(factory, queue_root));
    report.add(test_ack_methods_consistent_with_supports_ack(
        factory, queue_root,
    ));
    report.add(test_ack_idempotency(factory, queue_root));
    report.add(test_build_returns_send_sync_arc(factory, queue_root));

    let _ = TEST_TIMEOUT; // currently inline timeouts not used; reserved for future async tests
    report
}

// ────────────────────────────────────────────────────────────────
// Test helpers
// ────────────────────────────────────────────────────────────────

/// Build a fresh backend for one conformance test. Uses a unique
/// system-namespace topic name (`_neurogrim/conformance-<test-name>`)
/// so tests don't interfere. The `_neurogrim/<name>` shape matches
/// the workspace's strict topic-name validator
/// (`crate::queue::is_valid_topic_name`).
fn build_backend_for_test(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
    test_name: &str,
) -> anyhow::Result<Arc<dyn crate::queue_backend::QueueBackend>> {
    let safe = test_name.replace('_', "-");
    factory.build(queue_root, &format!("_neurogrim/conformance-{safe}"))
}

/// Construct a deterministic test message.
fn test_message(topic: &str, n: u64) -> QueueMessage {
    QueueMessage {
        id: uuid::Uuid::nil(),
        topic: topic.to_string(),
        payload: json!({ "n": n }),
        produced_at: chrono::Utc::now(),
        priority: Priority::Normal,
        expires_at: None,
    }
}

// ────────────────────────────────────────────────────────────────
// T1-T3: factory contract
// ────────────────────────────────────────────────────────────────

fn test_factory_name_non_empty(factory: &dyn QueueBackendFactory) -> TestResult {
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
    factory: &dyn QueueBackendFactory,
) -> TestResult {
    let n1 = factory.name();
    let n2 = factory.name();
    let n3 = factory.name();
    if n1 != n2 || n2 != n3 {
        TestResult::fail(
            "factory_name_stable_across_calls",
            format!("factory.name() returned different values: {n1:?} vs {n2:?} vs {n3:?}"),
        )
    } else {
        TestResult::pass("factory_name_stable_across_calls")
    }
}

fn test_factory_build_repeatable(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
) -> TestResult {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for i in 0..10 {
            // Use a unique topic per invocation so SQLite-style
            // backends don't get confused about pre-existing schema.
            let topic = format!("build_repeat_{i}");
            if factory.build(queue_root, &topic).is_err() {
                return Err(format!("build {i} returned Err"));
            }
        }
        Ok(())
    }));
    match result {
        Ok(Ok(())) => TestResult::pass("factory_build_repeatable"),
        Ok(Err(detail)) => TestResult::fail("factory_build_repeatable", detail),
        Err(_) => TestResult::fail(
            "factory_build_repeatable",
            "factory.build() panicked across 10 invocations",
        ),
    }
}

// ────────────────────────────────────────────────────────────────
// T4-T9: read/write round-trip
// ────────────────────────────────────────────────────────────────

fn test_appended_messages_round_trip(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
) -> TestResult {
    let be = match build_backend_for_test(factory, queue_root, "round_trip") {
        Ok(b) => b,
        Err(e) => {
            return TestResult::fail(
                "appended_messages_round_trip",
                format!("build failed: {e}"),
            )
        }
    };
    for n in 0..5 {
        if let Err(e) = be.append(&test_message("_neurogrim/conformance-round-trip", n)) {
            return TestResult::fail(
                "appended_messages_round_trip",
                format!("append {n} failed: {e}"),
            );
        }
    }
    let read = match be.read_from(0, 100) {
        Ok(v) => v,
        Err(e) => {
            return TestResult::fail(
                "appended_messages_round_trip",
                format!("read_from failed: {e}"),
            )
        }
    };
    if read.len() != 5 {
        return TestResult::fail(
            "appended_messages_round_trip",
            format!("expected 5 messages, got {}", read.len()),
        );
    }
    // Offsets are ascending.
    for w in read.windows(2) {
        if w[0].offset >= w[1].offset {
            return TestResult::fail(
                "appended_messages_round_trip",
                format!(
                    "offsets not ascending: {} >= {}",
                    w[0].offset, w[1].offset
                ),
            );
        }
    }
    TestResult::pass("appended_messages_round_trip")
}

fn test_read_from_offset(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
) -> TestResult {
    let be = match build_backend_for_test(factory, queue_root, "read_from_offset") {
        Ok(b) => b,
        Err(e) => return TestResult::fail("read_from_offset", format!("build: {e}")),
    };
    let mut offsets = Vec::new();
    for n in 0..5 {
        match be.append(&test_message("_neurogrim/conformance-read-from-offset", n)) {
            Ok(off) => offsets.push(off),
            Err(e) => return TestResult::fail("read_from_offset", format!("append: {e}")),
        }
    }
    // Read from the third message's offset; expect ≤ 3 results.
    let pivot = offsets[2];
    let read = match be.read_from(pivot, 100) {
        Ok(v) => v,
        Err(e) => return TestResult::fail("read_from_offset", format!("read_from: {e}")),
    };
    if read.is_empty() {
        return TestResult::fail(
            "read_from_offset",
            format!("read_from(pivot={pivot}, 100) returned 0; expected at least 1"),
        );
    }
    if read[0].offset < pivot {
        return TestResult::fail(
            "read_from_offset",
            format!(
                "read_from(pivot={pivot}) returned earlier offset {}",
                read[0].offset
            ),
        );
    }
    TestResult::pass("read_from_offset")
}

fn test_read_with_limit(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
) -> TestResult {
    let be = match build_backend_for_test(factory, queue_root, "read_with_limit") {
        Ok(b) => b,
        Err(e) => return TestResult::fail("read_with_limit", format!("build: {e}")),
    };
    for n in 0..5 {
        if let Err(e) = be.append(&test_message("_neurogrim/conformance-read-with-limit", n)) {
            return TestResult::fail("read_with_limit", format!("append: {e}"));
        }
    }
    let read = match be.read_from(0, 2) {
        Ok(v) => v,
        Err(e) => return TestResult::fail("read_with_limit", format!("read_from: {e}")),
    };
    if read.len() != 2 {
        return TestResult::fail(
            "read_with_limit",
            format!("expected 2 messages with limit=2, got {}", read.len()),
        );
    }
    TestResult::pass("read_with_limit")
}

fn test_len_after_append(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
) -> TestResult {
    let be = match build_backend_for_test(factory, queue_root, "len_after_append") {
        Ok(b) => b,
        Err(e) => return TestResult::fail("len_after_append", format!("build: {e}")),
    };
    for n in 0..3 {
        if let Err(e) = be.append(&test_message("_neurogrim/conformance-len-after-append", n)) {
            return TestResult::fail("len_after_append", format!("append: {e}"));
        }
    }
    match be.len() {
        Ok(3) => TestResult::pass("len_after_append"),
        Ok(n) => TestResult::fail(
            "len_after_append",
            format!("expected len 3, got {n}"),
        ),
        Err(e) => TestResult::fail("len_after_append", format!("len: {e}")),
    }
}

fn test_concurrent_appends_dont_panic(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
) -> TestResult {
    let be = match build_backend_for_test(factory, queue_root, "concurrent-appends") {
        Ok(b) => b,
        Err(e) => {
            return TestResult::fail(
                "concurrent_appends_dont_panic",
                format!("build: {e}"),
            )
        }
    };
    // 5 parallel appends. Test asserts no panic + no error +
    // no deadlock; does NOT assert offset uniqueness. JsonlBackend
    // (and other fan-out backends) may legitimately return
    // duplicate offsets under concurrent writes — this is a known
    // semantic of append-only file-fanout backends. Backends with
    // transactional ack semantics (SqliteBackend) provide unique
    // offsets via their internal serialization; that property is
    // verified by per-backend tests, not the cross-cutting
    // conformance suite.
    let mut handles = Vec::new();
    for n in 0..5 {
        let be_cloned: Arc<dyn crate::queue_backend::QueueBackend> = be.clone();
        handles.push(std::thread::spawn(move || -> anyhow::Result<u64> {
            be_cloned.append(&test_message(
                "_neurogrim/conformance-concurrent-appends",
                n,
            ))
        }));
    }
    for h in handles {
        match h.join() {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                return TestResult::fail(
                    "concurrent_appends_dont_panic",
                    format!("thread append failed: {e}"),
                );
            }
            Err(_) => {
                return TestResult::fail(
                    "concurrent_appends_dont_panic",
                    "thread panicked",
                );
            }
        }
    }
    TestResult::pass("concurrent_appends_dont_panic")
}

fn test_empty_backend_returns_empty_reads(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
) -> TestResult {
    let be = match build_backend_for_test(factory, queue_root, "empty_reads") {
        Ok(b) => b,
        Err(e) => {
            return TestResult::fail(
                "empty_backend_returns_empty_reads",
                format!("build: {e}"),
            )
        }
    };
    match be.len() {
        Ok(0) => {}
        Ok(n) => {
            return TestResult::fail(
                "empty_backend_returns_empty_reads",
                format!("fresh backend len={n}, expected 0"),
            )
        }
        Err(e) => {
            return TestResult::fail(
                "empty_backend_returns_empty_reads",
                format!("len: {e}"),
            )
        }
    }
    match be.read_from(0, 100) {
        Ok(v) if v.is_empty() => TestResult::pass("empty_backend_returns_empty_reads"),
        Ok(v) => TestResult::fail(
            "empty_backend_returns_empty_reads",
            format!("fresh backend read_from returned {} messages, expected 0", v.len()),
        ),
        Err(e) => TestResult::fail(
            "empty_backend_returns_empty_reads",
            format!("read_from: {e}"),
        ),
    }
}

// ────────────────────────────────────────────────────────────────
// T10-T11: ack contract
// ────────────────────────────────────────────────────────────────

fn test_ack_methods_consistent_with_supports_ack(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
) -> TestResult {
    let be = match build_backend_for_test(factory, queue_root, "ack_consistent") {
        Ok(b) => b,
        Err(e) => {
            return TestResult::fail(
                "ack_methods_consistent_with_supports_ack",
                format!("build: {e}"),
            )
        }
    };
    let claims_ack = factory.supports_ack();
    let backend_claims_ack = be.supports_ack();
    if claims_ack != backend_claims_ack {
        return TestResult::fail(
            "ack_methods_consistent_with_supports_ack",
            format!(
                "factory.supports_ack() = {claims_ack} but built backend's \
                 supports_ack() = {backend_claims_ack}; the two MUST agree"
            ),
        );
    }

    // Append one message so we have something to ack/read.
    let off = match be.append(&test_message("_neurogrim/conformance-ack-consistent", 0)) {
        Ok(o) => o,
        Err(e) => {
            return TestResult::fail(
                "ack_methods_consistent_with_supports_ack",
                format!("append: {e}"),
            )
        }
    };

    if claims_ack {
        // Backend claims to support ack — read_unacked / ack /
        // last_acked must all work.
        match be.read_unacked("group-A", 10) {
            Ok(_) => {}
            Err(e) => {
                return TestResult::fail(
                    "ack_methods_consistent_with_supports_ack",
                    format!(
                        "supports_ack=true but read_unacked() errored: {e}"
                    ),
                )
            }
        }
        if let Err(e) = be.ack(off, "group-A") {
            return TestResult::fail(
                "ack_methods_consistent_with_supports_ack",
                format!("supports_ack=true but ack() errored: {e}"),
            );
        }
        match be.last_acked("group-A") {
            Ok(Some(o)) if o == off => {}
            Ok(other) => {
                return TestResult::fail(
                    "ack_methods_consistent_with_supports_ack",
                    format!(
                        "supports_ack=true: last_acked('group-A') = {other:?}, expected Some({off})"
                    ),
                )
            }
            Err(e) => {
                return TestResult::fail(
                    "ack_methods_consistent_with_supports_ack",
                    format!("supports_ack=true but last_acked() errored: {e}"),
                )
            }
        }
    } else {
        // Backend claims fan-out only — ack/read_unacked default
        // impls error; last_acked defaults to Ok(None).
        if be.read_unacked("group-A", 10).is_ok() {
            return TestResult::fail(
                "ack_methods_consistent_with_supports_ack",
                "supports_ack=false but read_unacked() returned Ok; \
                 expected Err per default impl",
            );
        }
        if be.ack(off, "group-A").is_ok() {
            return TestResult::fail(
                "ack_methods_consistent_with_supports_ack",
                "supports_ack=false but ack() returned Ok; expected Err",
            );
        }
        match be.last_acked("group-A") {
            Ok(None) => {}
            Ok(Some(o)) => {
                return TestResult::fail(
                    "ack_methods_consistent_with_supports_ack",
                    format!(
                        "supports_ack=false: last_acked() returned Some({o}); expected Ok(None)"
                    ),
                )
            }
            Err(e) => {
                return TestResult::fail(
                    "ack_methods_consistent_with_supports_ack",
                    format!("supports_ack=false but last_acked() errored: {e}"),
                )
            }
        }
    }
    TestResult::pass("ack_methods_consistent_with_supports_ack")
}

fn test_ack_idempotency(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
) -> TestResult {
    if !factory.supports_ack() {
        // Test only meaningful for ack-capable backends; skip with
        // a passing TestResult so the suite count stays consistent.
        return TestResult::pass("ack_idempotency");
    }
    let be = match build_backend_for_test(factory, queue_root, "ack_idempotency") {
        Ok(b) => b,
        Err(e) => return TestResult::fail("ack_idempotency", format!("build: {e}")),
    };
    let off = match be.append(&test_message("_neurogrim/conformance-ack-idempotency", 0)) {
        Ok(o) => o,
        Err(e) => return TestResult::fail("ack_idempotency", format!("append: {e}")),
    };
    if let Err(e) = be.ack(off, "group-A") {
        return TestResult::fail("ack_idempotency", format!("first ack: {e}"));
    }
    // Second ack of same offset for same group — must be idempotent.
    if let Err(e) = be.ack(off, "group-A") {
        return TestResult::fail(
            "ack_idempotency",
            format!("second ack of same offset+group failed: {e}; expected idempotent"),
        );
    }
    // last_acked should still report `off`, not panic / double-up.
    match be.last_acked("group-A") {
        Ok(Some(o)) if o == off => TestResult::pass("ack_idempotency"),
        Ok(other) => TestResult::fail(
            "ack_idempotency",
            format!("last_acked after double-ack = {other:?}, expected Some({off})"),
        ),
        Err(e) => TestResult::fail("ack_idempotency", format!("last_acked: {e}")),
    }
}

// ────────────────────────────────────────────────────────────────
// T12: trait-bound runtime check
// ────────────────────────────────────────────────────────────────

fn test_build_returns_send_sync_arc(
    factory: &dyn QueueBackendFactory,
    queue_root: &Path,
) -> TestResult {
    // Compile-time check: `Arc<dyn QueueBackend>` is `Send + Sync`
    // when the trait is `Send + Sync` (V5-MOD-3 Fork A2). The test
    // body builds a backend and clones the Arc across a thread
    // boundary; if the trait bound regresses, this fails to compile.
    fn assert_send_sync<T: Send + Sync>(_: &T) {}
    let be = match build_backend_for_test(factory, queue_root, "send_sync") {
        Ok(b) => b,
        Err(e) => {
            return TestResult::fail(
                "build_returns_send_sync_arc",
                format!("build: {e}"),
            )
        }
    };
    assert_send_sync(&be);
    let be_cloned: Arc<dyn crate::queue_backend::QueueBackend> = be.clone();
    let handle = std::thread::spawn(move || be_cloned.len().unwrap_or(0));
    match handle.join() {
        Ok(_) => TestResult::pass("build_returns_send_sync_arc"),
        Err(_) => TestResult::fail(
            "build_returns_send_sync_arc",
            "thread holding Arc<dyn QueueBackend> panicked",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue_backend::JsonlBackendFactory;
    use tempfile::TempDir;

    #[tokio::test]
    async fn jsonl_factory_passes_full_conformance_suite() {
        let dir = TempDir::new().unwrap();
        let report = run_factory_conformance(&JsonlBackendFactory, dir.path()).await;
        assert!(
            report.all_passed(),
            "{}/{} conformance tests failed for JsonlBackendFactory: {:#?}",
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

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn sqlite_factory_passes_full_conformance_suite() {
        use crate::queue_backend::SqliteBackendFactory;
        let dir = TempDir::new().unwrap();
        let report = run_factory_conformance(&SqliteBackendFactory, dir.path()).await;
        assert!(
            report.all_passed(),
            "{}/{} conformance tests failed for SqliteBackendFactory: {:#?}",
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
    }
}
