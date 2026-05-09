//! Built-in sensory tool implementations.
//!
//! Each tool is an MCP server that produces CMDB-envelope JSON.
//! All tools implement the same contract: accept project_root, return CMDB.
//!
//! # V5-MOD-2 (2026-05-02) — `Sensor` trait migration
//!
//! Each `analyze_*` free function gains a [`Sensor`] + [`SensorFactory`]
//! pair in [`sensor_impls`]. The trait impls are delegating wrappers —
//! the analyzers themselves are unchanged. [`sensor_impls::built_in_factories`]
//! is the canonical 21-sensor list consumed by `neurogrim-cli`'s
//! `run_sensory` dispatch in V5-MOD-2 Phase 3.
//!
//! [`Sensor`]: neurogrim_core::sensor::Sensor
//! [`SensorFactory`]: neurogrim_core::sensor::SensorFactory

// Always-compiled shared infrastructure: CMDB envelope builder
// shared by all sensors. Pulls only always-on workspace deps
// (chrono, serde, serde_json) so it's safe to leave unfeature-gated.
pub mod cmdb;

// v2-Feature 7 Phase 1 (2026-05-09) — decision-diversity computation
// library. Pure-stdlib + serde; no rmcp tool router yet. Phase 7.2
// wraps this in the sensor-trait shape + adds a `decision-diversity`
// brain domain registration. Always-on so callers (CLI, tests, future
// drift sensors) can use the library without sensor-feature gating.
pub mod decision_diversity;

// v2-Feature 5 Phase 1 (2026-05-09) — documentation-graph sensor.
// Walks *.md files, extracts cross-references via pulldown-cmark,
// builds a directed graph, scores by orphan ratio + broken links +
// cycle count. Always-on (small dep) — Phase 2 may carve behind a
// feature flag if disk pressure justifies it.
pub mod documentation_graph;

// v2-Feature 6 Phase 6.4 (2026-05-09) — external-content-safety
// advisory domain. Reads `<project>/.claude/audit.jsonl` for
// `category=external_content` rows produced by the IDE's
// `external_content_scan` Tauri command (Phase 6.1) and scores
// the operator's recent injection-attempt exposure. Always-on
// (no extra deps); weight 0.0 (advisory) until ≥30 days of audit
// history validates the heuristic per LSP-Brains §15.5.
pub mod external_content_safety;

// V5-MOD-2 Phase 4 (2026-05-02) — per-sensor `#[cfg(feature)]`
// gates carve out source modules + heavy deps for slim builds.
// Default-features build pulls all 21 sensors (= v4 behavior).
// `--no-default-features --features sensor-X` builds only X.
#[cfg(feature = "sensor-agent-behavior")]
pub mod agent_behavior;
#[cfg(feature = "sensor-capability-hygiene")]
pub mod capability_hygiene;
#[cfg(feature = "sensor-code-quality")]
pub mod code_quality;
#[cfg(feature = "sensor-coherence")]
pub mod coherence;
#[cfg(feature = "sensor-deploy-readiness")]
pub mod deploy_readiness;
#[cfg(feature = "sensor-docker-topology")]
pub mod docker_topology;
#[cfg(feature = "sensor-domain-calibration")]
pub mod domain_calibration;
#[cfg(feature = "sensor-federated-patterns")]
pub mod federated_patterns;
#[cfg(feature = "sensor-git-health")]
pub mod git_health;
#[cfg(feature = "sensor-human-comms")]
pub mod human_comms;
#[cfg(feature = "sensor-operator-calibration")]
pub mod operator_calibration;
#[cfg(feature = "sensor-rust-health")]
pub mod rust_health;
#[cfg(feature = "sensor-secret-refs")]
pub mod secret_refs;
#[cfg(feature = "sensor-secrets-readiness")]
pub mod secrets_readiness;
#[cfg(feature = "sensor-security-standards")]
pub mod security_standards;
#[cfg(feature = "sensor-skill-coherence")]
pub mod skill_coherence;
// `supply_chain_calibration` is the calibration helper used by both
// the `supply_chain_sca` SENSOR and the `sca-calibrate` CLI command.
// Gated alongside `sensor-supply-chain-sca` — they're conceptually
// paired (no SCA = no calibrate). The CLI's `sca_calibrate` command
// is gated to match.
#[cfg(feature = "sensor-supply-chain-sca")]
pub mod supply_chain_calibration;
#[cfg(feature = "sensor-supply-chain-review")]
pub mod supply_chain_review;
#[cfg(feature = "sensor-supply-chain-sca")]
pub mod supply_chain_sca;
#[cfg(feature = "sensor-supply-chain-vigilance")]
pub mod supply_chain_vigilance;
#[cfg(feature = "sensor-test-health")]
pub mod test_results;
#[cfg(feature = "sensor-trust-budget")]
pub mod trust_budget;

// V5-MOD-2 Phase 2 (2026-05-02) — centralized `Sensor` +
// `SensorFactory` impls for all 21 built-in sensors plus the
// `built_in_factories()` aggregator. Phase 3 wires the dispatch
// through this list; Phase 4 adds per-entry `#[cfg(feature)]`
// gates. See `sensor_impls.rs` rustdoc for the "centralized vs
// per-module" plan deviation note.
pub mod sensor_impls;
pub use sensor_impls::built_in_factories;
