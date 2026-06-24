//! Wave 5 reference broker — Work Broker (`[InnateAbility]` role).
//!
//! Wraps the concept of NeuroGrim's `next_ready` backlog dispatcher as a
//! broker. MVP design (per Phase 1 Explore findings + the plan's operator
//! default of "Sense + dispatch only"):
//!
//! - **Cold-store schema:** in-memory `BacklogState` for MVP (would persist
//!   to JSON file in production). Holds the list of work units + their status.
//! - **Working state:** loaded BacklogState mirrored from cold store.
//! - **Overlay shape:** `ActiveWorkOverlay` — the curated top-N ready
//!   work units the agent sees.
//! - **Surfaced pipelines:**
//!   - `work-broker/dispatch-work-unit` — claim a work unit by id
//!   - `work-broker/arm-kill-switch` — operator emergency halt (per BB #19
//!     canonical governance)
//! - **Internal pipelines:**
//!   - `work-broker/work-broker-tick` — refresh overlay (called on each tick)
//!
//! Wave 5.5 (follow-on) would integrate with
//! `neurogrim_sensory::backlog::next_ready()` for real backlog parsing;
//! MVP uses operator-provided work units via cluster manifest extension.

use crate::broker::{Broker, BrokerError, Role, RoleSet, WorldEvent};
use crate::governance::GovernanceComposer;
use crate::overlay::{Overlay, WorkingState};
use crate::pipeline::{
    AuditClass, EffectClass, ParamMap, Pipeline, Step, Tunability, Visibility,
};
use crate::runner::{LeafContext, LeafError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A single work unit (MVP shape).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkUnit {
    pub id: String,
    pub title: String,
    pub status: WorkUnitStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WorkUnitStatus {
    Ready,
    Claimed,
    Blocked,
    Done,
}

/// MVP cold-store / working-state struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BacklogState {
    pub work_units: Vec<WorkUnit>,
}

/// What the agent sees — top-N ready work units (MVP: just the full list of
/// Ready ones; S1-T calibration tunes the curation).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActiveWorkOverlay {
    pub active_work: Vec<WorkUnit>,
    pub recent_claims: Vec<String>,
}

pub struct WorkBroker {
    id: String,
    working_state: WorkingState<BacklogState>,
    overlay: Arc<Overlay<ActiveWorkOverlay>>,
    governance: Arc<GovernanceComposer>,
}

impl WorkBroker {
    /// Create a new Work Broker with the given initial backlog state.
    pub fn new(
        id: impl Into<String>,
        initial: BacklogState,
        governance: Arc<GovernanceComposer>,
    ) -> Self {
        let initial_overlay = curate_overlay(&initial);
        Self {
            id: id.into(),
            working_state: WorkingState::new(initial),
            overlay: Arc::new(Overlay::new(initial_overlay)),
            governance,
        }
    }

    /// The catalog of pipelines this broker exposes. Used by Pipeline Catalog
    /// loader (BB #9) + cluster manifest validation. In MVP we return them
    /// directly (no YAML); production deployment would load from
    /// `<broker_id>-catalog.yaml`.
    pub fn catalog(&self) -> Vec<Pipeline> {
        let mut pipelines = vec![
            // Surfaced: dispatch-work-unit
            Pipeline {
                id: format!("{}/dispatch-work-unit", self.id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorConfirmed,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::HotStoreUpdate,
                params: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "work_unit_id": {
                            "type": "string",
                            "description": "The id of the work unit to claim (must appear in active_work)"
                        }
                    },
                    "required": ["work_unit_id"]
                }),
                preconditions: vec![format!(
                    "active_work"
                )],
                steps: vec![
                    Step::Leaf {
                        leaf_op: "claim_work_unit".to_string(),
                    },
                    Step::Leaf {
                        leaf_op: "refresh_overlay".to_string(),
                    },
                ],
                description: "Claim the named work unit from the active backlog.".to_string(),
                when_to_use:
                    "When the operator is ready to advance the next backlog item. Pick from the active_work list.".to_string(),
            },
            // Internal: work-broker-tick (called on each tick to refresh)
            Pipeline {
                id: format!("{}/work-broker-tick", self.id),
                visibility: Visibility::Internal,
                tunability: Tunability::OperatorOnly,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::HotStoreUpdate,
                params: serde_json::json!({}),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: "refresh_overlay".to_string(),
                }],
                description: "Re-project Active Work overlay from working state.".to_string(),
                when_to_use: "Framework-internal; runs on each tick.".to_string(),
            },
        ];
        // Append canonical governance pipelines (the agent sees these via
        // governance_pipelines() sidecar per reachability channel split).
        // arm-kill-switch is Surfaced (operator-controlled).
        pipelines.extend(GovernanceComposer::canonical_governance_pipelines(&self.id));
        pipelines
    }
}

fn curate_overlay(state: &BacklogState) -> ActiveWorkOverlay {
    ActiveWorkOverlay {
        active_work: state
            .work_units
            .iter()
            .filter(|w| matches!(w.status, WorkUnitStatus::Ready))
            .cloned()
            .collect(),
        recent_claims: state
            .work_units
            .iter()
            .filter(|w| matches!(w.status, WorkUnitStatus::Claimed))
            .map(|w| w.id.clone())
            .collect(),
    }
}

#[async_trait]
impl Broker for WorkBroker {
    fn id(&self) -> &str {
        &self.id
    }

    fn role_set(&self) -> RoleSet {
        RoleSet::single(Role::InnateAbility)
    }

    async fn read_overlay(&self) -> serde_json::Value {
        let snap = self.overlay.load();
        serde_json::to_value(&*snap).unwrap_or(serde_json::Value::Null)
    }

    async fn legal_pipelines(&self) -> Vec<Pipeline> {
        // MVP: return all Surfaced pipelines from catalog. Wave 5.5+ could
        // gate by precondition evaluation against current Overlay.
        self.catalog()
            .into_iter()
            .filter(|p| matches!(p.visibility, Visibility::Surfaced))
            .collect()
    }

    async fn governance_pipelines(&self) -> Vec<Pipeline> {
        // Reachability channel split per §4 + LB-3 closure: governance
        // pipelines exposed via this sidecar separately from
        // legal_pipelines().
        GovernanceComposer::canonical_governance_pipelines(&self.id)
    }

    async fn tick(&self, _event: WorldEvent) -> Result<(), BrokerError> {
        // Re-project overlay from working state.
        let state = self.working_state.lock().await;
        let new_overlay = curate_overlay(&state);
        self.overlay.swap(new_overlay);
        Ok(())
    }

    async fn execute_leaf(
        &self,
        name: &str,
        ctx: LeafContext,
    ) -> Result<serde_json::Value, LeafError> {
        match name {
            "claim_work_unit" => {
                let work_unit_id = ctx
                    .params
                    .get("work_unit_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        LeafError::InputInvalid("missing work_unit_id".to_string())
                    })?
                    .to_string();
                let mut state = self.working_state.lock().await;
                let unit = state
                    .work_units
                    .iter_mut()
                    .find(|w| w.id == work_unit_id)
                    .ok_or_else(|| {
                        LeafError::InputInvalid(format!(
                            "work unit `{}` not found",
                            work_unit_id
                        ))
                    })?;
                if !matches!(unit.status, WorkUnitStatus::Ready) {
                    return Err(LeafError::ExecutionFailed(format!(
                        "work unit `{}` is not ready (status: {:?})",
                        work_unit_id, unit.status
                    )));
                }
                unit.status = WorkUnitStatus::Claimed;
                Ok(serde_json::json!({
                    "claimed": work_unit_id,
                    "status": "claimed"
                }))
            }
            "refresh_overlay" => {
                let state = self.working_state.lock().await;
                let new_overlay = curate_overlay(&state);
                self.overlay.swap(new_overlay);
                Ok(serde_json::json!({"refreshed": true}))
            }
            "arm_kill_switch" => {
                // Surfaced arm-kill-switch pipeline calls this leaf-op.
                self.governance.arm_kill_switch();
                Ok(serde_json::json!({"armed": true}))
            }
            other => Err(LeafError::NotFound(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::PipelineRunner;
    use crate::trace::TraceSink;
    use tempfile::TempDir;

    fn make_demo_state() -> BacklogState {
        BacklogState {
            work_units: vec![
                WorkUnit {
                    id: "B-100".to_string(),
                    title: "Implement Wave 5".to_string(),
                    status: WorkUnitStatus::Ready,
                },
                WorkUnit {
                    id: "B-101".to_string(),
                    title: "Demo end-to-end".to_string(),
                    status: WorkUnitStatus::Ready,
                },
                WorkUnit {
                    id: "B-102".to_string(),
                    title: "Already done".to_string(),
                    status: WorkUnitStatus::Done,
                },
            ],
        }
    }

    fn make_runner_governance() -> (Arc<TraceSink>, Arc<GovernanceComposer>) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        (
            Arc::new(TraceSink::new(path)),
            Arc::new(GovernanceComposer::new(1000)),
        )
    }

    #[tokio::test]
    async fn work_broker_overlay_filters_to_ready() {
        let (_, governance) = make_runner_governance();
        let b = WorkBroker::new("work-broker", make_demo_state(), governance);
        let overlay = b.read_overlay().await;
        let active = overlay["active_work"].as_array().unwrap();
        assert_eq!(active.len(), 2); // only B-100 + B-101 are Ready
        assert!(active.iter().any(|w| w["id"] == "B-100"));
        assert!(active.iter().any(|w| w["id"] == "B-101"));
        assert!(!active.iter().any(|w| w["id"] == "B-102"));
    }

    #[tokio::test]
    async fn work_broker_role_set_is_innate_ability() {
        let (_, governance) = make_runner_governance();
        let b = WorkBroker::new("work-broker", BacklogState::default(), governance);
        assert!(b.role_set().contains(&Role::InnateAbility));
        assert!(!b.role_set().contains(&Role::Sense));
        assert!(!b.role_set().contains(&Role::Embodiment));
    }

    #[tokio::test]
    async fn work_broker_dispatch_via_runner_end_to_end() {
        let (sink, governance) = make_runner_governance();
        let broker: Arc<dyn Broker> = Arc::new(WorkBroker::new(
            "work-broker",
            make_demo_state(),
            governance.clone(),
        ));
        let catalog: Vec<Pipeline> = {
            // Catalog access via downcasting won't work cleanly; just
            // reconstruct the catalog from the same helper.
            let same =
                WorkBroker::new("work-broker", make_demo_state(), governance.clone());
            same.catalog()
        };
        let runner = PipelineRunner::new(sink.clone(), governance.clone());

        // Initial overlay shows 2 active work units
        let overlay_before = broker.read_overlay().await;
        assert_eq!(overlay_before["active_work"].as_array().unwrap().len(), 2);

        // Dispatch dispatch-work-unit for B-100
        let mut params = ParamMap::new();
        params.insert(
            "work_unit_id".to_string(),
            serde_json::Value::String("B-100".to_string()),
        );
        let outcome = runner
            .dispatch(
                broker.clone(),
                &catalog,
                "work-broker/dispatch-work-unit".to_string(),
                params,
            )
            .await
            .expect("dispatch should succeed");

        // The dispatch returns the result of the LAST step (refresh_overlay)
        assert_eq!(outcome.output["refreshed"], true);

        // Overlay should now show only 1 active work unit (B-101) and recent_claims
        // should contain B-100
        let overlay_after = broker.read_overlay().await;
        let active_after = overlay_after["active_work"].as_array().unwrap();
        assert_eq!(active_after.len(), 1);
        assert_eq!(active_after[0]["id"], "B-101");
        let recent = overlay_after["recent_claims"].as_array().unwrap();
        assert!(recent.iter().any(|c| c == "B-100"));

        // Trust budget consumed 1 (Surfaced pipeline)
        let (used, _) = governance.trust_budget_state();
        assert_eq!(used, 1);

        // Trace recorded
        let trace_contents = std::fs::read_to_string(sink.file_path()).unwrap();
        assert_eq!(trace_contents.lines().count(), 1);
        let trace: crate::TraceRecord =
            serde_json::from_str(trace_contents.lines().next().unwrap()).unwrap();
        assert_eq!(trace.pipeline_id, "work-broker/dispatch-work-unit");
        assert_eq!(trace.broker_id, "work-broker");
        assert_eq!(trace.audit_class, "capability");
    }

    #[tokio::test]
    async fn work_broker_dispatch_refuses_unknown_work_unit() {
        let (sink, governance) = make_runner_governance();
        let broker: Arc<dyn Broker> = Arc::new(WorkBroker::new(
            "work-broker",
            make_demo_state(),
            governance.clone(),
        ));
        let catalog = WorkBroker::new("work-broker", make_demo_state(), governance.clone()).catalog();
        let runner = PipelineRunner::new(sink, governance);
        let mut params = ParamMap::new();
        params.insert(
            "work_unit_id".to_string(),
            serde_json::Value::String("B-NONEXISTENT".to_string()),
        );
        let err = runner
            .dispatch(
                broker,
                &catalog,
                "work-broker/dispatch-work-unit".to_string(),
                params,
            )
            .await
            .unwrap_err();
        // LeafOpFailed wraps the InputInvalid from claim_work_unit
        match err {
            crate::DispatchError::LeafOpFailed { leaf_op, error } => {
                assert_eq!(leaf_op, "claim_work_unit");
                assert!(matches!(error, LeafError::InputInvalid(_)));
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }
}
