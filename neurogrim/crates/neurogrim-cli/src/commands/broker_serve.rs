//! `neurogrim broker-serve` — Wave 5.5 S*-T MVP broker harness for Claude Code.
//!
//! Long-running MCP server on stdio that exposes the broker substrate
//! (built in `neurogrim-brokers`) to Claude Code via a SINGLE
//! `dispatch_pipeline` tool. Operator caveat (Phase 1 planning):
//! brokers were specifically designed to AVOID MCP's "all-tools-blob"
//! failure mode. So this server exposes **one generic tool**, not N
//! per-pipeline tools. Discovery happens via `current-projection.md`
//! (auto-loaded via CLAUDE.md); the MCP tool is the wire protocol for
//! the action, not the choice surface.
//!
//! ## Daemon lifecycle (V0-RETROSPECTIVE §C5 simplification)
//!
//! When the MCP server is on stdio, the process lifetime IS the MCP session.
//! Claude Code launches the binary per its `.mcp.json` config; the binary
//! lives until the MCP client disconnects. No PID file or separate daemon
//! infrastructure needed for the V0 stdio model.

use anyhow::{Context, Result};
use neurogrim_brokers::{
    AwarenessMaterializer, BacklogState, Broker, BrokerRegistry, GovernanceComposer,
    HotStoreMaterializer, MaterializerComposer, PipelineRunner, TraceSink, WorkBroker,
};
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::{schemars, tool, tool_router, ServerHandler, ServiceExt};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Per-dispatch params for the single `dispatch_pipeline` MCP tool.
/// Outer fields (broker_id, pipeline_id) are typed; `params` is opaque
/// JSON (the per-pipeline schema is surfaced via `current-projection.md`
/// not via MCP).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DispatchParams {
    /// Broker that owns the pipeline (e.g., `work-broker`).
    pub broker_id: String,
    /// Pipeline id (e.g., `work-broker/dispatch-work-unit`). Must start
    /// with `<broker_id>/`.
    pub pipeline_id: String,
    /// Pipeline parameters (opaque to MCP; validated against the pipeline's
    /// param schema at dispatch time by the framework).
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Shared runtime context for materialization re-triggering.
#[derive(Clone)]
struct MaterializerContext {
    bootstrapped: Vec<(String, Arc<dyn Broker>)>,
    segments_dir: PathBuf,
    output_path: PathBuf,
    composition_order: Vec<String>,
    context_budget: usize,
}

impl MaterializerContext {
    async fn materialize_all(&self) -> Result<()> {
        for (broker_id, broker) in &self.bootstrapped {
            let hot = HotStoreMaterializer::new(broker_id.clone(), self.segments_dir.clone());
            hot.materialize(broker.clone())
                .await
                .map_err(|e| anyhow::anyhow!("hot-store materializer: {}", e))?;
            let aware = AwarenessMaterializer::new(broker_id.clone(), self.segments_dir.clone());
            aware
                .materialize(broker.clone())
                .await
                .map_err(|e| anyhow::anyhow!("awareness materializer: {}", e))?;
        }
        synthesize_governance_segment(&self.segments_dir, &self.bootstrapped)?;
        let composer = MaterializerComposer::new(
            self.output_path.clone(),
            self.segments_dir.clone(),
            self.context_budget,
        );
        composer
            .compose(&self.composition_order)
            .map_err(|e| anyhow::anyhow!("compose: {}", e))?;
        Ok(())
    }
}

/// The broker MCP server state. Holds the runtime infrastructure +
/// per-broker handles needed for dispatch + re-materialization.
#[derive(Clone)]
pub struct BrokerMcpServer {
    registry: Arc<BrokerRegistry>,
    runner: Arc<PipelineRunner>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl BrokerMcpServer {
    #[tool(
        description = "Dispatch a broker pipeline. The agent's discovery surface is `current-projection.md` (auto-loaded via CLAUDE.md), which lists each broker's Surfaced pipelines + their description + when_to_use + currently-legal status + parameter schema. This tool is the WIRE PROTOCOL for the dispatch action; the agent picks which pipeline to dispatch by reading current-projection.md, NOT by enumerating MCP tools. Returns the structured dispatch outcome or a refusal with failure_reason."
    )]
    async fn dispatch_pipeline(&self, Parameters(p): Parameters<DispatchParams>) -> String {
        let broker = match self.registry.broker(&p.broker_id) {
            Some(b) => b,
            None => {
                return refusal("broker_not_found", serde_json::json!({"broker_id": p.broker_id}));
            }
        };

        let catalog = self.registry.full_catalog();
        if catalog.is_empty() {
            return refusal("catalog_empty", serde_json::Value::Null);
        }

        let params_map: neurogrim_brokers::ParamMap = match &p.params {
            serde_json::Value::Object(map) => map.clone(),
            serde_json::Value::Null => Default::default(),
            other => {
                return refusal(
                    "params_must_be_object_or_null",
                    serde_json::json!({"received": other}),
                );
            }
        };

        match self
            .runner
            .dispatch(broker.clone(), &catalog, p.pipeline_id.clone(), params_map)
            .await
        {
            Ok(outcome) => serde_json::to_string_pretty(&serde_json::json!({
                "status": "success",
                "trace_id": outcome.trace_id,
                "output": outcome.output,
                "duration_ms": outcome.duration_ms,
            }))
            .unwrap_or_default(),
            Err(e) => refusal(&format!("{}", e), serde_json::Value::Null),
        }
    }
}

fn refusal(reason: &str, details: serde_json::Value) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "status": "refused",
        "failure_reason": reason,
        "details": details,
    }))
    .unwrap_or_default()
}

impl ServerHandler for BrokerMcpServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::default(),
            capabilities: rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: rmcp::model::Implementation {
                name: "neurogrim-broker-serve".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("NeuroGrim Broker Harness".to_string()),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Broker substrate MCP server. Exposes ONE tool: `dispatch_pipeline`. \
                 The agent's pipeline-discovery surface is `current-projection.md` \
                 (auto-loaded via CLAUDE.md); use the materializer output to learn \
                 which broker_id/pipeline_id + params to invoke. Brokers were \
                 designed to avoid MCP's all-tools-blob failure mode; this server \
                 preserves that by NOT exposing per-pipeline tools."
                    .to_string(),
            ),
        }
    }
}

pub async fn run(cluster_manifest_path: &str) -> Result<()> {
    let cluster_path = Path::new(cluster_manifest_path);
    eprintln!("✦ Loading cluster manifest: {}", cluster_path.display());

    let mut registry = BrokerRegistry::load_manifests(cluster_path)
        .with_context(|| format!("loading cluster manifest at {}", cluster_path.display()))?;

    // Construct concrete brokers. MVP: every broker declared in the cluster
    // manifest is constructed as a WorkBroker. Future versions will dispatch
    // by broker_type field in per-broker manifests.
    let governance = Arc::new(GovernanceComposer::new(10_000));

    let mut bootstrapped: Vec<(String, Arc<dyn Broker>)> = Vec::new();
    for broker_id in registry.cluster().brokers.keys().cloned().collect::<Vec<_>>() {
        let work_broker = Arc::new(WorkBroker::new(
            broker_id.clone(),
            BacklogState::default(),
            governance.clone(),
        ));
        let catalog = work_broker.catalog();
        let dyn_broker: Arc<dyn Broker> = work_broker;
        registry
            .register_with_catalog(dyn_broker.clone(), catalog)
            .with_context(|| format!("registering broker `{}`", broker_id))?;
        bootstrapped.push((broker_id, dyn_broker));
    }
    registry.validate().context("validating broker registry")?;
    eprintln!("✦ Registered {} broker(s)", bootstrapped.len());

    // Wire paths
    let cluster_dir = registry.cluster_manifest_dir().to_path_buf();
    let segments_dir = resolve_path(&cluster_dir, &registry.cluster().materializer.segments_dir);
    let output_path = resolve_path(&cluster_dir, &registry.cluster().materializer.output_path);
    let trace_path = segments_dir
        .parent()
        .unwrap_or(Path::new("."))
        .join("trace.jsonl");

    std::fs::create_dir_all(&segments_dir)
        .with_context(|| format!("creating segments dir at {}", segments_dir.display()))?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let trace_sink = Arc::new(TraceSink::new(trace_path));
    let runner = Arc::new(PipelineRunner::new(trace_sink, governance.clone()));

    let ctx = MaterializerContext {
        bootstrapped: bootstrapped.clone(),
        segments_dir: segments_dir.clone(),
        output_path: output_path.clone(),
        composition_order: registry.cluster().materializer.composition_order.clone(),
        context_budget: registry.cluster().materializer.context_budget_chars,
    };

    // Initial materialization
    ctx.materialize_all().await.context("initial materialization")?;
    eprintln!("✦ Initial projection written to: {}", output_path.display());

    // Wire on_dispatch_complete: spawn a tokio task to re-materialize.
    // Fire-and-forget; next agent turn might see slightly-stale state if
    // re-materialization hasn't completed yet (typically <50ms; acceptable
    // for MVP).
    {
        let ctx_for_callback = ctx.clone();
        runner.on_dispatch_complete(Arc::new(move |_broker_id, _outcome| {
            let ctx = ctx_for_callback.clone();
            tokio::spawn(async move {
                if let Err(e) = ctx.materialize_all().await {
                    eprintln!("⚠ re-materialization failed: {}", e);
                }
            });
        }));
    }

    // Boot MCP server on stdio
    let server = BrokerMcpServer {
        registry: Arc::new(registry),
        runner,
        tool_router: BrokerMcpServer::tool_router(),
    };
    eprintln!("✦ Summoning broker MCP server on stdio…");
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

fn resolve_path(cluster_dir: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cluster_dir.join(p)
    }
}

fn synthesize_governance_segment(
    segments_dir: &Path,
    bootstrapped: &[(String, Arc<dyn Broker>)],
) -> Result<()> {
    let mut body = String::from("## Governance pipelines (always-reachable)\n\n");
    body.push_str(
        "The following governance pipelines are framework-provided + composed \
         automatically into every Surfaced pipeline dispatch (per BB #19 + R-O-3 \
         reachability invariant):\n\n",
    );
    body.push_str("- `<broker_id>/check-trust-budget` — refuses if budget exhausted\n");
    body.push_str("- `<broker_id>/check-kill-switch` — refuses if armed\n");
    body.push_str("- `<broker_id>/record-dispatch` — writes audit anchor at start\n");
    body.push_str("- `<broker_id>/record-outcome` — writes audit at end\n");
    body.push_str(
        "- `<broker_id>/arm-kill-switch` — **Surfaced**, OperatorOnly: emergency halt\n\n",
    );
    body.push_str(&format!(
        "Active brokers in this cluster: {}\n",
        bootstrapped
            .iter()
            .map(|(id, _)| format!("`{}`", id))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    std::fs::write(segments_dir.join("governance-pipelines.md"), body)?;
    Ok(())
}
