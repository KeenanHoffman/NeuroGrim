//! Brain MCP server — exposes scoring tools to AI agents.

use motherbrain_core::agent_output::{build_agent_output, AgentOutput, CorrelationFired};
use motherbrain_core::awareness::LocalAwareness;
use motherbrain_core::correlation::{
    evaluate_condition, evaluate_incident_patterns, extract_domain_variables, DomainVariables,
    IncidentLedgerEntry,
};
use motherbrain_core::registry::{BrainRegistry, ExportedVariable};
use motherbrain_core::scoring::{build_scorecard, CmdbData};
use motherbrain_core::trajectory::compute_trajectory;
use motherbrain_core::types::ScoreSnapshot;

use chrono::{DateTime, Utc};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Brain MCP server state.
#[derive(Clone)]
pub struct BrainServer {
    registry: Arc<BrainRegistry>,
    project_root: PathBuf,
    cmdb_cache: Arc<RwLock<HashMap<String, CmdbData>>>,
    tool_router: ToolRouter<Self>,
}

impl BrainServer {
    pub fn new(registry: BrainRegistry, project_root: PathBuf) -> Self {
        Self {
            registry: Arc::new(registry),
            project_root,
            cmdb_cache: Arc::new(RwLock::new(HashMap::new())),
            tool_router: Self::tool_router(),
        }
    }

    async fn load_cmdb_from_disk(&self) -> HashMap<String, CmdbData> {
        let mut data = HashMap::new();
        for (domain_key, def) in &self.registry.config.domain_definitions {
            if let Some(ref source) = def.scoring_source {
                if source.source_type == "cmdb" {
                    if let Some(ref cmdb_path) = source.path {
                        let full_path = self.project_root.join(cmdb_path);
                        if let Ok(json_str) = tokio::fs::read_to_string(&full_path).await {
                            let json_str = json_str.trim_start_matches('\u{FEFF}');
                            if let Ok(cmdb) = serde_json::from_str::<serde_json::Value>(json_str) {
                                let sf = source.score_field.as_deref().unwrap_or("score");
                                let uf = source.updated_at_field.as_deref().unwrap_or("updated_at");
                                if let (Some(score), Some(ts_str)) = (cmdb.get(sf).and_then(|v| v.as_u64()), cmdb.get(uf).and_then(|v| v.as_str())) {
                                    if let Ok(ts) = ts_str.parse::<DateTime<Utc>>() {
                                        data.insert(domain_key.clone(), CmdbData { score: score.min(100) as u8, updated_at: ts });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        data
    }

    async fn load_score_history(&self) -> Vec<ScoreSnapshot> {
        let path = self.project_root.join(".claude/brain/score-history.json");
        tokio::fs::read_to_string(&path).await.ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    async fn load_incident_ledger(&self) -> Vec<IncidentLedgerEntry> {
        let path = self.project_root.join(".claude/brain/incident-ledger.json");
        tokio::fs::read_to_string(&path).await.ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    async fn run_scoring(&self, hat: Option<String>, persona: Option<String>) -> AgentOutput {
        let now = Utc::now();
        let cmdb_data = self.load_cmdb_from_disk().await;
        let history = self.load_score_history().await;
        let incident_ledger = self.load_incident_ledger().await;
        { let mut cache = self.cmdb_cache.write().await; *cache = cmdb_data.clone(); }

        let scorecard = build_scorecard(&self.registry, &cmdb_data, now);

        // Domain variables
        let mut raw_cmdbs: HashMap<String, serde_json::Value> = HashMap::new();
        for (dk, def) in &self.registry.config.domain_definitions {
            if let Some(ref src) = def.scoring_source {
                if let Some(ref p) = src.path {
                    if let Ok(s) = tokio::fs::read_to_string(self.project_root.join(p)).await {
                        if let Ok(v) = serde_json::from_str(&s) { raw_cmdbs.insert(dk.clone(), v); }
                    }
                }
            }
        }
        let exported: HashMap<String, HashMap<String, ExportedVariable>> = self.registry.config.domain_definitions.iter()
            .filter(|(_, d)| !d.exported_variables.is_empty()).map(|(k, d)| (k.clone(), d.exported_variables.clone())).collect();
        let domain_variables = extract_domain_variables(&raw_cmdbs, &exported);

        let unified_traj = compute_trajectory(&history, &self.registry.config.trajectory, None, &self.registry.config.domain_weights);
        let mut dom_trajs = HashMap::new();
        for dk in self.registry.config.domain_weights.keys() {
            dom_trajs.insert(dk.clone(), compute_trajectory(&history, &self.registry.config.trajectory, Some(dk), &self.registry.config.domain_weights));
        }

        let corrs: Vec<CorrelationFired> = self.registry.config.correlations.iter().filter_map(|c| {
            let name = c.get("name")?.as_str()?;
            let desc = c.get("description").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(ct) = c.get("condition_tree") { if !evaluate_condition(ct, &domain_variables, &history) { return None; } }
            Some(CorrelationFired { id: name.to_string(), description: desc.to_string(), skill: None })
        }).collect();

        let (incidents, skipped) = evaluate_incident_patterns(&self.registry.config.incident_patterns, &domain_variables, &history, &incident_ledger, &self.registry.config.severity_thresholds);

        build_agent_output(&scorecard, &domain_variables, vec![], vec![], vec![], corrs, incidents, skipped, Some(unified_traj), dom_trajs, None, hat, persona)
    }
}

// --- Tool parameter types ---

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LocalAwarenessParams {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SubagentOutcomeParams {
    /// Unique ID of the subagent request (matches request_id in the envelope).
    pub request_id: String,
    /// Capability key from the skill manifest (e.g. "lsp-symbol-scan").
    pub capability: String,
    /// Responsibility type (analysis, investigation, remediation, validation, synthesis, sensory).
    pub responsibility: String,
    /// Hat the subagent was required to wear (null for sensory).
    pub required_hat: Option<String>,
    /// Hat the subagent reported wearing in worn_hat field.
    pub worn_hat: Option<String>,
    /// Final envelope status: "ok", "partial", or "error".
    pub status: String,
    /// Whether the delimited envelope block was found in the response.
    pub envelope_found: bool,
    /// Whether the envelope JSON parsed and all required fields were present.
    pub schema_conformant: bool,
    /// Whether worn_hat matched required_hat.
    pub hat_compliant: bool,
    /// Confidence value from metadata.confidence (0.0–1.0).
    pub confidence: f64,
    /// Number of symbols in the response symbols array.
    pub symbol_count: usize,
    /// Number of retries issued before accepting or aborting (0, 1, or 2).
    pub retry_count: u8,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HealthParams {
    /// Output persona (executive, manager, developer, specialist, product-manager)
    pub persona: Option<String>,
    /// Hat name for domain emphasis
    pub hat: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TrajectoryParams {
    /// Domain name for domain-specific trajectory. Omit for unified.
    pub domain: Option<String>,
}

// --- Tool implementations ---

#[tool_router]
impl BrainServer {
    #[tool(description = "Get the unified health score with domain breakdown, trajectory, and cross-domain analysis. Returns full agent-mode JSON.")]
    async fn get_health_score(&self, Parameters(p): Parameters<HealthParams>) -> String {
        let output = self.run_scoring(p.hat, p.persona).await;
        serde_json::to_string_pretty(&output).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    #[tool(description = "Get trajectory analysis (velocity, acceleration, classification) for the unified score or a specific domain.")]
    async fn get_trajectory(&self, Parameters(p): Parameters<TrajectoryParams>) -> String {
        let history = self.load_score_history().await;
        let traj = compute_trajectory(&history, &self.registry.config.trajectory, p.domain.as_deref(), &self.registry.config.domain_weights);
        serde_json::to_string_pretty(&traj).unwrap_or_default()
    }

    #[tool(description = "Get prioritized remediation actions sorted by priority.")]
    async fn get_recommendations(&self) -> String {
        let output = self.run_scoring(None, None).await;
        serde_json::to_string_pretty(&output.top_recommendations).unwrap_or_default()
    }

    #[tool(description = "Re-invoke sensory tools and return updated scores.")]
    async fn refresh_sensory(&self) -> String {
        let cmdb_data = self.load_cmdb_from_disk().await;
        { let mut cache = self.cmdb_cache.write().await; *cache = cmdb_data; }
        let output = self.run_scoring(None, None).await;
        serde_json::to_string_pretty(&output).unwrap_or_default()
    }

    #[tool(description = "Validate the brain-registry.json configuration.")]
    async fn validate_registry(&self) -> String {
        match self.registry.validate() {
            Ok(()) => serde_json::json!({"valid": true, "domains": self.registry.config.domain_weights.len(), "schema_version": self.registry.meta.schema_version}).to_string(),
            Err(e) => serde_json::json!({"valid": false, "error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Get local machine-specific awareness: tool paths not on PATH, OS quirks, \
        known behavioral patterns. This data is machine-local and gitignored — it persists facts \
        agents discover about the local environment so they are not forgotten across sessions. \
        Use 'motherbrain awareness add' to record new facts.")]
    async fn get_local_awareness(
        &self,
        Parameters(_p): Parameters<LocalAwarenessParams>,
    ) -> String {
        let path = self.project_root.join(".claude/brain/local-awareness.json");
        let awareness = tokio::fs::read_to_string(&path)
            .await
            .ok()
            .and_then(|s| serde_json::from_str::<LocalAwareness>(&s).ok())
            .unwrap_or_else(LocalAwareness::empty);
        serde_json::to_string_pretty(&awareness)
            .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    #[tool(description = "Record a subagent invocation outcome for subagent-health scoring. \
        Call this after processing every subagent response, success or failure. \
        Appends one line to .claude/brain/subagent-outcomes.jsonl and recomputes \
        .claude/brain/subagent-health-cmdb.json from the last 20 outcomes.")]
    async fn record_subagent_outcome(
        &self,
        Parameters(p): Parameters<SubagentOutcomeParams>,
    ) -> String {
        let log_path = self.project_root.join(".claude/brain/subagent-outcomes.jsonl");
        let cmdb_path = self.project_root.join(".claude/brain/subagent-health-cmdb.json");

        // Build outcome line
        let ts = Utc::now().to_rfc3339();
        let outcome = serde_json::json!({
            "ts": ts,
            "request_id": p.request_id,
            "capability": p.capability,
            "responsibility": p.responsibility,
            "required_hat": p.required_hat,
            "worn_hat": p.worn_hat,
            "status": p.status,
            "envelope_found": p.envelope_found,
            "schema_conformant": p.schema_conformant,
            "hat_compliant": p.hat_compliant,
            "confidence": p.confidence,
            "symbol_count": p.symbol_count,
            "retry_count": p.retry_count,
        });

        // Append to event log
        let line = format!("{}\n", outcome);
        {
            use tokio::io::AsyncWriteExt;
            match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .await
            {
                Ok(mut f) => {
                    if let Err(e) = f.write_all(line.as_bytes()).await {
                        return serde_json::json!({"error": format!("failed to write outcome log: {}", e)}).to_string();
                    }
                }
                Err(e) => {
                    return serde_json::json!({"error": format!("failed to open outcome log: {}", e)}).to_string();
                }
            }
        }

        // Read last 20 lines and recompute CMDB
        let window = 20usize;
        let all_text = tokio::fs::read_to_string(&log_path).await.unwrap_or_default();
        let lines: Vec<&str> = all_text.lines().rev().take(window).collect();
        let total_invocations = all_text.lines().count();
        let window_count = lines.len();

        let mut envelope_found_count = 0usize;
        let mut schema_conformant_count = 0usize;
        let mut hat_compliant_count = 0usize;
        let mut confidence_sum = 0.0f64;
        let mut by_capability: HashMap<String, (usize, usize, f64)> = HashMap::new(); // (invocations, conformant, conf_sum)

        for line in &lines {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if v.get("envelope_found").and_then(|x| x.as_bool()).unwrap_or(false) { envelope_found_count += 1; }
                if v.get("schema_conformant").and_then(|x| x.as_bool()).unwrap_or(false) { schema_conformant_count += 1; }
                if v.get("hat_compliant").and_then(|x| x.as_bool()).unwrap_or(false) { hat_compliant_count += 1; }
                let conf = v.get("confidence").and_then(|x| x.as_f64()).unwrap_or(0.0);
                confidence_sum += conf;
                if let Some(cap) = v.get("capability").and_then(|x| x.as_str()) {
                    let entry = by_capability.entry(cap.to_string()).or_insert((0, 0, 0.0));
                    entry.0 += 1;
                    if v.get("schema_conformant").and_then(|x| x.as_bool()).unwrap_or(false) { entry.1 += 1; }
                    entry.2 += conf;
                }
            }
        }

        let wf = window_count as f64;
        let envelope_completeness_rate = if window_count > 0 { envelope_found_count as f64 / wf } else { 0.0 };
        let schema_conformance_rate = if window_count > 0 { schema_conformant_count as f64 / wf } else { 0.0 };
        let hat_compliance_rate = if window_count > 0 { hat_compliant_count as f64 / wf } else { 0.0 };
        let avg_confidence = if window_count > 0 { confidence_sum / wf } else { 0.0 };

        let score = (envelope_completeness_rate * 50.0 + hat_compliance_rate * 30.0 + schema_conformance_rate * 20.0).floor() as u8;
        let confidence_cmdb = (window_count as f64 / window as f64).min(1.0);

        let by_cap_json: serde_json::Value = by_capability.iter().map(|(k, (inv, conf_cnt, cs))| {
            (k.clone(), serde_json::json!({
                "invocations": inv,
                "conformance_rate": if *inv > 0 { *conf_cnt as f64 / *inv as f64 } else { 0.0 },
                "avg_confidence": if *inv > 0 { cs / *inv as f64 } else { 0.0 },
            }))
        }).collect::<serde_json::Map<_, _>>().into();

        let cmdb = serde_json::json!({
            "score": score,
            "updated_at": ts,
            "envelope_completeness_rate": envelope_completeness_rate,
            "schema_conformance_rate": schema_conformance_rate,
            "hat_compliance_rate": hat_compliance_rate,
            "avg_confidence": avg_confidence,
            "confidence": confidence_cmdb,
            "total_invocations": total_invocations,
            "window_invocations": window_count,
            "by_capability": by_cap_json,
        });

        if let Err(e) = tokio::fs::write(&cmdb_path, serde_json::to_string_pretty(&cmdb).unwrap_or_default()).await {
            return serde_json::json!({"error": format!("failed to write subagent-health cmdb: {}", e)}).to_string();
        }

        serde_json::json!({
            "recorded": true,
            "total_invocations": total_invocations,
            "window_invocations": window_count,
            "current_score": score,
        }).to_string()
    }
}

impl ServerHandler for BrainServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("MotherBrain LSP Brains scoring engine. Use get_health_score for the full project health picture.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
