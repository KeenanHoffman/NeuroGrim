//! Coherence sensory tool — cross-domain relationship health.
//!
//! Reads the `correlations` array from brain-registry.json, evaluates each
//! condition_tree against the live domain CMDBs, and scores how many
//! cross-domain relationships are in healthy (non-firing) configurations.
//!
//! Coherence is the association cortex: it doesn't repeat what individual
//! domains know — it asks what their signals mean *together*.
//!
//! Score starts at 100 and decreases by:
//!   - 35 pts per fired `critical` correlation
//!   - 20 pts per fired `warning` correlation
//!   -  5 pts per fired `info` correlation
//! A score of 100 means all defined correlations are in healthy configurations
//! (or no correlations are defined yet — coherence value grows with your registry).

use crate::cmdb::{build_cmdb, Finding};
use neurogrim_core::correlation::evaluate_condition;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct CoherenceServer {
    tool_router: ToolRouter<Self>,
}
impl CoherenceServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckCoherenceParams {
    pub project_root: String,
}

#[tool_router]
impl CoherenceServer {
    #[tool(
        description = "Check cross-domain coherence: evaluates the correlations defined in \
        brain-registry.json against live domain CMDBs. Scores relationship health — \
        how many cross-domain correlations are in healthy (non-firing) configurations. \
        Returns CMDB-envelope JSON."
    )]
    async fn check_coherence(&self, Parameters(p): Parameters<CheckCoherenceParams>) -> String {
        serde_json::to_string_pretty(&analyze_coherence(&p.project_root).await).unwrap_or_default()
    }
}

impl ServerHandler for CoherenceServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Cross-domain coherence sensory tool. Evaluates registry-defined correlations \
                against live CMDBs to surface relationships individual domains cannot see alone."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub async fn analyze_coherence(project_root: &str) -> Value {
    let root = Path::new(project_root);
    let mut findings: Vec<Finding> = Vec::new();
    let mut score: i32 = 100;
    let mut extras: Vec<(&str, Value)> = Vec::new();

    // ── Step 1: Read brain-registry.json ─────────────────────────────────────
    let registry_path = root.join(".claude/brain-registry.json");
    let registry_str = match tokio::fs::read_to_string(&registry_path).await {
        Ok(s) => s,
        Err(_) => {
            extras.push((
                "coherence_error",
                Value::String("brain-registry.json not found — run from project root".into()),
            ));
            extras.push(("correlations_evaluated", Value::from(0u8)));
            extras.push(("correlations_fired", Value::from(0u8)));
            extras.push(("highest_severity", Value::String("none".into())));
            extras.push(("correlation_details", json!([])));
            return build_cmdb("check-coherence", 0, findings, Some(extras), None);
        }
    };

    let registry: Value = match serde_json::from_str(&registry_str) {
        Ok(v) => v,
        Err(e) => {
            extras.push((
                "coherence_error",
                Value::String(format!("brain-registry.json parse error: {e}")),
            ));
            extras.push(("correlations_evaluated", Value::from(0u8)));
            extras.push(("correlations_fired", Value::from(0u8)));
            extras.push(("highest_severity", Value::String("none".into())));
            extras.push(("correlation_details", json!([])));
            return build_cmdb("check-coherence", 0, findings, Some(extras), None);
        }
    };

    // ── Step 2: Extract correlations + domain_definitions ────────────────────
    let correlations = registry
        .get("config")
        .and_then(|c| c.get("correlations"))
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();

    let domain_defs = registry
        .get("config")
        .and_then(|c| c.get("domain_definitions"))
        .cloned()
        .unwrap_or(json!({}));

    // ── Step 3: Collect unique domain names referenced across all correlations ─
    let mut domains_needed: HashSet<String> = HashSet::new();
    for corr in &correlations {
        if let Some(domains) = corr.get("domains").and_then(|d| d.as_array()) {
            for d in domains {
                if let Some(name) = d.as_str() {
                    domains_needed.insert(name.to_string());
                }
            }
        }
    }

    // ── Step 4: Load each referenced domain's CMDB ───────────────────────────
    let mut cmdb_data: HashMap<String, Value> = HashMap::new();
    for domain in &domains_needed {
        let cmdb_path_str = domain_defs
            .get(domain)
            .and_then(|def| def.get("scoring_source"))
            .and_then(|src| src.get("path"))
            .and_then(|p| p.as_str());

        if let Some(path_str) = cmdb_path_str {
            let full_path = root.join(path_str);
            if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                if let Ok(v) = serde_json::from_str::<Value>(&content) {
                    cmdb_data.insert(domain.clone(), v);
                }
            }
        }
    }

    // ── Step 5: Build domain variables map ───────────────────────────────────
    // Mirror the fallback path of extract_domain_variables in neurogrim-core:
    // all top-level bool/number fields become "domain:field" keys.
    let mut vars: HashMap<String, Value> = HashMap::new();
    for (domain, cmdb) in &cmdb_data {
        if let Some(obj) = cmdb.as_object() {
            for (field, value) in obj {
                // Skip structural / nested fields
                if matches!(
                    field.as_str(),
                    "meta" | "findings" | "control_references" | "correlation_details"
                ) {
                    continue;
                }
                match value {
                    Value::Bool(_) | Value::Number(_) => {
                        vars.insert(format!("{}:{}", domain, field), value.clone());
                    }
                    _ => {}
                }
            }
        }
    }

    // ── Step 6: Evaluate each correlation ────────────────────────────────────
    let deduction_for = |sev: &str| -> i32 {
        match sev {
            "critical" => 35,
            "warning" => 20,
            _ => 5, // info / unrecognised
        }
    };

    let severity_rank = |sev: &str| -> u8 {
        match sev {
            "critical" => 3,
            "warning" => 2,
            "info" => 1,
            _ => 0,
        }
    };

    let mut correlations_fired_count: u8 = 0;
    let mut highest_rank: u8 = 0;
    let mut highest_severity = "none".to_string();
    let mut correlation_details: Vec<Value> = Vec::new();

    for corr in &correlations {
        let id = corr
            .get("id")
            .or_else(|| corr.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let corr_type = corr
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("reinforcing");
        let severity = corr
            .get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("info");
        let description = corr
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let insight = corr.get("insight").and_then(|v| v.as_str()).unwrap_or("");
        let domains_val = corr.get("domains").cloned().unwrap_or(json!([]));

        let fired = match corr.get("condition_tree") {
            Some(ct) if !ct.is_null() => evaluate_condition(ct, &vars, &[]),
            _ => false,
        };

        if fired {
            let deduction = deduction_for(severity);
            score -= deduction;
            correlations_fired_count += 1;

            let rank = severity_rank(severity);
            if rank > highest_rank {
                highest_rank = rank;
                highest_severity = severity.to_string();
            }

            findings.push(Finding {
                name: id.to_string(),
                status: format!("fired:{}", corr_type),
                points: -deduction,
                detail: Some(if !insight.is_empty() {
                    insight.to_string()
                } else {
                    description.to_string()
                }),
            });
        }

        correlation_details.push(json!({
            "id":          id,
            "type":        corr_type,
            "severity":    severity,
            "fired":       fired,
            "domains":     domains_val,
            "description": description,
            "insight":     insight,
        }));
    }

    let correlations_evaluated = correlations.len() as u8;

    // ── Step 7: Assemble CMDB ─────────────────────────────────────────────────
    extras.push((
        "correlations_evaluated",
        Value::from(correlations_evaluated),
    ));
    extras.push(("correlations_fired", Value::from(correlations_fired_count)));
    extras.push(("highest_severity", Value::String(highest_severity)));
    extras.push(("correlation_details", Value::Array(correlation_details)));

    build_cmdb(
        "check-coherence",
        score.clamp(0, 100) as u8,
        findings,
        Some(extras),
        None,
    )
}
