//! # `neurogrim-sdk` — stable contract surface for plugin authors
//!
//! V5-SDK-1 (2026-05-02). This crate is a **thin re-export layer**
//! over the trait + factory + registry contracts shipped by
//! NeuroGrim's V5 modular conversions (Theme B), plus the adjacent
//! `Transport` (A2A) and `SecretBackend` (encrypted-secrets) traits.
//!
//! Versioned **independently** from `neurogrim-core` per the V5-SDK
//! epic. Initial version `0.1.0`. **Pre-1.0**: explicit allowance
//! for trait-shape changes between minor bumps if Theme B reveals a
//! flaw post-ship. Promotion to `1.0` requires (a) ≥6 weeks of soak
//! post-Theme-B-completion (Theme B closed 2026-05-02 → earliest
//! `1.0` ~2026-06-13), (b) at least one external adopter confirming
//! the surface works for their use case.
//!
//! ## Architecture
//!
//! NeuroGrim's plugin extensibility lives behind five trait surfaces.
//! All five are object-safe (`Box<dyn>` / `Arc<dyn>` dispatched in
//! production), `Send + Sync`, and pair with a factory + registry
//! pattern for runtime registration.
//!
//! | Trait | Factory | Registry | Source epic |
//! |---|---|---|---|
//! | [`ScoringSource`] | [`ScoringSourceFactory`] | [`ScoringSourceRegistry`] | V5-MOD-1 |
//! | [`Sensor`] | [`SensorFactory`] | [`SensorRegistry`] | V5-MOD-2 |
//! | [`QueueBackend`] | [`QueueBackendFactory`] | [`QueueBackendRegistry`] | V5-MOD-3 |
//! | [`TestRunner`] | [`TestRunnerFactory`] | [`TestRunnerRegistry`] | V5-FOUND-4 |
//! | [`Transport`] | (none — directly registered) | (none) | v3.x A2A |
//! | [`SecretBackend`] | (none — directly registered) | (none) | v4.2 S14 |
//!
//! ## Conformance suites
//!
//! Each Theme B trait ships a published conformance suite —
//! cross-cutting tests every conformant impl must pass. Third-party
//! authors copy the suite invocation verbatim into their own
//! crate's `tests/conformance.rs` and have a verifiable
//! "passes the same contract as built-ins" claim.
//!
//! **Available behind the `conformance` feature** (V5-SDK-2 partial
//! Phase 2 — 2026-05-04). The suites use `tokio::spawn` +
//! `tokio::time::timeout` in their public API, so they're gated to
//! keep tokio out of production binaries that don't run them. Add
//! to your crate's `[dev-dependencies]`:
//!
//! ```toml
//! [dev-dependencies]
//! neurogrim-sdk = { version = "0.1", features = ["conformance"] }
//! ```
//!
//! - [`scoring_source_conformance`]
//! - [`sensor_conformance`]
//! - [`queue_backend_conformance`]
//! - [`test_runner_conformance`] (V5-FOUND-4 / V5-SDK-2 close-out — 4-test suite)
//!
//! All four suites share the [`conformance::ConformanceReport`] +
//! [`conformance::TestResult`] types (V5-SDK-1 Phase 1.5 hoist —
//! consumers writing multiple plugin types see a single nominal
//! `ConformanceReport`, not four structurally-identical-but-
//! incompatible copies).
//!
//! ## Authoring guides
//!
//! Three walkthroughs covering the most common third-party plugin
//! patterns. Each shows the minimum-viable impl + conformance
//! test wiring + common pitfalls.
//!
//! ### Writing a conformant `Sensor` (V5-MOD-2)
//!
//! Sensors produce CMDB envelopes that the scoring pipeline
//! consumes. Use this when you want to plug a new data source
//! (Jira, GitHub, custom telemetry) into NeuroGrim's `cast`
//! dispatch. Built-in sensors live in `neurogrim-sensory` and
//! cover ~21 domains; third-party sensors register alongside.
//!
//! Cargo.toml:
//! ```toml
//! [dependencies]
//! neurogrim-sdk = "0.1"
//! async-trait = "0.1"
//! anyhow = "1"
//! serde_json = "1"
//! chrono = { version = "0.4", features = ["serde"] }
//!
//! [dev-dependencies]
//! tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
//! tempfile = "3"
//! # V5-SDK-2 partial Phase 4 — opt into the conformance feature at
//! # test-build time so `tests/conformance.rs` can reach the suite.
//! # Production builds (no `--tests`) stay tokio-clean.
//! neurogrim-sdk = { version = "0.1", features = ["conformance"] }
//! ```
//!
//! Minimum-viable impl (stateless, infallible-degrading; matches
//! the contract of 18-of-21 built-in sensors):
//!
//! ```ignore
//! use async_trait::async_trait;
//! use neurogrim_sdk::{Sensor, SensorFactory};
//! use serde_json::{json, Value};
//!
//! pub struct MySensor;
//!
//! #[async_trait]
//! impl Sensor for MySensor {
//!     async fn analyze(
//!         &self,
//!         project_root: &str,
//!     ) -> anyhow::Result<Value> {
//!         let now = chrono::Utc::now().to_rfc3339();
//!         Ok(json!({
//!             "meta": {
//!                 "schema_version": "1",
//!                 "updated_at": now,
//!                 "updated_by": "my-sensor",
//!             },
//!             "score": 100,
//!             "updated_at": now,
//!             "findings": [],
//!         }))
//!     }
//! }
//!
//! pub struct MySensorFactory;
//!
//! impl SensorFactory for MySensorFactory {
//!     fn name(&self) -> &'static str { "my-sensor" }
//!     fn build(&self) -> Box<dyn Sensor> { Box::new(MySensor) }
//! }
//! ```
//!
//! Conformance test (`tests/conformance.rs` in your crate):
//!
//! ```ignore
//! use neurogrim_sdk::sensor_conformance::run_factory_conformance;
//! use my_sensor::MySensorFactory;
//! use tempfile::TempDir;
//!
//! #[tokio::test]
//! async fn passes_full_conformance_suite() {
//!     let dir = TempDir::new().unwrap();
//!     let report = run_factory_conformance(&MySensorFactory, dir.path()).await;
//!     assert!(
//!         report.all_passed(),
//!         "{}/{} failed: {:#?}",
//!         report.failures().len(),
//!         report.total(),
//!         report.failures()
//!     );
//! }
//! ```
//!
//! **Contract pitfalls to avoid:**
//!
//! - Never panic. The conformance suite catches panics in
//!   `analyze` calls; if your sensor encounters an unexpected
//!   project state, return an `Err(anyhow!(...))` or a degraded
//!   `Ok(envelope)` with `score: 0` + a finding describing the
//!   failure.
//! - The `meta.schema_version` field MUST equal `"1"` (string,
//!   not integer).
//! - The top-level `score` MUST be an integer in `[0, 100]`.
//! - Both `meta.updated_at` and the top-level `updated_at` MUST
//!   be RFC3339 strings.
//! - Don't take a long time on skeletal input. The conformance
//!   suite has a 30-second timeout; sensors that block on
//!   missing-file IO should fast-fail.
//!
//! Reference: `examples/sensor-readme-quality/` (file-system
//! pattern), `examples/sensor-constant-score/` (minimal-deps
//! pattern; SDK reference example).
//!
//! ### Writing a conformant `ScoringSource` (V5-MOD-1)
//!
//! Scoring sources load a domain's pre-computed CMDB data for
//! the unified-score aggregation in `neurogrim score`. Use this
//! when you want to plug a new score-source pattern (HTTP-fetch
//! from a metrics service, database lookup, custom format).
//! Built-in: `cmdb` (file), `a2a` (peer-fetch), `function`
//! (no-op marker).
//!
//! Cargo.toml: same five deps as the Sensor template above
//! (the SDK contract surface is identical at the dep level).
//!
//! Minimum-viable impl:
//!
//! ```ignore
//! use async_trait::async_trait;
//! use neurogrim_sdk::{
//!     ScoringSource, ScoringSourceFactory,
//! };
//! // Note: ScoringSource takes &Path (not &str like Sensor) and
//! // returns Option<CmdbData>, not Result<Value>. CmdbData lives
//! // in neurogrim_core::scoring::CmdbData — re-import via your
//! // Cargo.toml's neurogrim-core dep, OR use the SDK's
//! // re-export when SDK 0.2.0 surfaces it.
//! // Today (0.1.0) this requires a direct `neurogrim-core` dep.
//!
//! pub struct MyScoringSource;
//! pub struct MyScoringSourceFactory;
//! ```
//!
//! Reference: `examples/scoring-source-prom/` (HTTP-fetch
//! pattern; Prometheus instant-query). The example's
//! `tests/conformance.rs` shows the full suite invocation.
//!
//! **Note on SDK 0.1.0 surface gap:** `CmdbData` is not yet
//! re-exported via `neurogrim-sdk` because it has cyclic-dep
//! considerations with `neurogrim-ecosystem`. Third-party
//! `ScoringSource` authors currently need a direct
//! `neurogrim-core` dep alongside `neurogrim-sdk`. Tracked for
//! SDK 0.2.0 polish.
//!
//! ### Writing a conformant `QueueBackend` (V5-MOD-3)
//!
//! Queue backends store bus messages for a single topic. Use
//! this when you want a new persistence shape (Redis, PostgreSQL,
//! DynamoDB, in-memory). Built-in: `jsonl` (file fan-out),
//! `sqlite` (transactional + ack-capable).
//!
//! Cargo.toml minimum:
//! ```toml
//! [dependencies]
//! neurogrim-sdk = "0.1"
//! anyhow = "1"
//! tracing = "0.1"
//! ```
//!
//! Minimum-viable impl skeleton (in-memory, no persistence,
//! ack-supported via `BTreeSet<u64>`):
//!
//! ```ignore
//! use neurogrim_sdk::{QueueBackend, QueueBackendFactory, StoredMessage, QueueMessage};
//! use std::collections::{BTreeSet, HashMap};
//! use std::path::Path;
//! use std::sync::{Arc, RwLock};
//!
//! pub struct MyQueueBackend {
//!     log: RwLock<Vec<StoredMessage>>,
//!     acks: RwLock<HashMap<String, BTreeSet<u64>>>,
//!     next_offset: RwLock<u64>,
//! }
//!
//! impl QueueBackend for MyQueueBackend {
//!     fn append(&self, msg: &QueueMessage) -> anyhow::Result<u64> {
//!         let mut next = self.next_offset.write().unwrap();
//!         let off = *next;
//!         *next += 1;
//!         drop(next);
//!         self.log.write().unwrap().push(StoredMessage {
//!             offset: off,
//!             message: msg.clone(),
//!         });
//!         Ok(off)
//!     }
//!     fn read_from(&self, since: u64, limit: usize) -> anyhow::Result<Vec<StoredMessage>> {
//!         Ok(self.log.read().unwrap().iter()
//!             .filter(|sm| sm.offset >= since)
//!             .take(limit)
//!             .cloned()
//!             .collect())
//!     }
//!     fn len(&self) -> anyhow::Result<u64> {
//!         Ok(self.log.read().unwrap().len() as u64)
//!     }
//!     // Override `supports_ack`/`read_unacked`/`ack`/`last_acked`
//!     // for ack-capable backends. See `examples/queue-backend-memory/`
//!     // for a full impl.
//! }
//!
//! pub struct MyQueueBackendFactory;
//!
//! impl QueueBackendFactory for MyQueueBackendFactory {
//!     fn name(&self) -> &'static str { "my-backend" }
//!     fn build(&self, _queue_root: &Path, topic: &str)
//!         -> anyhow::Result<Arc<dyn QueueBackend>>
//!     {
//!         Ok(Arc::new(MyQueueBackend {
//!             log: RwLock::new(Vec::new()),
//!             acks: RwLock::new(HashMap::new()),
//!             next_offset: RwLock::new(0),
//!         }))
//!     }
//! }
//! ```
//!
//! **Contract pitfalls to avoid:**
//!
//! - The trait is `Send + Sync` (V5-MOD-3 Fork A2). Use `Mutex`
//!   or `RwLock` for any interior mutability; method receivers
//!   are `&self`, not `&mut self`.
//! - Per-consumer-group ack tracking needs `BTreeSet<u64>` (or
//!   equivalent), NOT `HashMap<String, u64>` high-water-mark.
//!   The set lets you represent out-of-order acks (e.g., ack 1, 4
//!   means offsets 2, 3 are still pending). High-water-mark
//!   would treat 4 as "everything up to 4 is acked", which is
//!   wrong.
//! - `factory.supports_ack()` and `backend.supports_ack()` MUST
//!   agree. The conformance suite verifies this.
//! - Acking an offset that doesn't exist is an error, not a
//!   silent no-op.
//!
//! Reference: `examples/queue-backend-memory/` (full ack-supported
//! in-memory pattern with `BTreeSet` ack tracking).
//!
//! ## What's NOT here
//!
//! - **Implementation crates** (`JsonlBackend`, `SqliteBackend`,
//!   the 21 built-in sensors, the cmdb/a2a/function scoring sources):
//!   live in their respective backend / sensory / ecosystem crates.
//!   The SDK is the **contract** crate; impls reach via direct
//!   `neurogrim-core` / `neurogrim-sensory` / `neurogrim-ecosystem`
//!   dependency for adopters who need them.
//! - **`ScoringSourceConfig`** (the `brain-registry.json` serde
//!   shape): bound to the registry schema, can drift independently
//!   of the trait. SDK consumers depend on the trait's *behavior*,
//!   not the config's serde layout.
//! - **`TestRunner` (V5-FOUND-4):** shipped at V5-FOUND-4 close-out
//!   (2026-05-04). Pluggable contract for executing a workspace
//!   test selection; see [`TestRunner`] / [`TestRunnerFactory`] /
//!   [`TestRunnerRegistry`] above. v5.0 ships one impl
//!   (`NextestRunner` in `neurogrim-cli`); AgentDrivenRunner is
//!   deferred to v5.5 (BACKLOG B-51) once a Rust-side LLM client
//!   lands.
//!
//! ## Hello-world example
//!
//! See `examples/sensor-constant-score/` in the NeuroGrim
//! workspace for a worked third-party `Sensor` impl that depends
//! only on this crate. Companion examples (built-in pattern
//! references):
//!
//! - `examples/scoring-source-prom/` — V5-MOD-1 third-party
//!   pattern (HTTP-fetch from a Prometheus endpoint).
//! - `examples/sensor-readme-quality/` — V5-MOD-2 third-party
//!   pattern (FS-read; README quality scoring).
//! - `examples/queue-backend-memory/` — V5-MOD-3 third-party
//!   pattern (in-memory + ack semantics).

// ────────────────────────────────────────────────────────────────
// Theme B trait surfaces — V5-MOD-1 / V5-MOD-2 / V5-MOD-3
// ────────────────────────────────────────────────────────────────

pub use neurogrim_core::scoring_source::ScoringSource;
pub use neurogrim_core::scoring_source::ScoringSourceFactory;
pub use neurogrim_core::scoring_source::ScoringSourceRegistry;

pub use neurogrim_core::sensor::Sensor;
pub use neurogrim_core::sensor::SensorFactory;
pub use neurogrim_core::sensor::SensorRegistry;

pub use neurogrim_core::queue_backend::QueueBackend;
pub use neurogrim_core::queue_backend::QueueBackendFactory;
pub use neurogrim_core::queue_backend::QueueBackendRegistry;
pub use neurogrim_core::queue_backend::StoredMessage;

/// V5-MOD-3 — the canonical built-in queue backend factories
/// (`jsonl`, `sqlite` under the `sqlite` feature). Use at startup:
///
/// ```ignore
/// use neurogrim_sdk::{QueueBackendRegistry, queue_built_in_factories};
/// let mut registry = QueueBackendRegistry::new();
/// registry.register_all(queue_built_in_factories());
/// ```
pub use neurogrim_core::queue_backend::built_in_factories as queue_built_in_factories;

// ────────────────────────────────────────────────────────────────
// Test runner trait surface — V5-FOUND-4 / V5-SDK-2 close-out
// ────────────────────────────────────────────────────────────────

pub use neurogrim_core::test_runner::TestFailure;
pub use neurogrim_core::test_runner::TestRunReport;
pub use neurogrim_core::test_runner::TestRunner;
pub use neurogrim_core::test_runner::TestRunnerFactory;
pub use neurogrim_core::test_runner::TestRunnerRegistry;
pub use neurogrim_core::test_runner::TestSelection;

// ────────────────────────────────────────────────────────────────
// Adjacent stable trait surfaces — A2A + secrets
// ────────────────────────────────────────────────────────────────

pub use neurogrim_a2a::transport::Transport;
pub use neurogrim_secrets::backend::SecretBackend;

// ────────────────────────────────────────────────────────────────
// Conformance suites — re-exported as nested modules
// ────────────────────────────────────────────────────────────────

/// V5-MOD-1 conformance suite for [`ScoringSource`] impls.
///
/// Third-party authors copy the test pattern from
/// `examples/scoring-source-prom/tests/conformance.rs` (in the
/// NeuroGrim workspace) verbatim — substituting their own factory
/// type — for a verifiable "passes the same contract as built-ins"
/// guarantee.
///
/// V5-SDK-2 partial Phase 2 (2026-05-04) — feature-gated.
#[cfg(feature = "conformance")]
pub mod scoring_source_conformance {
    pub use neurogrim_core::scoring_source_conformance::*;
}

/// V5-MOD-2 conformance suite for [`Sensor`] impls.
///
/// Reference test pattern:
/// `examples/sensor-readme-quality/tests/conformance.rs`.
///
/// V5-SDK-2 partial Phase 2 (2026-05-04) — feature-gated.
#[cfg(feature = "conformance")]
pub mod sensor_conformance {
    pub use neurogrim_core::sensor_conformance::*;
}

/// V5-MOD-3 conformance suite for [`QueueBackend`] impls.
///
/// Reference test pattern:
/// `examples/queue-backend-memory/tests/conformance.rs`.
///
/// V5-SDK-2 partial Phase 2 (2026-05-04) — feature-gated.
#[cfg(feature = "conformance")]
pub mod queue_backend_conformance {
    pub use neurogrim_core::queue_backend_conformance::*;
}

/// V5-FOUND-4 conformance suite for [`TestRunner`] impls.
///
/// Reference test pattern: a third-party `TestRunner` author
/// writes `tests/conformance.rs` in their crate that calls
/// [`test_runner_conformance::run_factory_conformance`] against
/// their factory. The 4-test suite verifies factory contract +
/// no-panic on malformed selection. See
/// `neurogrim-core/src/test_runner_conformance.rs` rustdoc for
/// the full contract details.
///
/// V5-FOUND-4 (2026-05-04) — feature-gated.
#[cfg(feature = "conformance")]
pub mod test_runner_conformance {
    pub use neurogrim_core::test_runner_conformance::*;
}

/// Shared conformance types (V5-SDK-1 Phase 1.5 hoist — Fork F1).
///
/// All four V5-MOD-1/2/3/V5-FOUND-4 conformance suites use these
/// single nominal types. Consumers writing multiple plugin types
/// (e.g., a sensor + a queue backend + a test runner) get one
/// `ConformanceReport` across all suites, not four
/// structurally-identical-but-incompatible copies.
///
/// V5-SDK-2 partial Phase 2 (2026-05-04) — feature-gated alongside
/// the four suite modules; `ConformanceReport` + `TestResult` are
/// only useful in conjunction with one of the suites.
#[cfg(feature = "conformance")]
pub mod conformance {
    pub use neurogrim_core::conformance::*;
}

// ────────────────────────────────────────────────────────────────
// Core types reachable in trait method signatures
// ────────────────────────────────────────────────────────────────

pub use neurogrim_core::agent_output::AgentOutput;
pub use neurogrim_core::queue::Priority;
pub use neurogrim_core::queue::QueueMessage;
pub use neurogrim_core::registry::BrainRegistry;
pub use neurogrim_core::registry::DomainDefinition;
