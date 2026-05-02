//! Brain MCP server — exposes scoring tools to AI agents.

use neurogrim_core::agent_output::{build_agent_output, AgentOutput, CorrelationFired};
use neurogrim_core::awareness::LocalAwareness;
use neurogrim_core::correlation::{
    evaluate_condition, evaluate_incident_patterns, extract_domain_variables, DomainVariables,
    IncidentLedgerEntry,
};
use neurogrim_core::registry::{BrainRegistry, ExportedVariable};
use neurogrim_core::calibration_ledger::auto_trigger_calibration_writes;
use neurogrim_core::scoring::{build_scorecard, CmdbData};
use neurogrim_core::trajectory::compute_trajectory;
use neurogrim_core::types::ScoreSnapshot;

use chrono::{DateTime, Utc};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
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
    /// v4.2 S14-S-5 — outstanding proxy tokens minted by
    /// `secret_fetch`. Per-process; tokens don't survive restarts.
    /// `Clone` is a view into the same Arc-backed store.
    proxy_tokens: crate::proxy_tokens::ProxyTokenStore,
}

impl BrainServer {
    pub fn new(registry: BrainRegistry, project_root: PathBuf) -> Self {
        Self {
            registry: Arc::new(registry),
            project_root,
            cmdb_cache: Arc::new(RwLock::new(HashMap::new())),
            tool_router: Self::tool_router(),
            proxy_tokens: crate::proxy_tokens::ProxyTokenStore::new(),
        }
    }

    /// Read-only view of the proxy-token store. Used by the
    /// secret-readiness sensor to surface "agents have outstanding
    /// proxy tokens" findings.
    pub fn proxy_tokens(&self) -> &crate::proxy_tokens::ProxyTokenStore {
        &self.proxy_tokens
    }

    /// Accessor for `crate::autonomy` — read-only view of the
    /// loaded registry, used to derive the autonomy config at
    /// dispatch time.
    pub fn registry(&self) -> &BrainRegistry {
        &self.registry
    }

    /// Accessor for `crate::autonomy` — used to resolve the
    /// approvals queue path for the brain this server hosts.
    pub fn project_root(&self) -> &std::path::Path {
        &self.project_root
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
                                if let (Some(score), Some(ts_str)) = (
                                    cmdb.get(sf).and_then(|v| v.as_u64()),
                                    cmdb.get(uf).and_then(|v| v.as_str()),
                                ) {
                                    if let Ok(ts) = ts_str.parse::<DateTime<Utc>>() {
                                        // Optional envelope-supplied confidence
                                        // (E-B2-1, spec §3.8). When present,
                                        // takes precedence over age-decay.
                                        let confidence = cmdb
                                            .get("confidence")
                                            .and_then(|v| v.as_u64())
                                            .map(|n| n.min(100) as u8);
                                        data.insert(
                                            domain_key.clone(),
                                            CmdbData {
                                                score: score.min(100) as u8,
                                                updated_at: ts,
                                                confidence,
                                            },
                                        );
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
        tokio::fs::read_to_string(&path)
            .await
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    async fn load_incident_ledger(&self) -> Vec<IncidentLedgerEntry> {
        let path = self.project_root.join(".claude/brain/incident-ledger.json");
        tokio::fs::read_to_string(&path)
            .await
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    async fn run_scoring(&self, hat: Option<String>, human_persona: Option<String>) -> AgentOutput {
        // V5-FOUND-1 Phase 3 step 1: scoring pipeline instrumentation.
        // Span name `score.pipeline.run` is in
        // diagnostics_layer::kind_for_span_name's closed table; mapped
        // to EventKind::Scoring. Extras are filled in via span.record()
        // after the AgentOutput is built so domains_count/score/
        // confidence reflect the actual outcome. The Layer is a no-op
        // when NEUROGRIM_DIAG is unset, so this adds zero overhead in
        // production.
        let span = tracing::info_span!(
            "score.pipeline.run",
            domains_count = tracing::field::Empty,
            score = tracing::field::Empty,
            confidence = tracing::field::Empty,
        );
        let _entered = span.enter();

        let now = Utc::now();
        let cmdb_data = self.load_cmdb_from_disk().await;
        let history = self.load_score_history().await;
        let incident_ledger = self.load_incident_ledger().await;
        {
            let mut cache = self.cmdb_cache.write().await;
            *cache = cmdb_data.clone();
        }

        let scorecard = build_scorecard(&self.registry, &cmdb_data, now);

        // E-B2-2 C7 — auto-trigger plumbing for per-domain calibration
        // ledger (§17.3). Default-off; per-domain opt-in;
        // domain-calibration recursion guard hard-coded (§17.9). Errors
        // logged + skipped to preserve scoring liveness.
        let _ = auto_trigger_calibration_writes(
            &self.registry,
            &scorecard,
            &HashMap::new(),
            &self.project_root,
        );

        // Domain variables
        let mut raw_cmdbs: HashMap<String, serde_json::Value> = HashMap::new();
        for (dk, def) in &self.registry.config.domain_definitions {
            if let Some(ref src) = def.scoring_source {
                if let Some(ref p) = src.path {
                    if let Ok(s) = tokio::fs::read_to_string(self.project_root.join(p)).await {
                        if let Ok(v) = serde_json::from_str(&s) {
                            raw_cmdbs.insert(dk.clone(), v);
                        }
                    }
                }
            }
        }
        let exported: HashMap<String, HashMap<String, ExportedVariable>> = self
            .registry
            .config
            .domain_definitions
            .iter()
            .filter(|(_, d)| !d.exported_variables.is_empty())
            .map(|(k, d)| (k.clone(), d.exported_variables.clone()))
            .collect();
        let domain_variables = extract_domain_variables(&raw_cmdbs, &exported);

        let unified_traj = compute_trajectory(
            &history,
            &self.registry.config.trajectory,
            None,
            &self.registry.config.domain_weights,
        );
        let mut dom_trajs = HashMap::new();
        for dk in self.registry.config.domain_weights.keys() {
            dom_trajs.insert(
                dk.clone(),
                compute_trajectory(
                    &history,
                    &self.registry.config.trajectory,
                    Some(dk),
                    &self.registry.config.domain_weights,
                ),
            );
        }

        let corrs: Vec<CorrelationFired> = self
            .registry
            .config
            .correlations
            .iter()
            .filter_map(|c| {
                let name = c.get("name")?.as_str()?;
                let desc = c.get("description").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(ct) = c.get("condition_tree") {
                    if !evaluate_condition(ct, &domain_variables, &history) {
                        return None;
                    }
                }
                Some(CorrelationFired {
                    id: name.to_string(),
                    description: desc.to_string(),
                    skill: None,
                })
            })
            .collect();

        let (incidents, skipped) = evaluate_incident_patterns(
            &self.registry.config.incident_patterns,
            &domain_variables,
            &history,
            &incident_ledger,
            &self.registry.config.severity_thresholds,
        );

        let agent_output = build_agent_output(
            &scorecard,
            &domain_variables,
            vec![],
            vec![],
            vec![],
            corrs,
            incidents,
            skipped,
            Some(unified_traj),
            dom_trajs,
            None,
            hat,
            human_persona,
        );

        // V5-FOUND-1 Phase 3 step 1: record scoring extras for the
        // diagnostics-layer entry. domains_count is the ACTIVE
        // (non-advisory + advisory; the registry breadth) — simple
        // size of the agent_output.domains map. score and confidence
        // are the unified values the AgentOutput contract exposes.
        // All three are integers, matching the ledger schema's
        // `score` (number) + `confidence` (number) and
        // `domains_count` (integer) extras.
        span.record("domains_count", agent_output.domains.len() as i64);
        span.record("score", agent_output.score as i64);
        span.record("confidence", agent_output.unified_confidence as i64);

        agent_output
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
    /// Output human-persona (executive, manager, developer, specialist, product-manager)
    pub human_persona: Option<String>,
    /// Hat name for domain emphasis
    pub hat: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TrajectoryParams {
    /// Domain name for domain-specific trajectory. Omit for unified.
    pub domain: Option<String>,
}

// --- v3.2.1 onboarding-tool parameter types ---

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OrientParams {
    /// Optional hat to bias the rendered prose. Same hats supported by
    /// the CLI's `agent --hat <name>`.
    pub hat: Option<String>,
    /// v3.3 F4: list every declared domain instead of capping at the
    /// top 3. Default behavior auto-expands when the Brain is
    /// all-advisory (no weighted domains).
    pub all_domains: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExplainParams {
    /// Topic name. Omit to receive the list of available topics.
    /// Available: methodology, domain, sensor, hat, scoring,
    /// federation, cli, culture.
    pub topic: Option<String>,
}

// v4.1 S13-B-4: coordination bus MCP-tool params.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueuePublishParams {
    /// Topic name. `_neurogrim/<name>` is reserved for system topics;
    /// adopters use `<scope>/<name>` (lowercase kebab).
    pub topic: String,
    /// Free-form payload; the bus is payload-agnostic.
    pub payload: serde_json::Value,
    /// "low" | "normal" | "high". Defaults to normal.
    pub priority: Option<String>,
    /// Time-to-live in milliseconds. Default: never expires.
    pub expires_in_ms: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueueConsumeParams {
    pub topic: String,
    /// 0-indexed line offset to resume from. Consumers persist this
    /// themselves; the bus is stateless w.r.t. who's read what.
    /// Default: 0 (read from start of topic).
    pub since_offset: Option<u64>,
    /// Cap on returned messages. Default 100; max 1000.
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueuePeekParams {
    pub topic: String,
    /// How many messages to return from the tail. Default 10; max 100.
    pub count: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SecretFetchParams {
    /// Identifier of the secret. Looked up at
    /// `neurogrim-{brain_id}-{secret_id}` in the OS credential
    /// store (or the encrypted-file fallback).
    pub secret_id: String,
    /// Optional operator-supplied scope for audit trails. e.g.,
    /// "anthropic-api-once" → the operator-approved purpose for
    /// this token. Surfaces in audit logs but doesn't constrain
    /// the upstream call (claude-proxy enforces the actual scope).
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AwaitApprovalParams {
    /// The action_id returned by an earlier mutation tool that hit
    /// the Approve autonomy level. Operators resolve approvals on
    /// `_neurogrim/approval-resolutions`; this tool reads that
    /// ledger and returns the operator's decision (or `pending`
    /// when none exists yet).
    pub action_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DomainNewParams {
    /// Domain name (kebab-case). Must match `^[a-z][a-z0-9-]*$`.
    pub name: String,
    /// Humanized display name; defaults to title-case of `name`.
    pub description: Option<String>,
    /// Initial weight (0.0–1.0). Default 0.0 (advisory).
    pub weight: Option<f64>,
    /// Sensor type: "stub" (registry + CMDB only) or "python" (also
    /// scaffolds sensory/check_<name>.py). Default "stub".
    pub sensor_type: Option<String>,
    /// Overwrite an existing domain. Default false.
    pub force: Option<bool>,
    /// v3.3 F10: Operator-supplied sensor authoring intent. Stored
    /// as `_todo_<name>` on the domain's definition entry.
    pub sensor_intent: Option<String>,
}

// --- Tool implementations ---

#[tool_router]
impl BrainServer {
    #[tool(
        description = "Get the unified health score with domain breakdown, trajectory, and cross-domain analysis. Returns full agent-mode JSON."
    )]
    async fn get_health_score(&self, Parameters(p): Parameters<HealthParams>) -> String {
        let output = self.run_scoring(p.hat, p.human_persona).await;
        serde_json::to_string_pretty(&output)
            .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    #[tool(
        description = "Get trajectory analysis (velocity, acceleration, classification) for the unified score or a specific domain."
    )]
    async fn get_trajectory(&self, Parameters(p): Parameters<TrajectoryParams>) -> String {
        let history = self.load_score_history().await;
        let traj = compute_trajectory(
            &history,
            &self.registry.config.trajectory,
            p.domain.as_deref(),
            &self.registry.config.domain_weights,
        );
        serde_json::to_string_pretty(&traj).unwrap_or_default()
    }

    #[tool(description = "Get prioritized remediation actions sorted by priority.")]
    async fn get_recommendations(&self) -> String {
        let output = self.run_scoring(None, None).await;
        serde_json::to_string_pretty(&output.top_recommendations).unwrap_or_default()
    }

    #[tool(description = "Re-invoke sensory tools and return updated scores.")]
    async fn refresh_sensory(&self) -> String {
        // S13-B-5: autonomy gate. Mutation tool — default Approve.
        if let Some(early) = crate::autonomy::maybe_block(self, "refresh_sensory").await {
            return early;
        }
        let cmdb_data = self.load_cmdb_from_disk().await;
        {
            let mut cache = self.cmdb_cache.write().await;
            *cache = cmdb_data;
        }
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

    #[tool(
        description = "v3.2.1 — Agent-friendly prose orientation summary for an AI agent \
        entering this NeuroGrim project. Returns the same content as `neurogrim agent --prose`: \
        Brain identity, current score + trajectory, top signals, calls to action, available \
        skills/hats, federation peers. ANSI colors suppressed (output is plain text suitable \
        for embedding in agent context). Use this as the first introspection call when an \
        agent needs to understand 'what is this Brain'."
    )]
    async fn orient(&self, Parameters(p): Parameters<OrientParams>) -> String {
        let agent_output = self.run_scoring(p.hat, None).await;
        crate::prose::render_prose(
            &self.registry,
            &self.project_root,
            &agent_output,
            true,
            p.all_domains.unwrap_or(false),
        )
    }

    #[tool(
        description = "v3.2.1 — Read-only configuration auditor (mirror of `neurogrim doctor`). \
        Runs six check families against the loaded registry + on-disk artifacts: schema-validate, \
        domain-definitions alignment, principle-map alignment, CMDB path resolution, \
        culture.yaml presence, federation port uniqueness. Returns a JSON envelope with \
        per-finding severity (error/warn/info), category, and message, plus an aggregate \
        summary. Use this when configuring a Brain or before relying on its score output."
    )]
    async fn doctor(&self) -> String {
        let findings = crate::doctor::audit(&self.registry, &self.project_root);
        let errors = findings
            .iter()
            .filter(|f| matches!(f.severity, crate::doctor::Severity::Error))
            .count();
        let warns = findings
            .iter()
            .filter(|f| matches!(f.severity, crate::doctor::Severity::Warn))
            .count();
        serde_json::json!({
            "errors": errors,
            "warnings": warns,
            "exit_code": if errors > 0 { 2 } else if warns > 0 { 1 } else { 0 },
            "findings": findings,
        })
        .to_string()
    }

    #[tool(
        description = "v3.2.1 — Bundled methodology primer (mirror of `neurogrim explain`). \
        Eight self-contained topic files ship inside the binary: methodology, domain, sensor, \
        hat, scoring, federation, cli, culture. Pass `topic` to receive that topic's markdown \
        body. Omit `topic` to receive the list of available topics with one-line summaries. \
        Use this when an agent needs to learn the LSP Brains methodology without reading the \
        4000-line spec."
    )]
    async fn explain(&self, Parameters(p): Parameters<ExplainParams>) -> String {
        match p.topic.as_deref() {
            None => {
                let mut out = format!(
                    "neurogrim explain — bundled methodology primer ({})\n\n",
                    crate::explain::BUNDLED_VERSION
                );
                out.push_str("Available topics:\n");
                for (name, summary, _) in crate::explain::topics() {
                    out.push_str(&format!("  {:<13} {}\n", name, summary));
                }
                out.push_str(
                    "\nCall `explain` again with `topic=<name>` to read any topic.\n",
                );
                out
            }
            Some(name) => match crate::explain::lookup(name) {
                Some(body) => body.to_string(),
                None => {
                    let names = crate::explain::topic_names().join(", ");
                    format!(
                        "{{\"error\": \"unknown topic '{name}'. Available: {names}\"}}"
                    )
                }
            },
        }
    }

    #[tool(
        description = "v3.2.1 — Scaffold a new domain in this Brain's registry (mirror of \
        `neurogrim domain new`). Mutates brain-registry.json (adds entries to domain_weights, \
        principle_map, domain_definitions atomically), creates a stub CMDB, and optionally \
        scaffolds a Python sensor skeleton at sensory/check_<name>.py. \
        Required: `name` (kebab-case). Optional: `description` (humanized), `weight` (default \
        0.0 = advisory), `sensor_type` ('stub' or 'python', default 'stub'), `force` (default \
        false; required to overwrite an existing domain). Returns scaffolding outcome as JSON. \
        Use this when an agent needs to declare a new measurement target."
    )]
    async fn domain_new(&self, Parameters(p): Parameters<DomainNewParams>) -> String {
        // S13-B-5: autonomy gate. Mutation tool — default Approve.
        if let Some(early) = crate::autonomy::maybe_block(self, "domain_new").await {
            return early;
        }
        let sensor_type = match p.sensor_type.as_deref() {
            Some("python") => crate::domain::SensorType::Python,
            Some("stub") | None => crate::domain::SensorType::Stub,
            Some(other) => {
                return serde_json::json!({
                    "error": format!("invalid sensor_type '{other}'. Allowed: stub, python")
                })
                .to_string();
            }
        };
        let weight = p.weight.unwrap_or(0.0);
        let force = p.force.unwrap_or(false);
        let directory = self.project_root.to_string_lossy().to_string();

        let result = crate::domain::scaffold_domain(
            &p.name,
            p.description.as_deref(),
            weight,
            sensor_type,
            ".claude/brain-registry.json",
            &directory,
            force,
            p.sensor_intent.as_deref(),
        )
        .await;

        match result {
            Ok(outcome) => serde_json::json!({
                "ok": true,
                "name": outcome.name,
                "display_name": outcome.display_name,
                "weight": outcome.weight,
                "was_existing": outcome.was_existing,
                "registry_path": outcome.registry_path.display().to_string(),
                "cmdb_path": outcome.cmdb_path.display().to_string(),
                "sensor_path": outcome.sensor_path.as_ref().map(|p| p.display().to_string()),
                "next_steps": next_steps_text(&outcome),
            })
            .to_string(),
            Err(e) => serde_json::json!({
                "ok": false,
                "error": format!("{e:#}"),
            })
            .to_string(),
        }
    }

    #[tool(
        description = "Get local machine-specific awareness: tool paths not on PATH, OS quirks, \
        known behavioral patterns. This data is machine-local and gitignored — it persists facts \
        agents discover about the local environment so they are not forgotten across sessions. \
        Use 'neurogrim awareness add' to record new facts."
    )]
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

    #[tool(
        description = "Record a subagent invocation outcome for subagent-health scoring. \
        Call this after processing every subagent response, success or failure. \
        Appends one line to .claude/brain/subagent-outcomes.jsonl and recomputes \
        .claude/brain/subagent-health-cmdb.json from the last 20 outcomes."
    )]
    async fn record_subagent_outcome(
        &self,
        Parameters(p): Parameters<SubagentOutcomeParams>,
    ) -> String {
        // S13-B-5: autonomy gate. Mutation tool — default Approve.
        if let Some(early) =
            crate::autonomy::maybe_block(self, "record_subagent_outcome").await
        {
            return early;
        }
        let log_path = self
            .project_root
            .join(".claude/brain/subagent-outcomes.jsonl");
        let cmdb_path = self
            .project_root
            .join(".claude/brain/subagent-health-cmdb.json");

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
        let all_text = tokio::fs::read_to_string(&log_path)
            .await
            .unwrap_or_default();
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
                if v.get("envelope_found")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false)
                {
                    envelope_found_count += 1;
                }
                if v.get("schema_conformant")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false)
                {
                    schema_conformant_count += 1;
                }
                if v.get("hat_compliant")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false)
                {
                    hat_compliant_count += 1;
                }
                let conf = v.get("confidence").and_then(|x| x.as_f64()).unwrap_or(0.0);
                confidence_sum += conf;
                if let Some(cap) = v.get("capability").and_then(|x| x.as_str()) {
                    let entry = by_capability.entry(cap.to_string()).or_insert((0, 0, 0.0));
                    entry.0 += 1;
                    if v.get("schema_conformant")
                        .and_then(|x| x.as_bool())
                        .unwrap_or(false)
                    {
                        entry.1 += 1;
                    }
                    entry.2 += conf;
                }
            }
        }

        let wf = window_count as f64;
        let envelope_completeness_rate = if window_count > 0 {
            envelope_found_count as f64 / wf
        } else {
            0.0
        };
        let schema_conformance_rate = if window_count > 0 {
            schema_conformant_count as f64 / wf
        } else {
            0.0
        };
        let hat_compliance_rate = if window_count > 0 {
            hat_compliant_count as f64 / wf
        } else {
            0.0
        };
        let avg_confidence = if window_count > 0 {
            confidence_sum / wf
        } else {
            0.0
        };

        let score = (envelope_completeness_rate * 50.0
            + hat_compliance_rate * 30.0
            + schema_conformance_rate * 20.0)
            .floor() as u8;
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

        if let Err(e) = tokio::fs::write(
            &cmdb_path,
            serde_json::to_string_pretty(&cmdb).unwrap_or_default(),
        )
        .await
        {
            return serde_json::json!({"error": format!("failed to write subagent-health cmdb: {}", e)}).to_string();
        }

        serde_json::json!({
            "recorded": true,
            "total_invocations": total_invocations,
            "window_invocations": window_count,
            "current_score": score,
        })
        .to_string()
    }

    // ── v4.1 S13-B-4: coordination bus tools ────────────────────────

    #[tool(
        description = "v4.1 — Publish a message to the agent coordination bus. \
        Topic must follow the v4.1 namespace convention: `_neurogrim/<name>` for \
        system topics (reserved; adopters MUST NOT publish here) or \
        `<scope>/<name>` for adopter topics (lowercase kebab). The bus persists \
        the message at <project>/.claude/brain/queues/<topic>.jsonl and fans it \
        out to live SSE subscribers. Returns {message_id, topic, produced_at}. \
        Default autonomy: notify (cheap, low-blast)."
    )]
    async fn queue_publish(
        &self,
        Parameters(p): Parameters<QueuePublishParams>,
    ) -> String {
        // S13-B-5: autonomy gate. Mutation tool — default Approve.
        // Note: agents calling `queue_publish` to RESOLVE an
        // approval (publishing on `_neurogrim/approval-resolutions`)
        // would also hit this gate, which would block the resolution
        // path. Operators resolve via the dashboard UI (B-6) or the
        // CLI `neurogrim queue publish` (no autonomy gate on the
        // CLI by design — the operator IS the authority).
        if let Some(early) = crate::autonomy::maybe_block(self, "queue_publish").await {
            return early;
        }
        if !neurogrim_core::queue::Topic::is_valid(&p.topic) {
            return serde_json::json!({
                "error": "invalid topic name",
                "topic": p.topic,
                "hint": "use `_neurogrim/<name>` for reserved or `<scope>/<name>` lowercase kebab",
            })
            .to_string();
        }
        let mut msg = neurogrim_core::queue::QueueMessage::new(p.topic.clone(), p.payload);
        if let Some(prio) = p.priority {
            match prio.as_str() {
                "low" => msg.priority = neurogrim_core::queue::Priority::Low,
                "normal" => msg.priority = neurogrim_core::queue::Priority::Normal,
                "high" => msg.priority = neurogrim_core::queue::Priority::High,
                other => {
                    return serde_json::json!({
                        "error": "invalid priority",
                        "expected": ["low", "normal", "high"],
                        "got": other,
                    })
                    .to_string();
                }
            }
        }
        if let Some(ttl_ms) = p.expires_in_ms {
            let when = msg.produced_at + chrono::Duration::milliseconds(ttl_ms as i64);
            msg = msg.with_expires_at(when);
        }
        let path = self
            .project_root
            .join(".claude")
            .join("brain")
            .join("queues");
        // Mirror BusState::publish on-disk layout: subdirs for slash
        // segments, leaf gets `.jsonl` extension.
        let mut full = path;
        for seg in p.topic.split('/') {
            if !seg.is_empty() {
                full.push(seg);
            }
        }
        full.set_extension("jsonl");
        match neurogrim_core::queue::append(&full, &msg) {
            Ok(()) => serde_json::json!({
                "message_id": msg.id.to_string(),
                "topic": msg.topic,
                "produced_at": msg.produced_at.to_rfc3339(),
            })
            .to_string(),
            Err(e) => serde_json::json!({"error": format!("publish failed: {e}")}).to_string(),
        }
    }

    #[tool(
        description = "v4.1 — Read messages from a topic since the given offset. \
        Offset-based; the tool does NOT mark messages as consumed (the consumer \
        persists its own offset). Returns {topic, messages, next_offset} where \
        next_offset is the cursor for the next call. Maximum 1000 messages per \
        call. Default autonomy: notify."
    )]
    async fn queue_consume(
        &self,
        Parameters(p): Parameters<QueueConsumeParams>,
    ) -> String {
        if !neurogrim_core::queue::Topic::is_valid(&p.topic) {
            return serde_json::json!({"error": "invalid topic name", "topic": p.topic})
                .to_string();
        }
        let path = topic_disk_path(&self.project_root, &p.topic);
        let reader = match neurogrim_core::queue::JsonlQueueReader::open(&path) {
            Ok(r) => r,
            Err(e) => {
                return serde_json::json!({"error": format!("read failed: {e}")}).to_string();
            }
        };
        let since = p.since_offset.unwrap_or(0);
        let limit = p.limit.unwrap_or(100).min(1000) as usize;
        let messages: Vec<serde_json::Value> = reader
            .iter_from(since as usize)
            .take(limit)
            .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
            .collect();
        let returned = messages.len() as u64;
        serde_json::json!({
            "topic": p.topic,
            "messages": messages,
            "next_offset": since + returned,
        })
        .to_string()
    }

    #[tool(
        description = "v4.1 — Peek at the most recent N messages in a topic \
        without advancing any offset. Useful for the 'what just happened' \
        quick-glance pattern. Maximum 100 per call. Default autonomy: notify."
    )]
    async fn queue_peek(
        &self,
        Parameters(p): Parameters<QueuePeekParams>,
    ) -> String {
        if !neurogrim_core::queue::Topic::is_valid(&p.topic) {
            return serde_json::json!({"error": "invalid topic name", "topic": p.topic})
                .to_string();
        }
        let path = topic_disk_path(&self.project_root, &p.topic);
        let reader = match neurogrim_core::queue::JsonlQueueReader::open(&path) {
            Ok(r) => r,
            Err(e) => {
                return serde_json::json!({"error": format!("read failed: {e}")}).to_string();
            }
        };
        let count = p.count.unwrap_or(10).min(100) as usize;
        let tail = reader.tail(count);
        let messages: Vec<serde_json::Value> = tail
            .iter()
            .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
            .collect();
        serde_json::json!({"topic": p.topic, "messages": messages}).to_string()
    }

    #[tool(
        description = "v4.2 — Mint a single-use proxy token for a stored secret. \
        Default autonomy `Approve` — every fetch lands on the S13 approvals queue \
        and requires explicit operator approval. The agent NEVER sees the secret \
        value; it receives an opaque token that authorizes ONE upstream API call \
        through claude-proxy and expires in 60 seconds. Pass the token via the \
        `X-Scope-Token` header. Returns {status: 'pending_approval' | 'minted', \
        token?, expires_at?, action_id?}."
    )]
    async fn secret_fetch(
        &self,
        Parameters(p): Parameters<SecretFetchParams>,
    ) -> String {
        // v4.2 S14-S-5 + S13-B-5: every secret fetch goes through
        // the autonomy gate. Default action_type "mutate-state"
        // resolves to Approve; the operator approves via the
        // /brains/:id/approvals page; the agent polls await_approval.
        if let Some(early) = crate::autonomy::maybe_block(self, "secret_fetch").await {
            return early;
        }
        // Brain id derived from the registry (same convention as
        // `MasterSessionKey::load_or_generate` and the autonomy
        // module). For a single-Brain server, this is just the
        // host's id.
        let brain_id = if self.registry.meta.updated_by.trim().is_empty() {
            "neurogrim".to_string()
        } else {
            self.registry.meta.updated_by.clone()
        };
        let secret_key = neurogrim_secrets::SecretKey::new(brain_id, p.secret_id);
        let token = self.proxy_tokens.mint(secret_key, p.scope, None);
        // Compute the expires_at as a wall-clock RFC3339 stamp by
        // taking now + ttl. We can't read Instant in clock-time
        // form directly; chrono::Utc::now() + the TTL gives a
        // reasonable approximation for the agent.
        let expires_at = (chrono::Utc::now()
            + chrono::Duration::seconds(
                crate::proxy_tokens::DEFAULT_TTL_SECS as i64,
            ))
        .to_rfc3339();
        serde_json::json!({
            "status": "minted",
            "token": token.token_id,
            "expires_at": expires_at,
            "secret_id": token.secret_key.secret_id,
            "scope": token.scope,
            "hint": "pass this token to claude-proxy as `X-Scope-Token: <token>`. Single-use; expires in 60s.",
        })
        .to_string()
    }

    #[tool(
        description = "v4.1 — Poll the autonomy approvals ledger for a decision \
        on the given action_id. Returns {status: 'pending' | 'approved' | 'denied', \
        operator?, decided_at?}. Agents call this after a mutation tool returned \
        `status: pending_approval` to learn whether the operator approved or \
        denied. Reads from `_neurogrim/approval-resolutions`."
    )]
    async fn await_approval(
        &self,
        Parameters(p): Parameters<AwaitApprovalParams>,
    ) -> String {
        match crate::autonomy::read_approval_resolution(&self.project_root, &p.action_id) {
            Some(res) => serde_json::json!({
                "status": res.decision,
                "action_id": res.action_id,
                "operator": res.operator,
                "decided_at": res.decided_at,
            })
            .to_string(),
            None => serde_json::json!({
                "status": "pending",
                "action_id": p.action_id,
                "hint": "operator hasn't resolved yet — poll again, or visit /brains/:id/approvals in the dashboard",
            })
            .to_string(),
        }
    }
}

/// Mirror `BusState::topic_path` from the dashboard crate: subdirs
/// for slash segments + `.jsonl` extension on the leaf.
fn topic_disk_path(project_root: &std::path::Path, topic: &str) -> std::path::PathBuf {
    let mut p = project_root
        .join(".claude")
        .join("brain")
        .join("queues");
    for seg in topic.split('/') {
        if !seg.is_empty() {
            p.push(seg);
        }
    }
    p.set_extension("jsonl");
    p
}

/// Build the "next steps" hint text for an MCP `domain_new` response.
/// Mirrors the CLI's stderr printout but as a single string suitable
/// for an MCP JSON envelope.
fn next_steps_text(outcome: &crate::domain::ScaffoldOutcome) -> String {
    let mut s = String::new();
    if let Some(p) = outcome.sensor_path.as_ref() {
        let py_module = outcome.name.replace('-', "_");
        s.push_str(&format!(
            "1. Open {} and implement analyze() (see `explain sensor`).\n",
            p.display()
        ));
        s.push_str(&format!(
            "2. Refresh the CMDB: py -3 sensory/check_{}.py . > .claude/{}-cmdb.json\n",
            py_module, outcome.name
        ));
    } else {
        s.push_str(
            "1. Author a sensor that emits the CMDB envelope shape (see `explain sensor`).\n",
        );
        s.push_str(&format!(
            "2. Refresh the CMDB into {} once the sensor exists.\n",
            outcome.cmdb_path.display()
        ));
    }
    s.push_str("3. Verify the domain shows up via `orient` (or `neurogrim agent --prose`).\n");
    s.push_str("4. Validate registry shape via `doctor` (or `neurogrim doctor`).\n");
    s.push_str("5. Read `explain domain` if needed.\n");
    s
}

impl ServerHandler for BrainServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("NeuroGrim LSP Brains scoring engine. \
                v3.2.1: agents new to a project should call `orient` first \
                (Brain summary), then `doctor` (config audit), then \
                `explain methodology` (the model). Use `get_health_score` \
                for the full project health picture, `domain_new` to \
                scaffold a new measurement target.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
