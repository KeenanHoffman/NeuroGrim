//! V5-MOD-2 Phase 2 — `Sensor` + `SensorFactory` impls for all 21
//! built-in sensors plus a [`built_in_factories`] aggregator
//! (2026-05-02).
//!
//! # Why centralized (plan deviation note)
//!
//! The V5-MOD-2 plan's literal text (`.claude/plans/v5-mod-2-sensor-trait.md`
//! § Phase 2) called for the `XSensor` + `XSensorFactory` pair to live
//! "in the same file" as each `analyze_*` analyzer. **This module
//! centralizes them instead** — single file to edit when adding a new
//! sensor, single source of truth for the factory list, and the
//! [`built_in_factories`] aggregator (V5-MOD-2 Phase 3's planned
//! `registry.rs`) gets to live alongside the impls without an extra
//! file. Subagent 2's `register_sensor!` macro suggestion is satisfied
//! organically by the per-sensor block being short enough that a macro
//! buys nothing.
//!
//! Each sensor's analyzer (the existing `pub async fn analyze_*` free
//! function in its own module) is **untouched**; the trait impl here
//! is purely a delegating wrapper. Operator-visible behavior is
//! identical.
//!
//! # Sensor inventory (V5-MOD-2 Phase 0 audit, 2026-05-02)
//!
//! 21 built-in sensors total. Three return `anyhow::Result<Value>`
//! (`agent_behavior`, `docker_topology`, `git_health`); the other
//! eighteen return `Value` directly (silent-degrade with `score: 0`
//! envelopes on failure — preserved here, the trait wrapper just
//! `Ok(...)`s the value). The `secrets_readiness` sensor is the
//! Fork-C orphan: analyzer + 9 tests + `pub` re-export landed in
//! v4.2 S14, dispatch wiring didn't; reclaimed as a registered
//! sensor in V5-MOD-2 Phase 3.
//!
//! # Phase 4 hookup
//!
//! `built_in_factories()` will become `#[cfg(feature = "sensor-X")]`-
//! gated per-entry in V5-MOD-2 Phase 4 — operators who build with
//! `--no-default-features --features sensor-git-health` get a registry
//! containing exactly that sensor. The placeholder feature
//! `_phase0-all-deps` (in `Cargo.toml`) currently activates all
//! optional heavy deps so the v4-equivalent default build works.

use async_trait::async_trait;
use neurogrim_core::sensor::{Sensor, SensorFactory};
use serde_json::Value;

// Pull each analyzer into scope so the trait impls below can call
// them by their short names. Keeps the impls visually compact.
use crate::{
    agent_behavior::analyze_agent_behavior,
    capability_hygiene::analyze_capability_hygiene,
    code_quality::analyze_code_quality,
    coherence::analyze_coherence,
    deploy_readiness::analyze_deploy_readiness,
    docker_topology::analyze_docker_topology,
    domain_calibration::analyze_domain_calibration,
    federated_patterns::analyze_federated_patterns,
    git_health::analyze_git_health,
    human_comms::analyze_human_comms,
    operator_calibration::analyze_operator_calibration,
    rust_health::analyze_rust_health,
    secret_refs::analyze_secret_refs,
    secrets_readiness::analyze_secrets_readiness,
    security_standards::analyze_security_standards,
    skill_coherence::analyze_skill_coherence,
    supply_chain_review::analyze_supply_chain_review,
    supply_chain_sca::analyze_supply_chain_sca,
    supply_chain_vigilance::analyze_supply_chain_vigilance,
    test_results::analyze_test_health,
    trust_budget::analyze_trust_budget,
};

// ────────────────────────────────────────────────────────────────
// Helper macros — keep the per-sensor block to 1-2 lines so the
// inventory of 21 sensors is readable in a single screen.
// ────────────────────────────────────────────────────────────────

/// Generates `XSensor` + `XSensorFactory` for an analyzer that
/// returns `anyhow::Result<Value>`. The 3 fallible sensors use
/// this variant.
macro_rules! fallible_sensor {
    ($sensor:ident, $factory:ident, $name:literal, $analyzer:path) => {
        #[doc = concat!(
            "Wire-name `\"", $name, "\"`. Delegates to the existing ",
            "`analyze_*` free function in this crate."
        )]
        pub struct $sensor;

        #[async_trait]
        impl Sensor for $sensor {
            async fn analyze(
                &self,
                project_root: &str,
            ) -> anyhow::Result<Value> {
                $analyzer(project_root).await
            }
        }

        #[doc = concat!(
            "Factory for [`", stringify!($sensor), "`]. ",
            "`name() = \"", $name, "\"`."
        )]
        pub struct $factory;

        impl SensorFactory for $factory {
            fn name(&self) -> &'static str {
                $name
            }
            fn build(&self) -> Box<dyn Sensor> {
                Box::new($sensor)
            }
        }
    };
}

/// Generates `XSensor` + `XSensorFactory` for an analyzer that
/// returns `Value` directly (silent-degrade pattern). The 18
/// infallible sensors use this variant. The trait wrapper
/// `Ok(...)`s the value — operator-visible behavior is identical
/// to the v4 free-function semantics.
macro_rules! infallible_sensor {
    ($sensor:ident, $factory:ident, $name:literal, $analyzer:path) => {
        #[doc = concat!(
            "Wire-name `\"", $name, "\"`. Delegates to the existing ",
            "`analyze_*` free function in this crate (silent-degrade ",
            "on failure — produces a CMDB envelope with `score: 0` + ",
            "a `<sensor>_error` finding rather than `Err(...)`)."
        )]
        pub struct $sensor;

        #[async_trait]
        impl Sensor for $sensor {
            async fn analyze(
                &self,
                project_root: &str,
            ) -> anyhow::Result<Value> {
                Ok($analyzer(project_root).await)
            }
        }

        #[doc = concat!(
            "Factory for [`", stringify!($sensor), "`]. ",
            "`name() = \"", $name, "\"`."
        )]
        pub struct $factory;

        impl SensorFactory for $factory {
            fn name(&self) -> &'static str {
                $name
            }
            fn build(&self) -> Box<dyn Sensor> {
                Box::new($sensor)
            }
        }
    };
}

// ────────────────────────────────────────────────────────────────
// Sensor inventory — 21 entries
// ────────────────────────────────────────────────────────────────

// Fallible (3): analyzer returns anyhow::Result<Value>.
fallible_sensor!(AgentBehaviorSensor, AgentBehaviorSensorFactory, "agent-behavior", analyze_agent_behavior);
fallible_sensor!(DockerTopologySensor, DockerTopologySensorFactory, "docker-topology", analyze_docker_topology);
fallible_sensor!(GitHealthSensor, GitHealthSensorFactory, "git-health", analyze_git_health);

// Infallible / silent-degrade (18): analyzer returns Value directly.
infallible_sensor!(CapabilityHygieneSensor, CapabilityHygieneSensorFactory, "capability-hygiene", analyze_capability_hygiene);
infallible_sensor!(CodeQualitySensor, CodeQualitySensorFactory, "code-quality", analyze_code_quality);
infallible_sensor!(CoherenceSensor, CoherenceSensorFactory, "coherence", analyze_coherence);
infallible_sensor!(DeployReadinessSensor, DeployReadinessSensorFactory, "deploy-readiness", analyze_deploy_readiness);
infallible_sensor!(DomainCalibrationSensor, DomainCalibrationSensorFactory, "domain-calibration", analyze_domain_calibration);
infallible_sensor!(FederatedPatternsSensor, FederatedPatternsSensorFactory, "federated-patterns", analyze_federated_patterns);
infallible_sensor!(HumanCommsSensor, HumanCommsSensorFactory, "human-comms", analyze_human_comms);
infallible_sensor!(OperatorCalibrationSensor, OperatorCalibrationSensorFactory, "operator-calibration", analyze_operator_calibration);
infallible_sensor!(RustHealthSensor, RustHealthSensorFactory, "rust-health", analyze_rust_health);
infallible_sensor!(SecretRefsSensor, SecretRefsSensorFactory, "secret-refs", analyze_secret_refs);
infallible_sensor!(SecretsReadinessSensor, SecretsReadinessSensorFactory, "secrets-readiness", analyze_secrets_readiness);
infallible_sensor!(SecurityStandardsSensor, SecurityStandardsSensorFactory, "security-standards", analyze_security_standards);
infallible_sensor!(SkillCoherenceSensor, SkillCoherenceSensorFactory, "skill-coherence", analyze_skill_coherence);
infallible_sensor!(SupplyChainReviewSensor, SupplyChainReviewSensorFactory, "supply-chain-review", analyze_supply_chain_review);
infallible_sensor!(SupplyChainScaSensor, SupplyChainScaSensorFactory, "supply-chain-sca", analyze_supply_chain_sca);
infallible_sensor!(SupplyChainVigilanceSensor, SupplyChainVigilanceSensorFactory, "supply-chain-vigilance", analyze_supply_chain_vigilance);
infallible_sensor!(TestHealthSensor, TestHealthSensorFactory, "test-health", analyze_test_health);
infallible_sensor!(TrustBudgetSensor, TrustBudgetSensorFactory, "trust-budget", analyze_trust_budget);

// Note: `supply_chain_review`'s `cli_create` / `cli_resolve` /
// `cli_list` free functions (used by `neurogrim-cli/src/commands/
// sca_review.rs`) are NOT part of the analyzer surface and do NOT
// get trait wrappers — they remain free functions in
// `supply_chain_review.rs`. Per V5-MOD-2 plan-critic Subagent 3
// 🟡 finding.
//
// Note: `supply_chain_calibration.rs` exposes `LayerReport` /
// `cli_calibrate` / etc. for the `sca_calibrate` CLI command but
// has no `analyze_*` analyzer — it's calibration helper code, not
// a sensor. No factory here.

// ────────────────────────────────────────────────────────────────
// Aggregator
// ────────────────────────────────────────────────────────────────

/// All 21 built-in sensor factories.
///
/// V5-MOD-2 Phase 3 wires the dispatch site (`neurogrim-cli/src/
/// main.rs::run_sensory`) to call this and populate a
/// [`SensorRegistry`]:
///
/// ```ignore
/// use neurogrim_core::sensor::SensorRegistry;
/// use neurogrim_sensory::sensor_impls::built_in_factories;
///
/// let mut registry = SensorRegistry::new();
/// registry.register_all(built_in_factories());
/// ```
///
/// V5-MOD-2 Phase 4 adds `#[cfg(feature = "sensor-X")]` gates per
/// entry — operators who build with
/// `--no-default-features --features sensor-git-health` get a
/// registry containing only the gated sensors.
///
/// [`SensorRegistry`]: neurogrim_core::sensor::SensorRegistry
pub fn built_in_factories() -> Vec<Box<dyn SensorFactory>> {
    vec![
        // Fallible
        Box::new(AgentBehaviorSensorFactory),
        Box::new(DockerTopologySensorFactory),
        Box::new(GitHealthSensorFactory),
        // Infallible / silent-degrade
        Box::new(CapabilityHygieneSensorFactory),
        Box::new(CodeQualitySensorFactory),
        Box::new(CoherenceSensorFactory),
        Box::new(DeployReadinessSensorFactory),
        Box::new(DomainCalibrationSensorFactory),
        Box::new(FederatedPatternsSensorFactory),
        Box::new(HumanCommsSensorFactory),
        Box::new(OperatorCalibrationSensorFactory),
        Box::new(RustHealthSensorFactory),
        Box::new(SecretRefsSensorFactory),
        Box::new(SecretsReadinessSensorFactory),
        Box::new(SecurityStandardsSensorFactory),
        Box::new(SkillCoherenceSensorFactory),
        Box::new(SupplyChainReviewSensorFactory),
        Box::new(SupplyChainScaSensorFactory),
        Box::new(SupplyChainVigilanceSensorFactory),
        Box::new(TestHealthSensorFactory),
        Box::new(TrustBudgetSensorFactory),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurogrim_core::sensor::SensorRegistry;

    /// Smoke test: all 21 factories register cleanly into a
    /// `SensorRegistry`. Catches duplicate wire-names, factory
    /// `name()` returning empty strings, and any panic in
    /// `Box::new(...)`.
    #[test]
    fn all_built_in_factories_register_into_registry() {
        let mut registry = SensorRegistry::new();
        registry.register_all(built_in_factories());
        // 21 unique wire-names; no collisions.
        assert_eq!(
            registry.len(),
            21,
            "expected 21 built-in factories, got {}",
            registry.len()
        );
    }

    /// Wire-name parity: every name from the v4 `run_sensory` match
    /// in `neurogrim-cli/src/main.rs:601-621` must be present in
    /// the registry. Phase 3 wires the dispatch through this list;
    /// any missing name would silently break a previously-working
    /// `neurogrim cast <name>` invocation.
    #[test]
    fn wire_names_match_v4_run_sensory_dispatch() {
        // The 20 names in run_sensory's match arms (pre-V5-MOD-2)
        // PLUS secrets-readiness (Fork C orphan reclaimed in
        // Phase 3 — registered here as the 21st).
        let v4_dispatch_names = [
            "git-health",
            "rust-health",
            "code-quality",
            "test-health",
            "deploy-readiness",
            "security-standards",
            "coherence",
            "human-comms",
            "secret-refs",
            "docker-topology",
            "agent-behavior",
            "skill-coherence",
            "capability-hygiene",
            "supply-chain-sca",
            "supply-chain-vigilance",
            "supply-chain-review",
            "domain-calibration",
            "operator-calibration",
            "trust-budget",
            "federated-patterns",
            // Fork C: secrets-readiness orphan reclaimed.
            "secrets-readiness",
        ];

        let mut registry = SensorRegistry::new();
        registry.register_all(built_in_factories());

        for name in &v4_dispatch_names {
            assert!(
                registry.has(name),
                "wire-name {name:?} missing from registry; v4 \
                 dispatch parity broken"
            );
        }

        // And the count matches — no extras either.
        assert_eq!(
            registry.len(),
            v4_dispatch_names.len(),
            "registry has {} entries; v4 parity expects {}",
            registry.len(),
            v4_dispatch_names.len()
        );
    }

    /// Factory `name()` and `build().<no name on Sensor by design>`
    /// — but the registry's `register_all` would panic if any
    /// factory's name conflicted. This test rehearses the registry
    /// dispatch path for a non-trivial subset of sensors.
    #[tokio::test]
    async fn registry_can_build_and_invoke_a_sensor_end_to_end() {
        use std::path::PathBuf;
        // Pick a sensor that's safe to invoke against any tempdir
        // (no external process, no network). `human-comms` reads
        // `<root>/.claude/human-comms.yaml`; absent file produces
        // a degraded envelope with `score: 100` (no preferences =
        // honor defaults) per the existing analyzer semantics.
        let mut registry = SensorRegistry::new();
        registry.register_all(built_in_factories());

        let dir = tempfile::tempdir().expect("tempdir");
        let project_root = dir.path().to_string_lossy().to_string();

        let sensor = registry
            .build("human-comms")
            .expect("human-comms factory must be registered");
        let result = sensor.analyze(&project_root).await;
        assert!(
            result.is_ok(),
            "human-comms analyze on empty tempdir must Ok, got {result:?}"
        );
        let envelope = result.unwrap();
        assert!(
            envelope.get("score").is_some(),
            "envelope must have a `score` field; got {envelope:?}"
        );

        let _ = PathBuf::from(project_root); // suppress unused-import warning
    }
}
