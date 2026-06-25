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
use neurogrim_brokers::{BrokerHost, BrokerHostConfig};
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

/// The broker MCP server state. Wraps an in-process BrokerHost (B-62);
/// the MCP tool is a thin transport over `host.dispatch()`.
#[derive(Clone)]
pub struct BrokerMcpServer {
    host: Arc<BrokerHost>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl BrokerMcpServer {
    #[tool(
        description = "Dispatch a broker pipeline. The agent's discovery surface is `current-projection.md` (auto-loaded via CLAUDE.md), which lists each broker's Surfaced pipelines + their description + when_to_use + currently-legal status + parameter schema. This tool is the WIRE PROTOCOL for the dispatch action; the agent picks which pipeline to dispatch by reading current-projection.md, NOT by enumerating MCP tools. Returns the structured dispatch outcome or a refusal with failure_reason."
    )]
    async fn dispatch_pipeline(&self, Parameters(p): Parameters<DispatchParams>) -> String {
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
            .host
            .dispatch(&p.broker_id, p.pipeline_id.clone(), params_map)
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

#[rmcp::tool_handler]
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

    // A2 wiring: derive project_root from the cluster manifest location.
    // Standard layout is `<project_root>/.claude/brain/broker/cluster.toml`
    // → project_root is 3 parents up. WorkBroker uses this to call
    // next_ready() against the live backlog instead of empty in-memory state.
    let project_root = derive_project_root(cluster_path);
    if let Some(ref root) = project_root {
        eprintln!("✦ Project root detected: {}", root.display());
    } else {
        eprintln!(
            "⚠ Project root not detected from cluster manifest location; \
             WorkBroker will use empty in-memory BacklogState (legacy mode)"
        );
    }

    // A6a (B-62): boot the in-process BrokerHost. All orchestration logic
    // (registry, WorkBroker construction, materializer wiring, on-dispatch
    // callback registration, initial projection) lives in the host module
    // so the IDE can consume the same code path via Tauri IPC.
    let host = BrokerHost::boot(
        cluster_path,
        BrokerHostConfig {
            project_root,
            trust_budget_ceiling: 10_000,
        },
    )
    .await
    .with_context(|| format!("booting BrokerHost from {}", cluster_path.display()))?;

    eprintln!(
        "✦ Registered {} broker(s); initial projection written",
        host.bootstrapped().len()
    );

    // Boot MCP server on stdio
    let server = BrokerMcpServer {
        host: Arc::new(host),
        tool_router: BrokerMcpServer::tool_router(),
    };
    eprintln!("✦ Summoning broker MCP server on stdio…");
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// A2 helper: derive the project root from the cluster manifest's location.
/// Standard layout is `<project_root>/.claude/brain/broker/cluster.toml` —
/// the project root is the third parent of the manifest file. Returns None
/// if the manifest isn't nested deep enough (which means the broker harness
/// is running standalone without a project, and the WorkBroker falls back
/// to in-memory legacy mode).
fn derive_project_root(cluster_manifest_path: &Path) -> Option<PathBuf> {
    let absolute = std::fs::canonicalize(cluster_manifest_path).ok()?;
    absolute
        .parent()? // broker/
        .parent()? // brain/
        .parent()? // .claude/
        .parent()
        .map(|p| p.to_path_buf())
}
