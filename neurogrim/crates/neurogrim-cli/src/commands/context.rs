//! Shared context for all CLI commands — loads registry, CMDB data, runs scoring pipeline.

use anyhow::Result;
use chrono::{DateTime, Utc};
use neurogrim_core::agent_output::{build_agent_output, AgentOutput, CorrelationFired};
use neurogrim_core::correlation::{
    evaluate_condition, evaluate_incident_patterns, extract_domain_variables, IncidentLedgerEntry,
};
use neurogrim_core::governance::build_domain_recommendations;
use neurogrim_core::learning::{compute_all_effectiveness, ProposalLedgerEntry};
use neurogrim_core::registry::{BrainRegistry, ExportedVariable};
use neurogrim_core::scoring::{build_scorecard, CmdbData};
use neurogrim_core::trajectory::compute_trajectory;
use neurogrim_core::types::ScoreSnapshot;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Everything a command needs to produce output.
pub struct BrainContext {
    /// Parsed registry. Retained on the context so downstream consumers
    /// (future hats, human-persona filters, A2A Agent Card builders) can
    /// inspect domain weights + hat declarations without reloading.
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
    pub async fn load(
        registry_path: &str,
        hat: Option<String>,
        human_persona: Option<String>,
    ) -> Result<Self> {
        let json = tokio::fs::read_to_string(registry_path).await?;
        let registry = BrainRegistry::from_json(&json)?;
        registry.validate()?;

        let registry_dir = Path::new(registry_path).parent().unwrap_or(Path::new("."));
        let project_root = registry_dir
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();

        let now = Utc::now();
        let cmdb_data = load_cmdb_data(&registry, &project_root).await;
        let scorecard = build_scorecard(&registry, &cmdb_data, now);

        // Load history and ledgers
        let history = load_json_file::<Vec<ScoreSnapshot>>(
            &project_root.join(".claude/brain/score-history.json"),
        )
        .await;
        let incident_ledger = load_json_file::<Vec<IncidentLedgerEntry>>(
            &project_root.join(".claude/brain/incident-ledger.json"),
        )
        .await;
        let proposal_ledger = load_json_file::<Vec<ProposalLedgerEntry>>(
            &project_root.join(".claude/brain/proposal-ledger.json"),
        )
        .await;

        // Domain variables
        let raw_cmdbs = load_raw_cmdbs(&registry, &project_root).await;
        let exported: HashMap<String, HashMap<String, ExportedVariable>> = registry
            .config
            .domain_definitions
            .iter()
            .filter(|(_, d)| !d.exported_variables.is_empty())
            .map(|(k, d)| (k.clone(), d.exported_variables.clone()))
            .collect();
        let domain_variables = extract_domain_variables(&raw_cmdbs, &exported);

        // Trajectory
        let unified_traj = compute_trajectory(
            &history,
            &registry.config.trajectory,
            None,
            &registry.config.domain_weights,
        );
        let mut dom_trajs = HashMap::new();
        for dk in registry.config.domain_weights.keys() {
            dom_trajs.insert(
                dk.clone(),
                compute_trajectory(
                    &history,
                    &registry.config.trajectory,
                    Some(dk),
                    &registry.config.domain_weights,
                ),
            );
        }

        // Correlations
        let corrs: Vec<CorrelationFired> = registry
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

        // Incidents
        let (incidents, skipped) = evaluate_incident_patterns(
            &registry.config.incident_patterns,
            &domain_variables,
            &history,
            &incident_ledger,
            &registry.config.severity_thresholds,
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
            &scorecard,
            &domain_variables,
            vec![],
            vec![],
            recommendations,
            corrs,
            incidents,
            skipped,
            Some(unified_traj),
            dom_trajs,
            Some(effectiveness),
            hat,
            human_persona,
        );

        Ok(BrainContext {
            registry,
            project_root,
            agent_output,
        })
    }
}

async fn load_cmdb_data(
    registry: &BrainRegistry,
    project_root: &Path,
) -> HashMap<String, CmdbData> {
    let mut data = HashMap::new();
    for (dk, def) in &registry.config.domain_definitions {
        if let Some(ref src) = def.scoring_source {
            match src.source_type.as_str() {
                "cmdb" => {
                    if let Some(ref p) = src.path {
                        let full = project_root.join(p);
                        if let Ok(s) = tokio::fs::read_to_string(&full).await {
                            // Strip UTF-8 BOM if present (PowerShell writes BOM with -Encoding UTF8)
                            let s = s.trim_start_matches('\u{FEFF}');
                            if let Ok(cmdb) = serde_json::from_str::<serde_json::Value>(s) {
                                let sf = src.score_field.as_deref().unwrap_or("score");
                                let uf = src.updated_at_field.as_deref().unwrap_or("updated_at");
                                if let (Some(score), Some(ts_str)) = (
                                    cmdb.get(sf).and_then(|v| v.as_u64()),
                                    cmdb.get(uf).and_then(|v| v.as_str()),
                                ) {
                                    if let Ok(ts) = ts_str.parse::<DateTime<Utc>>() {
                                        data.insert(
                                            dk.clone(),
                                            CmdbData {
                                                score: score.min(100) as u8,
                                                updated_at: ts,
                                            },
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                "a2a" => {
                    // Spec §9 fractal composition: fetch the child Brain's
                    // current AgentOutput via A2A and use its unified score
                    // as this domain's raw score. Failures (unreachable peer,
                    // bad URL, version mismatch) fall through to `no_file_score`
                    // — same semantics as a missing CMDB file.
                    if let Some(env) = load_a2a_domain(dk, src).await {
                        data.insert(dk.clone(), env);
                    } else {
                        tracing::debug!(
                            "a2a scoring source for domain {dk} unresolved; \
                             scoring pipeline will fall back to no_file_score"
                        );
                    }
                }
                "function" => {
                    // Implementation-specific scoring functions — handled
                    // elsewhere in the pipeline, not via CmdbData. No-op here.
                }
                other => {
                    tracing::warn!("domain {dk}: unknown scoring_source.type {other:?}; ignoring");
                }
            }
        }
    }
    data
}

/// Fetch a domain's score from a peer Brain via A2A. Reuses
/// `neurogrim_ecosystem::invoke_child` so there's only one A2A-client
/// implementation in the tree. Returns None if anything fails; caller
/// should log + fall through to `no_file_score`.
async fn load_a2a_domain(
    domain_key: &str,
    src: &neurogrim_core::registry::ScoringSource,
) -> Option<CmdbData> {
    use neurogrim_core::ecosystem::{ChildEntry, ChildTransport};
    use neurogrim_ecosystem::invoke_child;

    let endpoint_str = src.endpoint.as_ref()?;
    let endpoint = match url::Url::parse(endpoint_str) {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!("domain {domain_key}: bad A2A endpoint {endpoint_str:?}: {e}");
            return None;
        }
    };
    let interface_version = src
        .interface_version
        .clone()
        .unwrap_or_else(|| "1".to_string());

    let entry = ChildEntry {
        id: domain_key.to_string(),
        display_name: None,
        transport: ChildTransport::A2A {
            a2a_endpoint: endpoint,
            agent_card_url: None,
        },
        depends_on: Vec::new(),
        weight: 1.0,
        interface_version,
        enabled: true,
    };

    // AgentOutput.scored_at is a String (RFC3339); parse into DateTime<Utc>.
    // Fall back to Utc::now() if the peer sent an unparseable timestamp —
    // the fetch itself was synchronous so "now" is honest.
    match invoke_child(&entry).await {
        Ok(agent_output) => {
            let ts = agent_output
                .scored_at
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            Some(CmdbData {
                score: agent_output.score,
                updated_at: ts,
            })
        }
        Err(e) => {
            tracing::warn!(
                "domain {domain_key}: A2A fetch failed ({e}); \
                 falling back to no_file_score"
            );
            None
        }
    }
}

async fn load_raw_cmdbs(
    registry: &BrainRegistry,
    project_root: &Path,
) -> HashMap<String, serde_json::Value> {
    let mut cmdbs = HashMap::new();
    for (dk, def) in &registry.config.domain_definitions {
        if let Some(ref src) = def.scoring_source {
            if let Some(ref p) = src.path {
                if let Ok(s) = tokio::fs::read_to_string(project_root.join(p)).await {
                    let s = s.trim_start_matches('\u{FEFF}');
                    if let Ok(v) = serde_json::from_str(s) {
                        cmdbs.insert(dk.clone(), v);
                    }
                }
            }
        }
    }
    cmdbs
}

async fn load_json_file<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    tokio::fs::read_to_string(path)
        .await
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Append a `ProposalLedgerEntry` to `.claude/brain/proposal-ledger.json`
/// capturing the recommendations the pipeline produced on this run.
///
/// Linked-list `pre_score`: if the previous ledger entry has a `post_score`
/// set, this entry's `pre_score` is that previous `post_score`. Creates a
/// temporal chain so `compute_all_effectiveness` can credit each round's
/// recommendations with the next round's delta. No user action required.
///
/// action_type is derived from `recommendation.gate` — a stable grouping
/// key for the learning math. Falls back to `recommendation.domain` if
/// gate is empty.
///
/// Best-effort; failures log a warning but don't break the enclosing
/// command.
pub async fn append_proposal_ledger(project_root: &Path, agent_output: &AgentOutput) {
    use neurogrim_core::learning::{Proposal, ProposalLedgerEntry};

    let brain_dir = project_root.join(".claude").join("brain");
    if let Err(e) = tokio::fs::create_dir_all(&brain_dir).await {
        tracing::warn!("cannot create {:?}: {e}; skipping ledger write", brain_dir);
        return;
    }
    let ledger_path = brain_dir.join("proposal-ledger.json");

    // Load existing ledger to find linked-list pre_score.
    let mut ledger: Vec<ProposalLedgerEntry> = match tokio::fs::read_to_string(&ledger_path).await {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let pre_score = ledger.last().and_then(|e| e.post_score).or(Some(0));

    // Derive proposals from top_recommendations.
    let proposals: Vec<Proposal> = agent_output
        .top_recommendations
        .iter()
        .map(|r| Proposal {
            id: Some(format!("{}:{}", r.domain, r.gate)),
            command: Some(r.command.clone()),
            domain: Some(r.domain.clone()),
            action_type: Some(if r.gate.is_empty() {
                r.domain.clone()
            } else {
                r.gate.clone()
            }),
        })
        .collect();

    // Git HEAD resolution is best-effort; skip if git isn't available
    // or the project isn't a repo.
    let commit = tokio::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project_root)
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let entry = ProposalLedgerEntry {
        timestamp: Utc::now().to_rfc3339(),
        proposals,
        pre_score,
        post_score: Some(agent_output.score as i64),
        commit,
        hat: agent_output.current_hat.clone(),
    };
    ledger.push(entry);

    match serde_json::to_string_pretty(&ledger) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&ledger_path, json).await {
                tracing::warn!("cannot write {:?}: {e}; ledger append dropped", ledger_path);
            }
        }
        Err(e) => {
            tracing::warn!("cannot serialize proposal ledger: {e}; ledger append dropped");
        }
    }
}

/// Append a ScoreSnapshot derived from the current `AgentOutput` to
/// `.claude/brain/score-history.json`. Creates the `brain/` subdir if
/// absent. Prunes entries older than `retention_days` (default 30).
///
/// Intended caller: user-facing commands (`score`, `health`) that represent
/// "I'm checking in on project state right now." Not called from read-only
/// view commands (`trend`, `agent`) or from `a2a-serve`'s per-request
/// handler — those would inflate history without representing new decisions.
///
/// Best-effort: returns unconditionally. Failure to persist history
/// surfaces as a tracing::warn but must not break the enclosing command.
pub async fn append_score_history(
    project_root: &Path,
    agent_output: &AgentOutput,
    retention_days: u32,
) {
    use neurogrim_core::types::{ScoreSnapshot, SnapshotDomain};

    let scored_at = agent_output
        .scored_at
        .parse::<DateTime<Utc>>()
        .unwrap_or_else(|_| Utc::now());
    let domains = agent_output
        .domains
        .iter()
        .map(|(k, d)| {
            (
                k.clone(),
                SnapshotDomain {
                    score: d.score,
                    confidence: d.confidence,
                },
            )
        })
        .collect();
    let snapshot = ScoreSnapshot {
        scored_at,
        score: agent_output.score,
        domains,
        hat: agent_output.current_hat.clone(),
    };

    let brain_dir = project_root.join(".claude").join("brain");
    if let Err(e) = tokio::fs::create_dir_all(&brain_dir).await {
        tracing::warn!("cannot create {:?}: {e}; skipping history write", brain_dir);
        return;
    }
    let history_path = brain_dir.join("score-history.json");

    let mut history: Vec<ScoreSnapshot> = match tokio::fs::read_to_string(&history_path).await {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    history.push(snapshot);

    // Retention pruning. Non-atomic read-modify-write; acceptable for
    // single-user CLI use. A file lock is follow-on work.
    let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
    history.retain(|s| s.scored_at >= cutoff);

    match serde_json::to_string_pretty(&history) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&history_path, json).await {
                tracing::warn!(
                    "cannot write {:?}: {e}; history append dropped",
                    history_path
                );
            }
        }
        Err(e) => {
            tracing::warn!("cannot serialize score history: {e}; history append dropped");
        }
    }
}
