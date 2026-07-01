//! Doc-broker Phase 0 Layer B — Documentation Broker (`[InnateAbility]` role).
//!
//! The `[InnateAbility]` companion to Layer A's documentation-graph [Sense]
//! model (`neurogrim_sensory::documentation_graph`). Where Layer A *sees* the
//! documentation cross-reference graph + freshness/version model, Layer B
//! *dispatches* the single next reconciliation action the sensor surfaces —
//! exactly mirroring how [`crate::work_broker::WorkBroker`] wraps
//! `neurogrim_sensory::backlog::next_ready`.
//!
//! - **Cold-store schema:** in-memory `DocsState` (legacy/test) OR
//!   live-from-disk via `documentation_graph::build_doc_report()` +
//!   `next_doc()` (project-root mode).
//! - **Working state:** loaded `DocsState` mirrored from cold store.
//! - **Overlay shape:** `ActiveDocsOverlay` — the actionable doc unit(s) the
//!   agent sees plus recently-reconciled ids.
//! - **Surfaced pipelines:**
//!   - `doc-broker/dispatch-doc-unit` — reconcile a doc unit by id
//!   - `doc-broker/arm-kill-switch` — operator emergency halt (canonical)
//!   - `doc-broker/disengage-kill-switch` — operator-only resume
//! - **Internal pipelines:**
//!   - `doc-broker/doc-broker-tick` — refresh overlay (called on each tick)
//!
//! When constructed via `new_with_project_root`, the broker projects its
//! overlay from `build_doc_report()` + `next_doc()` instead of the in-memory
//! `DocsState`. The discipline is "do the next thing the sensor says needs
//! reconciling," not "browse the docs" — a single item, never filler.

use crate::broker::{Broker, BrokerError, Role, RoleSet, WorldEvent};
use crate::governance::GovernanceComposer;
use crate::overlay::{Overlay, WorkingState};
use crate::pipeline::{
    AuditClass, EffectClass, Pipeline, Step, Tunability, Visibility,
};
use crate::runner::{LeafContext, LeafError};
use async_trait::async_trait;
use neurogrim_sensory::documentation_graph::{
    build_doc_report, default_anchor, default_doc_excludes, next_doc, EcosystemAnchor,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A single documentation reconciliation unit (MVP shape). `id` is the doc's
/// project-relative path (the sensor's canonical node id).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocUnit {
    pub id: String,
    pub path: String,
    pub tier: String,
    pub action: String,
    pub status: DocUnitStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DocUnitStatus {
    Pending,
    Reconciled,
}

/// MVP cold-store / working-state struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DocsState {
    pub doc_units: Vec<DocUnit>,
}

/// What the agent sees — the actionable doc units plus recently-reconciled
/// ids. Mirrors `ActiveWorkOverlay`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActiveDocsOverlay {
    pub active_docs: Vec<DocUnit>,
    pub recent_reconciles: Vec<String>,
}

pub struct DocBroker {
    id: String,
    working_state: WorkingState<DocsState>,
    overlay: Arc<Overlay<ActiveDocsOverlay>>,
    governance: Arc<GovernanceComposer>,
    /// When `Some(project_root)`, the broker projects its overlay from
    /// `build_doc_report()` + `next_doc()` against the live doc tree. When
    /// `None`, the broker stays in in-memory mode (tests + ephemeral
    /// fixtures).
    project_root: Option<PathBuf>,
    anchor: EcosystemAnchor,
    excludes: Vec<String>,
}

impl DocBroker {
    /// Create a new Doc Broker with the given initial docs state (in-memory;
    /// legacy + test mode).
    pub fn new(
        id: impl Into<String>,
        initial: DocsState,
        governance: Arc<GovernanceComposer>,
    ) -> Self {
        let initial_overlay = curate_overlay(&initial);
        Self {
            id: id.into(),
            working_state: WorkingState::new(initial),
            overlay: Arc::new(Overlay::new(initial_overlay)),
            governance,
            project_root: None,
            anchor: default_anchor(),
            excludes: default_doc_excludes(),
        }
    }

    /// Live-sensor wiring: create a Doc Broker backed by the documentation
    /// graph sensor at `project_root`. The initial overlay is projected from
    /// `build_doc_report()` + `next_doc()` immediately; subsequent ticks (or
    /// refresh_overlay leaf-op invocations) re-read from disk.
    ///
    /// Excludes = `default_doc_excludes()` PLUS `cereGrim/thesis` (privacy —
    /// the proprietary Grimoire Thesis tree must never enter the doc graph).
    pub fn new_with_project_root(
        id: impl Into<String>,
        project_root: PathBuf,
        governance: Arc<GovernanceComposer>,
    ) -> Self {
        let anchor = default_anchor();
        let mut excludes = default_doc_excludes();
        excludes.push("cereGrim/thesis".to_string());
        let initial_state = project_state_from_sensor(&project_root, &anchor, &excludes);
        let initial_overlay = curate_overlay(&initial_state);
        Self {
            id: id.into(),
            working_state: WorkingState::new(initial_state),
            overlay: Arc::new(Overlay::new(initial_overlay)),
            governance,
            project_root: Some(project_root),
            anchor,
            excludes,
        }
    }

    /// The catalog of pipelines this broker exposes. Mirrors
    /// `WorkBroker::catalog` field-for-field.
    pub fn catalog(&self) -> Vec<Pipeline> {
        let mut pipelines = vec![
            // Surfaced: dispatch-doc-unit
            Pipeline {
                id: format!("{}/dispatch-doc-unit", self.id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorConfirmed,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::HotStoreUpdate,
                params: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "doc_unit_id": {
                            "type": "string",
                            "description": "The id of the doc unit to reconcile (must appear in active_docs)"
                        }
                    },
                    "required": ["doc_unit_id"]
                }),
                preconditions: vec![format!("active_docs")],
                steps: vec![
                    Step::Leaf {
                        leaf_op: "reconcile_doc_unit".to_string(),
                    },
                    Step::Leaf {
                        leaf_op: "refresh_overlay".to_string(),
                    },
                ],
                description: "Reconcile the named doc unit from the active documentation queue."
                    .to_string(),
                when_to_use:
                    "When the operator is ready to reconcile the next documentation drift/staleness item. Pick from the active_docs list."
                        .to_string(),
                bypasses_kill_switch: false,
            },
            // Internal: doc-broker-tick (called on each tick to refresh)
            Pipeline {
                id: format!("{}/doc-broker-tick", self.id),
                visibility: Visibility::Internal,
                tunability: Tunability::OperatorOnly,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::HotStoreUpdate,
                params: serde_json::json!({}),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: "refresh_overlay".to_string(),
                }],
                description: "Re-project Active Docs overlay from working state.".to_string(),
                when_to_use: "Framework-internal; runs on each tick.".to_string(),
                bypasses_kill_switch: false,
            },
        ];
        // Append canonical governance pipelines (exposed via
        // governance_pipelines() sidecar per reachability channel split).
        pipelines.extend(GovernanceComposer::canonical_governance_pipelines(&self.id));
        pipelines
    }
}

fn curate_overlay(state: &DocsState) -> ActiveDocsOverlay {
    ActiveDocsOverlay {
        active_docs: state.doc_units.clone(),
        recent_reconciles: state
            .doc_units
            .iter()
            .filter(|d| matches!(d.status, DocUnitStatus::Reconciled))
            .map(|d| d.id.clone())
            .collect(),
    }
}

/// Live-sensor helper: call `build_doc_report` + `next_doc` against the given
/// `project_root` and project the result into a `DocsState`. The `next_doc`
/// output's single top item (when `ready`/non-idle) becomes the sole `Pending`
/// doc-unit; when `tier == "idle"` or `ready == false`, no unit is surfaced
/// and the overlay's `active_docs` is empty.
///
/// Intentionally narrow: it surfaces ONLY the single next-doc item, not the
/// whole doc tree. The broker's discipline is "reconcile the next thing the
/// sensor says needs it," never invent filler.
fn project_state_from_sensor(
    project_root: &Path,
    anchor: &EcosystemAnchor,
    excludes: &[String],
) -> DocsState {
    let report = build_doc_report(project_root, anchor.clone(), excludes);
    let next = next_doc(&report);
    let tier = next.get("tier").and_then(|v| v.as_str()).unwrap_or("idle");
    let ready = next.get("ready").and_then(|v| v.as_bool()).unwrap_or(false);
    if tier == "idle" || !ready {
        // Nothing actionable (idle). Empty docs state → empty overlay.
        return DocsState::default();
    }
    let doc = match next.get("doc") {
        Some(v) => v,
        None => return DocsState::default(),
    };
    let path = doc
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if path.is_empty() {
        return DocsState::default();
    }
    let action = next
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    DocsState {
        doc_units: vec![DocUnit {
            id: path.clone(),
            path,
            tier: tier.to_string(),
            action,
            status: DocUnitStatus::Pending,
        }],
    }
}

#[async_trait]
impl Broker for DocBroker {
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
        self.catalog()
            .into_iter()
            .filter(|p| matches!(p.visibility, Visibility::Surfaced))
            .collect()
    }

    async fn governance_pipelines(&self) -> Vec<Pipeline> {
        GovernanceComposer::canonical_governance_pipelines(&self.id)
    }

    async fn tick(&self, _event: WorldEvent) -> Result<(), BrokerError> {
        // When project_root is set, re-read the live doc report before
        // re-projecting; else re-curate from working state.
        if let Some(root) = &self.project_root {
            let fresh = project_state_from_sensor(root, &self.anchor, &self.excludes);
            let mut state = self.working_state.lock().await;
            *state = fresh;
            let new_overlay = curate_overlay(&state);
            self.overlay.swap(new_overlay);
        } else {
            let state = self.working_state.lock().await;
            let new_overlay = curate_overlay(&state);
            self.overlay.swap(new_overlay);
        }
        Ok(())
    }

    async fn execute_leaf(
        &self,
        name: &str,
        ctx: LeafContext,
    ) -> Result<serde_json::Value, LeafError> {
        match name {
            "reconcile_doc_unit" => {
                let doc_unit_id = ctx
                    .params
                    .get("doc_unit_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        LeafError::InputInvalid("missing doc_unit_id".to_string())
                    })?
                    .to_string();
                let mut state = self.working_state.lock().await;
                let unit = state
                    .doc_units
                    .iter_mut()
                    .find(|d| d.id == doc_unit_id)
                    .ok_or_else(|| {
                        LeafError::InputInvalid(format!(
                            "doc unit `{}` not found",
                            doc_unit_id
                        ))
                    })?;
                if !matches!(unit.status, DocUnitStatus::Pending) {
                    return Err(LeafError::ExecutionFailed(format!(
                        "doc unit `{}` is not pending (status: {:?})",
                        doc_unit_id, unit.status
                    )));
                }
                unit.status = DocUnitStatus::Reconciled;
                Ok(serde_json::json!({
                    "reconciled": doc_unit_id,
                    "status": "reconciled"
                }))
            }
            "refresh_overlay" => {
                if let Some(root) = &self.project_root {
                    let fresh = project_state_from_sensor(root, &self.anchor, &self.excludes);
                    let mut state = self.working_state.lock().await;
                    *state = fresh;
                    let new_overlay = curate_overlay(&state);
                    self.overlay.swap(new_overlay);
                } else {
                    let state = self.working_state.lock().await;
                    let new_overlay = curate_overlay(&state);
                    self.overlay.swap(new_overlay);
                }
                Ok(serde_json::json!({"refreshed": true}))
            }
            "arm_kill_switch" => {
                // Real kill-switch (mirrors WorkBroker) — the Surfaced
                // arm-kill-switch governance pipeline routes here.
                self.governance.arm_kill_switch();
                Ok(serde_json::json!({"armed": true}))
            }
            "disengage_kill_switch" => {
                self.governance.disarm_kill_switch();
                Ok(serde_json::json!({"armed": false}))
            }
            other => Err(LeafError::NotFound(other.to_string())),
        }
    }
}

/// Factory for the broker host. Keyed by `broker_type = "doc-broker"` in the
/// per-broker manifest; `BrokerHost::boot` looks this up in the
/// `BrokerFactoryRegistry` and calls it to construct the broker + catalog.
pub fn doc_broker_factory() -> crate::host::BrokerFactoryFn {
    Arc::new(
        |broker_id: &str,
         governance: Arc<GovernanceComposer>,
         project_root: Option<&PathBuf>|
         -> Result<(Arc<dyn Broker>, Vec<Pipeline>), crate::host::HostError> {
            let broker = match project_root {
                Some(root) => DocBroker::new_with_project_root(
                    broker_id.to_string(),
                    root.clone(),
                    governance,
                ),
                None => DocBroker::new(broker_id.to_string(), DocsState::default(), governance),
            };
            let catalog = broker.catalog();
            Ok((Arc::new(broker) as Arc<dyn Broker>, catalog))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::ParamMap;
    use crate::runner::PipelineRunner;
    use crate::trace::TraceSink;
    use tempfile::TempDir;

    fn make_demo_state() -> DocsState {
        DocsState {
            doc_units: vec![DocUnit {
                id: "docs/guide.md".to_string(),
                path: "docs/guide.md".to_string(),
                tier: "refresh-stale".to_string(),
                action: "Refresh `docs/guide.md` — update its version markers.".to_string(),
                status: DocUnitStatus::Pending,
            }],
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

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }

    #[tokio::test]
    async fn doc_broker_overlay_reflects_actionable_units() {
        let (_, governance) = make_runner_governance();
        // Non-idle in-memory state → one active doc.
        let b = DocBroker::new("doc-broker", make_demo_state(), governance.clone());
        let overlay = b.read_overlay().await;
        let active = overlay["active_docs"].as_array().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0]["id"], "docs/guide.md");

        // Idle (empty) state → empty overlay.
        let b2 = DocBroker::new("doc-broker", DocsState::default(), governance);
        let overlay2 = b2.read_overlay().await;
        assert!(overlay2["active_docs"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn doc_broker_role_set_is_innate_ability() {
        let (_, governance) = make_runner_governance();
        let b = DocBroker::new("doc-broker", DocsState::default(), governance);
        assert!(b.role_set().contains(&Role::InnateAbility));
        assert!(!b.role_set().contains(&Role::Sense));
        assert!(!b.role_set().contains(&Role::Embodiment));
    }

    /// Live-sensor: constructed with a project_root over a planted fixture
    /// tree with a version-drifted doc → surfaces a reconcile DocUnit.
    #[tokio::test]
    async fn doc_broker_with_project_root_surfaces_drift() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // A non-front-door doc stating v4.0 while the anchor ecosystem is 5.0.
        write(root, "guide.md", "# Guide\n\nThis follows v4.0 conventions.\n");

        let (_, governance) = make_runner_governance();
        let broker = DocBroker::new_with_project_root("doc-broker", root.to_path_buf(), governance);
        let overlay = broker.read_overlay().await;
        let active = overlay["active_docs"].as_array().unwrap();
        assert_eq!(active.len(), 1, "expected one actionable doc, got: {active:?}");
        assert_eq!(active[0]["id"], "guide.md");
        // The drift lives in a non-front-door doc → refresh-stale tier.
        assert_eq!(active[0]["tier"], "refresh-stale");
    }

    /// Live-sensor: an all-current fixture (front-door README, no drift) →
    /// idle → empty overlay.
    #[tokio::test]
    async fn doc_broker_with_project_root_empty_when_all_current() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Front-door, current status, no version marker → no signals.
        write(
            root,
            "README.md",
            "---\nstatus: current\n---\n# Readme\n\nNothing to reconcile here.\n",
        );

        let (_, governance) = make_runner_governance();
        let broker = DocBroker::new_with_project_root("doc-broker", root.to_path_buf(), governance);
        let overlay = broker.read_overlay().await;
        let active = overlay["active_docs"].as_array().unwrap();
        assert!(
            active.is_empty(),
            "expected empty active_docs when all-current, got: {active:?}"
        );
    }

    #[tokio::test]
    async fn doc_broker_dispatch_via_runner_end_to_end() {
        let (sink, governance) = make_runner_governance();
        let broker: Arc<dyn Broker> = Arc::new(DocBroker::new(
            "doc-broker",
            make_demo_state(),
            governance.clone(),
        ));
        let catalog = DocBroker::new("doc-broker", make_demo_state(), governance.clone()).catalog();
        let runner = PipelineRunner::new(sink.clone(), governance.clone());

        // Initial overlay shows 1 active doc unit.
        let overlay_before = broker.read_overlay().await;
        assert_eq!(overlay_before["active_docs"].as_array().unwrap().len(), 1);

        let mut params = ParamMap::new();
        params.insert(
            "doc_unit_id".to_string(),
            serde_json::Value::String("docs/guide.md".to_string()),
        );
        let outcome = runner
            .dispatch(
                broker.clone(),
                &catalog,
                "doc-broker/dispatch-doc-unit".to_string(),
                params,
            )
            .await
            .expect("dispatch should succeed");

        // Last step is refresh_overlay.
        assert_eq!(outcome.output["refreshed"], true);

        // Overlay now marks the unit reconciled + records it in recent_reconciles.
        let overlay_after = broker.read_overlay().await;
        let active_after = overlay_after["active_docs"].as_array().unwrap();
        assert_eq!(active_after.len(), 1);
        assert_eq!(active_after[0]["status"], "reconciled");
        let recent = overlay_after["recent_reconciles"].as_array().unwrap();
        assert!(recent.iter().any(|c| c == "docs/guide.md"));

        // Trust budget consumed 1 (Surfaced pipeline).
        let (used, _) = governance.trust_budget_state();
        assert_eq!(used, 1);

        // Trace recorded.
        let trace_contents = std::fs::read_to_string(sink.file_path()).unwrap();
        assert_eq!(trace_contents.lines().count(), 1);
        let trace: crate::TraceRecord =
            serde_json::from_str(trace_contents.lines().next().unwrap()).unwrap();
        assert_eq!(trace.pipeline_id, "doc-broker/dispatch-doc-unit");
        assert_eq!(trace.broker_id, "doc-broker");
        assert_eq!(trace.audit_class, "capability");
    }

    #[tokio::test]
    async fn doc_broker_dispatch_refuses_unknown_doc_unit() {
        let (sink, governance) = make_runner_governance();
        let broker: Arc<dyn Broker> = Arc::new(DocBroker::new(
            "doc-broker",
            make_demo_state(),
            governance.clone(),
        ));
        let catalog = DocBroker::new("doc-broker", make_demo_state(), governance.clone()).catalog();
        let runner = PipelineRunner::new(sink, governance);
        let mut params = ParamMap::new();
        params.insert(
            "doc_unit_id".to_string(),
            serde_json::Value::String("docs/nonexistent.md".to_string()),
        );
        let err = runner
            .dispatch(
                broker,
                &catalog,
                "doc-broker/dispatch-doc-unit".to_string(),
                params,
            )
            .await
            .unwrap_err();
        match err {
            crate::DispatchError::LeafOpFailed { leaf_op, error } => {
                assert_eq!(leaf_op, "reconcile_doc_unit");
                assert!(matches!(error, LeafError::InputInvalid(_)));
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    /// Host boot: a cluster manifest declaring a broker with
    /// `broker_type = "doc-broker"` + `doc_broker_factory()` registered in the
    /// BrokerFactoryRegistry → boot succeeds, the catalog includes
    /// `doc-broker/dispatch-doc-unit`, and the kill-switch is reachable.
    #[tokio::test]
    async fn host_boot_with_doc_broker_factory() {
        use crate::host::{BrokerFactoryRegistry, BrokerHost, BrokerHostConfig};

        let tmp = TempDir::new().unwrap();
        let cluster_dir = tmp.path().join(".claude").join("brain").join("broker");
        std::fs::create_dir_all(&cluster_dir).unwrap();
        let cluster_path = cluster_dir.join("cluster.toml");
        std::fs::write(
            &cluster_path,
            r#"[cluster]
id = "doc-broker-test-cluster"
name = "Doc broker test cluster"
brokers_dir = "./"

[cluster.brokers.doc-broker]
manifest_path = "doc-broker.toml"

[cluster.materializer]
composition_order = ["overlay-doc-broker", "awareness-routing-doc-broker"]
output_path = "current-projection.md"
segments_dir = "segments"
context_budget_chars = 16384
"#,
        )
        .unwrap();
        std::fs::write(
            cluster_dir.join("doc-broker.toml"),
            r#"[broker]
id = "doc-broker"
name = "Doc Broker"
roles = ["innate-ability"]
broker_type = "doc-broker"
cold_store_path = "doc-broker-cold/"
catalog_path = "doc-broker-catalog.yaml"
"#,
        )
        .unwrap();

        let mut factories = BrokerFactoryRegistry::new();
        factories.register("doc-broker", doc_broker_factory());

        let host = BrokerHost::boot(
            &cluster_path,
            BrokerHostConfig {
                project_root: None,
                trust_budget_ceiling: 100,
                broker_factories: factories,
            },
        )
        .await
        .expect("boot should succeed");

        assert_eq!(host.bootstrapped().len(), 1);
        assert_eq!(host.bootstrapped()[0].0, "doc-broker");

        // Catalog surfaces dispatch-doc-unit.
        let catalog = host.registry.full_catalog();
        assert!(
            catalog.iter().any(|p| p.id == "doc-broker/dispatch-doc-unit"),
            "full_catalog must include doc-broker/dispatch-doc-unit"
        );

        // Kill-switch reachable via the canonical governance pipeline.
        let outcome = host
            .dispatch(
                "doc-broker",
                "doc-broker/arm-kill-switch".to_string(),
                ParamMap::new(),
            )
            .await
            .expect("arm-kill-switch dispatch should succeed");
        assert!(!outcome.trace_id.is_empty());
    }
}
