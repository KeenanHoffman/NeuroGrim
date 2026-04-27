//! Agent output assembly (spec Section 6).
//!
//! Produces the 11 required JSON fields conforming to agent-output-v1.schema.json.

use crate::correlation::{DomainVariables, IncidentMatch};
use crate::learning::ActionEffectiveness;
use crate::types::{Scorecard, TrajectoryResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Complete agent output conforming to agent-output-v1.schema.json.
///
/// Implements `Deserialize` so downstream crates (ecosystem dispatch) can parse
/// child Brain stdout or A2A payloads back into Rust. Deserialization IS the
/// validation: fields not matching the schema cause `serde_json` to fail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    // --- 11 Required Fields ---
    pub schema_version: String,
    pub scored_at: String,
    pub score: u8,
    pub domains: HashMap<String, AgentDomain>,
    pub dirty_gates: Vec<String>,
    pub stale_artifacts: Vec<String>,
    pub domain_variables: HashMap<String, Value>,
    pub top_recommendations: Vec<Recommendation>,
    pub correlations_fired: Vec<CorrelationFired>,
    pub incident_patterns: Vec<AgentIncident>,
    pub skipped_temporal: Vec<String>,

    // --- Brains-2.0 E-B2-1: peer of `score`, weighted-mean confidence ---
    /// Weighted-mean confidence across scored (non-advisory) domains:
    /// `round(sum(d.confidence * d.weight) / sum(d.weight))`. Mirrors how
    /// `score` (unified score) is aggregated. Receivers SHOULD use this
    /// for peer-to-peer trust decisions ("their score is 85 but
    /// confidence is 20 — discount").
    ///
    /// `#[serde(default)]` ensures backward-compat with v2.6 A2A peers
    /// that do not yet emit this field — deserialization succeeds with
    /// `unified_confidence == 0`. Combined with v2.7's spec-glossary
    /// disambiguation (envelope.confidence vs unified_confidence vs
    /// children[].confidence), this gives graceful upgrade across the
    /// four-Brain ecosystem.
    #[serde(default)]
    pub unified_confidence: u8,

    // --- Optional Fields ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal_effectiveness: Option<HashMap<String, ActionEffectiveness>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trajectory: Option<TrajectoryResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_hat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_human_persona: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDomain {
    pub score: u8,
    pub effective_score: u8,
    pub confidence: u8,
    pub weight: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trajectory: Option<TrajectoryResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub domain: String,
    pub gate: String,
    pub status: String,
    pub command: String,
    pub blocks: Vec<String>,
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
    /// Human-readable description of the recommendation and its urgency context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationFired {
    pub id: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIncident {
    pub id: String,
    pub name: String,
    pub hypothesis: String,
    pub narrative: String,
    pub signals: HashMap<String, Value>,
    pub severity: String,
    pub recurrence_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_remediation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_children: Option<Vec<String>>,
}

/// Build the complete agent output from computed data.
pub fn build_agent_output(
    scorecard: &Scorecard,
    domain_variables: &DomainVariables,
    dirty_gates: Vec<String>,
    stale_artifacts: Vec<String>,
    recommendations: Vec<Recommendation>,
    correlations: Vec<CorrelationFired>,
    incidents: Vec<IncidentMatch>,
    skipped_temporal: Vec<String>,
    unified_trajectory: Option<TrajectoryResult>,
    domain_trajectories: HashMap<String, TrajectoryResult>,
    effectiveness: Option<HashMap<String, ActionEffectiveness>>,
    hat: Option<String>,
    human_persona: Option<String>,
) -> AgentOutput {
    let mut domains = HashMap::new();
    for (key, ds) in &scorecard.domains {
        domains.insert(
            key.clone(),
            AgentDomain {
                score: ds.raw_score.value(),
                effective_score: ds.effective_score.value(),
                confidence: ds.confidence.value(),
                weight: ds.weight.value(),
                trajectory: domain_trajectories.get(key).cloned(),
            },
        );
    }

    // Convert domain variables to schema-compatible format
    let schema_vars: HashMap<String, Value> = domain_variables
        .iter()
        .filter(|(_, v)| v.is_boolean() || v.is_number() || v.is_string())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Convert incidents to schema format (add narrative and signals)
    let agent_incidents: Vec<AgentIncident> = incidents
        .iter()
        .map(|inc| {
            let signals = build_incident_signals(&inc.id, domain_variables);
            let narrative = build_incident_narrative(inc, &signals);
            AgentIncident {
                id: inc.id.clone(),
                name: inc.name.clone(),
                hypothesis: inc.hypothesis.clone().unwrap_or_default(),
                narrative,
                signals,
                severity: inc.severity.clone(),
                recurrence_count: inc.recurrence_count,
                skill_remediation: inc.skill_remediation.clone(),
                affected_children: None,
            }
        })
        .collect();

    AgentOutput {
        schema_version: "1".to_string(),
        scored_at: scorecard.scored_at.to_rfc3339(),
        score: scorecard.unified_score.value(),
        // E-B2-1 C6: weighted-mean of per-domain confidence over
        // non-advisory domains. See scoring::unified_confidence.
        unified_confidence: crate::scoring::unified_confidence(&scorecard.domains).value(),
        domains,
        dirty_gates,
        stale_artifacts,
        domain_variables: schema_vars,
        top_recommendations: recommendations,
        correlations_fired: correlations,
        incident_patterns: agent_incidents,
        skipped_temporal,
        proposal_effectiveness: effectiveness,
        trajectory: unified_trajectory,
        current_hat: hat,
        current_human_persona: human_persona,
    }
}

/// Build incident signals from domain variables.
fn build_incident_signals(_pattern_id: &str, vars: &DomainVariables) -> HashMap<String, Value> {
    // Include all domain variables as signals
    // In a full implementation, this would filter to only variables
    // referenced by the pattern's condition tree
    vars.iter()
        .filter(|(_, v)| v.is_boolean() || v.is_number() || v.is_string())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Build a human-readable narrative for an incident.
fn build_incident_narrative(incident: &IncidentMatch, signals: &HashMap<String, Value>) -> String {
    let mut parts = Vec::new();

    if let Some(ref hyp) = incident.hypothesis {
        parts.push(hyp.clone());
    }

    if !signals.is_empty() {
        let signal_strs: Vec<String> = signals
            .iter()
            .take(5) // Limit to avoid overwhelming
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        parts.push(format!("Signals: {}", signal_strs.join(", ")));
    }

    parts.push(format!(
        "Recurrence: {} (severity: {})",
        incident.recurrence_count, incident.severity
    ));

    if let Some(ref skill) = incident.skill_remediation {
        parts.push(format!("Remediation: {}", skill));
    }

    parts.join(". ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use chrono::Utc;

    #[test]
    fn agent_output_has_required_fields() {
        let mut domains = HashMap::new();
        domains.insert(
            "test-health".to_string(),
            DomainScore {
                domain: "test-health".to_string(),
                raw_score: Score::new(85),
                confidence: Confidence::full(),
                effective_score: Score::new(85),
                weight: Weight::new(1.0),
                trajectory: None,
            },
        );

        let scorecard = Scorecard {
            unified_score: Score::new(85),
            domains,
            scored_at: Utc::now(),
            floor_applied: None,
        };

        let output = build_agent_output(
            &scorecard,
            &DomainVariables::new(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
            HashMap::new(),
            None,
            None,
            None,
        );

        assert_eq!(output.schema_version, "1");
        assert_eq!(output.score, 85);
        assert!(output.domains.contains_key("test-health"));
        // E-B2-1 C6: unified_confidence is the weighted-mean of
        // per-domain confidence. Single domain with weight=1.0 +
        // Confidence::full() → unified_confidence = 100.
        assert_eq!(output.unified_confidence, 100);

        // Verify it serializes to valid JSON
        let json = serde_json::to_value(&output).unwrap();
        assert!(json.get("schema_version").is_some());
        assert!(json.get("scored_at").is_some());
        assert!(json.get("score").is_some());
        assert!(json.get("unified_confidence").is_some());
        assert!(json.get("domains").is_some());
        assert!(json.get("dirty_gates").is_some());
        assert!(json.get("stale_artifacts").is_some());
        assert!(json.get("domain_variables").is_some());
        assert!(json.get("top_recommendations").is_some());
        assert!(json.get("correlations_fired").is_some());
        assert!(json.get("incident_patterns").is_some());
        assert!(json.get("skipped_temporal").is_some());
    }

    #[test]
    fn agent_output_unified_confidence_defaults_when_absent() {
        // E-B2-1 C6: backward-compat for v2.6 A2A peers. When a peer
        // sends AgentOutput JSON without `unified_confidence`,
        // serde's default kicks in (u8 default = 0). This is the
        // graceful-upgrade contract — receivers SHOULD treat
        // unified_confidence == 0 as "peer is at v2.6 or earlier".
        let json = serde_json::json!({
            "schema_version": "1",
            "scored_at": "2026-04-27T12:00:00Z",
            "score": 75,
            "domains": {},
            "dirty_gates": [],
            "stale_artifacts": [],
            "domain_variables": {},
            "top_recommendations": [],
            "correlations_fired": [],
            "incident_patterns": [],
            "skipped_temporal": []
        });
        let output: AgentOutput = serde_json::from_value(json)
            .expect("v2.6 AgentOutput (no unified_confidence) must still deserialize");
        assert_eq!(output.unified_confidence, 0);
    }

    #[test]
    fn agent_output_domain_fields_correct() {
        let mut domains = HashMap::new();
        domains.insert(
            "code-quality".to_string(),
            DomainScore {
                domain: "code-quality".to_string(),
                raw_score: Score::new(80),
                confidence: Confidence::new(75.0),
                effective_score: Score::new(60),
                weight: Weight::new(0.35),
                trajectory: None,
            },
        );

        let scorecard = Scorecard {
            unified_score: Score::new(60),
            domains,
            scored_at: Utc::now(),
            floor_applied: None,
        };

        let output = build_agent_output(
            &scorecard,
            &DomainVariables::new(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
            HashMap::new(),
            None,
            None,
            None,
        );

        let d = &output.domains["code-quality"];
        assert_eq!(d.score, 80);
        assert_eq!(d.effective_score, 60);
        assert_eq!(d.confidence, 75);
        assert_eq!(d.weight, 0.35);
    }

    #[test]
    fn optional_fields_omitted_when_none() {
        let scorecard = Scorecard {
            unified_score: Score::new(50),
            domains: HashMap::new(),
            scored_at: Utc::now(),
            floor_applied: None,
        };

        let output = build_agent_output(
            &scorecard,
            &DomainVariables::new(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
            HashMap::new(),
            None,
            None,
            None,
        );

        let json = serde_json::to_value(&output).unwrap();
        // Optional fields should not be present
        assert!(json.get("current_hat").is_none());
        assert!(json.get("current_human_persona").is_none());
        assert!(json.get("trajectory").is_none());
        assert!(json.get("proposal_effectiveness").is_none());
    }
}
