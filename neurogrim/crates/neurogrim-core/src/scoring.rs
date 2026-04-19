use crate::confidence::{exponential_decay, ConfidenceConfig};
use crate::registry::{BrainRegistry, DomainDefinition};
use crate::types::*;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Compute the effective score for a domain (spec Section 4.5).
///
/// Multiplier model: effective = floor(raw * confidence / 100)
/// Floor model: effective = min(raw, ceiling) when confidence < threshold, else raw
pub fn effective_score(
    raw: Score,
    confidence: Confidence,
    model: ScoringModel,
    floor_threshold: u8,
    floor_ceiling: u8,
) -> Score {
    match model {
        ScoringModel::Multiplier => {
            let value = (raw.value() as f64 * confidence.value() as f64 / 100.0).floor() as i64;
            Score::new(value)
        }
        ScoringModel::Floor => {
            if confidence.value() < floor_threshold {
                Score::new(raw.value().min(floor_ceiling) as i64)
            } else {
                raw
            }
        }
    }
}

/// Compute the unified score from domain effective scores and weights (spec Section 4.6).
///
/// unified = floor(clamp(0, 100, sum(effective_score[d] * weight[d] for d in scored_domains)))
/// Advisory domains (weight = 0.0) are excluded.
pub fn unified_score(domain_scores: &HashMap<String, DomainScore>) -> Score {
    let weighted_sum: f64 = domain_scores
        .values()
        .filter(|ds| !ds.weight.is_advisory())
        .map(|ds| ds.effective_score.value() as f64 * ds.weight.value())
        .sum();

    Score::new(weighted_sum.floor() as i64)
}

/// Apply domain floor constraints (spec Section 4.6).
///
/// If a domain's effective score falls below its floor min_score, the unified score
/// is capped at the floor's unified_cap. Most restrictive cap wins.
pub fn apply_floor_constraints(
    unified: Score,
    domain_scores: &HashMap<String, DomainScore>,
    definitions: &HashMap<String, DomainDefinition>,
) -> (Score, Option<FloorApplication>) {
    let mut most_restrictive_cap: Option<FloorApplication> = None;

    for (domain_key, ds) in domain_scores {
        if let Some(def) = definitions.get(domain_key) {
            if let Some(ref floor) = def.floor {
                if ds.effective_score.value() < floor.min_score {
                    let should_replace = match &most_restrictive_cap {
                        None => true,
                        Some(existing) => floor.unified_cap < existing.unified_cap,
                    };
                    if should_replace {
                        most_restrictive_cap = Some(FloorApplication {
                            domain: domain_key.clone(),
                            domain_score: ds.effective_score.value(),
                            min_score: floor.min_score,
                            unified_cap: floor.unified_cap,
                            message: floor.message.clone().unwrap_or_default(),
                        });
                    }
                }
            }
        }
    }

    match most_restrictive_cap {
        Some(ref floor_app) => {
            let capped = Score::new(unified.value().min(floor_app.unified_cap) as i64);
            (capped, Some(floor_app.clone()))
        }
        None => (unified, None),
    }
}

/// Build a complete scorecard from registry configuration and CMDB data.
///
/// This is the main orchestration function that combines:
/// 1. Domain score retrieval from CMDB data
/// 2. Confidence decay computation
/// 3. Effective score calculation
/// 4. Unified score with floor constraints
pub fn build_scorecard(
    registry: &BrainRegistry,
    cmdb_data: &HashMap<String, CmdbData>,
    now: DateTime<Utc>,
) -> Scorecard {
    let confidence_config = ConfidenceConfig {
        fresh_days: registry.config.confidence_thresholds.cmdb_fresh_days,
        stale_days: registry.config.confidence_thresholds.cmdb_stale_days,
        very_stale_days: registry.config.confidence_thresholds.cmdb_very_stale_days,
    };

    let scoring_model = registry.config.scoring.model;
    let floor_threshold = registry.config.scoring.floor_confidence_threshold;
    let floor_ceiling = registry.config.scoring.floor_score_ceiling;

    let mut domain_scores = HashMap::new();

    for (domain_key, weight_value) in &registry.config.domain_weights {
        let weight = Weight::new(*weight_value);

        let (raw, updated_at) = match cmdb_data.get(domain_key) {
            Some(cmdb) => (Score::new(cmdb.score as i64), Some(cmdb.updated_at)),
            None => {
                // No CMDB data — use no_file_score from definition, or 0
                let no_file_score = registry
                    .config
                    .domain_definitions
                    .get(domain_key)
                    .and_then(|def| def.scoring_source.as_ref())
                    .map(|src| src.no_file_score.unwrap_or(0))
                    .unwrap_or(0);
                (Score::new(no_file_score as i64), None)
            }
        };

        let confidence = exponential_decay(updated_at, now, &confidence_config);
        let eff = effective_score(
            raw,
            confidence,
            scoring_model,
            floor_threshold,
            floor_ceiling,
        );

        domain_scores.insert(
            domain_key.clone(),
            DomainScore {
                domain: domain_key.clone(),
                raw_score: raw,
                confidence,
                effective_score: eff,
                weight,
                trajectory: None, // Populated separately by trajectory module
            },
        );
    }

    let unified = unified_score(&domain_scores);
    let (final_score, floor_applied) =
        apply_floor_constraints(unified, &domain_scores, &registry.config.domain_definitions);

    Scorecard {
        unified_score: final_score,
        domains: domain_scores,
        scored_at: now,
        floor_applied,
    }
}

/// Parsed CMDB data for scoring input.
#[derive(Debug, Clone)]
pub struct CmdbData {
    pub score: u8,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_score_multiplier_model() {
        // raw=80, confidence=75 → floor(80*75/100) = floor(60.0) = 60
        let result = effective_score(
            Score::new(80),
            Confidence::new(75.0),
            ScoringModel::Multiplier,
            30,
            30,
        );
        assert_eq!(result.value(), 60);
    }

    #[test]
    fn effective_score_multiplier_full_confidence() {
        let result = effective_score(
            Score::new(85),
            Confidence::full(),
            ScoringModel::Multiplier,
            30,
            30,
        );
        assert_eq!(result.value(), 85);
    }

    #[test]
    fn effective_score_multiplier_zero_confidence() {
        let result = effective_score(
            Score::new(100),
            Confidence::zero(),
            ScoringModel::Multiplier,
            30,
            30,
        );
        assert_eq!(result.value(), 0);
    }

    #[test]
    fn effective_score_floor_model_below_threshold() {
        // confidence=20 < threshold=30 → min(raw=80, ceiling=30) = 30
        let result = effective_score(
            Score::new(80),
            Confidence::new(20.0),
            ScoringModel::Floor,
            30,
            30,
        );
        assert_eq!(result.value(), 30);
    }

    #[test]
    fn effective_score_floor_model_above_threshold() {
        // confidence=50 >= threshold=30 → raw=80
        let result = effective_score(
            Score::new(80),
            Confidence::new(50.0),
            ScoringModel::Floor,
            30,
            30,
        );
        assert_eq!(result.value(), 80);
    }

    #[test]
    fn unified_score_weighted_sum() {
        let mut domains = HashMap::new();
        domains.insert(
            "code-quality".to_string(),
            DomainScore {
                domain: "code-quality".to_string(),
                raw_score: Score::new(70),
                confidence: Confidence::full(),
                effective_score: Score::new(70),
                weight: Weight::new(0.35),
                trajectory: None,
            },
        );
        domains.insert(
            "test-health".to_string(),
            DomainScore {
                domain: "test-health".to_string(),
                raw_score: Score::new(50),
                confidence: Confidence::full(),
                effective_score: Score::new(50),
                weight: Weight::new(0.35),
                trajectory: None,
            },
        );
        domains.insert(
            "deploy-readiness".to_string(),
            DomainScore {
                domain: "deploy-readiness".to_string(),
                raw_score: Score::new(80),
                confidence: Confidence::full(),
                effective_score: Score::new(80),
                weight: Weight::new(0.30),
                trajectory: None,
            },
        );

        let result = unified_score(&domains);
        // 70*0.35 + 50*0.35 + 80*0.30 = 24.5 + 17.5 + 24.0 = 66.0
        assert_eq!(result.value(), 66);
    }

    #[test]
    fn unified_score_excludes_advisory_domains() {
        let mut domains = HashMap::new();
        domains.insert(
            "scored".to_string(),
            DomainScore {
                domain: "scored".to_string(),
                raw_score: Score::new(80),
                confidence: Confidence::full(),
                effective_score: Score::new(80),
                weight: Weight::new(1.0),
                trajectory: None,
            },
        );
        domains.insert(
            "advisory".to_string(),
            DomainScore {
                domain: "advisory".to_string(),
                raw_score: Score::new(10),
                confidence: Confidence::full(),
                effective_score: Score::new(10),
                weight: Weight::new(0.0), // Advisory
                trajectory: None,
            },
        );

        let result = unified_score(&domains);
        assert_eq!(result.value(), 80); // Advisory domain excluded
    }

    #[test]
    fn score_clamping() {
        assert_eq!(Score::new(150).value(), 100);
        assert_eq!(Score::new(-10).value(), 0);
        assert_eq!(Score::new(50).value(), 50);
    }
}
