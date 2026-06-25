//! Phase D integration test — exercises D1 (BB #27 cross-broker
//! sub_pipeline composition), D2 (BB #11 Workflow Engine), and D3
//! (BB #20 Skill Filter) substrate primitives together in a single
//! end-to-end scenario.
//!
//! Per plan §D exit gate: verifies all three Phase D primitives ship
//! a working API surface that composes.

use neurogrim_brokers::{
    catalog::{validate_catalog_with_policy, CrossBrokerPolicy},
    AuditClass, Broker, BrokerError, EffectClass, GovernanceComposer, LeafContext, LeafError,
    NoOpRanker, Overlay, ParamMap, Pipeline, PipelineRunner, RankerContext, Role, RoleSet,
    SegmentRanker, Step, SuspendedDispatch, TraceSink, Tunability, Visibility, WakeCondition,
    WorkflowEngine, WorldEvent,
};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

// Minimal test broker that supports cross-broker sub_pipeline references.
struct TestBroker {
    id: String,
    overlay: Arc<Overlay<serde_json::Value>>,
}

impl TestBroker {
    fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            overlay: Arc::new(Overlay::new(serde_json::json!({"calls": 0}))),
        }
    }
}

#[async_trait]
impl Broker for TestBroker {
    fn id(&self) -> &str {
        &self.id
    }
    fn role_set(&self) -> RoleSet {
        RoleSet::single(Role::InnateAbility)
    }
    async fn read_overlay(&self) -> serde_json::Value {
        (*self.overlay.load()).clone()
    }
    async fn legal_pipelines(&self) -> Vec<Pipeline> {
        vec![]
    }
    async fn governance_pipelines(&self) -> Vec<Pipeline> {
        vec![]
    }
    async fn tick(&self, _: WorldEvent) -> Result<(), BrokerError> {
        Ok(())
    }
    async fn execute_leaf(
        &self,
        name: &str,
        _ctx: LeafContext,
    ) -> Result<serde_json::Value, LeafError> {
        match name {
            "do-work" => Ok(serde_json::json!({"broker": self.id, "did": "work"})),
            other => Err(LeafError::NotFound(other.to_string())),
        }
    }
}

fn surfaced_pipeline(broker_id: &str, name: &str, steps: Vec<Step>) -> Pipeline {
    Pipeline {
        id: format!("{}/{}", broker_id, name),
        visibility: Visibility::Surfaced,
        tunability: Tunability::OperatorOnly,
        audit_class: AuditClass::Capability,
        effect_class: EffectClass::HotStoreUpdate,
        params: serde_json::json!({}),
        preconditions: vec![],
        steps,
        description: String::new(),
        when_to_use: String::new(),
        bypasses_kill_switch: false,
    }
}

/// D1 exit gate: cross-broker sub_pipeline composition works end-to-end.
#[tokio::test]
async fn d1_cross_broker_sub_pipeline_dispatch_round_trip() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let trace_path = tmp.path().to_path_buf();
    std::mem::forget(tmp);

    let governance = Arc::new(GovernanceComposer::new(100));
    let sink = Arc::new(TraceSink::new(trace_path));

    // Two brokers: parent + callee. Parent's pipeline sub-dispatches
    // into callee's leaf-op.
    let parent = Arc::new(TestBroker::new("parent"));
    let callee = Arc::new(TestBroker::new("callee"));

    let callee_leaf = surfaced_pipeline(
        "callee",
        "do-the-work",
        vec![Step::Leaf {
            leaf_op: "do-work".to_string(),
        }],
    );
    let parent_pipeline = surfaced_pipeline(
        "parent",
        "delegate",
        vec![Step::SubPipeline {
            sub_pipeline: "callee/do-the-work".to_string(),
            params: ParamMap::new(),
        }],
    );

    // D1 catalog policy: validate with CrossBrokerPolicy::Allow.
    validate_catalog_with_policy(
        &[parent_pipeline.clone()],
        "parent",
        CrossBrokerPolicy::Allow,
    )
    .expect("D1 Allow policy should accept cross-broker references");

    // Build a registry that knows both brokers + register the runner.
    use neurogrim_brokers::registry::BrokerRegistry;
    let cluster_dir = tempfile::TempDir::new().unwrap();
    let cluster_path = cluster_dir.path().join(".claude/brain/broker/cluster.toml");
    std::fs::create_dir_all(cluster_path.parent().unwrap()).unwrap();
    std::fs::write(
        &cluster_path,
        r#"[cluster]
id = "d1-test"
name = "D1 cross-broker test"
brokers_dir = "./"

[cluster.brokers.parent]
manifest_path = "parent.toml"
[cluster.brokers.callee]
manifest_path = "callee.toml"
"#,
    )
    .unwrap();
    let per_broker_template = r#"[broker]
id = "{}"
name = "{}"
roles = ["innate-ability"]
cold_store_path = "{}-cold/"
catalog_path = "{}-catalog.yaml"
"#;
    for id in ["parent", "callee"] {
        std::fs::write(
            cluster_path.parent().unwrap().join(format!("{}.toml", id)),
            per_broker_template.replace("{}", id),
        )
        .unwrap();
    }
    let mut registry = BrokerRegistry::load_manifests(&cluster_path).unwrap();
    registry
        .register_with_catalog(parent.clone() as Arc<dyn Broker>, vec![parent_pipeline.clone()])
        .unwrap();
    registry
        .register_with_catalog(callee.clone() as Arc<dyn Broker>, vec![callee_leaf.clone()])
        .unwrap();
    let registry = Arc::new(registry);

    let runner = PipelineRunner::new(sink, governance);
    runner.set_registry(registry.clone());

    // Dispatch parent's pipeline → triggers cross-broker sub_pipeline to callee.
    let outcome = runner
        .dispatch(
            parent.clone() as Arc<dyn Broker>,
            &[parent_pipeline],
            "parent/delegate".to_string(),
            ParamMap::new(),
        )
        .await
        .expect("D1: cross-broker dispatch should succeed");

    // The output should be the callee's leaf-op's payload (D1 contract:
    // sub_pipeline's output flows back through the parent's step result).
    assert_eq!(outcome.output["broker"], "callee");
    assert_eq!(outcome.output["did"], "work");
}

/// D2 exit gate: Workflow Engine MVP supports suspend → drain → resume.
#[tokio::test]
async fn d2_workflow_engine_suspend_drain_resume() {
    let engine = WorkflowEngine::new();
    assert_eq!(engine.suspended_count(), 0);

    // Suspend three dispatches with different wake conditions.
    let id1 = engine.suspend(
        SuspendedDispatch {
            broker_id: "x".to_string(),
            pipeline_id: "x/step-b".to_string(),
            params: ParamMap::new(),
            resume_at_step: 1,
            payload: serde_json::json!({"k": "v"}),
            origin_trace_id: "trace-x-1".to_string(),
            reason: "waiting on tick".to_string(),
        },
        WakeCondition::Tick,
    );
    let _id2 = engine.suspend(
        SuspendedDispatch {
            broker_id: "y".to_string(),
            pipeline_id: "y/wait".to_string(),
            params: ParamMap::new(),
            resume_at_step: 0,
            payload: serde_json::Value::Null,
            origin_trace_id: "trace-y-1".to_string(),
            reason: "waiting on duration".to_string(),
        },
        WakeCondition::AfterDuration(Duration::from_millis(30)),
    );
    assert_eq!(engine.suspended_count(), 2);

    // Drain with tick_now=true: only the Tick waiter drains.
    let ready = engine.drain_ready(true);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].0, id1);
    assert_eq!(ready[0].1.broker_id, "x");

    // The duration waiter is still queued.
    assert_eq!(engine.suspended_count(), 1);

    // Wait past its window, then drain.
    std::thread::sleep(Duration::from_millis(50));
    let ready = engine.drain_ready(false);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].1.broker_id, "y");
    assert_eq!(engine.suspended_count(), 0);
}

/// D3 exit gate: SegmentRanker trait + NoOpRanker default behave correctly.
#[tokio::test]
async fn d3_segment_ranker_filters_for_top_k() {
    use neurogrim_brokers::CandidateSegment;

    let segments = vec![
        CandidateSegment {
            name: "a".to_string(),
            body: "alpha".to_string(),
        },
        CandidateSegment {
            name: "b".to_string(),
            body: "beta".to_string(),
        },
        CandidateSegment {
            name: "c".to_string(),
            body: "gamma".to_string(),
        },
    ];

    // NoOpRanker returns all segments in input order.
    let ranked = NoOpRanker.rank(&segments, &RankerContext::default());
    assert_eq!(ranked.len(), 3);
    assert_eq!(ranked[0].name, "a");
    assert_eq!(ranked[2].name, "c");

    // Custom top-K ranker: returns only first 2.
    struct TopTwo;
    impl SegmentRanker for TopTwo {
        fn rank<'a>(
            &self,
            segments: &'a [CandidateSegment],
            _: &RankerContext,
        ) -> Vec<&'a CandidateSegment> {
            segments.iter().take(2).collect()
        }
    }
    let ranked = TopTwo.rank(&segments, &RankerContext::default());
    assert_eq!(ranked.len(), 2);
    assert_eq!(ranked[1].name, "b");
}

/// D exit-gate composite test: D1 + D2 + D3 all loaded + functional in
/// the same crate without interference. Validates the substrate's
/// public API surface lets consumers compose all three primitives.
#[test]
fn d_exit_gate_api_surface_loads_cleanly() {
    use neurogrim_brokers::{
        BrokerFactoryRegistry, BrokerHostConfig, Frame, NoOpRanker, RankerContext,
        SuspendedDispatch, WakeCondition, WorkflowEngine,
    };
    // Construct one of each (smoke test the type names + constructors).
    let _engine = WorkflowEngine::new();
    let _cond = WakeCondition::Tick;
    let _ = WakeCondition::AfterDuration(Duration::from_secs(1));
    let _frame = Frame::new();
    let _ctx = RankerContext::default();
    let _ranker = NoOpRanker;
    let _factories = BrokerFactoryRegistry::new();
    let _config = BrokerHostConfig::default();
    let _dispatch = SuspendedDispatch {
        broker_id: "x".to_string(),
        pipeline_id: "x/y".to_string(),
        params: ParamMap::new(),
        resume_at_step: 0,
        payload: serde_json::Value::Null,
        origin_trace_id: String::new(),
        reason: String::new(),
    };
    // If this compiles, the public API surface for Phase D is intact.
}
