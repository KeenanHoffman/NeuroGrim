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
pub mod confidence;
pub mod correlation;
pub mod diagnostics_ledger;
pub mod ecosystem;
pub mod governance;
pub mod learning;
pub mod ports;
#[cfg(feature = "sqlite")]
pub mod metrics;
pub mod queue;
pub mod queue_backend;
pub mod queue_config;
pub mod registry;
#[cfg(feature = "sqlite")]
pub mod skill_invocations;
pub mod scoring;
pub mod scoring_source;
pub mod scoring_sources;
pub mod trajectory;
pub mod types;
