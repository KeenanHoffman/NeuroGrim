//! A9 — Capability matcher pre-dispatch subgate + `CapabilityRegistry` trait.
//!
//! Lifts the IDE's `capability/mod.rs` + `capability/batch_approval.rs`
//! shape into substrate. The pattern: a `CapabilityRegistry` decides
//! whether each pipeline's capability identifier is currently declared
//! allowed; a `CapabilitySubgate` reads the registry and refuses
//! dispatches that aren't allowed.
//!
//! ## Why a pluggable registry
//!
//! Capability enforcement is operator-specific: the IDE has
//! `v9-enforcement.json` matrices + batch-approval registries; cereGrim
//! may have a simpler allowlist. The substrate provides the trait + a
//! stub `AllowAll` default; operators register a real registry during
//! host construction.
//!
//! ## Capability identifier scheme
//!
//! The registry receives the full `Pipeline` — it can match on `pipeline.id`,
//! `pipeline.audit_class`, `pipeline.effect_class`, params, or any combination.
//! The substrate doesn't impose a single scheme; it's the registry's choice.

use crate::governance::{GovernanceRefusal, PreDispatchSubgate};
use crate::pipeline::{ParamMap, Pipeline};
use std::sync::Arc;

/// Result of a capability check. `Allowed` permits dispatch; `Refused`
/// blocks it (and the registry-provided reason becomes the subgate's
/// `GovernanceRefusal::Subgate.reason`).
#[derive(Debug, Clone)]
pub enum CapabilityDecision {
    Allowed,
    Refused { reason: String },
}

/// Pluggable capability check. Real implementations might consult an
/// enforcement matrix, a per-pipeline allowlist, a batch-approval registry,
/// or any combination.
pub trait CapabilityRegistry: Send + Sync {
    /// **P5a — pane-aware capability decisions.** `params` is the dispatch's
    /// `ParamMap`, threaded so a registry can decide based on the specific
    /// pane (e.g. per-pane verified-origin / grant state via
    /// `params["pane_id"]`), not just the pipeline shape. The `AllowAll`
    /// default ignores it; a real IDE enforcement matrix reads it. Non-browser
    /// pipelines carry no `pane_id`, so a pane-aware registry must treat
    /// "no pane" as its own case rather than assume one.
    fn check(&self, pipeline: &Pipeline, params: &ParamMap) -> CapabilityDecision;
}

/// Default registry that allows every dispatch — useful for tests and for
/// deployments that haven't yet wired a real registry. Production deployments
/// should register an enforcing registry.
pub struct AllowAll;

impl CapabilityRegistry for AllowAll {
    fn check(&self, _pipeline: &Pipeline, _params: &ParamMap) -> CapabilityDecision {
        CapabilityDecision::Allowed
    }
}

/// Pre-dispatch subgate that consults a `CapabilityRegistry`. Composes via
/// A4. The registry-provided refusal reason flows through to the trace
/// `failure_reason` so operators can debug capability denials from
/// trace.jsonl alone.
pub struct CapabilitySubgate {
    name: String,
    registry: Arc<dyn CapabilityRegistry>,
}

impl CapabilitySubgate {
    pub fn new(name: impl Into<String>, registry: Arc<dyn CapabilityRegistry>) -> Self {
        Self {
            name: name.into(),
            registry,
        }
    }
}

impl PreDispatchSubgate for CapabilitySubgate {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self, pipeline: &Pipeline, params: &ParamMap) -> Result<(), GovernanceRefusal> {
        match self.registry.check(pipeline, params) {
            CapabilityDecision::Allowed => Ok(()),
            CapabilityDecision::Refused { reason } => Err(GovernanceRefusal::Subgate {
                name: self.name.clone(),
                reason,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{AuditClass, EffectClass, Tunability, Visibility};

    fn make_pipeline(id: &str) -> Pipeline {
        Pipeline {
            id: id.to_string(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorOnly,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::WorldEffect,
            params: serde_json::json!({}),
            preconditions: vec![],
            steps: vec![],
            description: String::new(),
            when_to_use: String::new(),
            bypasses_kill_switch: false,
        }
    }

    #[test]
    fn allow_all_default_permits_every_dispatch() {
        let subgate = CapabilitySubgate::new("test-capability", Arc::new(AllowAll));
        subgate.check(&make_pipeline("t/anything"), &ParamMap::new()).unwrap();
        subgate.check(&make_pipeline("t/anything-else"), &ParamMap::new()).unwrap();
    }

    struct AllowlistRegistry {
        allowed: Vec<String>,
    }
    impl CapabilityRegistry for AllowlistRegistry {
        fn check(&self, pipeline: &Pipeline, _params: &ParamMap) -> CapabilityDecision {
            if self.allowed.iter().any(|id| id == &pipeline.id) {
                CapabilityDecision::Allowed
            } else {
                CapabilityDecision::Refused {
                    reason: format!("pipeline `{}` not in allowlist", pipeline.id),
                }
            }
        }
    }

    #[test]
    fn registry_refusal_propagates_through_subgate() {
        let registry = Arc::new(AllowlistRegistry {
            allowed: vec!["t/known".to_string()],
        });
        let subgate = CapabilitySubgate::new("test-capability", registry);
        subgate.check(&make_pipeline("t/known"), &ParamMap::new()).unwrap();
        let err = subgate.check(&make_pipeline("t/unknown"), &ParamMap::new()).unwrap_err();
        match err {
            GovernanceRefusal::Subgate { name, reason } => {
                assert_eq!(name, "test-capability");
                assert!(reason.contains("not in allowlist"));
                assert!(reason.contains("t/unknown"));
            }
            other => panic!("expected Subgate refusal, got {:?}", other),
        }
    }

    /// P5a — a registry can now make a per-pane decision because `check`
    /// receives the dispatch `params`. Here only `pane-allowed` is permitted;
    /// a dispatch for `pane-blocked` (same pipeline) is refused, and a
    /// non-browser dispatch with no `pane_id` falls back to a pipeline-only
    /// allow.
    struct PaneAwareRegistry;
    impl CapabilityRegistry for PaneAwareRegistry {
        fn check(&self, pipeline: &Pipeline, params: &ParamMap) -> CapabilityDecision {
            match params.get("pane_id").and_then(|v| v.as_str()) {
                Some("pane-allowed") => CapabilityDecision::Allowed,
                Some(other) => CapabilityDecision::Refused {
                    reason: format!("pane `{other}` not verified for `{}`", pipeline.id),
                },
                // No pane_id (non-browser pipeline) — pipeline-only decision.
                None => CapabilityDecision::Allowed,
            }
        }
    }

    #[test]
    fn capability_decision_can_be_pane_aware() {
        let subgate = CapabilitySubgate::new("ide-capability", Arc::new(PaneAwareRegistry));
        let p = make_pipeline("browser-dom-write/click");

        let mut allowed = ParamMap::new();
        allowed.insert("pane_id".to_string(), serde_json::json!("pane-allowed"));
        subgate.check(&p, &allowed).unwrap();

        let mut blocked = ParamMap::new();
        blocked.insert("pane_id".to_string(), serde_json::json!("pane-blocked"));
        let err = subgate.check(&p, &blocked).unwrap_err();
        assert!(matches!(err, GovernanceRefusal::Subgate { .. }));

        // Non-browser pipeline, no pane_id — the fallback path permits.
        let work = make_pipeline("work-broker/next");
        subgate.check(&work, &ParamMap::new()).unwrap();
    }
}
