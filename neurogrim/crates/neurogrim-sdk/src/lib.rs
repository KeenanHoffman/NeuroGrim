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
//! - [`scoring_source_conformance`]
//! - [`sensor_conformance`]
//! - [`queue_backend_conformance`]
//!
//! All three suites share the [`conformance::ConformanceReport`] +
//! [`conformance::TestResult`] types (V5-SDK-1 Phase 1.5 hoist —
//! consumers writing multiple plugin types see a single nominal
//! `ConformanceReport`, not three structurally-identical-but-
//! incompatible copies).
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
//! - **`TestRunner` (V5-FOUND-4):** unshipped at V5-SDK-1 release.
//!   Will be added as a pure additive minor bump (`0.2.0`) when
//!   V5-FOUND-4 lands.
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
pub mod scoring_source_conformance {
    pub use neurogrim_core::scoring_source_conformance::*;
}

/// V5-MOD-2 conformance suite for [`Sensor`] impls.
///
/// Reference test pattern:
/// `examples/sensor-readme-quality/tests/conformance.rs`.
pub mod sensor_conformance {
    pub use neurogrim_core::sensor_conformance::*;
}

/// V5-MOD-3 conformance suite for [`QueueBackend`] impls.
///
/// Reference test pattern:
/// `examples/queue-backend-memory/tests/conformance.rs`.
pub mod queue_backend_conformance {
    pub use neurogrim_core::queue_backend_conformance::*;
}

/// Shared conformance types (V5-SDK-1 Phase 1.5 hoist — Fork F1).
///
/// All three V5-MOD-1/2/3 conformance suites use these single
/// nominal types. Consumers writing multiple plugin types
/// (e.g., a sensor + a queue backend) get one
/// `ConformanceReport` across all suites, not three
/// structurally-identical-but-incompatible copies.
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
