//! B-62 / A6a — `BrokerHost`: in-process broker hosting + dispatch.
//!
//! Extracted from `neurogrim-cli/src/commands/broker_serve.rs` so the IDE
//! (and other consumers) can host brokers IN-PROCESS without going through
//! the MCP-stdio transport. `broker-serve` is now a thin MCP-stdio adapter
//! over this host; the IDE consumes the same host directly via Tauri IPC
//! (A6b lands in Phase B PoC).
//!
//! ## Design (ultra-pass U3)
//!
//! The original `broker-serve` did everything — loaded the cluster
//! manifest, constructed WorkBrokers, wired the materializer, registered
//! the on-dispatch callback, AND served stdio. That coupling meant the IDE
//! couldn't reuse 80% of the orchestration code. This module splits the
//! concerns:
//!
//! - **`BrokerHost::boot(cluster_manifest_path, config)`** — load + wire
//!   the substrate. Returns a ready-to-use host.
//! - **`BrokerHost::dispatch(broker_id, pipeline_id, params)`** — the
//!   single dispatch entrypoint that the MCP `dispatch_pipeline` tool +
//!   the IDE's Tauri IPC + the demo driver all call into.
//! - **`BrokerHost::tick()`** — manual re-materialization trigger (the
//!   on-dispatch callback fires this automatically; ticks let operators
//!   force a refresh outside the dispatch cycle).
//!
//! The MCP wire stays in `broker_serve.rs` (the wire-protocol layer);
//! everything else lives here.

use crate::broker::Broker;
use crate::governance::GovernanceComposer;
use crate::materializer::awareness::AwarenessMaterializer;
use crate::materializer::hot_store::HotStoreMaterializer;
use crate::materializer::MaterializerComposer;
use crate::pipeline::ParamMap;
use crate::registry::BrokerRegistry;
use crate::runner::{DispatchError, DispatchOutcome, PipelineRunner};
use crate::trace::TraceSink;
use crate::work_broker::{BacklogState, WorkBroker};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

/// Configuration for `BrokerHost::boot`.
#[derive(Debug, Clone)]
pub struct BrokerHostConfig {
    /// When `Some`, WorkBrokers are constructed via `new_with_project_root`
    /// (live backlog sensor mode, A2 closure). When `None`, in-memory legacy
    /// mode (empty BacklogState; useful for tests + ephemeral fixtures).
    pub project_root: Option<PathBuf>,
    /// Trust-budget ceiling for the GovernanceComposer (BB #19 MVP scope).
    /// Default 10_000 dispatches.
    pub trust_budget_ceiling: u64,
}

impl Default for BrokerHostConfig {
    fn default() -> Self {
        Self {
            project_root: None,
            trust_budget_ceiling: 10_000,
        }
    }
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error("registry error: {0}")]
    Registry(#[from] crate::registry::RegistryError),
    #[error("materializer error: {0}")]
    Materializer(#[from] crate::materializer::MaterializerError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("other: {0}")]
    Other(#[from] anyhow::Error),
}

/// Shared runtime context for materialization re-triggering. Cloned into the
/// on_dispatch_complete callback closure so the runner can re-materialize
/// asynchronously after each dispatch.
#[derive(Clone)]
struct MaterializerContext {
    bootstrapped: Vec<(String, Arc<dyn Broker>)>,
    segments_dir: PathBuf,
    output_path: PathBuf,
    composition_order: Vec<String>,
    context_budget: usize,
}

impl MaterializerContext {
    async fn materialize_all(&self) -> Result<(), HostError> {
        for (broker_id, broker) in &self.bootstrapped {
            let hot = HotStoreMaterializer::new(broker_id.clone(), self.segments_dir.clone());
            hot.materialize(broker.clone()).await
                .map_err(|e| HostError::Other(anyhow::anyhow!("hot-store materializer: {}", e)))?;
            let aware = AwarenessMaterializer::new(broker_id.clone(), self.segments_dir.clone());
            aware.materialize(broker.clone()).await
                .map_err(|e| HostError::Other(anyhow::anyhow!("awareness materializer: {}", e)))?;
        }
        synthesize_governance_segment(&self.segments_dir, &self.bootstrapped)
            .map_err(|e| HostError::Other(anyhow::anyhow!("governance segment: {}", e)))?;
        let composer = MaterializerComposer::new(
            self.output_path.clone(),
            self.segments_dir.clone(),
            self.context_budget,
        );
        composer.compose(&self.composition_order)
            .map_err(|e| HostError::Other(anyhow::anyhow!("compose: {}", e)))?;
        Ok(())
    }
}

/// In-process broker host. Owns the registry, runner, governance, and
/// bootstrapped broker instances. Consumers (broker-serve MCP wire, IDE
/// Tauri IPC, demo drivers) construct one of these and call `dispatch` on
/// it; the host handles materialization re-triggering internally.
pub struct BrokerHost {
    pub registry: Arc<BrokerRegistry>,
    pub runner: Arc<PipelineRunner>,
    pub governance: Arc<GovernanceComposer>,
    bootstrapped: Vec<(String, Arc<dyn Broker>)>,
    ctx: MaterializerContext,
}

impl BrokerHost {
    /// Load a cluster manifest, construct concrete brokers, wire the
    /// materializer + on-dispatch callback, perform initial materialization,
    /// and return a ready-to-use host.
    pub async fn boot(
        cluster_manifest_path: &Path,
        config: BrokerHostConfig,
    ) -> Result<Self, HostError> {
        let mut registry = BrokerRegistry::load_manifests(cluster_manifest_path)?;

        let governance = Arc::new(GovernanceComposer::new(config.trust_budget_ceiling));

        // Construct concrete brokers. MVP: every broker declared in the
        // cluster manifest is constructed as a WorkBroker. Future versions
        // dispatch by broker_type field in per-broker manifests.
        let mut bootstrapped: Vec<(String, Arc<dyn Broker>)> = Vec::new();
        let broker_ids: Vec<String> = registry.cluster().brokers.keys().cloned().collect();
        for broker_id in broker_ids {
            let work_broker = Arc::new(match config.project_root.clone() {
                Some(root) => WorkBroker::new_with_project_root(
                    broker_id.clone(),
                    root,
                    governance.clone(),
                ),
                None => WorkBroker::new(
                    broker_id.clone(),
                    BacklogState::default(),
                    governance.clone(),
                ),
            });
            let catalog = work_broker.catalog();
            let dyn_broker: Arc<dyn Broker> = work_broker;
            registry.register_with_catalog(dyn_broker.clone(), catalog)?;
            bootstrapped.push((broker_id, dyn_broker));
        }
        registry.validate()?;

        // Wire materializer paths
        let cluster_dir = registry.cluster_manifest_dir().to_path_buf();
        let segments_dir =
            resolve_path(&cluster_dir, &registry.cluster().materializer.segments_dir);
        let output_path =
            resolve_path(&cluster_dir, &registry.cluster().materializer.output_path);
        let trace_path = segments_dir
            .parent()
            .unwrap_or(Path::new("."))
            .join("trace.jsonl");

        std::fs::create_dir_all(&segments_dir)?;
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let trace_sink = Arc::new(TraceSink::new(trace_path));
        let runner = Arc::new(PipelineRunner::new(trace_sink, governance.clone()));
        // A12 / A13: load operator-declared Frame defaults from cluster manifest.
        let frame = registry.cluster().frame.to_frame();
        runner.set_frame(frame);

        let ctx = MaterializerContext {
            bootstrapped: bootstrapped.clone(),
            segments_dir,
            output_path,
            composition_order: registry.cluster().materializer.composition_order.clone(),
            context_budget: registry.cluster().materializer.context_budget_chars,
        };

        // Initial materialization (synchronous; host isn't usable until the
        // first projection exists).
        ctx.materialize_all().await?;

        // Wire on_dispatch_complete: fire-and-forget async re-materialization
        // after every successful dispatch. Next agent turn may briefly see
        // stale projection state (~50ms typical); acceptable per V0-RETRO §D3.
        let ctx_for_callback = ctx.clone();
        runner.on_dispatch_complete(Arc::new(move |_broker_id, _outcome| {
            let ctx = ctx_for_callback.clone();
            tokio::spawn(async move {
                if let Err(e) = ctx.materialize_all().await {
                    eprintln!("⚠ re-materialization failed: {}", e);
                }
            });
        }));

        Ok(Self {
            registry: Arc::new(registry),
            runner,
            governance,
            bootstrapped,
            ctx,
        })
    }

    /// The single dispatch entrypoint. All consumers (MCP wire, Tauri IPC,
    /// demo drivers) call this. Validates broker existence + aggregates the
    /// per-broker catalogs + delegates to PipelineRunner.
    pub async fn dispatch(
        &self,
        broker_id: &str,
        pipeline_id: String,
        params: ParamMap,
    ) -> Result<DispatchOutcome, DispatchError> {
        let broker = self.registry.broker(broker_id).ok_or_else(|| {
            DispatchError::PipelineNotFound {
                broker_id: broker_id.to_string(),
                pipeline_id: pipeline_id.clone(),
            }
        })?;
        let catalog = self.registry.full_catalog();
        self.runner
            .dispatch(broker, &catalog, pipeline_id, params)
            .await
    }

    /// Synchronous re-materialization trigger. Useful for operators who
    /// need a fresh projection outside the dispatch cycle (e.g., file edits
    /// changed the backlog; the next agent turn should see updated state
    /// without requiring an intervening dispatch).
    pub async fn tick(&self) -> Result<(), HostError> {
        self.ctx.materialize_all().await
    }

    /// List the bootstrapped brokers (id + Arc). Used by consumers needing
    /// to iterate brokers (e.g., to expose them per-broker over a transport).
    pub fn bootstrapped(&self) -> &[(String, Arc<dyn Broker>)] {
        &self.bootstrapped
    }
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
) -> std::io::Result<()> {
    let mut body = String::from("## Governance pipelines (always-reachable)\n\n");
    body.push_str(
        "The following governance pipelines are framework-provided + composed \
         automatically into every Surfaced pipeline dispatch (per BB #19 + R-O-3 \
         reachability invariant):\n\n",
    );
    body.push_str("- `<broker_id>/check-trust-budget` — refuses if budget exhausted\n");
    body.push_str("- `<broker_id>/check-kill-switch` — refuses if armed (A1/B-64: arm/disengage bypass this)\n");
    body.push_str("- `<broker_id>/record-dispatch` — writes audit anchor at start\n");
    body.push_str("- `<broker_id>/record-outcome` — writes audit at end (A1/C8: fires on success AND refusal branches)\n");
    body.push_str("- `<broker_id>/arm-kill-switch` — **Surfaced**, OperatorOnly: emergency halt\n");
    body.push_str("- `<broker_id>/disengage-kill-switch` — **Surfaced**, OperatorOnly: resume (A1 escape hatch)\n\n");
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_minimal_cluster(dir: &Path) -> PathBuf {
        let cluster_dir = dir.join(".claude").join("brain").join("broker");
        std::fs::create_dir_all(&cluster_dir).unwrap();
        let cluster_path = cluster_dir.join("cluster.toml");
        std::fs::write(
            &cluster_path,
            r#"[cluster]
id = "host-test-cluster"
name = "Host test cluster"
brokers_dir = "./"

[cluster.brokers.work-broker]
manifest_path = "work-broker.toml"

[cluster.materializer]
composition_order = ["overlay-work-broker", "awareness-routing-work-broker"]
output_path = "current-projection.md"
segments_dir = "segments"
context_budget_chars = 16384
"#,
        )
        .unwrap();
        std::fs::write(
            cluster_dir.join("work-broker.toml"),
            r#"[broker]
id = "work-broker"
name = "Work Broker"
roles = ["innate-ability"]
cold_store_path = "work-broker-cold/"
catalog_path = "work-broker-catalog.yaml"
"#,
        )
        .unwrap();
        cluster_path
    }

    #[tokio::test]
    async fn host_boot_and_dispatch_round_trip() {
        let tmp = TempDir::new().unwrap();
        let cluster_path = write_minimal_cluster(tmp.path());
        let host = BrokerHost::boot(
            &cluster_path,
            BrokerHostConfig {
                project_root: None,
                trust_budget_ceiling: 100,
            },
        )
        .await
        .unwrap();

        // Bootstrapped 1 broker
        assert_eq!(host.bootstrapped().len(), 1);
        assert_eq!(host.bootstrapped()[0].0, "work-broker");

        // Dispatch arm-kill-switch (Surfaced governance pipeline; A1 fix:
        // now actually flips the armed flag via its real leaf-op).
        let outcome = host
            .dispatch(
                "work-broker",
                "work-broker/arm-kill-switch".to_string(),
                ParamMap::new(),
            )
            .await
            .unwrap();
        assert!(!outcome.trace_id.is_empty());
        assert!(host.governance.is_kill_switch_armed());

        // Disengage via the bypass-enabled pipeline (A1.5/B-64: this MUST
        // succeed even though kill-switch is armed; otherwise the operator
        // is stuck).
        let outcome = host
            .dispatch(
                "work-broker",
                "work-broker/disengage-kill-switch".to_string(),
                ParamMap::new(),
            )
            .await
            .unwrap();
        assert!(!outcome.trace_id.is_empty());
        assert!(!host.governance.is_kill_switch_armed());
    }

    #[tokio::test]
    async fn host_initial_materialization_produces_projection() {
        let tmp = TempDir::new().unwrap();
        let cluster_path = write_minimal_cluster(tmp.path());
        let _host = BrokerHost::boot(&cluster_path, BrokerHostConfig::default())
            .await
            .unwrap();

        // current-projection.md should exist after boot
        let projection = tmp
            .path()
            .join(".claude/brain/broker/current-projection.md");
        assert!(projection.exists(), "projection must be written by boot");
        let content = std::fs::read_to_string(projection).unwrap();
        assert!(content.contains("Active brokers in this cluster: `work-broker`"));
    }
}
