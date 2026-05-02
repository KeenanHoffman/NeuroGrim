//! `Sensor` trait + `SensorFactory` + `SensorRegistry` — the
//! pluggable contract for CMDB-producing sensors (V5-MOD-2 Phase 1,
//! 2026-05-02).
//!
//! Replaces the 21-arm string-match dispatch in
//! [`neurogrim_cli::main::run_sensory`] (`main.rs:599-622`,
//! pre-V5-MOD-2). Phase 3 of V5-MOD-2 routes the dispatch site
//! through the registry and reclaims the `secrets_readiness`
//! orphan as the 21st registered sensor.
//!
//! # The two traits, and why both
//!
//! - [`Sensor`] — the sensor itself. Object-safe so we can dispatch
//!   via `Box<dyn Sensor>`. Carries the `analyze` method that
//!   produces a `cmdb-envelope-v1`-conformant JSON value for one
//!   sensor type (e.g., `GitHealthSensor` shells out to `git status`
//!   and parses the output; `JiraSensor` HTTP-fetches an issue
//!   tracker and counts open P0/P1 bugs).
//! - [`SensorFactory`] — produces a `Sensor` impl for a given
//!   wire-name. The registry holds factories (one per known
//!   sensor); resolving a sensor name to a working impl is
//!   `registry.get(name) → factory.build() → Box<dyn Sensor>`.
//!
//! Splitting build-from-name and analyze lets factories carry
//! per-sensor initialization that's expensive to redo per call
//! (HTTP clients, connection pools) without conflating that with
//! the call-time `analyze(...)` contract. For built-in sensors
//! where the impl is stateless, the factory's `build()` is
//! essentially `Box::new(MySensor)` — no overhead.
//!
//! # Why no inherent fast-path method (V5-MOD-2 Fork B)
//!
//! V5-MOD-1's [`crate::scoring_source::ScoringSource`] uses a
//! two-method pattern (`load_inherent` + trait `load`) to bypass
//! `#[async_trait]`'s future-boxing on the perf-critical scoring
//! dispatch (19 sources × scoring run, p95 ≤ 19 ms ceiling). **Do
//! NOT cargo-cult this pattern for `Sensor`.** Sensor IO is at the
//! seconds-per-call scale (git, cargo audit, network calls); the
//! ~50ns boxing overhead per dispatch is rounding error against
//! that. Sensors implement `analyze` directly via `#[async_trait]`;
//! no inherent method, no `BuiltinSensor` enum dispatcher needed.
//! Saves 21+ duplicate method declarations workspace-wide.
//!
//! # First-arg type — `&str`, not `&Path` (V5-MOD-2 Fork A)
//!
//! Unlike `ScoringSource::load(&Path)`, `Sensor::analyze` takes
//! `&str` for the project root. Reason: the 21 existing analyzers
//! in `neurogrim-sensory/src/` already take `&str`; promoting them
//! to `&Path` would require `to_string_lossy()` round-trips at the
//! trait boundary (Windows non-UTF-8 surrogate-pair regression) or
//! eager migration of all 21 signatures (out of scope for V5-MOD-2;
//! filed as a v5.5 BACKLOG item). The SDK consumer-facing
//! inconsistency between `ScoringSource::load(&Path)` and
//! `Sensor::analyze(&str)` is documented in the V5-SDK epic
//! hand-off note.
//!
//! # Object-safety + async
//!
//! The trait uses `#[async_trait]` (matches the workspace's
//! existing `Transport` trait + V5-MOD-1's `ScoringSource`).
//! Object-safety is empirically proven by the
//! `_object_safety_check_*` tests at the bottom of this module —
//! they fail to compile if a future change accidentally breaks
//! object-safety.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

/// Pluggable contract for CMDB-producing sensors.
///
/// Implementations correspond 1:1 to the wire-names in the
/// `run_sensory` dispatch (`git-health`, `code-quality`, etc.,
/// plus any third-party-registered sensors). Each impl knows how
/// to fetch its data and produce a `cmdb-envelope-v1`-conformant
/// JSON envelope.
///
/// # Contract
///
/// - **`analyze`** — runs the sensor against `project_root` and
///   returns a CMDB envelope. The returned [`Value`] MUST conform
///   to `cmdb-envelope-v1.schema.json` (vendored at
///   `neurogrim-core/data/schemas/`); the Phase 5 conformance
///   suite enforces this. **MUST NOT panic.** Errors should be
///   returned as `Err(...)` with the underlying error chain (for
///   sensors whose historical free-function returned `Result`)
///   OR absorbed into a degraded `Ok(envelope)` with `score: 0`
///   + a finding describing the failure (for sensors whose
///   historical free-function silently degraded). The trait
///   wrapper's job is to preserve operator-visible behavior 1:1
///   from the v4 free-function semantics.
///
/// # Object-safety
///
/// The trait is object-safe (`Box<dyn Sensor>`). The
/// `_object_safety_check_sensor` test in this module fails to
/// compile if a future change accidentally breaks object-safety.
#[async_trait]
pub trait Sensor: Send + Sync {
    /// Run the sensor against the project root, returning a
    /// `cmdb-envelope-v1`-conformant JSON envelope.
    ///
    /// Args:
    /// - `project_root`: the project root path as a string slice.
    ///   Sensors that need a `Path` can do `Path::new(project_root)`
    ///   internally.
    ///
    /// MUST NOT panic. Failure semantics depend on the sensor —
    /// see the trait-level contract.
    async fn analyze(&self, project_root: &str) -> Result<Value>;
}

/// Factory: produces a [`Sensor`] impl for a given wire-name.
///
/// Factories are themselves `Send + Sync` so the registry can be
/// shared across the `tokio` runtime without `Arc<Mutex>` ceremony.
///
/// # Why a factory and not just direct registration of `Box<dyn Sensor>`?
///
/// Factories let an impl carry per-sensor initialization that's
/// expensive to redo per call (HTTP clients, connection pools,
/// configuration parsing). The dispatch path is
/// `registry.get(name) → factory.build() → Box<dyn Sensor>`; the
/// factory amortizes setup, the resulting `Sensor` does the
/// per-call work.
///
/// For built-in sensors where the impl is stateless, the factory's
/// `build()` is essentially `Box::new(MySensor)` — no overhead.
/// For sensors that hold heavy state (an HTTP client, for
/// example), the factory can cache.
pub trait SensorFactory: Send + Sync {
    /// Stable wire-name for this sensor. Matches the dispatch
    /// arm in `run_sensory` (e.g., `"git-health"`).
    fn name(&self) -> &'static str;

    /// Construct a new [`Sensor`] impl. Called by the dispatch
    /// path once per `neurogrim cast <name>` invocation (or
    /// once at startup + cached, depending on factory
    /// implementation).
    fn build(&self) -> Box<dyn Sensor>;
}

/// Hand-rolled registry mapping sensor wire-names to factories
/// that produce [`Sensor`] impls.
///
/// **Why hand-rolled** (V5-MOD-1 plan-critic Subagent 2 finding,
/// reapplied to V5-MOD-2): the workspace has no existing
/// static-registration substrate (`inventory`, `linkme`, `ctor` —
/// none present). The `dependency-discipline` skill enforces a
/// 4-point pre-flight on new deps; this `HashMap`-backed registry
/// is the same ~40 lines with zero supply-chain review burden,
/// and registration is *explicit* (visible in startup code rather
/// than magical at link time).
///
/// # Built-in registration
///
/// `SensorRegistry::new()` returns an empty registry. The 21
/// built-in factories live in `neurogrim-sensory` (via
/// `neurogrim_sensory::built_in_factories()`, Phase 3); the
/// consuming binary calls [`Self::register_all`] at startup to
/// populate. Third-party crates register their own factories
/// via [`Self::register`] alongside the built-ins.
///
/// # Concurrency
///
/// `SensorRegistry` is `Send + Sync` (factories are constrained
/// to `Send + Sync` by the trait). The dispatch path can hold an
/// `Arc<SensorRegistry>` shared across the tokio runtime without
/// `Mutex` ceremony — registration is a startup-time concern.
pub struct SensorRegistry {
    factories: HashMap<&'static str, Box<dyn SensorFactory>>,
}

impl SensorRegistry {
    /// Empty registry. Caller registers factories explicitly.
    pub fn new() -> Self {
        SensorRegistry {
            factories: HashMap::new(),
        }
    }

    /// Register a factory by its `name()`. If a factory with the
    /// same name is already registered, it's replaced
    /// (last-write-wins; intentional so consumers can override
    /// built-ins for testing).
    pub fn register(&mut self, factory: Box<dyn SensorFactory>) {
        let name = factory.name();
        self.factories.insert(name, factory);
    }

    /// Convenience: register multiple factories from an iterator.
    /// Same last-write-wins semantics as [`Self::register`].
    /// The Phase 3 call site is `registry.register_all(
    /// neurogrim_sensory::built_in_factories())` — single
    /// expression to populate the registry from the canonical
    /// built-in list.
    pub fn register_all(
        &mut self,
        factories: impl IntoIterator<Item = Box<dyn SensorFactory>>,
    ) {
        for factory in factories {
            self.register(factory);
        }
    }

    /// Look up a factory by wire-name. Returns `None` if no
    /// factory is registered for that name.
    pub fn get(&self, name: &str) -> Option<&dyn SensorFactory> {
        self.factories.get(name).map(|f| f.as_ref())
    }

    /// Convenience wrapper: look up a factory and immediately
    /// build a `Box<dyn Sensor>`. Returns `None` if no factory is
    /// registered for the given name.
    pub fn build(&self, name: &str) -> Option<Box<dyn Sensor>> {
        self.get(name).map(|f| f.build())
    }

    /// True if a factory is registered for the given wire-name.
    pub fn has(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    /// Iterate over registered wire-names. Useful for diagnostics
    /// and the proposed `--list-sensors` flag (v5.5 polish —
    /// forwarded from V5-MOD-2 plan 🔵 suggestion, mirrors V5-MOD-1's
    /// same suggestion for `--list-scoring-sources`).
    pub fn registered_names(&self) -> impl Iterator<Item = &&'static str> {
        self.factories.keys()
    }

    /// Number of registered factories. O(1).
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// True if no factories are registered.
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }
}

impl Default for SensorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-only test: the trait MUST be object-safe so it can
    /// be dispatched via `Box<dyn Sensor>`. If a future change
    /// accidentally breaks object-safety (e.g., adding a `Self:
    /// Sized` method, a generic non-dispatchable method, or a
    /// method returning `Self`), this function fails to compile.
    #[allow(dead_code)]
    fn _object_safety_check_sensor(_: Box<dyn Sensor>) {
        // Body intentionally empty; the compile-time check is
        // the assertion. The Box<dyn> dispatch is what V5-MOD-2's
        // architecture depends on.
    }

    /// Same compile-only object-safety guard for the factory
    /// trait. The registry holds `Box<dyn SensorFactory>` per
    /// wire-name, so this trait must also be object-safe.
    #[allow(dead_code)]
    fn _object_safety_check_factory(_: Box<dyn SensorFactory>) {
        // Body intentionally empty.
    }

    /// Minimal test impl exercising the `Box<dyn Sensor>` path
    /// end-to-end (early-validation per V5-MOD-1's same pattern).
    /// Catches any future-boxing wart before the conformance
    /// suite generalizes the pattern in Phase 5.
    struct MockSensor;

    #[async_trait]
    impl Sensor for MockSensor {
        async fn analyze(&self, _project_root: &str) -> Result<Value> {
            Ok(serde_json::json!({
                "meta": {
                    "schema_version": "1",
                    "updated_at": "2026-05-02T00:00:00Z",
                    "updated_by": "mock-sensor"
                },
                "score": 100,
                "updated_at": "2026-05-02T00:00:00Z"
            }))
        }
    }

    struct MockFactory;
    impl SensorFactory for MockFactory {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn build(&self) -> Box<dyn Sensor> {
            Box::new(MockSensor)
        }
    }

    #[tokio::test]
    async fn boxed_sensor_can_be_invoked_through_dyn_dispatch() {
        let sensor: Box<dyn Sensor> = Box::new(MockSensor);
        let result = sensor.analyze("./fake-project-root").await;
        assert!(result.is_ok(), "mock sensor must succeed");
        let envelope = result.unwrap();
        assert_eq!(envelope["score"], 100);
        assert_eq!(envelope["meta"]["updated_by"], "mock-sensor");
    }

    #[test]
    fn boxed_factory_can_build_a_sensor() {
        let factory: Box<dyn SensorFactory> = Box::new(MockFactory);
        assert_eq!(factory.name(), "mock");
        let sensor = factory.build();
        // Sensor itself doesn't expose name() (Subagent 1 finding —
        // factory.name() is canonical). The build-and-invoke
        // smoke test is exercised by the test above.
        let _: Box<dyn Sensor> = sensor;
    }

    // ─── Registry tests ─────────────────────────────────────────────

    #[test]
    fn empty_registry_has_no_factories() {
        let reg = SensorRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert_eq!(reg.registered_names().count(), 0);
        assert!(!reg.has("git-health"));
        assert!(reg.get("anything").is_none());
        assert!(reg.build("anything").is_none());
    }

    #[test]
    fn register_then_lookup_and_build() {
        let mut reg = SensorRegistry::new();
        reg.register(Box::new(MockFactory));
        assert!(!reg.is_empty());
        assert_eq!(reg.len(), 1);
        assert!(reg.has("mock"));
        assert!(reg.get("mock").is_some());
        let sensor = reg.build("mock").expect("mock must build");
        // Smoke check the built sensor works end-to-end.
        let _: Box<dyn Sensor> = sensor;
    }

    #[test]
    fn unknown_name_returns_none() {
        let mut reg = SensorRegistry::new();
        reg.register(Box::new(MockFactory));
        assert!(reg.get("does-not-exist").is_none());
        assert!(reg.build("does-not-exist").is_none());
        assert!(!reg.has("does-not-exist"));
    }

    #[test]
    fn register_all_populates_from_iterator() {
        // Rehearses the Phase 3 wire-up shape:
        // `registry.register_all(neurogrim_sensory::built_in_factories())`.
        let factories: Vec<Box<dyn SensorFactory>> = vec![Box::new(MockFactory)];
        let mut reg = SensorRegistry::new();
        reg.register_all(factories);
        assert_eq!(reg.len(), 1);
        assert!(reg.has("mock"));
    }

    /// Registering the same name twice replaces the prior factory
    /// (last-write-wins). Consumers rely on this for test overrides
    /// — e.g., wrap a built-in with an instrumented variant.
    struct MockFactoryV2;
    impl SensorFactory for MockFactoryV2 {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn build(&self) -> Box<dyn Sensor> {
            // V2 produces a sensor that returns a different score —
            // proves the registry actually replaced the factory.
            struct MockV2;
            #[async_trait]
            impl Sensor for MockV2 {
                async fn analyze(&self, _: &str) -> Result<Value> {
                    Ok(serde_json::json!({
                        "meta": {
                            "schema_version": "1",
                            "updated_at": "2026-05-02T00:00:00Z",
                            "updated_by": "mock-v2"
                        },
                        "score": 50,
                        "updated_at": "2026-05-02T00:00:00Z"
                    }))
                }
            }
            Box::new(MockV2)
        }
    }

    #[tokio::test]
    async fn last_write_wins_on_duplicate_registration() {
        let mut reg = SensorRegistry::new();
        reg.register(Box::new(MockFactory));
        reg.register(Box::new(MockFactoryV2));
        let sensor = reg.build("mock").unwrap();
        let envelope = sensor.analyze("./fake").await.unwrap();
        assert_eq!(
            envelope["meta"]["updated_by"], "mock-v2",
            "later registration must replace earlier"
        );
        // Still only one entry under "mock".
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn default_constructor_matches_new() {
        let reg_a = SensorRegistry::new();
        let reg_b = SensorRegistry::default();
        assert_eq!(reg_a.len(), reg_b.len());
        assert!(reg_a.is_empty() && reg_b.is_empty());
    }
}
