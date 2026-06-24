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

/// Governance Composer — holds trust budget + kill switch state; provides
/// pre-dispatch + post-dispatch checks that the Runner calls automatically
/// for Surfaced pipelines.
pub struct GovernanceComposer {
    trust_budget: TrustBudgetMvp,
    kill_switch: KillSwitchMvp,
}

impl GovernanceComposer {
    /// Create a new Governance Composer with the given trust-budget ceiling
    /// (MVP single global pool; dispatch-count unit).
    pub fn new(trust_budget_ceiling: u64) -> Self {
        Self {
            trust_budget: TrustBudgetMvp::new(trust_budget_ceiling),
            kill_switch: KillSwitchMvp::new(),
        }
    }

    /// `check-trust-budget` + `check-kill-switch` composed pre-dispatch.
    /// Returns the first refusal found, or Ok if both pass.
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
            // arm-kill-switch is Surfaced (operator-controllable via the agent
            // if the operator opts in via cluster manifest). MVP brokers
            // declare it in their catalogs as a Surfaced pipeline; the
            // dispatch path of `arm-kill-switch` calls
            // GovernanceComposer::arm_kill_switch() via a framework leaf-op.
            Pipeline {
                id: format!("{}/arm-kill-switch", broker_id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorOnly,
                audit_class: AuditClass::Governance,
                effect_class: EffectClass::WorldEffect,
                params: serde_json::json!({}),
                preconditions: vec![],
                steps: vec![],
                description: "Arm the (global) kill switch; halts all subsequent dispatches.".to_string(),
                when_to_use: "Operator-controlled emergency halt. The LLM must NOT arm this without operator approval.".to_string(),
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
    fn canonical_governance_pipelines_includes_4_internal_plus_arm() {
        let pipelines = GovernanceComposer::canonical_governance_pipelines("test-broker");
        assert_eq!(pipelines.len(), 5);
        let surfaced: Vec<_> = pipelines
            .iter()
            .filter(|p| matches!(p.visibility, Visibility::Surfaced))
            .collect();
        assert_eq!(surfaced.len(), 1);
        assert!(surfaced[0].id.ends_with("/arm-kill-switch"));
        // The 4 default-composed governance pipelines are Internal
        let internal: Vec<_> = pipelines
            .iter()
            .filter(|p| matches!(p.visibility, Visibility::Internal))
            .collect();
        assert_eq!(internal.len(), 4);
        // All must carry audit_class: Governance
        assert!(pipelines.iter().all(|p| matches!(p.audit_class, AuditClass::Governance)));
        // All must carry Untunable tier (for the 4 Internal) OR OperatorOnly (for arm)
        for p in &pipelines {
            match p.visibility {
                Visibility::Internal => assert!(matches!(p.tunability, Tunability::Untunable)),
                Visibility::Surfaced => assert!(matches!(p.tunability, Tunability::OperatorOnly)),
            }
        }
    }
}
