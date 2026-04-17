//! Governance model (spec Section 5).
//!
//! Implements the 5-step autonomy resolution algorithm, gate management,
//! recommendation generation with trajectory-weighted urgency ranking,
//! and recommendation priority computation.

use crate::agent_output::Recommendation;
use crate::learning::ActionEffectiveness;
use crate::registry::BrainConfig;
use crate::types::{AutonomyLevel, Scorecard, TrajectoryClassification, TrajectoryResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Recommendation generation
// ---------------------------------------------------------------------------

/// Known built-in sensory tool names. Used to derive the `command` field.
const BUILTIN_SENSORY_TOOLS: &[&str] = &[
    "git-health",
    "test-health",
    "code-quality",
    "deploy-readiness",
];

/// Build a prioritized list of domain recommendations.
///
/// For each domain in the scorecard:
/// - Advisory domains (weight 0.0) are skipped.
/// - Domains where effective_score >= 75 AND trajectory is not Degrading/Volatile are skipped.
/// - Urgency is computed as `(100 - effective_score) * trajectory_multiplier * effectiveness_multiplier`.
///
/// Results are sorted descending by urgency and limited to `max_count`.
pub fn build_domain_recommendations(
    scorecard: &Scorecard,
    domain_trajectories: &HashMap<String, TrajectoryResult>,
    registry: &BrainConfig,
    effectiveness: &HashMap<String, ActionEffectiveness>,
    max_count: usize,
) -> Vec<Recommendation> {
    let mut candidates: Vec<(f64, Recommendation)> = Vec::new();

    for (domain_key, domain_score) in &scorecard.domains {
        // Skip advisory domains
        if domain_score.weight.is_advisory() {
            continue;
        }

        let eff_score = domain_score.effective_score.value();
        let trajectory = domain_trajectories.get(domain_key);
        let classification = trajectory
            .map(|t| t.classification)
            .unwrap_or(TrajectoryClassification::NoData);

        // Skip healthy domains unless they are actively declining
        if eff_score >= 75
            && classification != TrajectoryClassification::Degrading
            && classification != TrajectoryClassification::Volatile
        {
            continue;
        }

        let status = derive_status(eff_score, classification);
        let command = derive_command(domain_key, registry);
        let description = build_description(domain_key, eff_score, classification, trajectory);

        let urgency = (100.0 - eff_score as f64)
            * trajectory_urgency(classification)
            * effectiveness_urgency(effectiveness, domain_key);

        candidates.push((
            urgency,
            Recommendation {
                domain: domain_key.clone(),
                gate: format!("{}-score-improvement", domain_key),
                status,
                command,
                blocks: vec![],
                depends_on: vec![],
                skill: None,
                description: Some(description),
            },
        ));
    }

    // Sort descending by urgency, then alphabetically by domain for stable ordering
    candidates.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.domain.cmp(&b.1.domain))
    });

    candidates
        .into_iter()
        .take(max_count)
        .map(|(_, rec)| rec)
        .collect()
}

/// Urgency multiplier based on trajectory classification.
fn trajectory_urgency(c: TrajectoryClassification) -> f64 {
    match c {
        TrajectoryClassification::Degrading  => 2.0,
        TrajectoryClassification::Volatile   => 1.5,
        TrajectoryClassification::NoData     => 1.2,
        TrajectoryClassification::Stable     => 1.0,
        TrajectoryClassification::Improving  => 0.6,
    }
}

/// Secondary urgency multiplier from historical effectiveness data.
/// Only applies when there are sufficient samples.
fn effectiveness_urgency(
    effectiveness: &HashMap<String, ActionEffectiveness>,
    domain: &str,
) -> f64 {
    // Try domain-specific key first, fall back to generic sensory-refresh
    let key = format!("sensory-refresh:{}", domain);
    let eff = effectiveness
        .get(&key)
        .or_else(|| effectiveness.get("sensory-refresh"));

    if let Some(e) = eff {
        if e.sufficient {
            if e.effectiveness_rate >= 0.8 {
                return 1.2; // High effectiveness: doing this tends to work
            }
            if e.effectiveness_rate < 0.4 {
                return 0.8; // Low effectiveness: deprioritize slightly
            }
        }
    }
    1.0
}

/// Derive the recommendation status from score and trajectory.
fn derive_status(effective_score: u8, classification: TrajectoryClassification) -> String {
    if effective_score < 50 {
        "critical".to_string()
    } else if classification == TrajectoryClassification::Degrading && effective_score >= 75 {
        "declining".to_string()
    } else {
        "needs-attention".to_string()
    }
}

/// Derive the actionable command from the domain key and registry config.
fn derive_command(domain_key: &str, registry: &BrainConfig) -> String {
    // 1. Known built-in sensory tool
    if BUILTIN_SENSORY_TOOLS.contains(&domain_key) {
        return format!("motherbrain sensory {}", domain_key);
    }

    // 2. Domain has a configured sensory server
    if registry.sensory_servers.contains_key(domain_key) {
        return format!("# Run sensory server for {}", domain_key);
    }

    // 3. Domain has a CMDB path — suggest refreshing it
    if let Some(def) = registry.domain_definitions.get(domain_key) {
        if let Some(ref src) = def.scoring_source {
            if let Some(ref path) = src.path {
                return format!("# Refresh CMDB at {}", path);
            }
        }
    }

    // 4. Generic fallback
    format!("# Improve {} — see motherbrain health for details", domain_key)
}

/// Build the human-readable description for a recommendation.
fn build_description(
    domain: &str,
    effective_score: u8,
    classification: TrajectoryClassification,
    trajectory: Option<&TrajectoryResult>,
) -> String {
    let traj_label = match classification {
        TrajectoryClassification::Degrading  => "degrading",
        TrajectoryClassification::Volatile   => "volatile",
        TrajectoryClassification::Improving  => "improving",
        TrajectoryClassification::Stable     => "stable",
        TrajectoryClassification::NoData     => "no trajectory data",
    };

    if let Some(t) = trajectory {
        if t.velocity.abs() > 0.1 {
            return format!(
                "{} is at {}/100 — {} (velocity: {:+.1})",
                domain, effective_score, traj_label, t.velocity
            );
        }
    }

    format!("{} is at {}/100 — {}", domain, effective_score, traj_label)
}

// ---------------------------------------------------------------------------
// Autonomy resolution
// ---------------------------------------------------------------------------

/// Proposal effectiveness data (from learning module).
#[derive(Debug, Clone, Default)]
pub struct ProposalConfidence {
    pub effectiveness_rate: f64,
    pub sample_count: u32,
    pub success_count: u32,
}

/// Autonomy configuration extracted from registry.
#[derive(Debug, Clone)]
pub struct AutonomyConfig {
    pub action_defaults: std::collections::HashMap<String, AutonomyLevel>,
    pub hat_bias: std::collections::HashMap<String, AutonomyLevel>,
    pub safety_invariants: Vec<SafetyInvariant>,
    pub auto_threshold: f64,
    pub auto_min_samples: u32,
    pub notify_threshold: f64,
    pub notify_min_samples: u32,
    pub global_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyInvariant {
    pub rule: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforced_level: Option<AutonomyLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_level: Option<AutonomyLevel>,
}

/// Resolve the autonomy level for an action using the 5-step algorithm.
///
/// Step 1: Hat bias (or action type default)
/// Step 2: Confidence from proposal effectiveness
/// Step 3: Take more restrictive of steps 1 and 2
/// Step 4: Apply safety invariants (can only tighten)
/// Step 5: Apply global override (can only tighten)
pub fn resolve_autonomy(
    action_type: &str,
    config: &AutonomyConfig,
    confidence: &ProposalConfidence,
) -> AutonomyLevel {
    // Step 1: Base level from hat bias or action default
    let base_level = config
        .hat_bias
        .get(action_type)
        .copied()
        .or_else(|| config.action_defaults.get(action_type).copied())
        .unwrap_or(AutonomyLevel::Approve);

    // Step 2: Confidence-derived level
    let conf_level = if confidence.sample_count >= config.auto_min_samples
        && confidence.effectiveness_rate >= config.auto_threshold
    {
        Some(AutonomyLevel::Auto)
    } else if confidence.sample_count >= config.notify_min_samples
        && confidence.effectiveness_rate >= config.notify_threshold
    {
        Some(AutonomyLevel::Notify)
    } else if confidence.sample_count > 0 {
        Some(AutonomyLevel::Approve)
    } else {
        None // No data, skip step 2
    };

    // Step 3: Merge — take more restrictive
    let mut level = match conf_level {
        Some(cl) => base_level.max(cl), // AutonomyLevel: Ord is least→most restrictive
        None => base_level,
    };

    // Step 4: Safety invariants (can only tighten)
    for invariant in &config.safety_invariants {
        if matches_action(&invariant.rule, action_type) {
            if let Some(enforced) = invariant.enforced_level {
                level = enforced;
            }
            if let Some(minimum) = invariant.minimum_level {
                level = level.max(minimum);
            }
        }
    }

    // Step 5: Global override (can only tighten)
    if let Some(ref override_policy) = config.global_override {
        if override_policy == "all_manual" && level < AutonomyLevel::Approve {
            level = AutonomyLevel::Approve;
        }
    }

    level
}

/// Check if a safety invariant rule matches an action type.
/// Rules like "destroy_always_blocked" match action type "destroy".
/// Rules like "deploy_never_auto" match action type "deploy".
fn matches_action(rule: &str, action_type: &str) -> bool {
    let normalized_action = action_type.replace('-', "_");
    rule.starts_with(&normalized_action)
}

/// Parse autonomy config from registry JSON.
pub fn parse_autonomy_config(
    autonomy_json: &Value,
    hat_name: Option<&str>,
    hats_json: &Value,
) -> AutonomyConfig {
    let mut action_defaults = std::collections::HashMap::new();
    let mut hat_bias = std::collections::HashMap::new();

    // Parse action type defaults
    if let Some(types) = autonomy_json.get("action_types").and_then(|v| v.as_object()) {
        for (action, config) in types {
            if let Some(level_str) = config.get("default_level").and_then(|v| v.as_str()) {
                if let Some(level) = parse_level(level_str) {
                    action_defaults.insert(action.clone(), level);
                }
            }
        }
    }

    // Parse hat autonomy bias
    if let Some(hat_name) = hat_name {
        if let Some(hat) = hats_json.get(hat_name).and_then(|v| v.as_object()) {
            if let Some(bias) = hat.get("autonomy_bias").and_then(|v| v.as_object()) {
                for (action, level_val) in bias {
                    if let Some(level_str) = level_val.as_str() {
                        if let Some(level) = parse_level(level_str) {
                            hat_bias.insert(action.clone(), level);
                        }
                    }
                }
            }
        }
    }

    // Parse safety invariants
    let safety_invariants = autonomy_json
        .get("safety_invariants")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default();

    // Parse effectiveness thresholds
    let thresholds = autonomy_json.get("effectiveness_thresholds");
    let auto_threshold = thresholds
        .and_then(|t| t.get("auto_threshold"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.8);
    let auto_min_samples = thresholds
        .and_then(|t| t.get("min_samples"))
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as u32;
    let notify_threshold = thresholds
        .and_then(|t| t.get("notify_threshold"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);

    AutonomyConfig {
        action_defaults,
        hat_bias,
        safety_invariants,
        auto_threshold,
        auto_min_samples,
        notify_threshold,
        notify_min_samples: auto_min_samples, // Same threshold used in PS
        global_override: None,
    }
}

fn parse_level(s: &str) -> Option<AutonomyLevel> {
    match s {
        "auto" => Some(AutonomyLevel::Auto),
        "notify" => Some(AutonomyLevel::Notify),
        "approve" => Some(AutonomyLevel::Approve),
        "blocked" => Some(AutonomyLevel::Blocked),
        _ => None,
    }
}

/// Compute recommendation priority (spec Section 5.3).
/// priority = tier_weight * downstream_multiplier
/// downstream_multiplier = 1.0 + (0.5 * blocks_count)
pub fn recommendation_priority(tier_weight: f64, blocks_count: usize) -> f64 {
    tier_weight * (1.0 + 0.5 * blocks_count as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> AutonomyConfig {
        let mut defaults = std::collections::HashMap::new();
        defaults.insert("clear-gate".to_string(), AutonomyLevel::Notify);
        defaults.insert("refresh-snapshot".to_string(), AutonomyLevel::Auto);
        defaults.insert("deploy".to_string(), AutonomyLevel::Approve);
        defaults.insert("destroy".to_string(), AutonomyLevel::Blocked);

        AutonomyConfig {
            action_defaults: defaults,
            hat_bias: std::collections::HashMap::new(),
            safety_invariants: vec![
                SafetyInvariant {
                    rule: "destroy_always_blocked".to_string(),
                    enforced_level: Some(AutonomyLevel::Blocked),
                    minimum_level: None,
                },
                SafetyInvariant {
                    rule: "deploy_never_auto".to_string(),
                    enforced_level: None,
                    minimum_level: Some(AutonomyLevel::Approve),
                },
            ],
            auto_threshold: 0.8,
            auto_min_samples: 3,
            notify_threshold: 0.5,
            notify_min_samples: 3,
            global_override: None,
        }
    }

    #[test]
    fn default_levels_used_without_hat() {
        let config = default_config();
        let no_data = ProposalConfidence::default();
        assert_eq!(resolve_autonomy("clear-gate", &config, &no_data), AutonomyLevel::Notify);
        assert_eq!(resolve_autonomy("deploy", &config, &no_data), AutonomyLevel::Approve);
    }

    #[test]
    fn destroy_always_blocked() {
        let config = default_config();
        // Even with high effectiveness, destroy stays blocked
        let high_conf = ProposalConfidence {
            effectiveness_rate: 1.0,
            sample_count: 100,
            success_count: 100,
        };
        assert_eq!(resolve_autonomy("destroy", &config, &high_conf), AutonomyLevel::Blocked);
    }

    #[test]
    fn deploy_never_auto() {
        let config = default_config();
        let high_conf = ProposalConfidence {
            effectiveness_rate: 1.0,
            sample_count: 100,
            success_count: 100,
        };
        // Deploy with high confidence should be approve (not auto)
        let result = resolve_autonomy("deploy", &config, &high_conf);
        assert!(result >= AutonomyLevel::Approve);
    }

    #[test]
    fn hat_bias_overrides_default() {
        let mut config = default_config();
        config.hat_bias.insert("clear-gate".to_string(), AutonomyLevel::Auto);
        let no_data = ProposalConfidence::default();
        assert_eq!(resolve_autonomy("clear-gate", &config, &no_data), AutonomyLevel::Auto);
    }

    #[test]
    fn high_effectiveness_lowers_to_auto() {
        let config = default_config();
        let high = ProposalConfidence {
            effectiveness_rate: 0.9,
            sample_count: 10,
            success_count: 9,
        };
        // clear-gate default is notify, but high confidence → auto
        // Step 3: max(auto, notify) = notify (notify is more restrictive)
        // Wait - AutonomyLevel ordering: Auto < Notify < Approve < Blocked
        // So max(Auto, Notify) = Notify
        assert_eq!(resolve_autonomy("refresh-snapshot", &config, &high), AutonomyLevel::Auto);
    }

    #[test]
    fn low_effectiveness_tightens_to_approve() {
        let config = default_config();
        let low = ProposalConfidence {
            effectiveness_rate: 0.3,
            sample_count: 10,
            success_count: 3,
        };
        // refresh-snapshot default is auto, but low confidence → approve
        // Step 3: max(auto, approve) = approve
        assert_eq!(resolve_autonomy("refresh-snapshot", &config, &low), AutonomyLevel::Approve);
    }

    #[test]
    fn global_override_all_manual() {
        let mut config = default_config();
        config.global_override = Some("all_manual".to_string());
        let no_data = ProposalConfidence::default();
        // refresh-snapshot would be auto, but all_manual tightens to approve
        assert_eq!(resolve_autonomy("refresh-snapshot", &config, &no_data), AutonomyLevel::Approve);
    }

    #[test]
    fn recommendation_priority_formula() {
        assert_eq!(recommendation_priority(4.0, 0), 4.0);
        assert_eq!(recommendation_priority(4.0, 2), 8.0); // 4.0 * (1.0 + 0.5 * 2)
        assert_eq!(recommendation_priority(3.0, 1), 4.5); // 3.0 * (1.0 + 0.5 * 1)
    }

    // -----------------------------------------------------------------------
    // build_domain_recommendations tests
    // -----------------------------------------------------------------------

    use crate::registry::{BrainConfig, DomainDefinition, ScoringSource};
    use crate::types::{Confidence, DomainScore, Score, Scorecard, Weight};
    use chrono::Utc;

    fn make_scorecard_with(domains: Vec<(&str, u8, f64)>) -> Scorecard {
        // domains: (key, effective_score, weight)
        let mut map = HashMap::new();
        for (key, score, weight) in domains {
            map.insert(
                key.to_string(),
                DomainScore {
                    domain: key.to_string(),
                    raw_score: Score::new(score as i64),
                    confidence: Confidence::full(),
                    effective_score: Score::new(score as i64),
                    weight: Weight::new(weight),
                    trajectory: None,
                },
            );
        }
        Scorecard {
            unified_score: Score::new(50),
            domains: map,
            scored_at: Utc::now(),
            floor_applied: None,
        }
    }

    fn make_traj(c: TrajectoryClassification, velocity: f64) -> TrajectoryResult {
        TrajectoryResult {
            velocity,
            acceleration: 0.0,
            classification: c,
            samples: 10,
        }
    }

    fn empty_registry() -> BrainConfig {
        BrainConfig {
            domain_weights: HashMap::new(),
            advisory_domains: vec![],
            principle_map: HashMap::new(),
            domain_definitions: HashMap::new(),
            scoring: Default::default(),
            gate_tiers: HashMap::new(),
            confidence_thresholds: Default::default(),
            staleness_thresholds: Default::default(),
            severity_thresholds: Default::default(),
            autonomy: serde_json::Value::Null,
            trajectory: Default::default(),
            attention_budget: Default::default(),
            personas: HashMap::new(),
            hats: HashMap::new(),
            correlations: vec![],
            incident_patterns: vec![],
            sensory_servers: HashMap::new(),
            extra: HashMap::new(),
        }
    }

    fn no_effectiveness() -> HashMap<String, ActionEffectiveness> {
        HashMap::new()
    }

    #[test]
    fn recommendations_empty_for_all_healthy_domains() {
        let scorecard = make_scorecard_with(vec![
            ("code-quality", 90, 0.35),
            ("test-health", 85, 0.35),
            ("deploy-readiness", 80, 0.30),
        ]);
        let trajs: HashMap<String, TrajectoryResult> = [
            ("code-quality".to_string(), make_traj(TrajectoryClassification::Stable, 0.0)),
            ("test-health".to_string(), make_traj(TrajectoryClassification::Improving, 2.0)),
            ("deploy-readiness".to_string(), make_traj(TrajectoryClassification::Stable, 0.0)),
        ].into();
        let recs = build_domain_recommendations(
            &scorecard, &trajs, &empty_registry(), &no_effectiveness(), 5,
        );
        assert!(recs.is_empty(), "healthy domains should produce no recommendations");
    }

    #[test]
    fn declining_domain_ranked_above_lower_stable_domain() {
        // degrading@65 vs stable@55 — degrading should rank first despite higher score
        let scorecard = make_scorecard_with(vec![
            ("a", 65, 0.5),  // degrading
            ("b", 55, 0.5),  // stable
        ]);
        let trajs: HashMap<String, TrajectoryResult> = [
            ("a".to_string(), make_traj(TrajectoryClassification::Degrading, -3.0)),
            ("b".to_string(), make_traj(TrajectoryClassification::Stable, 0.0)),
        ].into();
        let recs = build_domain_recommendations(
            &scorecard, &trajs, &empty_registry(), &no_effectiveness(), 5,
        );
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].domain, "a", "degrading domain should be ranked first");
        assert_eq!(recs[1].domain, "b");
    }

    #[test]
    fn improving_domain_deprioritized_below_stable() {
        // improving@60 should rank below stable@62
        let scorecard = make_scorecard_with(vec![
            ("improving", 60, 0.5),
            ("stable", 62, 0.5),
        ]);
        let trajs: HashMap<String, TrajectoryResult> = [
            ("improving".to_string(), make_traj(TrajectoryClassification::Improving, 3.0)),
            ("stable".to_string(), make_traj(TrajectoryClassification::Stable, 0.0)),
        ].into();
        let recs = build_domain_recommendations(
            &scorecard, &trajs, &empty_registry(), &no_effectiveness(), 5,
        );
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].domain, "stable", "stable@62 should rank above improving@60");
        assert_eq!(recs[1].domain, "improving");
    }

    #[test]
    fn max_count_limits_output() {
        let scorecard = make_scorecard_with(vec![
            ("a", 40, 0.2), ("b", 45, 0.2), ("c", 50, 0.2),
            ("d", 55, 0.2), ("e", 60, 0.1), ("f", 65, 0.1),
        ]);
        let recs = build_domain_recommendations(
            &scorecard, &HashMap::new(), &empty_registry(), &no_effectiveness(), 3,
        );
        assert_eq!(recs.len(), 3);
    }

    #[test]
    fn advisory_domains_excluded() {
        let scorecard = make_scorecard_with(vec![
            ("jira", 20, 0.0),   // advisory — must be excluded
            ("test-health", 50, 1.0),
        ]);
        let recs = build_domain_recommendations(
            &scorecard, &HashMap::new(), &empty_registry(), &no_effectiveness(), 5,
        );
        assert!(!recs.iter().any(|r| r.domain == "jira"), "advisory domain must be excluded");
        assert_eq!(recs.len(), 1);
    }

    #[test]
    fn command_derived_for_builtin_tools() {
        let scorecard = make_scorecard_with(vec![("test-health", 30, 1.0)]);
        let recs = build_domain_recommendations(
            &scorecard, &HashMap::new(), &empty_registry(), &no_effectiveness(), 5,
        );
        assert_eq!(recs.len(), 1);
        assert!(
            recs[0].command.contains("motherbrain sensory test-health"),
            "command should reference motherbrain sensory for built-in tools"
        );
    }

    #[test]
    fn command_fallback_for_custom_domain() {
        let scorecard = make_scorecard_with(vec![("my-custom", 30, 1.0)]);
        let recs = build_domain_recommendations(
            &scorecard, &HashMap::new(), &empty_registry(), &no_effectiveness(), 5,
        );
        assert_eq!(recs.len(), 1);
        assert!(recs[0].command.contains("my-custom"), "fallback command should name the domain");
    }

    #[test]
    fn description_field_populated() {
        let scorecard = make_scorecard_with(vec![("test-health", 30, 1.0)]);
        let recs = build_domain_recommendations(
            &scorecard, &HashMap::new(), &empty_registry(), &no_effectiveness(), 5,
        );
        assert!(recs[0].description.is_some());
        let desc = recs[0].description.as_ref().unwrap();
        assert!(desc.contains("test-health"), "description must mention domain");
        assert!(desc.contains("30"), "description must include score");
    }

    #[test]
    fn degrading_healthy_domain_included_with_declining_status() {
        // A domain at 80 (green) that is degrading should still appear
        let scorecard = make_scorecard_with(vec![("code-quality", 80, 1.0)]);
        let trajs: HashMap<String, TrajectoryResult> = [(
            "code-quality".to_string(),
            make_traj(TrajectoryClassification::Degrading, -4.0),
        )].into();
        let recs = build_domain_recommendations(
            &scorecard, &trajs, &empty_registry(), &no_effectiveness(), 5,
        );
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].status, "declining");
    }

    #[test]
    fn gate_is_synthetic_identifier() {
        let scorecard = make_scorecard_with(vec![("test-health", 40, 1.0)]);
        let recs = build_domain_recommendations(
            &scorecard, &HashMap::new(), &empty_registry(), &no_effectiveness(), 5,
        );
        assert_eq!(recs[0].gate, "test-health-score-improvement");
    }
}
