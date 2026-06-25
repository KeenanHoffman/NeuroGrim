//! BB #10 — Pipeline Runner.
//!
//! Single-tick executor for MVP. No suspension; no Workflow Engine (BB #11
//! deferred to S1-T per plan scope).
//!
//! ## Execution sequence (Wave 2)
//!
//! For each dispatch:
//! 1. Catalog lookup — find the pipeline by ID in the broker's catalog.
//! 2. Param validation — for MVP, ensure dispatch ParamMap matches the
//!    pipeline's `params` JSON schema field names (lightweight; full
//!    JSON Schema validation lands in Wave 4 governance).
//! 3. Precondition evaluation — every precondition expression must return
//!    `true` against the broker's current Overlay + dispatch params (per
//!    BROKER-CONTRACT.md central invariant).
//! 4. Step execution — walk steps in order; call leaf-ops via
//!    `Broker::execute_leaf`; recurse into intra-broker sub-pipelines.
//! 5. Trace recording — append a TraceRecord with snapshot delta (Wave 2
//!    BB #12); audit_class per pipeline.
//! 6. Return DispatchOutcome with trace_id + structured output.

use crate::broker::Broker;
use crate::catalog;
use crate::frame::Frame;
use crate::governance::{GovernanceComposer, GovernanceRefusal};
use crate::pipeline::{ParamMap, Pipeline, PipelineId, Step, Visibility};
use crate::trace::{SnapshotDelta, TraceRecord, TraceSink};
use chrono::Utc;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum DispatchError {
    #[error("pipeline not found in broker `{broker_id}`: {pipeline_id}")]
    PipelineNotFound {
        broker_id: String,
        pipeline_id: PipelineId,
    },

    #[error("parameter validation failed: {field}: {reason}")]
    ParamValidation { field: String, reason: String },

    #[error("precondition not met: {0}")]
    PreconditionUnmet(String),

    #[error("precondition evaluation error: {0}")]
    PreconditionEvalFailed(#[from] crate::catalog::CatalogError),

    #[error("governance refused dispatch: {0}")]
    GovernanceRefused(#[from] GovernanceRefusal),

    #[error("leaf-op failed: {leaf_op}: {error}")]
    LeafOpFailed {
        leaf_op: String,
        error: LeafError,
    },

    #[error("sub-pipeline failed: {pipeline_id}: {source}")]
    SubPipelineFailed {
        pipeline_id: PipelineId,
        #[source]
        source: Box<DispatchError>,
    },

    #[error("trace recording failed: {0}")]
    TraceFailed(#[from] crate::trace::TraceError),

    #[error("other: {0}")]
    Other(#[from] anyhow::Error),
}

/// Errors leaf-ops can produce.
#[derive(Debug, Error)]
pub enum LeafError {
    #[error("leaf-op not found: {0}")]
    NotFound(String),

    #[error("leaf-op input invalid: {0}")]
    InputInvalid(String),

    #[error("leaf-op execution failed: {0}")]
    ExecutionFailed(String),

    #[error("other: {0}")]
    Other(#[from] anyhow::Error),
}

/// Context passed to leaf-ops. MVP keeps this minimal; later waves may
/// extend with workflow state, governance hooks, etc.
#[derive(Debug, Clone)]
pub struct LeafContext {
    pub broker_id: String,
    pub pipeline_id: PipelineId,
    pub params: ParamMap,
    /// Snapshot of the broker's Overlay at dispatch time.
    pub overlay_snapshot: serde_json::Value,
    /// A13 / BB #35: operator-declared Frame (stakes, posture, budgets,
    /// thresholds, ...). Leaf-ops + precondition DSL read frame values.
    /// Sub-pipelines inherit the parent frame (V0; per-sub-pipeline override
    /// lands in S1-T).
    pub frame: Frame,
}

/// Dispatch outcome returned from Runner to the consumer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DispatchOutcome {
    pub trace_id: String,
    /// The structured output of the final step (or merged outputs for
    /// multi-step pipelines; MVP just returns the last step's output).
    pub output: serde_json::Value,
    pub duration_ms: u64,
}

/// Callback type for on_dispatch_complete hook (V0-RETROSPECTIVE §C6 / §D3
/// closure). Receives the broker_id + the dispatch outcome; typically used
/// by the operator's main binary to re-trigger materialization. Synchronous
/// (called inline after trace recording) so the hook can synchronously
/// re-materialize before returning to the caller (the agent).
pub type DispatchCallback =
    Arc<dyn Fn(&str, &DispatchOutcome) + Send + Sync>;

/// Pipeline Runner. Holds shared infrastructure (trace sink, governance
/// composer, prior-snapshot cache for delta computation, A13 frame);
/// brokers + catalogs are passed per dispatch.
pub struct PipelineRunner {
    trace_sink: Arc<TraceSink>,
    governance: Arc<GovernanceComposer>,
    /// Most recent overlay snapshot per broker, used for trace delta
    /// computation. tokio Mutex because dispatches are async.
    prior_snapshots: tokio::sync::Mutex<
        std::collections::HashMap<String, (String, serde_json::Value)>,
    >,
    /// Callbacks invoked after each successful dispatch (V0-RETROSPECTIVE
    /// §C6 / §D3). Decouples Runner from Materializer: operator's main
    /// binary registers a callback that re-runs the Materializer Composer.
    on_dispatch_complete: std::sync::RwLock<Vec<DispatchCallback>>,
    /// A13 / BB #35: operator-declared Frame loaded from cluster manifest.
    /// Read-only at the runner level; future `propose_tune()` API will
    /// respect each pipeline's `tunability` field.
    frame: std::sync::RwLock<Frame>,
    /// D1 / BB #27: optional registry for cross-broker sub_pipeline lookup.
    /// When set, sub_pipeline steps referencing `<other_broker_id>/<...>`
    /// look up the callee broker + its catalog from the registry instead
    /// of erroring out. When None (legacy), cross-broker still rejects.
    registry: std::sync::RwLock<Option<Arc<crate::registry::BrokerRegistry>>>,
}

impl PipelineRunner {
    pub fn new(trace_sink: Arc<TraceSink>, governance: Arc<GovernanceComposer>) -> Self {
        Self {
            trace_sink,
            governance,
            prior_snapshots: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            on_dispatch_complete: std::sync::RwLock::new(Vec::new()),
            frame: std::sync::RwLock::new(Frame::new()),
            registry: std::sync::RwLock::new(None),
        }
    }

    /// D1 / BB #27: install a registry so cross-broker sub_pipeline lookups
    /// work. Host should call this after constructing the runner +
    /// populating the registry.
    pub fn set_registry(&self, registry: Arc<crate::registry::BrokerRegistry>) {
        *self.registry.write().expect("registry lock poisoned") = Some(registry);
    }

    /// A13: replace the runner's Frame. Operator-driven (host setup time);
    /// runtime tuning lands in S1-T via a `propose_tune()` API that
    /// respects per-pipeline tunability.
    pub fn set_frame(&self, frame: Frame) {
        *self.frame.write().expect("frame lock poisoned") = frame;
    }

    /// A13: read a snapshot of the current Frame. Cheap clone per dispatch
    /// (Frame is a small HashMap).
    pub fn frame_snapshot(&self) -> Frame {
        self.frame.read().expect("frame lock poisoned").clone()
    }

    /// Access the governance composer (for operator-side admin commands).
    pub fn governance(&self) -> Arc<GovernanceComposer> {
        self.governance.clone()
    }

    /// Register a callback to fire after each successful dispatch.
    /// Operator's main binary uses this to wire Materializer Composer
    /// auto-trigger. Multiple callbacks supported.
    pub fn on_dispatch_complete(&self, callback: DispatchCallback) {
        self.on_dispatch_complete
            .write()
            .expect("on_dispatch_complete write lock poisoned")
            .push(callback);
    }

    /// Dispatch a pipeline against a broker.
    ///
    /// **A1/C8 fix:** trace records are written for BOTH success and refusal
    /// branches — audit-completeness invariant per BB #19. V0 only wrote on
    /// success, silently dropping every refused dispatch from the audit
    /// ledger. The flow is now:
    ///
    /// 1. `dispatch_inner` runs the pipeline logic; on Err, returns the
    ///    `DispatchError` plus whatever partial state was learned (pipeline
    ///    metadata if catalog lookup succeeded).
    /// 2. The wrapper always computes the snapshot delta + writes a
    ///    TraceRecord with either `{"status":"success","output":...}` or
    ///    `{"status":"refused","failure_reason":"..."}`.
    /// 3. `on_dispatch_complete` callbacks fire on success only (refusals
    ///    don't typically change overlay state, so re-materialization isn't
    ///    needed; if a future broker needs refusal-driven re-materialization,
    ///    move this into the unconditional branch).
    pub async fn dispatch(
        &self,
        broker: Arc<dyn Broker>,
        catalog: &[Pipeline],
        pipeline_id: PipelineId,
        params: ParamMap,
    ) -> Result<DispatchOutcome, DispatchError> {
        let start = std::time::Instant::now();
        let trace_id = Uuid::new_v4().to_string();

        // Run the dispatch flow; track which pipeline was selected (None if
        // catalog lookup failed) so the trace can attribute audit_class.
        let mut selected_pipeline: Option<Pipeline> = None;
        let inner_result = self
            .dispatch_inner(
                broker.clone(),
                catalog,
                pipeline_id.clone(),
                params.clone(),
                &trace_id,
                &mut selected_pipeline,
            )
            .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Audit-completeness invariant (A1/C8): always compute snapshot delta
        // + write trace, regardless of outcome. PipelineNotFound + governance
        // refusals default to audit_class="governance" since no specific
        // capability pipeline was selected; other errors use the selected
        // pipeline's class.
        let post_snapshot = broker.read_overlay().await;
        let (delta, delta_from) = self
            .compute_snapshot_delta(broker.id(), &post_snapshot, &trace_id)
            .await;

        let audit_class = selected_pipeline
            .as_ref()
            .map(|p| format!("{:?}", p.audit_class).to_lowercase())
            .unwrap_or_else(|| "governance".to_string());

        let outcome_json = match &inner_result {
            Ok(output) => serde_json::json!({
                "status": "success",
                "output": output.clone(),
            }),
            Err(e) => serde_json::json!({
                "status": "refused",
                "failure_reason": format!("{}", e),
            }),
        };

        let record = TraceRecord {
            schema_version: "1".to_string(),
            ts: Utc::now().to_rfc3339(),
            trace_id: trace_id.clone(),
            pipeline_id: pipeline_id.clone(),
            broker_id: broker.id().to_string(),
            params: serde_json::Value::Object(params),
            snapshot_delta_from: delta_from,
            snapshot_delta: delta,
            outcome: outcome_json,
            audit_class,
            duration_ms,
        };
        // Best-effort trace write: don't let a failed trace write mask the
        // actual dispatch outcome. Log but proceed.
        if let Err(e) = self.trace_sink.append(&record) {
            eprintln!("[brokers] WARN: trace write failed: {}", e);
        }

        match inner_result {
            Ok(output) => {
                let outcome = DispatchOutcome {
                    trace_id,
                    output,
                    duration_ms,
                };
                // Fire on_dispatch_complete callbacks (success only).
                let callbacks = self
                    .on_dispatch_complete
                    .read()
                    .expect("on_dispatch_complete read lock poisoned");
                for cb in callbacks.iter() {
                    cb(broker.id(), &outcome);
                }
                drop(callbacks);
                Ok(outcome)
            }
            Err(e) => Err(e),
        }
    }

    /// Inner dispatch flow — runs the pipeline logic, returns Result<output,
    /// DispatchError>. Populates `selected_pipeline` as soon as the catalog
    /// lookup succeeds so the wrapper can attribute audit_class correctly
    /// even when later stages fail.
    async fn dispatch_inner(
        &self,
        broker: Arc<dyn Broker>,
        catalog: &[Pipeline],
        pipeline_id: PipelineId,
        params: ParamMap,
        _trace_id: &str,
        selected_pipeline: &mut Option<Pipeline>,
    ) -> Result<serde_json::Value, DispatchError> {
        // 1. Catalog lookup
        let pipeline = catalog
            .iter()
            .find(|p| p.id == pipeline_id)
            .ok_or_else(|| DispatchError::PipelineNotFound {
                broker_id: broker.id().to_string(),
                pipeline_id: pipeline_id.clone(),
            })?
            .clone();
        *selected_pipeline = Some(pipeline.clone());

        // 2. Governance pre-dispatch checks (BB #19; Surfaced only — Internal
        //    and AuditOnly skip governance per BROKER-INTERNALS.md §2.4).
        //    A1.5 / B-64: the pipeline-aware check honors
        //    `bypasses_kill_switch` so arm/disengage stay reachable while
        //    armed.
        if matches!(pipeline.visibility, Visibility::Surfaced) {
            self.governance.pre_dispatch_checks_for(&pipeline)?;
        }

        // 3. Param validation (Wave 5+ elevates to JSON Schema; MVP minimal)
        validate_params_minimal(&pipeline, &params)?;

        // 4. Read broker Overlay snapshot for precondition + leaf-op ctx
        let overlay_snapshot = broker.read_overlay().await;
        let frame = self.frame_snapshot();

        // 5. Precondition evaluation. A13: {frame.X} placeholders in
        //    predicate strings are substituted from the Frame before the
        //    catalog's DSL evaluates against overlay + params.
        for predicate in &pipeline.preconditions {
            let resolved = frame.substitute_placeholders(predicate);
            let holds =
                catalog::evaluate_precondition(&resolved, &overlay_snapshot, &params)?;
            if !holds {
                return Err(DispatchError::PreconditionUnmet(predicate.clone()));
            }
        }

        // 6. Governance record_dispatch (consumes 1 trust-budget unit;
        //    Surfaced only)
        if matches!(pipeline.visibility, Visibility::Surfaced) {
            self.governance.record_dispatch();
        }

        // 7. Step execution
        let leaf_ctx = LeafContext {
            broker_id: broker.id().to_string(),
            pipeline_id: pipeline_id.clone(),
            params: params.clone(),
            overlay_snapshot: overlay_snapshot.clone(),
            frame,
        };
        let output = self
            .execute_steps(broker.clone(), catalog, &pipeline.steps, &leaf_ctx)
            .await?;

        Ok(output)
    }

    /// Execute a sequence of steps; return the LAST step's output (MVP).
    fn execute_steps<'a>(
        &'a self,
        broker: Arc<dyn Broker>,
        catalog: &'a [Pipeline],
        steps: &'a [Step],
        ctx: &'a LeafContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, DispatchError>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut last_output = serde_json::Value::Null;
            for step in steps {
                last_output = self.execute_step(broker.clone(), catalog, step, ctx).await?;
            }
            Ok(last_output)
        })
    }

    fn execute_step<'a>(
        &'a self,
        broker: Arc<dyn Broker>,
        catalog: &'a [Pipeline],
        step: &'a Step,
        ctx: &'a LeafContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, DispatchError>> + Send + 'a>>
    {
        Box::pin(async move {
            match step {
                Step::Leaf { leaf_op } => broker
                    .execute_leaf(leaf_op, ctx.clone())
                    .await
                    .map_err(|e| DispatchError::LeafOpFailed {
                        leaf_op: leaf_op.clone(),
                        error: e,
                    }),
                Step::SubPipeline {
                    sub_pipeline,
                    params,
                } => {
                    // D1 / BB #27: prefix mismatch → cross-broker dispatch
                    // (look up callee from registry if installed).
                    let callee_broker_id = sub_pipeline
                        .split_once('/')
                        .map(|(b, _)| b.to_string())
                        .unwrap_or_default();
                    let same_broker = callee_broker_id == ctx.broker_id;
                    if same_broker {
                        let outcome = self
                            .dispatch(
                                broker.clone(),
                                catalog,
                                sub_pipeline.clone(),
                                params.clone(),
                            )
                            .await
                            .map_err(|e| DispatchError::SubPipelineFailed {
                                pipeline_id: sub_pipeline.clone(),
                                source: Box::new(e),
                            })?;
                        Ok(outcome.output)
                    } else {
                        // Cross-broker: must have a registry installed.
                        // Extract callee broker + catalog from the lock-held
                        // scope, then drop the guard BEFORE awaiting the
                        // sub-dispatch (RwLockReadGuard is !Send).
                        let (callee, callee_catalog) = {
                            let registry_guard = self.registry.read()
                                .expect("registry lock poisoned");
                            let registry = registry_guard.as_ref().ok_or_else(|| {
                                DispatchError::SubPipelineFailed {
                                    pipeline_id: sub_pipeline.clone(),
                                    source: Box::new(DispatchError::Other(anyhow::anyhow!(
                                        "cross-broker sub_pipeline `{}` requires a registry; \
                                         host must call runner.set_registry() before dispatch",
                                        sub_pipeline
                                    ))),
                                }
                            })?;
                            let callee = registry.broker(&callee_broker_id).ok_or_else(|| {
                                DispatchError::SubPipelineFailed {
                                    pipeline_id: sub_pipeline.clone(),
                                    source: Box::new(DispatchError::Other(anyhow::anyhow!(
                                        "cross-broker callee `{}` not registered",
                                        callee_broker_id
                                    ))),
                                }
                            })?;
                            let callee_catalog = registry
                                .catalog_for(&callee_broker_id)
                                .cloned()
                                .unwrap_or_default();
                            (callee, callee_catalog)
                        };
                        let outcome = self
                            .dispatch(
                                callee,
                                &callee_catalog,
                                sub_pipeline.clone(),
                                params.clone(),
                            )
                            .await
                            .map_err(|e| DispatchError::SubPipelineFailed {
                                pipeline_id: sub_pipeline.clone(),
                                source: Box::new(e),
                            })?;
                        Ok(outcome.output)
                    }
                }
                Step::Guard { predicate, inner } => {
                    // A13: frame.X placeholder substitution before DSL eval.
                    let resolved = ctx.frame.substitute_placeholders(predicate);
                    let holds = catalog::evaluate_precondition(
                        &resolved,
                        &ctx.overlay_snapshot,
                        &ctx.params,
                    )?;
                    if holds {
                        self.execute_step(broker, catalog, inner, ctx).await
                    } else {
                        Ok(serde_json::Value::Null)
                    }
                }
                Step::Branch {
                    predicate,
                    if_true,
                    if_false,
                } => {
                    // A13: frame.X placeholder substitution before DSL eval.
                    let resolved = ctx.frame.substitute_placeholders(predicate);
                    let holds = catalog::evaluate_precondition(
                        &resolved,
                        &ctx.overlay_snapshot,
                        &ctx.params,
                    )?;
                    let chosen = if holds { if_true } else { if_false };
                    self.execute_step(broker, catalog, chosen, ctx).await
                }
            }
        })
    }

    async fn compute_snapshot_delta(
        &self,
        broker_id: &str,
        current: &serde_json::Value,
        trace_id: &str,
    ) -> (serde_json::Value, Option<String>) {
        let mut snapshots = self.prior_snapshots.lock().await;
        match snapshots.get(broker_id).cloned() {
            Some((prior_trace_id, prior_snapshot)) => {
                let delta = SnapshotDelta::compute(&prior_snapshot, current);
                snapshots.insert(broker_id.to_string(), (trace_id.to_string(), current.clone()));
                (delta.to_json(), Some(prior_trace_id))
            }
            None => {
                snapshots.insert(broker_id.to_string(), (trace_id.to_string(), current.clone()));
                // First snapshot: log the full state, no delta-from
                (current.clone(), None)
            }
        }
    }
}

/// A11 — Param validation against the pipeline's JSON Schema fragment.
///
/// V0 only checked that every declared `properties` field name appeared in
/// `params` (treating ALL properties as required). That's wrong: JSON
/// Schema has an explicit `required` array; properties not listed there are
/// optional. V0 also performed no type/enum checking — malformed params
/// reached leaf-ops with less helpful error sites.
///
/// A11 implements a lightweight JSON Schema validator covering the subset
/// actually used by broker pipelines:
/// - `required: ["field", ...]` — listed fields must be present
/// - `properties.<field>.type: "string" | "number" | "integer" | "boolean" | "array" | "object" | "null"`
/// - `properties.<field>.enum: [...]` — value must be one of the listed
///
/// Full Draft 2020-12 (regex format, $ref, oneOf/anyOf/allOf, etc.) lands
/// when an actual pipeline needs it; the broker contract intentionally
/// keeps pipeline param schemas simple per BROKER-INTERNALS.md §1.3.
fn validate_params_minimal(pipeline: &Pipeline, params: &ParamMap) -> Result<(), DispatchError> {
    let schema = &pipeline.params;
    // Required array (JSON Schema convention). If absent, no fields are
    // required — every declared property is optional.
    let required: Vec<&str> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    for field in &required {
        if !params.contains_key(*field) {
            return Err(DispatchError::ParamValidation {
                field: field.to_string(),
                reason: "required parameter missing".to_string(),
            });
        }
    }
    // Per-property type + enum validation.
    if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
        for (field_name, field_schema) in props {
            if let Some(value) = params.get(field_name) {
                if let Err(reason) = validate_value_against_schema(value, field_schema) {
                    return Err(DispatchError::ParamValidation {
                        field: field_name.clone(),
                        reason,
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_value_against_schema(
    value: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), String> {
    // Type check
    if let Some(expected_type) = schema.get("type").and_then(|v| v.as_str()) {
        let actual_type = match value {
            serde_json::Value::String(_) => "string",
            serde_json::Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    if expected_type == "integer" {
                        "integer"
                    } else {
                        "number"
                    }
                } else {
                    "number"
                }
            }
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
            serde_json::Value::Null => "null",
        };
        // Accept "integer" as a valid "number" too.
        let type_matches = actual_type == expected_type
            || (expected_type == "number"
                && (actual_type == "number" || actual_type == "integer"));
        if !type_matches {
            return Err(format!(
                "expected type `{}`, got `{}`",
                expected_type, actual_type
            ));
        }
    }
    // Enum check
    if let Some(enum_vals) = schema.get("enum").and_then(|v| v.as_array()) {
        if !enum_vals.iter().any(|v| v == value) {
            return Err(format!(
                "value not in declared enum: {}",
                serde_json::to_string(enum_vals).unwrap_or_default()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::{Role, RoleSet, WorldEvent};
    use crate::pipeline::{AuditClass, EffectClass, Tunability, Visibility};
    use async_trait::async_trait;
    use std::sync::Arc;

    /// A test broker that executes simple leaf-ops.
    pub struct TestBroker {
        id: String,
        catalog: Vec<Pipeline>,
        overlay: tokio::sync::Mutex<serde_json::Value>,
    }

    impl TestBroker {
        fn new(id: impl Into<String>, catalog: Vec<Pipeline>, overlay: serde_json::Value) -> Self {
            Self {
                id: id.into(),
                catalog,
                overlay: tokio::sync::Mutex::new(overlay),
            }
        }
    }

    #[async_trait]
    impl Broker for TestBroker {
        fn id(&self) -> &str {
            &self.id
        }
        fn role_set(&self) -> RoleSet {
            RoleSet::single(Role::Sense)
        }
        async fn read_overlay(&self) -> serde_json::Value {
            self.overlay.lock().await.clone()
        }
        async fn legal_pipelines(&self) -> Vec<Pipeline> {
            self.catalog
                .iter()
                .filter(|p| matches!(p.visibility, Visibility::Surfaced))
                .cloned()
                .collect()
        }
        async fn governance_pipelines(&self) -> Vec<Pipeline> {
            vec![]
        }
        async fn tick(&self, _event: WorldEvent) -> Result<(), crate::broker::BrokerError> {
            Ok(())
        }
        async fn execute_leaf(
            &self,
            name: &str,
            ctx: LeafContext,
        ) -> Result<serde_json::Value, LeafError> {
            match name {
                "echo" => Ok(serde_json::json!({"echoed": ctx.params})),
                "set-overlay-field" => {
                    let mut o = self.overlay.lock().await;
                    if let Some(obj) = o.as_object_mut() {
                        obj.insert(
                            "set_by".to_string(),
                            serde_json::Value::String(ctx.pipeline_id.clone()),
                        );
                    }
                    Ok(serde_json::json!({"updated": true}))
                }
                other => Err(LeafError::NotFound(other.to_string())),
            }
        }
    }

    fn make_pipeline(id: &str, preconditions: Vec<&str>, steps: Vec<Step>) -> Pipeline {
        Pipeline {
            id: id.to_string(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorOnly,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::HotStoreUpdate,
            params: serde_json::json!({}),
            preconditions: preconditions.into_iter().map(String::from).collect(),
            steps,
            description: String::new(),
            when_to_use: String::new(),
            bypasses_kill_switch: false,
        }
    }

    async fn make_runner() -> (Arc<TraceSink>, PipelineRunner) {
        make_runner_with_budget(10_000).await
    }

    async fn make_runner_with_budget(budget: u64) -> (Arc<TraceSink>, PipelineRunner) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        std::mem::forget(tmp); // keep file alive for test
        let sink = Arc::new(TraceSink::new(path));
        let governance = Arc::new(GovernanceComposer::new(budget));
        let runner = PipelineRunner::new(sink.clone(), governance);
        (sink, runner)
    }

    #[tokio::test]
    async fn dispatch_simple_leaf_step() {
        let (_, runner) = make_runner().await;
        let pipeline = make_pipeline(
            "t/echo",
            vec![],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        let broker = Arc::new(TestBroker::new("t", vec![pipeline.clone()], serde_json::json!({})));
        let mut params = ParamMap::new();
        params.insert("hello".to_string(), serde_json::Value::String("world".to_string()));
        let outcome = runner
            .dispatch(broker, &[pipeline], "t/echo".to_string(), params)
            .await
            .unwrap();
        assert_eq!(outcome.output["echoed"]["hello"], "world");
    }

    #[tokio::test]
    async fn dispatch_rejects_missing_pipeline() {
        let (_, runner) = make_runner().await;
        let broker = Arc::new(TestBroker::new("t", vec![], serde_json::json!({})));
        let err = runner
            .dispatch(broker, &[], "t/nonexistent".to_string(), ParamMap::new())
            .await
            .unwrap_err();
        assert!(matches!(err, DispatchError::PipelineNotFound { .. }));
    }

    #[tokio::test]
    async fn dispatch_enforces_preconditions() {
        let (_, runner) = make_runner().await;
        let pipeline = make_pipeline(
            "t/needs-data",
            vec!["required_field"],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        // Overlay missing the required_field
        let broker = Arc::new(TestBroker::new("t", vec![pipeline.clone()], serde_json::json!({})));
        let err = runner
            .dispatch(
                broker,
                &[pipeline],
                "t/needs-data".to_string(),
                ParamMap::new(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, DispatchError::PreconditionUnmet(_)));
    }

    #[tokio::test]
    async fn dispatch_evaluates_intra_broker_sub_pipeline() {
        let (_, runner) = make_runner().await;
        let child = make_pipeline(
            "t/child",
            vec![],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        let parent = make_pipeline(
            "t/parent",
            vec![],
            vec![Step::SubPipeline {
                sub_pipeline: "t/child".to_string(),
                params: ParamMap::new(),
            }],
        );
        let catalog = vec![parent.clone(), child];
        let broker = Arc::new(TestBroker::new("t", catalog.clone(), serde_json::json!({})));
        let outcome = runner
            .dispatch(broker, &catalog, "t/parent".to_string(), ParamMap::new())
            .await
            .unwrap();
        assert!(outcome.output["echoed"].is_object());
    }

    #[tokio::test]
    async fn governance_refuses_dispatch_when_trust_budget_exhausted() {
        let (_, runner) = make_runner_with_budget(1).await;
        let pipeline = make_pipeline(
            "t/echo",
            vec![],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        let broker = Arc::new(TestBroker::new("t", vec![pipeline.clone()], serde_json::json!({})));
        // First dispatch succeeds (budget 1 → consumes 1)
        runner
            .dispatch(broker.clone(), &[pipeline.clone()], "t/echo".to_string(), ParamMap::new())
            .await
            .unwrap();
        // Second dispatch fails (budget exhausted)
        let err = runner
            .dispatch(broker, &[pipeline], "t/echo".to_string(), ParamMap::new())
            .await
            .unwrap_err();
        assert!(matches!(err, DispatchError::GovernanceRefused(_)));
    }

    #[tokio::test]
    async fn governance_kill_switch_refuses_subsequent_dispatches() {
        let (_, runner) = make_runner_with_budget(100).await;
        let pipeline = make_pipeline(
            "t/echo",
            vec![],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        let broker = Arc::new(TestBroker::new("t", vec![pipeline.clone()], serde_json::json!({})));
        // Pre-arm: dispatch ok
        runner
            .dispatch(broker.clone(), &[pipeline.clone()], "t/echo".to_string(), ParamMap::new())
            .await
            .unwrap();
        // Arm the kill switch via the governance composer
        runner.governance().arm_kill_switch();
        // Now dispatch fails
        let err = runner
            .dispatch(broker.clone(), &[pipeline.clone()], "t/echo".to_string(), ParamMap::new())
            .await
            .unwrap_err();
        assert!(matches!(err, DispatchError::GovernanceRefused(_)));
        // Disarm
        runner.governance().disarm_kill_switch();
        runner
            .dispatch(broker, &[pipeline], "t/echo".to_string(), ParamMap::new())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn governance_skips_for_internal_pipelines() {
        let (_, runner) = make_runner_with_budget(1).await;
        // Make an Internal pipeline; governance should NOT consume budget
        let mut internal = make_pipeline(
            "t/internal-tick",
            vec![],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        internal.visibility = Visibility::Internal;
        let broker = Arc::new(TestBroker::new("t", vec![internal.clone()], serde_json::json!({})));
        // Dispatch the Internal pipeline 5 times; budget should still allow
        // the 1 Surfaced dispatch below
        for _ in 0..5 {
            runner
                .dispatch(
                    broker.clone(),
                    &[internal.clone()],
                    "t/internal-tick".to_string(),
                    ParamMap::new(),
                )
                .await
                .unwrap();
        }
        let (used, _) = runner.governance().trust_budget_state();
        assert_eq!(used, 0); // Internal pipelines don't consume budget
    }

    #[tokio::test]
    async fn on_dispatch_complete_callback_fires_after_success() {
        let (_, runner) = make_runner().await;
        let pipeline = make_pipeline(
            "t/echo",
            vec![],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        let broker = Arc::new(TestBroker::new("t", vec![pipeline.clone()], serde_json::json!({})));

        let fired = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let fired_clone = fired.clone();
        runner.on_dispatch_complete(Arc::new(move |broker_id, outcome| {
            fired_clone
                .lock()
                .unwrap()
                .push(format!("{}:{}", broker_id, outcome.trace_id));
        }));

        runner
            .dispatch(broker, &[pipeline], "t/echo".to_string(), ParamMap::new())
            .await
            .unwrap();

        let fired_records = fired.lock().unwrap();
        assert_eq!(fired_records.len(), 1);
        assert!(fired_records[0].starts_with("t:"));
    }

    #[tokio::test]
    async fn dispatch_records_trace_with_snapshot_delta() {
        let (sink, runner) = make_runner().await;
        let pipeline = make_pipeline(
            "t/update",
            vec![],
            vec![Step::Leaf {
                leaf_op: "set-overlay-field".to_string(),
            }],
        );
        let broker = Arc::new(TestBroker::new(
            "t",
            vec![pipeline.clone()],
            serde_json::json!({"counter": 0}),
        ));
        runner
            .dispatch(broker.clone(), &[pipeline.clone()], "t/update".to_string(), ParamMap::new())
            .await
            .unwrap();
        runner
            .dispatch(broker, &[pipeline], "t/update".to_string(), ParamMap::new())
            .await
            .unwrap();
        // Trace file should contain 2 records
        let lines = std::fs::read_to_string(sink.file_path()).unwrap();
        assert_eq!(lines.lines().count(), 2);
    }

    /// A1/C8 regression: refused dispatches MUST appear in trace.jsonl.
    /// V0 silently dropped every refusal from the audit ledger.
    #[tokio::test]
    async fn refused_dispatches_record_to_trace_jsonl() {
        let (sink, runner) = make_runner_with_budget(1).await;
        let pipeline = make_pipeline(
            "t/echo",
            vec![],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        let broker = Arc::new(TestBroker::new(
            "t",
            vec![pipeline.clone()],
            serde_json::json!({}),
        ));

        // 1st dispatch: success (budget = 1 → consumes 1)
        runner
            .dispatch(broker.clone(), &[pipeline.clone()], "t/echo".to_string(), ParamMap::new())
            .await
            .unwrap();
        // 2nd dispatch: governance refusal (budget exhausted)
        let err = runner
            .dispatch(broker.clone(), &[pipeline.clone()], "t/echo".to_string(), ParamMap::new())
            .await
            .unwrap_err();
        assert!(matches!(err, DispatchError::GovernanceRefused(_)));
        // 3rd dispatch: leaf-op refusal (unknown leaf)
        let bad_pipeline = make_pipeline(
            "t/bad",
            vec![],
            vec![Step::Leaf {
                leaf_op: "definitely-not-a-leaf".to_string(),
            }],
        );
        // Use a fresh runner so the failed leaf doesn't get gated by exhausted budget
        let (sink2, runner2) = make_runner().await;
        let broker2 = Arc::new(TestBroker::new(
            "t",
            vec![bad_pipeline.clone()],
            serde_json::json!({}),
        ));
        let err = runner2
            .dispatch(broker2, &[bad_pipeline], "t/bad".to_string(), ParamMap::new())
            .await
            .unwrap_err();
        assert!(matches!(err, DispatchError::LeafOpFailed { .. }));

        // Both sinks should now have ONE record per dispatch attempt.
        let lines1 = std::fs::read_to_string(sink.file_path()).unwrap();
        assert_eq!(
            lines1.lines().count(),
            2,
            "trace must record both success + governance-refusal"
        );
        // The 2nd line should be the refusal
        let refusal: serde_json::Value =
            serde_json::from_str(lines1.lines().nth(1).unwrap()).unwrap();
        assert_eq!(refusal["outcome"]["status"], "refused");
        assert!(refusal["outcome"]["failure_reason"]
            .as_str()
            .unwrap()
            .contains("trust budget"));
        assert_eq!(refusal["audit_class"], "capability"); // attributed to t/echo pipeline

        let lines2 = std::fs::read_to_string(sink2.file_path()).unwrap();
        assert_eq!(lines2.lines().count(), 1, "leaf-op refusal must trace");
        let leaf_refusal: serde_json::Value =
            serde_json::from_str(lines2.lines().next().unwrap()).unwrap();
        assert_eq!(leaf_refusal["outcome"]["status"], "refused");
        assert!(leaf_refusal["outcome"]["failure_reason"]
            .as_str()
            .unwrap()
            .contains("leaf-op"));
    }

    /// A11: JSON Schema validation — required fields enforced.
    #[tokio::test]
    async fn a11_required_field_rejects_missing_param() {
        let (_, runner) = make_runner().await;
        let mut pipeline = make_pipeline(
            "t/needs-x",
            vec![],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        pipeline.params = serde_json::json!({
            "type": "object",
            "properties": { "x": { "type": "string" } },
            "required": ["x"]
        });
        let broker = Arc::new(TestBroker::new("t", vec![pipeline.clone()], serde_json::json!({})));
        // Missing required → ParamValidation error
        let err = runner
            .dispatch(broker, &[pipeline], "t/needs-x".to_string(), ParamMap::new())
            .await
            .unwrap_err();
        match err {
            DispatchError::ParamValidation { field, .. } => assert_eq!(field, "x"),
            other => panic!("expected ParamValidation, got {:?}", other),
        }
    }

    /// A11: declared optional fields don't require presence (V0 erroneously
    /// required every declared property).
    #[tokio::test]
    async fn a11_optional_field_passes_when_absent() {
        let (_, runner) = make_runner().await;
        let mut pipeline = make_pipeline(
            "t/opt",
            vec![],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        pipeline.params = serde_json::json!({
            "type": "object",
            "properties": {
                "x": { "type": "string" },
                "y": { "type": "string" }
            },
            "required": ["x"]
        });
        let broker = Arc::new(TestBroker::new("t", vec![pipeline.clone()], serde_json::json!({})));
        // Only x supplied — should succeed (y is optional)
        let mut params = ParamMap::new();
        params.insert("x".to_string(), serde_json::Value::String("hi".into()));
        runner
            .dispatch(broker, &[pipeline], "t/opt".to_string(), params)
            .await
            .unwrap();
    }

    /// A11: type mismatch rejected.
    #[tokio::test]
    async fn a11_type_mismatch_rejected() {
        let (_, runner) = make_runner().await;
        let mut pipeline = make_pipeline(
            "t/typed",
            vec![],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        pipeline.params = serde_json::json!({
            "type": "object",
            "properties": { "n": { "type": "integer" } },
            "required": ["n"]
        });
        let broker = Arc::new(TestBroker::new("t", vec![pipeline.clone()], serde_json::json!({})));
        let mut params = ParamMap::new();
        params.insert("n".to_string(), serde_json::Value::String("not-a-number".into()));
        let err = runner
            .dispatch(broker, &[pipeline], "t/typed".to_string(), params)
            .await
            .unwrap_err();
        match err {
            DispatchError::ParamValidation { field, reason } => {
                assert_eq!(field, "n");
                assert!(reason.contains("expected type"));
            }
            other => panic!("expected ParamValidation, got {:?}", other),
        }
    }

    /// A11: enum constraint rejected when value outside the declared set.
    #[tokio::test]
    async fn a11_enum_constraint_rejected() {
        let (_, runner) = make_runner().await;
        let mut pipeline = make_pipeline(
            "t/enum",
            vec![],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        pipeline.params = serde_json::json!({
            "type": "object",
            "properties": {
                "tier": { "type": "string", "enum": ["low", "medium", "high"] }
            },
            "required": ["tier"]
        });
        let broker = Arc::new(TestBroker::new("t", vec![pipeline.clone()], serde_json::json!({})));
        let mut params = ParamMap::new();
        params.insert("tier".to_string(), serde_json::Value::String("nope".into()));
        let err = runner
            .dispatch(broker.clone(), &[pipeline.clone()], "t/enum".to_string(), params.clone())
            .await
            .unwrap_err();
        match err {
            DispatchError::ParamValidation { field, .. } => assert_eq!(field, "tier"),
            other => panic!("expected ParamValidation, got {:?}", other),
        }
        // Accept a valid value.
        params.insert("tier".to_string(), serde_json::Value::String("medium".into()));
        runner
            .dispatch(broker, &[pipeline], "t/enum".to_string(), params)
            .await
            .unwrap();
    }

    /// A13 / BB #35: precondition DSL substitutes {frame.X} placeholders
    /// from the runner's Frame before evaluating. Operators can author a
    /// precondition like `frame_required_field` that resolves to whatever
    /// the operator-declared frame says — runtime-tunable without rewriting
    /// the catalog.
    #[tokio::test]
    async fn a13_precondition_substitutes_frame_placeholders() {
        let (_, runner) = make_runner().await;
        // Pipeline precondition refers to {frame.required_overlay_field}.
        // With the frame set, that resolves to a real field name and the
        // precondition checks against overlay.
        let pipeline = make_pipeline(
            "t/needs-frame",
            vec!["{frame.required_overlay_field}"],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        // Set frame: required_overlay_field = "active_work" (the overlay
        // field the test broker has).
        runner.set_frame(Frame::from_pairs(vec![(
            "required_overlay_field",
            serde_json::json!("active_work"),
        )]));
        let broker = Arc::new(TestBroker::new(
            "t",
            vec![pipeline.clone()],
            serde_json::json!({"active_work": [{"id": "X"}]}),
        ));
        runner
            .dispatch(broker, &[pipeline], "t/needs-frame".to_string(), ParamMap::new())
            .await
            .unwrap();
    }

    /// A13: when the frame placeholder isn't set, the precondition string is
    /// preserved literally — the DSL tries to evaluate `{frame.missing}` and
    /// fails because no overlay field has that name, so dispatch refuses
    /// with PreconditionUnmet.
    #[tokio::test]
    async fn a13_unset_frame_placeholder_preserved_then_precondition_fails() {
        let (_, runner) = make_runner().await;
        let pipeline = make_pipeline(
            "t/needs-frame",
            vec!["{frame.missing}"],
            vec![Step::Leaf {
                leaf_op: "echo".to_string(),
            }],
        );
        // Frame is empty (default).
        let broker = Arc::new(TestBroker::new("t", vec![pipeline.clone()], serde_json::json!({})));
        let err = runner
            .dispatch(broker, &[pipeline], "t/needs-frame".to_string(), ParamMap::new())
            .await
            .unwrap_err();
        assert!(matches!(err, DispatchError::PreconditionUnmet(_)));
    }

    /// A1/C8 regression: PipelineNotFound MUST also trace (caller asked for
    /// something that doesn't exist; operator wants this in the audit log so
    /// agent-asking-for-nonexistent-pipelines is forensically visible).
    #[tokio::test]
    async fn pipeline_not_found_records_to_trace_jsonl() {
        let (sink, runner) = make_runner().await;
        let broker = Arc::new(TestBroker::new("t", vec![], serde_json::json!({})));
        let err = runner
            .dispatch(broker, &[], "t/nonexistent".to_string(), ParamMap::new())
            .await
            .unwrap_err();
        assert!(matches!(err, DispatchError::PipelineNotFound { .. }));
        let lines = std::fs::read_to_string(sink.file_path()).unwrap();
        assert_eq!(lines.lines().count(), 1);
        let trace: serde_json::Value = serde_json::from_str(lines.lines().next().unwrap()).unwrap();
        assert_eq!(trace["outcome"]["status"], "refused");
        // No pipeline matched, so audit_class defaults to governance.
        assert_eq!(trace["audit_class"], "governance");
        assert_eq!(trace["pipeline_id"], "t/nonexistent");
    }
}
