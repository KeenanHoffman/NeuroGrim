//! Compile-test verifying every re-exported V5-SDK-1 trait /
//! factory / registry / type is reachable through the
//! `neurogrim_sdk` crate path. If a re-export breaks (e.g., the
//! underlying neurogrim-core trait moves or is renamed), this
//! file fails to compile — catching the breakage at PR time.
//!
//! Tests are runtime no-ops; the compile is the assertion.

use neurogrim_sdk::*;

#[test]
fn theme_b_traits_are_object_safe_via_sdk() {
    // V5-MOD-1 / V5-MOD-2 / V5-MOD-3: object-safe trait dispatch.
    fn _scoring_source(_: Box<dyn ScoringSource>) {}
    fn _scoring_source_factory(_: Box<dyn ScoringSourceFactory>) {}
    fn _sensor(_: Box<dyn Sensor>) {}
    fn _sensor_factory(_: Box<dyn SensorFactory>) {}
    fn _queue_backend(_: std::sync::Arc<dyn QueueBackend>) {}
    fn _queue_backend_factory(_: Box<dyn QueueBackendFactory>) {}
    // Forces the type checker to verify the trait paths resolve.
    let _ = (_scoring_source, _scoring_source_factory);
    let _ = (_sensor, _sensor_factory);
    let _ = (_queue_backend, _queue_backend_factory);
}

#[test]
fn adjacent_stable_traits_reachable() {
    fn _transport(_: Box<dyn Transport>) {}
    fn _secret_backend(_: Box<dyn SecretBackend>) {}
    let _ = (_transport, _secret_backend);
}

#[test]
fn registries_constructible_via_sdk() {
    let _: ScoringSourceRegistry = ScoringSourceRegistry::new();
    let _: SensorRegistry = SensorRegistry::new();
    let _: QueueBackendRegistry = QueueBackendRegistry::new();
}

#[test]
fn queue_built_in_factories_reachable() {
    let factories = queue_built_in_factories();
    // Without sqlite feature: 1 factory (jsonl). With: 2.
    assert!(!factories.is_empty(), "queue built-in factories non-empty");
}

#[test]
fn conformance_types_unified_across_suites() {
    // V5-SDK-1 Phase 1.5 (Fork F1) verification: ConformanceReport
    // and TestResult are the SAME nominal type across all three
    // suites + the canonical `conformance` module.
    fn assert_same<T>(_: &T, _: &T) {}
    let canonical: conformance::ConformanceReport = conformance::ConformanceReport::new();
    let from_sources: scoring_source_conformance::ConformanceReport =
        scoring_source_conformance::ConformanceReport::new();
    let from_sensors: sensor_conformance::ConformanceReport =
        sensor_conformance::ConformanceReport::new();
    let from_queues: queue_backend_conformance::ConformanceReport =
        queue_backend_conformance::ConformanceReport::new();
    // Same nominal type → assert_same compiles only if the four
    // are bound to a single T at trait-resolution time.
    assert_same(&canonical, &from_sources);
    assert_same(&canonical, &from_sensors);
    assert_same(&canonical, &from_queues);
}

#[test]
fn core_types_reachable_via_sdk() {
    // Compile-only: paths resolve.
    fn _agent_output() -> Option<AgentOutput> { None }
    fn _registry() -> Option<BrainRegistry> { None }
    fn _domain() -> Option<DomainDefinition> { None }
    fn _msg() -> Option<QueueMessage> { None }
    fn _stored() -> Option<StoredMessage> { None }
    fn _priority() -> Priority { Priority::Normal }
    let _ = (
        _agent_output, _registry, _domain, _msg, _stored, _priority,
    );
}
