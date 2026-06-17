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

// `unused_imports` may fire when --no-default-features is used and
// no sensor-X features compile a single trait impl — these imports
// only surface inside macro-expanded code. The "no sensors built"
// case is a legitimate slim build (e.g., a custom binary registers
// only third-party `Sensor` impls), so the lints are noise.
#[allow(unused_imports)]
use async_trait::async_trait;
#[allow(unused_imports)]
use neurogrim_core::sensor::{Sensor, SensorFactory};
#[allow(unused_imports)]
use serde_json::Value;

// Pull each analyzer into scope so the trait impls below can call
// them by their short names. Each `use` is gated by the same
// `sensor-X` feature as its source module in `lib.rs`.
#[cfg(feature = "sensor-agent-behavior")]
use crate::agent_behavior::analyze_agent_behavior;
#[cfg(feature = "sensor-capability-hygiene")]
use crate::capability_hygiene::analyze_capability_hygiene;
#[cfg(feature = "sensor-code-quality")]
use crate::code_quality::analyze_code_quality;
#[cfg(feature = "sensor-coherence")]
use crate::coherence::analyze_coherence;
#[cfg(feature = "sensor-decision-diversity")]
use crate::decision_diversity::analyze_decision_diversity;
#[cfg(feature = "sensor-deploy-readiness")]
use crate::deploy_readiness::analyze_deploy_readiness;
#[cfg(feature = "sensor-docker-topology")]
use crate::docker_topology::analyze_docker_topology;
#[cfg(feature = "sensor-documentation-graph")]
use crate::documentation_graph::analyze_documentation_graph;
#[cfg(feature = "sensor-backlog")]
use crate::backlog::analyze_backlog;
#[cfg(feature = "sensor-domain-calibration")]
use crate::domain_calibration::analyze_domain_calibration;
#[cfg(feature = "sensor-federated-patterns")]
use crate::federated_patterns::analyze_federated_patterns;
#[cfg(feature = "sensor-git-health")]
use crate::git_health::analyze_git_health;
#[cfg(feature = "sensor-human-comms")]
use crate::human_comms::analyze_human_comms;
#[cfg(feature = "sensor-operator-calibration")]
use crate::operator_calibration::analyze_operator_calibration;
#[cfg(feature = "sensor-rust-health")]
use crate::rust_health::analyze_rust_health;
#[cfg(feature = "sensor-secret-refs")]
use crate::secret_refs::analyze_secret_refs;
#[cfg(feature = "sensor-secrets-readiness")]
use crate::secrets_readiness::analyze_secrets_readiness;
#[cfg(feature = "sensor-security-standards")]
use crate::security_standards::analyze_security_standards;
#[cfg(feature = "sensor-skill-coherence")]
use crate::skill_coherence::analyze_skill_coherence;
#[cfg(feature = "sensor-supply-chain-review")]
use crate::supply_chain_review::analyze_supply_chain_review;
#[cfg(feature = "sensor-supply-chain-sca")]
use crate::supply_chain_sca::analyze_supply_chain_sca;
#[cfg(feature = "sensor-supply-chain-vigilance")]
use crate::supply_chain_vigilance::analyze_supply_chain_vigilance;
#[cfg(feature = "sensor-test-health")]
use crate::test_results::analyze_test_health;
#[cfg(feature = "sensor-trust-budget")]
use crate::trust_budget::analyze_trust_budget;

// ────────────────────────────────────────────────────────────────
// Helper macros — keep the per-sensor block to 1-2 lines so the
// inventory of 21 sensors is readable in a single screen.
// ────────────────────────────────────────────────────────────────

/// Generates `XSensor` + `XSensorFactory` for an analyzer that
/// returns `anyhow::Result<Value>`. The 3 fallible sensors use
/// this variant.
#[allow(unused_macros)]
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
#[allow(unused_macros)]
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
#[cfg(feature = "sensor-agent-behavior")]
fallible_sensor!(AgentBehaviorSensor, AgentBehaviorSensorFactory, "agent-behavior", analyze_agent_behavior);
#[cfg(feature = "sensor-docker-topology")]
fallible_sensor!(DockerTopologySensor, DockerTopologySensorFactory, "docker-topology", analyze_docker_topology);
#[cfg(feature = "sensor-git-health")]
fallible_sensor!(GitHealthSensor, GitHealthSensorFactory, "git-health", analyze_git_health);

// Infallible / silent-degrade (18): analyzer returns Value directly.
#[cfg(feature = "sensor-capability-hygiene")]
infallible_sensor!(CapabilityHygieneSensor, CapabilityHygieneSensorFactory, "capability-hygiene", analyze_capability_hygiene);
#[cfg(feature = "sensor-code-quality")]
infallible_sensor!(CodeQualitySensor, CodeQualitySensorFactory, "code-quality", analyze_code_quality);
#[cfg(feature = "sensor-coherence")]
infallible_sensor!(CoherenceSensor, CoherenceSensorFactory, "coherence", analyze_coherence);
#[cfg(feature = "sensor-decision-diversity")]
infallible_sensor!(DecisionDiversitySensor, DecisionDiversitySensorFactory, "decision-diversity", analyze_decision_diversity);
#[cfg(feature = "sensor-deploy-readiness")]
infallible_sensor!(DeployReadinessSensor, DeployReadinessSensorFactory, "deploy-readiness", analyze_deploy_readiness);
#[cfg(feature = "sensor-documentation-graph")]
infallible_sensor!(DocumentationGraphSensor, DocumentationGraphSensorFactory, "documentation-graph", analyze_documentation_graph);
#[cfg(feature = "sensor-backlog")]
infallible_sensor!(BacklogSensor, BacklogSensorFactory, "backlog", analyze_backlog);
#[cfg(feature = "sensor-domain-calibration")]
infallible_sensor!(DomainCalibrationSensor, DomainCalibrationSensorFactory, "domain-calibration", analyze_domain_calibration);
#[cfg(feature = "sensor-federated-patterns")]
infallible_sensor!(FederatedPatternsSensor, FederatedPatternsSensorFactory, "federated-patterns", analyze_federated_patterns);
#[cfg(feature = "sensor-human-comms")]
infallible_sensor!(HumanCommsSensor, HumanCommsSensorFactory, "human-comms", analyze_human_comms);
#[cfg(feature = "sensor-operator-calibration")]
infallible_sensor!(OperatorCalibrationSensor, OperatorCalibrationSensorFactory, "operator-calibration", analyze_operator_calibration);
#[cfg(feature = "sensor-rust-health")]
infallible_sensor!(RustHealthSensor, RustHealthSensorFactory, "rust-health", analyze_rust_health);
#[cfg(feature = "sensor-secret-refs")]
infallible_sensor!(SecretRefsSensor, SecretRefsSensorFactory, "secret-refs", analyze_secret_refs);
#[cfg(feature = "sensor-secrets-readiness")]
infallible_sensor!(SecretsReadinessSensor, SecretsReadinessSensorFactory, "secrets-readiness", analyze_secrets_readiness);
#[cfg(feature = "sensor-security-standards")]
infallible_sensor!(SecurityStandardsSensor, SecurityStandardsSensorFactory, "security-standards", analyze_security_standards);
#[cfg(feature = "sensor-skill-coherence")]
infallible_sensor!(SkillCoherenceSensor, SkillCoherenceSensorFactory, "skill-coherence", analyze_skill_coherence);
#[cfg(feature = "sensor-supply-chain-review")]
infallible_sensor!(SupplyChainReviewSensor, SupplyChainReviewSensorFactory, "supply-chain-review", analyze_supply_chain_review);
#[cfg(feature = "sensor-supply-chain-sca")]
infallible_sensor!(SupplyChainScaSensor, SupplyChainScaSensorFactory, "supply-chain-sca", analyze_supply_chain_sca);
#[cfg(feature = "sensor-supply-chain-vigilance")]
infallible_sensor!(SupplyChainVigilanceSensor, SupplyChainVigilanceSensorFactory, "supply-chain-vigilance", analyze_supply_chain_vigilance);
#[cfg(feature = "sensor-test-health")]
infallible_sensor!(TestHealthSensor, TestHealthSensorFactory, "test-health", analyze_test_health);
#[cfg(feature = "sensor-trust-budget")]
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
    // `mut` is conditionally needed: only used when at least one
    // sensor-X feature compiles a `factories.push(...)` line.
    // `--no-default-features` (zero sensors) leaves the vec empty.
    #[allow(unused_mut)]
    let mut factories: Vec<Box<dyn SensorFactory>> = Vec::new();

    // Fallible (3)
    #[cfg(feature = "sensor-agent-behavior")]
    factories.push(Box::new(AgentBehaviorSensorFactory));
    #[cfg(feature = "sensor-docker-topology")]
    factories.push(Box::new(DockerTopologySensorFactory));
    #[cfg(feature = "sensor-git-health")]
    factories.push(Box::new(GitHealthSensorFactory));

    // Infallible / silent-degrade (18)
    #[cfg(feature = "sensor-capability-hygiene")]
    factories.push(Box::new(CapabilityHygieneSensorFactory));
    #[cfg(feature = "sensor-code-quality")]
    factories.push(Box::new(CodeQualitySensorFactory));
    #[cfg(feature = "sensor-coherence")]
    factories.push(Box::new(CoherenceSensorFactory));
    #[cfg(feature = "sensor-decision-diversity")]
    factories.push(Box::new(DecisionDiversitySensorFactory));
    #[cfg(feature = "sensor-deploy-readiness")]
    factories.push(Box::new(DeployReadinessSensorFactory));
    #[cfg(feature = "sensor-documentation-graph")]
    factories.push(Box::new(DocumentationGraphSensorFactory));
    #[cfg(feature = "sensor-backlog")]
    factories.push(Box::new(BacklogSensorFactory));
    #[cfg(feature = "sensor-domain-calibration")]
    factories.push(Box::new(DomainCalibrationSensorFactory));
    #[cfg(feature = "sensor-federated-patterns")]
    factories.push(Box::new(FederatedPatternsSensorFactory));
    #[cfg(feature = "sensor-human-comms")]
    factories.push(Box::new(HumanCommsSensorFactory));
    #[cfg(feature = "sensor-operator-calibration")]
    factories.push(Box::new(OperatorCalibrationSensorFactory));
    #[cfg(feature = "sensor-rust-health")]
    factories.push(Box::new(RustHealthSensorFactory));
    #[cfg(feature = "sensor-secret-refs")]
    factories.push(Box::new(SecretRefsSensorFactory));
    #[cfg(feature = "sensor-secrets-readiness")]
    factories.push(Box::new(SecretsReadinessSensorFactory));
    #[cfg(feature = "sensor-security-standards")]
    factories.push(Box::new(SecurityStandardsSensorFactory));
    #[cfg(feature = "sensor-skill-coherence")]
    factories.push(Box::new(SkillCoherenceSensorFactory));
    #[cfg(feature = "sensor-supply-chain-review")]
    factories.push(Box::new(SupplyChainReviewSensorFactory));
    #[cfg(feature = "sensor-supply-chain-sca")]
    factories.push(Box::new(SupplyChainScaSensorFactory));
    #[cfg(feature = "sensor-supply-chain-vigilance")]
    factories.push(Box::new(SupplyChainVigilanceSensorFactory));
    #[cfg(feature = "sensor-test-health")]
    factories.push(Box::new(TestHealthSensorFactory));
    #[cfg(feature = "sensor-trust-budget")]
    factories.push(Box::new(TrustBudgetSensorFactory));

    factories
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurogrim_core::sensor::SensorRegistry;

    /// Smoke test: built-in factories register cleanly into a
    /// `SensorRegistry`. Catches duplicate wire-names, factory
    /// `name()` returning empty strings, and any panic in
    /// `Box::new(...)`. Feature-aware: registry length equals
    /// `built_in_factories().len()` (which varies with enabled
    /// `sensor-X` features).
    #[test]
    fn all_built_in_factories_register_into_registry() {
        let factories = built_in_factories();
        let n = factories.len();
        let mut registry = SensorRegistry::new();
        registry.register_all(factories);
        // No duplicate wire-names → registry length matches input.
        assert_eq!(
            registry.len(),
            n,
            "expected {n} built-in factories registered, got {}",
            registry.len()
        );
    }

    /// Wire-name parity: every name from the v4 `run_sensory` match
    /// in `neurogrim-cli/src/main.rs:601-621` must be present in
    /// the registry **when its `sensor-X` feature is enabled**.
    /// Each name is `#[cfg]`-gated so the test passes on any
    /// feature subset.
    #[test]
    fn wire_names_match_v4_run_sensory_dispatch() {
        // Each entry compiles in only when its sensor-X feature
        // is active. Default-features build pulls all 21 (= v4
        // dispatch parity); slim builds pull only the enabled
        // subset.
        let expected: Vec<&str> = vec![
            #[cfg(feature = "sensor-git-health")]
            "git-health",
            #[cfg(feature = "sensor-rust-health")]
            "rust-health",
            #[cfg(feature = "sensor-code-quality")]
            "code-quality",
            #[cfg(feature = "sensor-test-health")]
            "test-health",
            #[cfg(feature = "sensor-deploy-readiness")]
            "deploy-readiness",
            #[cfg(feature = "sensor-security-standards")]
            "security-standards",
            #[cfg(feature = "sensor-coherence")]
            "coherence",
            #[cfg(feature = "sensor-human-comms")]
            "human-comms",
            #[cfg(feature = "sensor-secret-refs")]
            "secret-refs",
            #[cfg(feature = "sensor-docker-topology")]
            "docker-topology",
            #[cfg(feature = "sensor-agent-behavior")]
            "agent-behavior",
            #[cfg(feature = "sensor-skill-coherence")]
            "skill-coherence",
            #[cfg(feature = "sensor-capability-hygiene")]
            "capability-hygiene",
            #[cfg(feature = "sensor-supply-chain-sca")]
            "supply-chain-sca",
            #[cfg(feature = "sensor-supply-chain-vigilance")]
            "supply-chain-vigilance",
            #[cfg(feature = "sensor-supply-chain-review")]
            "supply-chain-review",
            #[cfg(feature = "sensor-domain-calibration")]
            "domain-calibration",
            #[cfg(feature = "sensor-operator-calibration")]
            "operator-calibration",
            #[cfg(feature = "sensor-trust-budget")]
            "trust-budget",
            #[cfg(feature = "sensor-federated-patterns")]
            "federated-patterns",
            // Fork C: secrets-readiness orphan reclaimed in Phase 3.
            #[cfg(feature = "sensor-secrets-readiness")]
            "secrets-readiness",
            // v2-Feature 5 (2026-05-09) — documentation-graph sensor
            // wired into dispatch alongside its brain-registry domain.
            #[cfg(feature = "sensor-documentation-graph")]
            "documentation-graph",
            // IDE-BACKLOG B0 (2026-06-17) — backlog-symbol sensor.
            #[cfg(feature = "sensor-backlog")]
            "backlog",
            // v2-Feature 7 (2026-05-09) — decision-diversity research
            // sensor wired into dispatch. Always scores 100 (advisory).
            #[cfg(feature = "sensor-decision-diversity")]
            "decision-diversity",
        ];

        let mut registry = SensorRegistry::new();
        registry.register_all(built_in_factories());

        for name in &expected {
            assert!(
                registry.has(name),
                "wire-name {name:?} missing from registry; \
                 feature-aware dispatch parity broken"
            );
        }

        assert_eq!(
            registry.len(),
            expected.len(),
            "registry has {} entries; feature-aware list expects {}",
            registry.len(),
            expected.len()
        );
    }

    /// Factory `name()` and `build().<no name on Sensor by design>`
    /// — but the registry's `register_all` would panic if any
    /// factory's name conflicted. This test rehearses the registry
    /// dispatch path for a non-trivial subset of sensors. Gated on
    /// `sensor-human-comms` since it builds + invokes that sensor.
    #[cfg(feature = "sensor-human-comms")]
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
