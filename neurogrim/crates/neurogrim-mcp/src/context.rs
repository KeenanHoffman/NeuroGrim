//! Shared brain context — loads registry, CMDB data, runs the full
//! scoring pipeline. Used by both the CLI commands and the v3.4
//! dashboard server (neurogrim-dashboard).
//!
//! v3.4 Phase 0.1: relocated from `neurogrim-cli` (bin-only crate) to
//! `neurogrim-mcp` so the dashboard server can call into it. Same
//! pattern as v3.2.1's prose / doctor / domain moves — the cli
//! crate is bin-only, so anything that needs to be reused from
//! another crate has to live in a library crate.

use anyhow::Result;
use chrono::{DateTime, Utc};
use neurogrim_core::agent_output::{build_agent_output, AgentOutput, CorrelationFired};
use neurogrim_core::correlation::{
    evaluate_condition, evaluate_incident_patterns, extract_domain_variables, IncidentLedgerEntry,
};
use neurogrim_core::governance::build_domain_recommendations;
use neurogrim_core::learning::{compute_all_effectiveness, ProposalLedgerEntry};
use neurogrim_core::registry::{BrainRegistry, ExportedVariable};
use neurogrim_core::calibration_ledger::auto_trigger_calibration_writes;
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
        // V5-FOUND-1 Phase 3 step 1 (CLI pathway): scoring pipeline
        // instrumentation. Same span name as the MCP-server pathway
        // (run_scoring in server.rs:132) so both converge under
        // EventKind::Scoring in the ledger. Extras are filled in
        // via span.record() after build_agent_output completes.
        // The Layer is a no-op when NEUROGRIM_DIAG is unset, so
        // this adds zero overhead in production.
        let span = tracing::info_span!(
            "score.pipeline.run",
            domains_count = tracing::field::Empty,
            score = tracing::field::Empty,
            confidence = tracing::field::Empty,
        );
        let _entered = span.enter();

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

        // E-B2-2 C7 — auto-trigger plumbing for the per-domain calibration
        // ledger (§17.3). Default-off via `enable_calibration_writes`;
        // per-domain opt-in via `calibration_trigger`. Recursion guard
        // (§17.9) hard-coded for the `domain-calibration` domain itself.
        // Errors are logged + skipped — auto-trigger MUST NOT crash scoring.
        // v1 passes empty findings map; SignalClassFired auto-fire from
        // production paths is per-domain follow-on work.
        let _ = auto_trigger_calibration_writes(
            &registry,
            &scorecard,
            &HashMap::new(),
            &project_root,
        );

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

        // V5-FOUND-1 Phase 3 step 1 (CLI pathway): record scoring
        // extras for the diagnostics-layer entry. Same fields as the
        // server.rs pathway so V5-MOD-1's perf-gate sees a unified
        // distribution regardless of which entry point ran.
        span.record("domains_count", agent_output.domains.len() as i64);
        span.record("score", agent_output.score as i64);
        span.record("confidence", agent_output.unified_confidence as i64);

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
    // V5-MOD-1 Phase 3 (2026-05-02): the prior string-match
    // dispatch on `source_type` ∈ {cmdb, a2a, function} is
    // replaced by registry-based factory dispatch. Each domain's
    // ScoringSourceConfig is resolved via the global registry
    // (cmdb + function from neurogrim-core; a2a from
    // neurogrim-ecosystem). Unknown source_types log a warn and
    // skip — same semantics as the old `other => warn` arm. The
    // cmdb / a2a / function semantics are bit-identical via the
    // verbatim ports in V5-MOD-1 Phase 2.
    let source_registry = crate::scoring_source_registry::default_registry();
    let mut data = HashMap::new();
    for (dk, def) in &registry.config.domain_definitions {
        if let Some(ref src) = def.scoring_source {
            let Some(factory) = source_registry.get(&src.source_type) else {
                tracing::warn!(
                    "domain {dk}: unknown scoring_source.type {:?}; ignoring",
                    src.source_type
                );
                continue;
            };
            let source = factory.build();
            if let Some(cmdb) = source.load(dk, src, project_root).await {
                data.insert(dk.clone(), cmdb);
            } else if src.source_type == "a2a" {
                // Preserve the v4 debug-log breadcrumb specifically
                // for the a2a path (operators rely on it for
                // troubleshooting unresolved fractal-composition
                // children). cmdb / function paths log their own
                // warns inside the impl when relevant.
                tracing::debug!(
                    "a2a scoring source for domain {dk} unresolved; \
                     scoring pipeline will fall back to no_file_score"
                );
            }
        }
    }
    data
}

/// Fetch a domain's score from a peer Brain via A2A. Reuses
/// `neurogrim_ecosystem::invoke_child` so there's only one A2A-client
/// implementation in the tree. Returns None if anything fails; caller
/// should log + fall through to `no_file_score`.
// V5-MOD-1 Phase 3 (2026-05-02): the prior `load_a2a_domain`
// helper was moved verbatim into
// `neurogrim_ecosystem::scoring_source::A2aSource::load`
// (Phase 2). The dispatch site at `load_cmdb_data` above now
// resolves through the global ScoringSourceRegistry, so
// `load_a2a_domain` is no longer called from this crate.

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

/// Append a ScoreSnapshot derived from the current `AgentOutput` to the
/// `_neurogrim/score-snapshots` SQLite bus topic. Creates parent dirs if
/// absent. Sets `expires_at = now + retention_days` so the dashboard can
/// filter without date arithmetic on read.
///
/// On first call for a project, auto-migrates any existing
/// `score-history.json` into the SQLite topic so operators don't lose
/// history when upgrading.
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
    use neurogrim_core::queue::{QueueMessage, SCORE_SNAPSHOTS_TOPIC};
    use neurogrim_core::queue_backend::{QueueBackend, SqliteBackend};
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

    // SQLite path: .claude/brain/queues/_neurogrim/score-snapshots.sqlite
    let sqlite_path = brain_dir
        .join("queues")
        .join("_neurogrim")
        .join("score-snapshots.sqlite");
    if let Some(parent) = sqlite_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("cannot create {:?}: {e}; skipping history write", parent);
            return;
        }
    }

    // One-time migration: if SQLite doesn't exist yet, seed it from the
    // legacy score-history.json so operators don't lose history.
    let needs_migration = !sqlite_path.exists();
    let mut backend = match SqliteBackend::open(&sqlite_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("score-snapshots backend open failed: {e}; history append dropped");
            return;
        }
    };
    if needs_migration {
        let json_path = brain_dir.join("score-history.json");
        migrate_score_history_json(&mut backend, &json_path, retention_days);
    }

    let payload = match serde_json::to_value(&snapshot) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("cannot serialize score snapshot: {e}; history append dropped");
            return;
        }
    };
    let expires = Utc::now() + chrono::Duration::days(retention_days as i64);
    let msg = QueueMessage::new(SCORE_SNAPSHOTS_TOPIC, payload).with_expires_at(expires);
    if let Err(e) = backend.append(&msg) {
        tracing::warn!("score snapshot append to bus failed: {e}");
    }
}

/// Migrate a legacy `score-history.json` into `backend`. Each JSON entry
/// becomes one bus message with `expires_at = None` (historical entries are
/// kept indefinitely; future entries carry the caller's retention policy).
/// Silent on missing / unreadable JSON — the SQLite will just start fresh.
fn migrate_score_history_json(
    backend: &mut neurogrim_core::queue_backend::SqliteBackend,
    json_path: &std::path::Path,
    retention_days: u32,
) {
    use neurogrim_core::queue::{QueueMessage, SCORE_SNAPSHOTS_TOPIC};
    use neurogrim_core::queue_backend::QueueBackend;

    let text = match std::fs::read_to_string(json_path) {
        Ok(t) => t,
        Err(_) => return, // no legacy file — nothing to migrate
    };
    let entries: Vec<serde_json::Value> = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return,
    };
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    let mut migrated = 0usize;
    for entry in entries {
        // Skip entries older than the caller's retention window.
        let scored_at = entry
            .get("scored_at")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok());
        if let Some(ts) = scored_at {
            if ts < cutoff {
                continue;
            }
        }
        let msg = QueueMessage::new(SCORE_SNAPSHOTS_TOPIC, entry);
        if let Err(e) = backend.append(&msg) {
            tracing::warn!("score history migration: append failed for entry {migrated}: {e}");
            break;
        }
        migrated += 1;
    }
    if migrated > 0 {
        tracing::info!(
            "score history migrated {migrated} entries from {} to SQLite bus topic",
            json_path.display()
        );
    }
}
