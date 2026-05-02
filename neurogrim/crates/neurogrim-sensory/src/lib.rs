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

pub mod agent_behavior;
pub mod capability_hygiene;
pub mod cmdb;
pub mod code_quality;
pub mod coherence;
pub mod deploy_readiness;
pub mod docker_topology;
pub mod domain_calibration;
pub mod federated_patterns;
pub mod git_health;
pub mod human_comms;
pub mod operator_calibration;
pub mod rust_health;
pub mod secret_refs;
pub mod secrets_readiness;
pub mod security_standards;
pub mod skill_coherence;
pub mod supply_chain_calibration;
pub mod supply_chain_review;
pub mod supply_chain_sca;
pub mod supply_chain_vigilance;
pub mod test_results;
pub mod trust_budget;

// V5-MOD-2 Phase 2 (2026-05-02) — centralized `Sensor` +
// `SensorFactory` impls for all 21 built-in sensors plus the
// `built_in_factories()` aggregator. Phase 3 wires the dispatch
// through this list. See `sensor_impls.rs` rustdoc for the
// "centralized vs per-module" plan deviation note.
pub mod sensor_impls;
pub use sensor_impls::built_in_factories;
