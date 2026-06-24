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
use crate::pipeline::{ParamMap, Pipeline, PipelineId, Step};
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

    #[error("governance refused dispatch: {failure_reason}")]
    GovernanceRefused { failure_reason: String },

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

/// Pipeline Runner. Holds shared infrastructure (trace sink, prior-snapshot
/// cache for delta computation); brokers + catalogs are passed per dispatch.
pub struct PipelineRunner {
    trace_sink: Arc<TraceSink>,
    /// Most recent overlay snapshot per broker, used for trace delta
    /// computation. tokio Mutex because dispatches are async.
    prior_snapshots: tokio::sync::Mutex<
        std::collections::HashMap<String, (String, serde_json::Value)>,
    >,
}

impl PipelineRunner {
    pub fn new(trace_sink: Arc<TraceSink>) -> Self {
        Self {
            trace_sink,
            prior_snapshots: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Dispatch a pipeline against a broker.
    pub async fn dispatch(
        &self,
        broker: Arc<dyn Broker>,
        catalog: &[Pipeline],
        pipeline_id: PipelineId,
        params: ParamMap,
    ) -> Result<DispatchOutcome, DispatchError> {
        let start = std::time::Instant::now();
        let trace_id = Uuid::new_v4().to_string();

        // 1. Catalog lookup
        let pipeline = catalog
            .iter()
            .find(|p| p.id == pipeline_id)
            .ok_or_else(|| DispatchError::PipelineNotFound {
                broker_id: broker.id().to_string(),
                pipeline_id: pipeline_id.clone(),
            })?
            .clone();

        // 2. Param validation (Wave 4 elevates to JSON Schema; MVP minimal)
        validate_params_minimal(&pipeline, &params)?;

        // 3. Read broker Overlay snapshot for precondition + leaf-op ctx
        let overlay_snapshot = broker.read_overlay().await;

        // 4. Precondition evaluation
        for predicate in &pipeline.preconditions {
            let holds = catalog::evaluate_precondition(predicate, &overlay_snapshot, &params)?;
            if !holds {
                return Err(DispatchError::PreconditionUnmet(predicate.clone()));
            }
        }

        // 5. Step execution
        let leaf_ctx = LeafContext {
            broker_id: broker.id().to_string(),
            pipeline_id: pipeline_id.clone(),
            params: params.clone(),
            overlay_snapshot: overlay_snapshot.clone(),
        };
        let output = self
            .execute_steps(broker.clone(), catalog, &pipeline.steps, &leaf_ctx)
            .await?;

        let duration_ms = start.elapsed().as_millis() as u64;

        // 6. Compute snapshot delta + record trace
        let post_snapshot = broker.read_overlay().await;
        let (delta, delta_from) = self
            .compute_snapshot_delta(broker.id(), &post_snapshot, &trace_id)
            .await;

        let record = TraceRecord {
            schema_version: "1".to_string(),
            ts: Utc::now().to_rfc3339(),
            trace_id: trace_id.clone(),
            pipeline_id: pipeline_id.clone(),
            broker_id: broker.id().to_string(),
            params: serde_json::Value::Object(params),
            snapshot_delta_from: delta_from,
            snapshot_delta: delta,
            outcome: serde_json::json!({"status": "success", "output": output.clone()}),
            audit_class: format!("{:?}", pipeline.audit_class).to_lowercase(),
            duration_ms,
        };
        self.trace_sink.append(&record)?;

        Ok(DispatchOutcome {
            trace_id,
            output,
            duration_ms,
        })
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
                    // Intra-broker sub_pipeline dispatch (cross-broker was rejected
                    // at catalog load per BB #27 deferral)
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
                }
                Step::Guard { predicate, inner } => {
                    let holds = catalog::evaluate_precondition(
                        predicate,
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
                    let holds = catalog::evaluate_precondition(
                        predicate,
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

/// Minimal params validation: check that all declared param schema fields
/// are present (Wave 4 elevates to full JSON Schema).
fn validate_params_minimal(pipeline: &Pipeline, params: &ParamMap) -> Result<(), DispatchError> {
    if let Some(props) = pipeline.params.get("properties") {
        if let Some(props_obj) = props.as_object() {
            for (field, _schema) in props_obj {
                if !params.contains_key(field) {
                    return Err(DispatchError::ParamValidation {
                        field: field.clone(),
                        reason: "required parameter missing".to_string(),
                    });
                }
            }
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
        }
    }

    async fn make_runner() -> (Arc<TraceSink>, PipelineRunner) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        std::mem::forget(tmp); // keep file alive for test
        let sink = Arc::new(TraceSink::new(path));
        let runner = PipelineRunner::new(sink.clone());
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
}
