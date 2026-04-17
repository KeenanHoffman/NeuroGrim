//! Shared context for all CLI commands — loads registry, CMDB data, runs scoring pipeline.

use anyhow::Result;
use chrono::{DateTime, Utc};
use motherbrain_core::agent_output::{build_agent_output, AgentOutput, CorrelationFired};
use motherbrain_core::correlation::{
    evaluate_condition, evaluate_incident_patterns, extract_domain_variables,
    IncidentLedgerEntry,
};
use motherbrain_core::governance::build_domain_recommendations;
use motherbrain_core::learning::{compute_all_effectiveness, ProposalLedgerEntry};
use motherbrain_core::registry::{BrainRegistry, ExportedVariable};
use motherbrain_core::scoring::{build_scorecard, CmdbData};
use motherbrain_core::trajectory::compute_trajectory;
use motherbrain_core::types::ScoreSnapshot;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Everything a command needs to produce output.
pub struct BrainContext {
    /// Parsed registry. Retained on the context so downstream consumers
    /// (future hats, persona filters, A2A Agent Card builders) can inspect
    /// domain weights + hat declarations without reloading.
    #[allow(dead_code)]
    pub registry: BrainRegistry,
    /// Resolved project root derived from the registry path. Retained for
    /// consumers that need filesystem-relative resolution for subprocess or
    /// fixture loading; not yet read by the core commands.
    #[allow(dead_code)]
    pub project_root: PathBuf,
    pub agent_output: AgentOutput,
}

impl BrainContext {
    /// Load registry, CMDB data, and run the full scoring pipeline.
    pub async fn load(registry_path: &str, hat: Option<String>, persona: Option<String>) -> Result<Self> {
        let json = tokio::fs::read_to_string(registry_path).await?;
        let registry = BrainRegistry::from_json(&json)?;
        registry.validate()?;

        let registry_dir = Path::new(registry_path).parent().unwrap_or(Path::new("."));
        let project_root = registry_dir.parent().unwrap_or(Path::new(".")).to_path_buf();

        let now = Utc::now();
        let cmdb_data = load_cmdb_data(&registry, &project_root).await;
        let scorecard = build_scorecard(&registry, &cmdb_data, now);

        // Load history and ledgers
        let history = load_json_file::<Vec<ScoreSnapshot>>(&project_root.join(".claude/brain/score-history.json")).await;
        let incident_ledger = load_json_file::<Vec<IncidentLedgerEntry>>(&project_root.join(".claude/brain/incident-ledger.json")).await;
        let proposal_ledger = load_json_file::<Vec<ProposalLedgerEntry>>(&project_root.join(".claude/brain/proposal-ledger.json")).await;

        // Domain variables
        let raw_cmdbs = load_raw_cmdbs(&registry, &project_root).await;
        let exported: HashMap<String, HashMap<String, ExportedVariable>> = registry.config.domain_definitions.iter()
            .filter(|(_, d)| !d.exported_variables.is_empty())
            .map(|(k, d)| (k.clone(), d.exported_variables.clone())).collect();
        let domain_variables = extract_domain_variables(&raw_cmdbs, &exported);

        // Trajectory
        let unified_traj = compute_trajectory(&history, &registry.config.trajectory, None, &registry.config.domain_weights);
        let mut dom_trajs = HashMap::new();
        for dk in registry.config.domain_weights.keys() {
            dom_trajs.insert(dk.clone(), compute_trajectory(&history, &registry.config.trajectory, Some(dk), &registry.config.domain_weights));
        }

        // Correlations
        let corrs: Vec<CorrelationFired> = registry.config.correlations.iter().filter_map(|c| {
            let name = c.get("name")?.as_str()?;
            let desc = c.get("description").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(ct) = c.get("condition_tree") {
                if !evaluate_condition(ct, &domain_variables, &history) { return None; }
            }
            Some(CorrelationFired { id: name.to_string(), description: desc.to_string(), skill: None })
        }).collect();

        // Incidents
        let (incidents, skipped) = evaluate_incident_patterns(
            &registry.config.incident_patterns, &domain_variables, &history, &incident_ledger, &registry.config.severity_thresholds,
        );

        // Proposal effectiveness — computed from ledger, used for ranking + agent output
        let effectiveness = compute_all_effectiveness(&proposal_ledger, 3);

        // Recommendations — ranked by trajectory urgency + effectiveness
        let recommendations = build_domain_recommendations(
            &scorecard,
            &dom_trajs,
            &registry.config,
            &effectiveness,
            5,
        );

        let agent_output = build_agent_output(
            &scorecard, &domain_variables, vec![], vec![], recommendations, corrs, incidents, skipped,
            Some(unified_traj), dom_trajs, Some(effectiveness), hat, persona,
        );

        Ok(BrainContext { registry, project_root, agent_output })
    }
}

async fn load_cmdb_data(registry: &BrainRegistry, project_root: &Path) -> HashMap<String, CmdbData> {
    let mut data = HashMap::new();
    for (dk, def) in &registry.config.domain_definitions {
        if let Some(ref src) = def.scoring_source {
            if src.source_type == "cmdb" {
                if let Some(ref p) = src.path {
                    let full = project_root.join(p);
                    if let Ok(s) = tokio::fs::read_to_string(&full).await {
                        // Strip UTF-8 BOM if present (PowerShell writes BOM with -Encoding UTF8)
                        let s = s.trim_start_matches('\u{FEFF}');
                        if let Ok(cmdb) = serde_json::from_str::<serde_json::Value>(s) {
                            let sf = src.score_field.as_deref().unwrap_or("score");
                            let uf = src.updated_at_field.as_deref().unwrap_or("updated_at");
                            if let (Some(score), Some(ts_str)) = (cmdb.get(sf).and_then(|v| v.as_u64()), cmdb.get(uf).and_then(|v| v.as_str())) {
                                if let Ok(ts) = ts_str.parse::<DateTime<Utc>>() {
                                    data.insert(dk.clone(), CmdbData { score: score.min(100) as u8, updated_at: ts });
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

async fn load_raw_cmdbs(registry: &BrainRegistry, project_root: &Path) -> HashMap<String, serde_json::Value> {
    let mut cmdbs = HashMap::new();
    for (dk, def) in &registry.config.domain_definitions {
        if let Some(ref src) = def.scoring_source {
            if let Some(ref p) = src.path {
                if let Ok(s) = tokio::fs::read_to_string(project_root.join(p)).await {
                    let s = s.trim_start_matches('\u{FEFF}');
                    if let Ok(v) = serde_json::from_str(s) { cmdbs.insert(dk.clone(), v); }
                }
            }
        }
    }
    cmdbs
}

async fn load_json_file<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    tokio::fs::read_to_string(path).await
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
