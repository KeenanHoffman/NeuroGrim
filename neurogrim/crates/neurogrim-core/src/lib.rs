//! # neurogrim-core
//!
//! Pure scoring, correlation, and trajectory logic for the LSP Brains
//! methodology — zero I/O, no async, no protocol concerns. Consumed by
//! [`neurogrim-sensory`], [`neurogrim-mcp`], [`neurogrim-a2a`],
//! [`neurogrim-ecosystem`], and [`neurogrim-cli`].
//!
//! ## What's here
//!
//! - **[`registry`]** — `BrainRegistry`, `DomainDefinition`, `ScoringSourceConfig`:
//!   the parsed shape of `brain-registry.json`.
//! - **[`scoring`]** — `Scorecard`, `build_scorecard`, `unified_confidence`:
//!   per-domain → unified-score aggregation with confidence weighting and
//!   floor-gate semantics (LSP Brains spec §4).
//! - **[`agent_output`]** — `AgentOutput`, `AgentDomain`, `Recommendation`:
//!   the canonical machine-readable contract emitted by
//!   `neurogrim agent`. Schema-versioned (`schema_version: "1"`); used by
//!   A2A peers and ecosystem aggregation.
//! - **[`correlation`]** — `evaluate_condition`, `evaluate_incident_patterns`:
//!   cross-domain pattern detection (spec §4.6, §15).
//! - **[`trajectory`]** — `compute_trajectory`, `TrajectoryResult`,
//!   `TrajectoryClassification`: velocity / acceleration / classification
//!   over a windowed score history (spec §7).
//! - **[`governance`]** — `build_domain_recommendations`, gate-tier ranking
//!   (spec §5).
//! - **[`learning`]** — proposal-effectiveness ledger; closes the agent
//!   feedback loop (principle #4).
//! - **[`calibration_ledger`]** — Brains-2.0 §17 per-domain calibration
//!   meta-observer plumbing.
//! - **[`diagnostics_ledger`]** — V5-FOUND-1 Phase 1 append-only ledger
//!   for timed-operation diagnostics events. Schema at
//!   `data/schemas/diagnostics-ledger-v1.schema.json`; structural privacy
//!   floor (forbidden-extras-keys + per-kind allowed-extras list)
//!   enforced at write time.
//! - **[`scoring_source`]** — V5-MOD-1 (2026-05-02) `ScoringSource` trait +
//!   `ScoringSourceFactory` for the pluggable scoring-source dispatch.
//!   Replaces the string-match in `neurogrim-mcp::context::load_cmdb_data`;
//!   built-in factories (`cmdb`, `a2a`, `function`) ship in V5-MOD-1
//!   Phase 2; the registry is hand-rolled (no `inventory` dep).
//! - **[`llm_backend`]** — V5-feature-1 (2026-05-09) `LlmBackend` trait +
//!   `LlmBackendFactory` + `LlmBackendRegistry` for the pluggable LLM
//!   subagent dispatch. Built-in factories ship in consumer crates
//!   (`neurogrim-mcp` for HTTP-shaped backends; `neurogrim-cli` for the
//!   `codex-cli` subprocess backend) so this crate's "zero I/O" posture
//!   stays intact. The registry lives here so any consumer can program
//!   against the same trait. Same hand-rolled `HashMap<&str, Box<dyn>>`
//!   substrate as scoring_source / sensor / queue_backend.
//! - **[`sensor`]** — V5-MOD-2 (2026-05-02) `Sensor` trait +
//!   `SensorFactory` + `SensorRegistry` for the pluggable sensor
//!   dispatch. Replaces the 21-arm string match in
//!   `neurogrim-cli/src/main.rs:599-622` (`run_sensory`). Built-in
//!   factories live in `neurogrim-sensory` (Phase 2); dispatch
//!   conversion in Phase 3. Same hand-rolled `HashMap` registry
//!   substrate as `scoring_source`; differs by taking `&str` not
//!   `&Path` (analyzer-signature parity, Fork A) and dropping the
//!   inherent fast-path method (sensor IO at seconds-per-call,
//!   Fork B).
//! - **[`ports`]** — v3.5.0 per-project random port allocator. Picks two
//!   ports (dashboard + a2a) from the IANA dynamic range, persists the
//!   choice to `.claude/brain/ports.json`, idempotent on subsequent reads.
//! - **[`queue`]** — v4.1 S13-B-1 append-only event-log substrate.
//!   `QueueMessage`, `Priority`, `Topic`, `append`, `JsonlQueueReader`.
//!   Pattern 1 (fan-out, multi-consumer, no-ack); Pattern 2
//!   (request/response coordination — `await_approval`) ships in S13-B-5
//!   on top of this primitive.
//! - **[`queue_backend`]** — v4.1 S13-B-3 pluggable persistence trait.
//!   `QueueBackend`, `JsonlBackend`, and (under the `sqlite` feature)
//!   `SqliteBackend` with per-consumer-group ack semantics for
//!   `ack_required: true` topics. JSONL preserves "everything
//!   inspectable as files"; SQLite earns its keep for transactional
//!   exactly-once consumption.
//! - **[`queue_config`]** — v4.1 S13-B-3 per-topic configuration
//!   schema (`<brain>/.claude/brain/queue-config.yaml`). Adopters
//!   opt into SQLite per topic; topics not listed default to JSONL.
//! - **[`ecosystem`]** — `ChildEntry`, `EcosystemRegistry`, topological
//!   ordering for fractal-composition score aggregation (spec §9).
//! - **[`awareness`]** — `LocalAwareness`: per-machine fact store (tool
//!   paths, OS quirks). Surface for `neurogrim awareness`.
//! - **[`confidence`]**, **[`types`]** — primitive newtypes (`Score`,
//!   `Weight`, `Confidence`, `ScoreLabel`, `TrajectoryClassification`).
//!
//! ## Stability
//!
//! `AgentOutput` is the cross-version contract — peers at older minor
//! versions deserialize gracefully via `#[serde(default)]` on additive
//! fields. The other modules are not yet stability-marked; consumers
//! outside this workspace should expect breaking changes between minor
//! releases until v4.x.
//!
//! ## See also
//!
//! - [`neurogrim-cli`](https://crates.io/crates/neurogrim-cli) — the
//!   binary that wires everything together
//! - [LSP Brains specification](https://github.com/KeenanHoffman/LSP-Brains)
//!   — RFC-2119 normative spec this crate implements

pub mod agent_output;
pub mod awareness;
pub mod calibration_ledger;
// V5-SDK-1 Phase 1.5 (2026-05-02 — Fork F1): shared
// `ConformanceReport` + `TestResult` types for the V5 conformance
// suites. Pre-V5-SDK-1, each suite (V5-MOD-1's
// `scoring_source_conformance`, V5-MOD-2's `sensor_conformance`,
// V5-MOD-3's `queue_backend_conformance`) shipped its own copy
// of these types. Hoisted here before SDK 0.1.0 ships so
// consumers writing multiple plugin types share a single nominal
// `ConformanceReport` across all suites.
//
// V5-SDK-2 partial Phase 1 (2026-05-03) — gated behind the
// `conformance` feature alongside the three suite modules. The
// shared `ConformanceReport` + `TestResult` types are only useful
// in conjunction with one of the suites, so they share the gate.
#[cfg(feature = "conformance")]
pub mod conformance;
pub mod confidence;
pub mod correlation;
pub mod diagnostics_ledger;
pub mod ecosystem;
pub mod governance;
pub mod learning;
// V5-feature-1 (2026-05-09) — pluggable LLM-subagent dispatch.
// Trait + factory + registry only; built-in impls live in consumer
// crates (neurogrim-mcp, neurogrim-cli) to keep this crate I/O-free.
pub mod llm_backend;
pub mod ports;
#[cfg(feature = "sqlite")]
pub mod metrics;
pub mod queue;
pub mod queue_backend;
// V5-MOD-3 Phase 4 (2026-05-02) — `QueueBackend` conformance suite
// for third-party impls. 12 cross-cutting + backend-specific tests
// (factory contract, append/read round-trip, concurrent appends,
// ack semantics, Send+Sync runtime check). Mirrors V5-MOD-1's
// `scoring_source_conformance` and V5-MOD-2's `sensor_conformance`.
//
// V5-SDK-2 partial Phase 1 (2026-05-03) — feature-gated.
#[cfg(feature = "conformance")]
pub mod queue_backend_conformance;
pub mod queue_config;
pub mod registry;
#[cfg(feature = "sqlite")]
pub mod skill_invocations;
pub mod scoring;
pub mod scoring_source;
// V5-SDK-2 partial Phase 1 (2026-05-03) — feature-gated; the
// suite uses `tokio::spawn` + `tokio::time::timeout`.
#[cfg(feature = "conformance")]
pub mod scoring_source_conformance;
pub mod scoring_sources;
// V5-FOUND-4 Phase 1 (2026-05-04) — pluggable test-runner trait
// + types + registry. Always-on (the trait surface is a stable
// contract); the conformance suite below is feature-gated.
pub mod test_runner;
// V5-FOUND-4 Phase 1 (2026-05-04) — feature-gated test-runner
// conformance suite. 4 cross-cutting tests; uses
// `tokio::time::timeout`.
#[cfg(feature = "conformance")]
pub mod test_runner_conformance;
// V5-MOD-2 Phase 1 (2026-05-02) — `Sensor` trait + `SensorFactory`
// + `SensorRegistry` for the pluggable sensor dispatch. Replaces
// the 21-arm string match in `neurogrim-cli/src/main.rs:599-622`
// (`run_sensory`). Built-in factories ship in V5-MOD-2 Phase 2;
// dispatch conversion in Phase 3.
pub mod sensor;
// V5-MOD-2 Phase 5 (2026-05-02) — `Sensor` conformance suite for
// third-party impls. 10 cross-cutting + sensor-specific tests
// (factory contract, async safety, CMDB envelope shape, score
// range, meta block well-formedness, timeout, idempotency).
// Mirrors V5-MOD-1's `scoring_source_conformance` pattern.
//
// V5-SDK-2 partial Phase 1 (2026-05-03) — feature-gated.
#[cfg(feature = "conformance")]
pub mod sensor_conformance;
pub mod trajectory;
pub mod types;
