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
use crate::pipeline::Pipeline;
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
    fn check(&self, pipeline: &Pipeline) -> CapabilityDecision;
}

/// Default registry that allows every dispatch — useful for tests and for
/// deployments that haven't yet wired a real registry. Production deployments
/// should register an enforcing registry.
pub struct AllowAll;

impl CapabilityRegistry for AllowAll {
    fn check(&self, _pipeline: &Pipeline) -> CapabilityDecision {
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

    fn check(&self, pipeline: &Pipeline) -> Result<(), GovernanceRefusal> {
        match self.registry.check(pipeline) {
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
        subgate.check(&make_pipeline("t/anything")).unwrap();
        subgate.check(&make_pipeline("t/anything-else")).unwrap();
    }

    struct AllowlistRegistry {
        allowed: Vec<String>,
    }
    impl CapabilityRegistry for AllowlistRegistry {
        fn check(&self, pipeline: &Pipeline) -> CapabilityDecision {
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
        subgate.check(&make_pipeline("t/known")).unwrap();
        let err = subgate.check(&make_pipeline("t/unknown")).unwrap_err();
        match err {
            GovernanceRefusal::Subgate { name, reason } => {
                assert_eq!(name, "test-capability");
                assert!(reason.contains("not in allowlist"));
                assert!(reason.contains("t/unknown"));
            }
            other => panic!("expected Subgate refusal, got {:?}", other),
        }
    }
}
