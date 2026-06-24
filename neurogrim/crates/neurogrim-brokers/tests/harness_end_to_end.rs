//! End-to-end integration test: the FULL broker harness substrate exercised
//! through the canonical operator workflow. This is the Wave 5 exit-gate
//! demonstration (per plan), MINUS the MCP server + Claude Code session
//! (those are Wave 5.5 follow-up; the substrate end-to-end test verifies
//! everything that doesn't require Claude Code to run live).
//!
//! Operator workflow being demonstrated:
//!
//! 1. Operator writes a cluster manifest TOML declaring 1 broker (Work Broker)
//! 2. Operator writes a per-broker manifest TOML for the Work Broker
//! 3. Operator's main binary (which Wave 5.5 will turn into
//!    `neurogrim broker-serve`) calls:
//!    - `BrokerRegistry::load_manifests(...)` to load TOML
//!    - Constructs the concrete `WorkBroker` instance
//!    - Calls `registry.register(broker)` + `registry.validate()`
//! 4. On each tick / dispatch:
//!    - Hot-Store Materializer writes broker overlay to segment file
//!    - Awareness Materializer writes catalog routing signals to segment file
//!    - Materializer Composer concatenates segments → `current-projection.md`
//! 5. Agent (via single `dispatch_pipeline` MCP tool, Wave 5.5) dispatches
//!    `work-broker/dispatch-work-unit` with `work_unit_id: B-100`
//!    Here we invoke the Runner directly (Wave 5.5 routes through MCP).
//! 6. Framework enforces: pre-dispatch governance checks (kill-switch + trust
//!    budget); precondition evaluation; step execution (claim + refresh);
//!    trace record with snapshot delta
//! 7. Re-materialization: agent sees updated overlay in next-turn projection

use neurogrim_brokers::{
    AwarenessMaterializer, BacklogState, Broker, BrokerRegistry, GovernanceComposer,
    HotStoreMaterializer, MaterializerComposer, ParamMap, PipelineRunner, TraceSink, WorkBroker,
    WorkUnit, WorkUnitStatus,
};
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

fn write_cluster_fixtures(tmp: &TempDir) -> std::path::PathBuf {
    let cluster_path = tmp.path().join("cluster.toml");
    let broker_path = tmp.path().join("work-broker.toml");
    std::fs::write(
        &cluster_path,
        format!(
            r#"
[cluster]
id = "ceregrim-demo"
name = "cereGrim S*-T MVP Demo"
brokers_dir = "./"

[cluster.brokers.work-broker]
manifest_path = "work-broker.toml"

[cluster.materializer]
composition_order = ["overlay-work-broker", "awareness-routing-work-broker"]
output_path = "{output}"
segments_dir = "{segments}"
context_budget_chars = 16384
"#,
            output = tmp
                .path()
                .join("current-projection.md")
                .display()
                .to_string()
                .replace('\\', "/"),
            segments = tmp.path().join("segments").display().to_string().replace('\\', "/"),
        ),
    )
    .unwrap();
    std::fs::write(
        &broker_path,
        r#"
[broker]
id = "work-broker"
name = "Work Broker"
roles = ["innate-ability"]
cold_store_path = "./work-broker-cold/"
catalog_path = "./work-broker-catalog.yaml"
"#,
    )
    .unwrap();
    cluster_path
}

fn make_initial_backlog() -> BacklogState {
    BacklogState {
        work_units: vec![
            WorkUnit {
                id: "B-100".to_string(),
                title: "Implement Wave 5 reference broker".to_string(),
                status: WorkUnitStatus::Ready,
            },
            WorkUnit {
                id: "B-101".to_string(),
                title: "Wire up MCP server".to_string(),
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

async fn materialize_all(
    broker: Arc<dyn Broker>,
    cluster_dir: &Path,
    operator_order: &[String],
) -> std::io::Result<()> {
    let segments_dir = cluster_dir.join("segments");
    let output_path = cluster_dir.join("current-projection.md");

    let hot = HotStoreMaterializer::new(broker.id().to_string(), segments_dir.clone());
    hot.materialize(broker.clone()).await.unwrap();

    let awareness = AwarenessMaterializer::new(broker.id().to_string(), segments_dir.clone());
    awareness.materialize(broker.clone()).await.unwrap();

    // Synthesize a minimal governance segment (per BB #22a governance-first
    // override; the MVP harness writes this to satisfy the Composer's
    // expectation that governance-pipelines.md exists).
    let governance_segment = segments_dir.join("governance-pipelines.md");
    std::fs::create_dir_all(&segments_dir)?;
    std::fs::write(
        &governance_segment,
        "## Governance pipelines (always-reachable)\n\n\
         - `work-broker/arm-kill-switch` (Surfaced, OperatorOnly)\n\
         - `work-broker/check-trust-budget` (Internal, Untunable)\n\
         - `work-broker/check-kill-switch` (Internal, Untunable)\n\
         - `work-broker/record-dispatch` (Internal, Untunable)\n\
         - `work-broker/record-outcome` (Internal, Untunable)\n\n\
         Trust budget enforced + kill switch enforced per BB #19.\n",
    )?;

    let composer = MaterializerComposer::new(output_path, segments_dir, 16_384);
    composer.compose(operator_order).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(())
}

#[tokio::test]
async fn harness_end_to_end_workflow() {
    let tmp = TempDir::new().unwrap();
    let cluster_path = write_cluster_fixtures(&tmp);

    // === Step 1-3: Load manifests + construct broker + register ===
    let mut registry = BrokerRegistry::load_manifests(&cluster_path).unwrap();

    let governance = Arc::new(GovernanceComposer::new(1000));
    let work_broker = Arc::new(WorkBroker::new(
        "work-broker",
        make_initial_backlog(),
        governance.clone(),
    ));
    let work_broker_dyn: Arc<dyn Broker> = work_broker.clone();
    registry.register(work_broker_dyn.clone()).unwrap();
    registry.validate().unwrap();

    // === Step 4: Materialize (initial projection) ===
    let order: Vec<String> = registry.cluster().materializer.composition_order.clone();
    materialize_all(work_broker_dyn.clone(), tmp.path(), &order)
        .await
        .unwrap();
    let projection_path = tmp.path().join("current-projection.md");
    let initial_projection = std::fs::read_to_string(&projection_path).unwrap();
    // Governance-first override: "Governance pipelines" heading must come
    // before "Awareness routing" / "Overlay state" headings
    assert!(initial_projection.contains("Governance pipelines"));
    assert!(initial_projection.contains("work-broker"));
    assert!(initial_projection.contains("B-100"));
    let gov_pos = initial_projection.find("Governance pipelines").unwrap();
    let overlay_pos = initial_projection.find("Overlay state").unwrap();
    assert!(gov_pos < overlay_pos, "governance must precede overlay (R-O-3)");

    // === Step 5: Agent dispatches work-broker/dispatch-work-unit ===
    let tmp_trace = tempfile::NamedTempFile::new().unwrap();
    let trace_path = tmp_trace.path().to_path_buf();
    std::mem::forget(tmp_trace);
    let trace_sink = Arc::new(TraceSink::new(trace_path));
    let runner = PipelineRunner::new(trace_sink.clone(), governance.clone());

    let catalog = WorkBroker::new("work-broker", make_initial_backlog(), governance.clone())
        .catalog();

    let mut params = ParamMap::new();
    params.insert(
        "work_unit_id".to_string(),
        serde_json::Value::String("B-100".to_string()),
    );
    let outcome = runner
        .dispatch(
            work_broker_dyn.clone(),
            &catalog,
            "work-broker/dispatch-work-unit".to_string(),
            params,
        )
        .await
        .expect("end-to-end dispatch should succeed");
    assert_eq!(outcome.output["refreshed"], true);

    // === Step 6: Verify framework enforcement ===
    // Trust budget consumed 1 (Surfaced pipeline)
    let (used, ceiling) = governance.trust_budget_state();
    assert_eq!(used, 1);
    assert_eq!(ceiling, 1000);
    // Trace written
    let trace_lines: Vec<String> = std::fs::read_to_string(trace_sink.file_path())
        .unwrap()
        .lines()
        .map(String::from)
        .collect();
    assert_eq!(trace_lines.len(), 1);

    // === Step 7: Re-materialize → agent sees updated state ===
    materialize_all(work_broker_dyn.clone(), tmp.path(), &order)
        .await
        .unwrap();
    let updated_projection = std::fs::read_to_string(&projection_path).unwrap();
    // B-100 should now be in recent_claims (not active_work)
    assert!(updated_projection.contains("recent_claims"));
    let new_overlay = work_broker.read_overlay().await;
    let active_after = new_overlay["active_work"].as_array().unwrap();
    assert_eq!(active_after.len(), 1);
    assert_eq!(active_after[0]["id"], "B-101");
}

#[tokio::test]
async fn harness_kill_switch_halts_dispatches() {
    let tmp = TempDir::new().unwrap();
    let cluster_path = write_cluster_fixtures(&tmp);
    let mut registry = BrokerRegistry::load_manifests(&cluster_path).unwrap();

    let governance = Arc::new(GovernanceComposer::new(100));
    let broker: Arc<dyn Broker> = Arc::new(WorkBroker::new(
        "work-broker",
        make_initial_backlog(),
        governance.clone(),
    ));
    registry.register(broker.clone()).unwrap();
    registry.validate().unwrap();

    let tmp_trace = tempfile::NamedTempFile::new().unwrap();
    let trace_path = tmp_trace.path().to_path_buf();
    std::mem::forget(tmp_trace);
    let runner = PipelineRunner::new(
        Arc::new(TraceSink::new(trace_path)),
        governance.clone(),
    );
    let catalog = WorkBroker::new("work-broker", make_initial_backlog(), governance.clone())
        .catalog();

    // Operator arms kill switch
    governance.arm_kill_switch();
    let mut params = ParamMap::new();
    params.insert(
        "work_unit_id".to_string(),
        serde_json::Value::String("B-100".to_string()),
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
    assert!(matches!(
        err,
        neurogrim_brokers::DispatchError::GovernanceRefused(_)
    ));
}
