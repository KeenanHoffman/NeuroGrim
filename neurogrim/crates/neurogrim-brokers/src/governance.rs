//! BB #19 — Governance Composer.
//!
//! Framework-provided governance pipelines that compose into Surfaced
//! pipelines per BROKER-INTERNALS.md §2.4. Implementation is
//! framework-internal (the 4 default pipelines are Rust functions invoked
//! by the Pipeline Runner directly, NOT through the broker catalog) but
//! brokers EXPOSE them via `governance_pipelines()` for operator visibility
//! per the reachability channel split (§4 + LB-3 closure).
//!
//! ## MVP scope (Wave 4)
//!
//! Four default-composed governance pipelines per spec, automatically run by
//! the Runner before/after every **Surfaced** pipeline (Internal pipelines
//! skip governance per spec):
//!
//! 1. `check-trust-budget` — refuses if over-budget. Per ultra-pass U7
//!    simplification: single global pool, unit = `dispatch-count`,
//!    fixed-ceiling, manual-reset-only. Degenerate; S1-T rewrites with the
//!    full §4 trust-budget formalization (units / scopes / allocation /
//!    replenishment).
//! 2. `check-kill-switch` — refuses if armed. MVP scopes: global only.
//!    Per-pipeline + per-broker scopes land in S1-T.
//! 3. `record-dispatch` — writes audit anchor at dispatch start. MVP uses
//!    the Trace Sink (BB #12) as the audit ledger; spec's separate audit
//!    ledger lands when needed.
//! 4. `record-outcome` — writes at dispatch end. Same Trace Sink path.
//!
//! Plus operator-controllable Surfaced pipeline:
//! - `arm-kill-switch` — OperatorOnly tunability; brokers opt-in by
//!   including it in their catalog (Wave 5: Work Broker includes it).

use crate::pipeline::{AuditClass, EffectClass, Pipeline, Tunability, Visibility};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum GovernanceRefusal {
    #[error("trust budget exhausted: {used}/{ceiling}")]
    TrustBudgetExhausted { used: u64, ceiling: u64 },

    #[error("kill switch armed (scope: {scope})")]
    KillSwitchArmed { scope: String },

    /// A4 — refusal produced by a registered pre-dispatch subgate
    /// (rate-limit, capability, admission, …). `name` identifies the subgate
    /// for trace attribution; `reason` is the subgate-specific message.
    #[error("subgate `{name}` refused dispatch: {reason}")]
    Subgate { name: String, reason: String },
}

/// MVP simplified trust budget per ultra-pass U7.
/// Single global pool; dispatch-count unit; fixed-ceiling; manual reset only.
/// **S1-T MUST rewrite this** with the full §4 formalization.
#[derive(Debug)]
struct TrustBudgetMvp {
    used: std::sync::atomic::AtomicU64,
    ceiling: u64,
}

impl TrustBudgetMvp {
    fn new(ceiling: u64) -> Self {
        Self {
            used: std::sync::atomic::AtomicU64::new(0),
            ceiling,
        }
    }

    fn check(&self) -> Result<(), GovernanceRefusal> {
        let used = self.used.load(std::sync::atomic::Ordering::Acquire);
        if used >= self.ceiling {
            Err(GovernanceRefusal::TrustBudgetExhausted {
                used,
                ceiling: self.ceiling,
            })
        } else {
            Ok(())
        }
    }

    fn consume(&self) {
        self.used.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }

    fn reset(&self) {
        self.used.store(0, std::sync::atomic::Ordering::Release);
    }

    fn state(&self) -> (u64, u64) {
        (
            self.used.load(std::sync::atomic::Ordering::Acquire),
            self.ceiling,
        )
    }
}

/// Kill switch state. MVP: global scope only.
#[derive(Debug)]
struct KillSwitchMvp {
    armed: std::sync::atomic::AtomicBool,
}

impl KillSwitchMvp {
    fn new() -> Self {
        Self {
            armed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn check(&self) -> Result<(), GovernanceRefusal> {
        if self.armed.load(std::sync::atomic::Ordering::Acquire) {
            Err(GovernanceRefusal::KillSwitchArmed {
                scope: "global".to_string(),
            })
        } else {
            Ok(())
        }
    }

    fn arm(&self) {
        self.armed
            .store(true, std::sync::atomic::Ordering::Release);
    }

    fn disarm(&self) {
        self.armed
            .store(false, std::sync::atomic::Ordering::Release);
    }

    fn is_armed(&self) -> bool {
        self.armed.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// A4 — pluggable pre-dispatch governance subgate. Implementations carry
/// their own state (e.g., a sliding-window rate limiter, a capability
/// matcher) and return a refusal when the dispatch should be blocked.
///
/// Registration order = check order; subgates run AFTER the framework's
/// `check-trust-budget` + `check-kill-switch` and BEFORE `record-dispatch`,
/// per the spec's canonical composition order. This slot is the
/// "post-kill-switch / pre-record-dispatch" insertion point for the
/// IDE-driven subgates A7 (rate-limit), A8 (system-pressure), A9
/// (capability) — and any operator-added subgates beyond those.
pub trait PreDispatchSubgate: Send + Sync {
    /// Name used in trace attribution + error messages.
    fn name(&self) -> &str;
    /// Decide whether to permit the dispatch. Return `Err(GovernanceRefusal
    /// ::Subgate { name, reason })` to block; pass `self.name().to_string()`
    /// as `name` so the refusal correctly identifies which subgate fired.
    fn check(&self, pipeline: &Pipeline) -> Result<(), GovernanceRefusal>;
}

/// Governance Composer — holds trust budget + kill switch state + a
/// registry of pluggable pre-dispatch subgates (A4); provides pre-dispatch
/// + post-dispatch checks that the Runner calls automatically for Surfaced
/// pipelines.
pub struct GovernanceComposer {
    trust_budget: TrustBudgetMvp,
    kill_switch: KillSwitchMvp,
    /// A4 — pluggable pre-dispatch subgates. RwLock<Vec<...>> so registration
    /// is lockless on the hot dispatch path (read lock per dispatch).
    pre_dispatch_subgates: std::sync::RwLock<Vec<Arc<dyn PreDispatchSubgate>>>,
}

impl GovernanceComposer {
    /// Create a new Governance Composer with the given trust-budget ceiling
    /// (MVP single global pool; dispatch-count unit).
    pub fn new(trust_budget_ceiling: u64) -> Self {
        Self {
            trust_budget: TrustBudgetMvp::new(trust_budget_ceiling),
            kill_switch: KillSwitchMvp::new(),
            pre_dispatch_subgates: std::sync::RwLock::new(Vec::new()),
        }
    }

    /// A4 — register a pre-dispatch subgate. Subgates fire in registration
    /// order after the framework's trust-budget + kill-switch checks; the
    /// first subgate that returns `Err` short-circuits the dispatch with
    /// `GovernanceRefusal::Subgate { name, reason }`.
    ///
    /// Operator's main binary calls this during host construction to wire
    /// A7 (rate-limit), A8 (system-pressure), A9 (capability), etc.
    pub fn register_pre_dispatch_subgate(&self, subgate: Arc<dyn PreDispatchSubgate>) {
        self.pre_dispatch_subgates
            .write()
            .expect("pre_dispatch_subgates lock poisoned")
            .push(subgate);
    }

    /// Number of currently-registered pre-dispatch subgates. Used by tests
    /// + diagnostics.
    pub fn pre_dispatch_subgate_count(&self) -> usize {
        self.pre_dispatch_subgates
            .read()
            .expect("pre_dispatch_subgates lock poisoned")
            .len()
    }

    /// `check-trust-budget` + `check-kill-switch` composed pre-dispatch.
    /// Returns the first refusal found, or Ok if both pass.
    ///
    /// **A1/C7 fix:** the kill-switch check is gated by the dispatching
    /// pipeline's `bypasses_kill_switch` field. Framework-provided
    /// `arm-kill-switch` and `disengage-kill-switch` carry that flag so they
    /// can dispatch even while armed (otherwise arming creates a permanent
    /// dead-end — operator can never disengage). Per V0-RETRO §C7 + ultra-
    /// pass U2 (B-64).
    pub fn pre_dispatch_checks_for(&self, pipeline: &Pipeline) -> Result<(), GovernanceRefusal> {
        self.trust_budget.check()?;
        if !pipeline.bypasses_kill_switch {
            self.kill_switch.check()?;
        }
        // A4 — run extended subgates in registration order. Slot:
        // post-kill-switch / pre-record-dispatch per spec composition order.
        let subgates = self
            .pre_dispatch_subgates
            .read()
            .expect("pre_dispatch_subgates lock poisoned");
        for subgate in subgates.iter() {
            subgate.check(pipeline)?;
        }
        Ok(())
    }

    /// Legacy entry point — pre-dispatch checks WITHOUT knowing which
    /// pipeline. Kept for backward-compat (tests that pre-flight governance
    /// independently of a specific Pipeline). Always honors the kill-switch
    /// (equivalent to `bypasses_kill_switch = false`).
    pub fn pre_dispatch_checks(&self) -> Result<(), GovernanceRefusal> {
        self.trust_budget.check()?;
        self.kill_switch.check()?;
        Ok(())
    }

    /// `record-dispatch` — called at dispatch start; consumes one unit
    /// from the trust budget. (Spec's "record-dispatch" writes the audit
    /// anchor; in MVP the Trace Sink itself is the audit ledger, so we just
    /// debit + let the Runner record the trace.)
    pub fn record_dispatch(&self) {
        self.trust_budget.consume();
    }

    /// Arm the (global) kill switch. Operator-controlled via the
    /// `arm-kill-switch` Surfaced governance pipeline.
    pub fn arm_kill_switch(&self) {
        self.kill_switch.arm();
    }

    /// Disarm the (global) kill switch. Operator-only.
    pub fn disarm_kill_switch(&self) {
        self.kill_switch.disarm();
    }

    pub fn is_kill_switch_armed(&self) -> bool {
        self.kill_switch.is_armed()
    }

    /// Reset the trust budget to zero. MVP only path is operator-initiated
    /// (no time-decay, no metric-driven). S1-T expands per §4.
    pub fn reset_trust_budget(&self) {
        self.trust_budget.reset();
    }

    pub fn trust_budget_state(&self) -> (u64, u64) {
        self.trust_budget.state()
    }

    /// Returns the canonical governance pipeline metadata for brokers to
    /// expose via `governance_pipelines()` per the reachability channel
    /// split (§4 + LB-3 closure). These pipelines are framework-internal at
    /// dispatch (the GovernanceComposer methods above handle them); they
    /// appear in the catalog ONLY for operator visibility + the agent's
    /// awareness routing.
    ///
    /// **A1/C7 fix:** `arm-kill-switch` + `disengage-kill-switch` now carry
    /// a real `Step::Leaf` (was empty in V0; the Surfaced pipelines returned
    /// vacuous 0-step success without ever flipping the armed flag, which
    /// was the C7 root cause). Brokers including these in their catalogs
    /// MUST implement the `arm_kill_switch` and `disengage_kill_switch`
    /// leaf-ops (call `GovernanceComposer::arm_kill_switch()` /
    /// `disengage_kill_switch()` on the shared `Arc<GovernanceComposer>`).
    /// Both pipelines carry `bypasses_kill_switch = true` (per B-64) so the
    /// disengage path stays reachable while armed.
    pub fn canonical_governance_pipelines(broker_id: &str) -> Vec<Pipeline> {
        vec![
            framework_pipeline(
                broker_id,
                "check-trust-budget",
                "Refuses dispatch if the broker's trust budget is exhausted.",
                "Composed automatically before every Surfaced pipeline.",
            ),
            framework_pipeline(
                broker_id,
                "check-kill-switch",
                "Refuses dispatch if the kill switch is armed.",
                "Composed automatically before every Surfaced pipeline.",
            ),
            framework_pipeline(
                broker_id,
                "record-dispatch",
                "Writes an audit anchor at dispatch start; consumes one trust-budget unit.",
                "Composed automatically by the Runner.",
            ),
            framework_pipeline(
                broker_id,
                "record-outcome",
                "Writes the audit outcome at dispatch end.",
                "Composed automatically by the Runner.",
            ),
            // arm-kill-switch is Surfaced (operator-controllable). The
            // canonical pipeline now carries a real Leaf step (A1/C7 fix).
            Pipeline {
                id: format!("{}/arm-kill-switch", broker_id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorOnly,
                audit_class: AuditClass::Governance,
                effect_class: EffectClass::WorldEffect,
                params: serde_json::json!({}),
                preconditions: vec![],
                steps: vec![crate::pipeline::Step::Leaf {
                    leaf_op: "arm_kill_switch".to_string(),
                }],
                description: "Arm the (global) kill switch; halts all subsequent dispatches.".to_string(),
                when_to_use: "Operator-controlled emergency halt. The LLM must NOT arm this without operator approval.".to_string(),
                bypasses_kill_switch: true,
            },
            // disengage-kill-switch is Surfaced + OperatorOnly + bypasses
            // (otherwise the operator could never disengage after arming).
            Pipeline {
                id: format!("{}/disengage-kill-switch", broker_id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorOnly,
                audit_class: AuditClass::Governance,
                effect_class: EffectClass::WorldEffect,
                params: serde_json::json!({}),
                preconditions: vec![],
                steps: vec![crate::pipeline::Step::Leaf {
                    leaf_op: "disengage_kill_switch".to_string(),
                }],
                description: "Disengage the (global) kill switch; allows subsequent dispatches to resume.".to_string(),
                when_to_use: "Operator-controlled. Only invoke after confirming the cause that prompted arming has been resolved.".to_string(),
                bypasses_kill_switch: true,
            },
        ]
    }
}

fn framework_pipeline(
    broker_id: &str,
    name: &str,
    description: &str,
    when_to_use: &str,
) -> Pipeline {
    Pipeline {
        id: format!("{}/{}", broker_id, name),
        visibility: Visibility::Internal,
        tunability: Tunability::Untunable,
        audit_class: AuditClass::Governance,
        effect_class: EffectClass::ReadOnly,
        params: serde_json::json!({}),
        preconditions: vec![],
        steps: vec![],
        description: description.to_string(),
        when_to_use: when_to_use.to_string(),
        bypasses_kill_switch: false,
    }
}

/// Helper for downstream consumers: the shared `Arc<GovernanceComposer>` is
/// what the Runner + brokers both hold.
pub type SharedGovernance = Arc<GovernanceComposer>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_budget_refuses_when_exhausted() {
        let g = GovernanceComposer::new(2);
        assert!(g.pre_dispatch_checks().is_ok());
        g.record_dispatch();
        assert!(g.pre_dispatch_checks().is_ok());
        g.record_dispatch();
        // Now used = 2 = ceiling
        let err = g.pre_dispatch_checks().unwrap_err();
        assert!(matches!(err, GovernanceRefusal::TrustBudgetExhausted { .. }));
    }

    #[test]
    fn trust_budget_reset_restores_capacity() {
        let g = GovernanceComposer::new(1);
        g.record_dispatch();
        assert!(g.pre_dispatch_checks().is_err());
        g.reset_trust_budget();
        assert!(g.pre_dispatch_checks().is_ok());
        let (used, ceiling) = g.trust_budget_state();
        assert_eq!(used, 0);
        assert_eq!(ceiling, 1);
    }

    #[test]
    fn kill_switch_refuses_when_armed() {
        let g = GovernanceComposer::new(100);
        assert!(g.pre_dispatch_checks().is_ok());
        g.arm_kill_switch();
        let err = g.pre_dispatch_checks().unwrap_err();
        assert!(matches!(err, GovernanceRefusal::KillSwitchArmed { .. }));
        assert!(g.is_kill_switch_armed());
        g.disarm_kill_switch();
        assert!(!g.is_kill_switch_armed());
        assert!(g.pre_dispatch_checks().is_ok());
    }

    #[test]
    fn canonical_governance_pipelines_includes_4_internal_plus_arm_plus_disengage() {
        let pipelines = GovernanceComposer::canonical_governance_pipelines("test-broker");
        // A1 fix: was 5 (arm only); now 6 (arm + disengage).
        assert_eq!(pipelines.len(), 6);
        let surfaced: Vec<_> = pipelines
            .iter()
            .filter(|p| matches!(p.visibility, Visibility::Surfaced))
            .collect();
        assert_eq!(surfaced.len(), 2);
        assert!(surfaced.iter().any(|p| p.id.ends_with("/arm-kill-switch")));
        assert!(surfaced.iter().any(|p| p.id.ends_with("/disengage-kill-switch")));
        // The 4 default-composed governance pipelines are Internal
        let internal: Vec<_> = pipelines
            .iter()
            .filter(|p| matches!(p.visibility, Visibility::Internal))
            .collect();
        assert_eq!(internal.len(), 4);
        // All must carry audit_class: Governance
        assert!(pipelines.iter().all(|p| matches!(p.audit_class, AuditClass::Governance)));
        // All must carry Untunable tier (for the 4 Internal) OR OperatorOnly (for arm/disengage)
        for p in &pipelines {
            match p.visibility {
                Visibility::Internal => assert!(matches!(p.tunability, Tunability::Untunable)),
                Visibility::Surfaced => assert!(matches!(p.tunability, Tunability::OperatorOnly)),
                Visibility::AuditOnly => panic!("canonical governance pipelines should never use AuditOnly"),
            }
        }
        // A1.5 / B-64: arm + disengage must carry bypasses_kill_switch = true.
        for p in &surfaced {
            assert!(p.bypasses_kill_switch, "{} must bypass kill-switch", p.id);
        }
        for p in &internal {
            assert!(!p.bypasses_kill_switch, "{} must NOT bypass kill-switch", p.id);
        }
    }

    #[test]
    fn arm_kill_switch_canonical_pipeline_has_real_leaf_step() {
        // A1/C7 fix: V0 had `steps: vec![]` — arm-kill-switch would
        // "succeed" without ever flipping the armed flag. Regression test:
        // the canonical arm-kill-switch pipeline must dispatch a leaf-op
        // named `arm_kill_switch`.
        let pipelines = GovernanceComposer::canonical_governance_pipelines("t");
        let arm = pipelines
            .iter()
            .find(|p| p.id == "t/arm-kill-switch")
            .expect("arm-kill-switch must exist");
        assert_eq!(arm.steps.len(), 1);
        match &arm.steps[0] {
            crate::pipeline::Step::Leaf { leaf_op } => {
                assert_eq!(leaf_op, "arm_kill_switch");
            }
            other => panic!("arm-kill-switch step must be Leaf, got {:?}", other),
        }
        let disengage = pipelines
            .iter()
            .find(|p| p.id == "t/disengage-kill-switch")
            .expect("disengage-kill-switch must exist");
        assert_eq!(disengage.steps.len(), 1);
        match &disengage.steps[0] {
            crate::pipeline::Step::Leaf { leaf_op } => {
                assert_eq!(leaf_op, "disengage_kill_switch");
            }
            other => panic!("disengage-kill-switch step must be Leaf, got {:?}", other),
        }
    }

    /// A4 — pre-dispatch subgates registered in order; first refusal
    /// short-circuits the dispatch with `GovernanceRefusal::Subgate`.
    #[test]
    fn pre_dispatch_subgates_fire_in_registration_order() {
        struct AlwaysAllow;
        impl PreDispatchSubgate for AlwaysAllow {
            fn name(&self) -> &str {
                "always-allow"
            }
            fn check(&self, _: &Pipeline) -> Result<(), GovernanceRefusal> {
                Ok(())
            }
        }
        struct AlwaysRefuse {
            counter: std::sync::atomic::AtomicU64,
        }
        impl PreDispatchSubgate for AlwaysRefuse {
            fn name(&self) -> &str {
                "always-refuse"
            }
            fn check(&self, _: &Pipeline) -> Result<(), GovernanceRefusal> {
                self.counter
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                Err(GovernanceRefusal::Subgate {
                    name: self.name().to_string(),
                    reason: "test refusal".to_string(),
                })
            }
        }

        let g = GovernanceComposer::new(100);
        assert_eq!(g.pre_dispatch_subgate_count(), 0);
        let refuser = Arc::new(AlwaysRefuse {
            counter: std::sync::atomic::AtomicU64::new(0),
        });
        g.register_pre_dispatch_subgate(Arc::new(AlwaysAllow));
        g.register_pre_dispatch_subgate(refuser.clone());
        // Second subgate registered after first; verify count.
        assert_eq!(g.pre_dispatch_subgate_count(), 2);

        let p = Pipeline {
            id: "t/x".to_string(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorOnly,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::ReadOnly,
            params: serde_json::json!({}),
            preconditions: vec![],
            steps: vec![],
            description: String::new(),
            when_to_use: String::new(),
            bypasses_kill_switch: false,
        };
        let err = g.pre_dispatch_checks_for(&p).unwrap_err();
        match err {
            GovernanceRefusal::Subgate { name, reason } => {
                assert_eq!(name, "always-refuse");
                assert_eq!(reason, "test refusal");
            }
            other => panic!("expected Subgate refusal, got {:?}", other),
        }
        assert_eq!(
            refuser.counter.load(std::sync::atomic::Ordering::Acquire),
            1,
            "AlwaysRefuse must have run exactly once"
        );
    }

    /// A4 — subgates run AFTER framework checks (trust-budget + kill-switch).
    /// If a framework check fails first, subgates don't fire.
    #[test]
    fn pre_dispatch_subgates_skipped_when_framework_check_fails_first() {
        struct CountingSubgate {
            count: std::sync::atomic::AtomicU64,
        }
        impl PreDispatchSubgate for CountingSubgate {
            fn name(&self) -> &str {
                "counter"
            }
            fn check(&self, _: &Pipeline) -> Result<(), GovernanceRefusal> {
                self.count
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                Ok(())
            }
        }
        let g = GovernanceComposer::new(0); // budget = 0 → trust-budget refuses immediately
        let counter = Arc::new(CountingSubgate {
            count: std::sync::atomic::AtomicU64::new(0),
        });
        g.register_pre_dispatch_subgate(counter.clone());
        let p = Pipeline {
            id: "t/x".to_string(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorOnly,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::ReadOnly,
            params: serde_json::json!({}),
            preconditions: vec![],
            steps: vec![],
            description: String::new(),
            when_to_use: String::new(),
            bypasses_kill_switch: false,
        };
        // Trust-budget should refuse before subgate runs
        let err = g.pre_dispatch_checks_for(&p).unwrap_err();
        assert!(matches!(err, GovernanceRefusal::TrustBudgetExhausted { .. }));
        assert_eq!(
            counter.count.load(std::sync::atomic::Ordering::Acquire),
            0,
            "subgate should NOT have run when framework check failed first"
        );
    }

    #[test]
    fn pre_dispatch_checks_for_honors_bypasses_kill_switch() {
        // A1.5 / B-64: when armed, dispatches refuse — EXCEPT pipelines
        // carrying bypasses_kill_switch = true (the arm/disengage escape
        // hatch). Without this the operator's only path out of armed state
        // is permanently blocked.
        let g = GovernanceComposer::new(100);
        g.arm_kill_switch();
        let pipelines = GovernanceComposer::canonical_governance_pipelines("t");
        let arm = pipelines.iter().find(|p| p.id == "t/arm-kill-switch").unwrap();
        let disengage = pipelines.iter().find(|p| p.id == "t/disengage-kill-switch").unwrap();
        // Both must pass pre_dispatch_checks_for even while armed
        assert!(g.pre_dispatch_checks_for(arm).is_ok());
        assert!(g.pre_dispatch_checks_for(disengage).is_ok());
        // A non-bypass pipeline must be refused
        let non_bypass = Pipeline {
            id: "t/something-else".to_string(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorOnly,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::ReadOnly,
            params: serde_json::json!({}),
            preconditions: vec![],
            steps: vec![],
            description: String::new(),
            when_to_use: String::new(),
            bypasses_kill_switch: false,
        };
        let err = g.pre_dispatch_checks_for(&non_bypass).unwrap_err();
        assert!(matches!(err, GovernanceRefusal::KillSwitchArmed { .. }));
    }
}
