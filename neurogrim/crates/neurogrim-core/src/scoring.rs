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

/// Compute the unified confidence — weighted mean of per-domain
/// confidence across scored (non-advisory) domains (E-B2-1, spec §6.X).
///
/// Formula: `round(sum(d.confidence * d.weight) / sum(d.weight))`.
/// Mirrors how `unified_score` aggregates effective scores; the only
/// shape difference is normalization-by-weight-sum (because we want a
/// mean, not a weighted contribution).
///
/// Advisory domains (weight = 0.0) are excluded from both numerator
/// and denominator. Empty scored set OR zero total weight → 0.
///
/// Receivers SHOULD use the resulting `unified_confidence` for
/// peer-to-peer trust decisions: a peer with score=85 but
/// unified_confidence=20 is a low-quality signal regardless of the
/// score itself.
pub fn unified_confidence(domain_scores: &HashMap<String, DomainScore>) -> Confidence {
    let scored: Vec<&DomainScore> = domain_scores
        .values()
        .filter(|ds| !ds.weight.is_advisory())
        .collect();

    if scored.is_empty() {
        return Confidence::zero();
    }

    let weight_sum: f64 = scored.iter().map(|ds| ds.weight.value()).sum();
    if weight_sum <= 0.0 {
        return Confidence::zero();
    }

    let weighted_conf_sum: f64 = scored
        .iter()
        .map(|ds| ds.confidence.value() as f64 * ds.weight.value())
        .sum();

    Confidence::new((weighted_conf_sum / weight_sum).round())
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

/// Resolve confidence for a domain: prefer envelope-supplied confidence
/// (when the sensor recorded an explicit value in the CMDB) over age-decay
/// computation from `updated_at`. Part of E-B2-1's reader-fallback design
/// (spec §3.8 + §6.X) — when a sensor knows its own freshness signal
/// independent of clock-skew, the aggregator trusts it. When absent, the
/// aggregator falls back to the existing exponential-decay model.
///
/// Out-of-range envelope values (>100) are clamped via `Confidence::new`.
pub fn resolve_confidence(
    envelope_confidence: Option<u8>,
    updated_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    config: &ConfidenceConfig,
) -> Confidence {
    match envelope_confidence {
        Some(c) => Confidence::new(c as f64),
        None => exponential_decay(updated_at, now, config),
    }
}

/// Build a complete scorecard from registry configuration and CMDB data.
///
/// This is the main orchestration function that combines:
/// 1. Domain score retrieval from CMDB data
/// 2. Confidence resolution (envelope-supplied OR age-decay fallback)
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

        let (raw, updated_at, envelope_confidence) = match cmdb_data.get(domain_key) {
            Some(cmdb) => (
                Score::new(cmdb.score as i64),
                Some(cmdb.updated_at),
                cmdb.confidence,
            ),
            None => {
                // No CMDB data — use no_file_score from definition, or 0
                let no_file_score = registry
                    .config
                    .domain_definitions
                    .get(domain_key)
                    .and_then(|def| def.scoring_source.as_ref())
                    .map(|src| src.no_file_score.unwrap_or(0))
                    .unwrap_or(0);
                (Score::new(no_file_score as i64), None, None)
            }
        };

        let confidence =
            resolve_confidence(envelope_confidence, updated_at, now, &confidence_config);
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
///
/// `confidence` is `Some(n)` when the CMDB envelope carries an explicit
/// `confidence` field (sensor-supplied per spec §3.8); `None` when the
/// envelope omits it (aggregator falls back to age-decay of `updated_at`).
#[derive(Debug, Clone)]
pub struct CmdbData {
    pub score: u8,
    pub updated_at: DateTime<Utc>,
    pub confidence: Option<u8>,
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

    fn test_confidence_config() -> ConfidenceConfig {
        ConfidenceConfig {
            fresh_days: 1.0,
            stale_days: 3.0,
            very_stale_days: 7.0,
        }
    }

    #[test]
    fn resolve_confidence_prefers_envelope_when_some() {
        // Envelope says 50; updated_at is fresh (would yield ~100 from decay).
        // resolve_confidence MUST honor the envelope.
        let now = Utc::now();
        let fresh = now;
        let result = resolve_confidence(Some(50), Some(fresh), now, &test_confidence_config());
        assert_eq!(result.value(), 50);
    }

    #[test]
    fn resolve_confidence_envelope_zero_is_honored() {
        // Sensor explicitly recorded zero confidence — must NOT be treated
        // as "absent" and fall back to age-decay.
        let now = Utc::now();
        let fresh = now;
        let result = resolve_confidence(Some(0), Some(fresh), now, &test_confidence_config());
        assert_eq!(result.value(), 0);
    }

    #[test]
    fn resolve_confidence_falls_back_to_decay_when_none() {
        // No envelope confidence; fresh updated_at; decay yields ~100.
        let now = Utc::now();
        let fresh = now;
        let result = resolve_confidence(None, Some(fresh), now, &test_confidence_config());
        // Fresh timestamp + None envelope → exponential_decay returns ~100.
        assert!(
            result.value() >= 99,
            "expected fresh decay ~100, got {}",
            result.value()
        );
    }

    #[test]
    fn resolve_confidence_falls_back_when_no_updated_at_either() {
        // No envelope; no updated_at; exponential_decay returns 0
        // (distinguishes "missing" from "stale").
        let now = Utc::now();
        let result = resolve_confidence(None, None, now, &test_confidence_config());
        assert_eq!(result.value(), 0);
    }

    #[test]
    fn resolve_confidence_envelope_clamps_above_100() {
        // Defensive: even if a buggy sensor wrote 200, Confidence::new
        // clamps to 100. Schema validation should also catch this.
        let now = Utc::now();
        let result = resolve_confidence(Some(200), Some(now), now, &test_confidence_config());
        assert_eq!(result.value(), 100);
    }

    #[test]
    fn unified_confidence_weighted_mean_three_domains() {
        // E-B2-1 C6: weighted-mean across three scored domains.
        // domain A: confidence=80, weight=0.5
        // domain B: confidence=60, weight=0.3
        // domain C: confidence=40, weight=0.2
        // weighted_sum = 80*0.5 + 60*0.3 + 40*0.2 = 40 + 18 + 8 = 66
        // weight_sum = 1.0 → unified = round(66 / 1.0) = 66
        let mut domains = HashMap::new();
        domains.insert(
            "a".to_string(),
            DomainScore {
                domain: "a".to_string(),
                raw_score: Score::new(85),
                confidence: Confidence::new(80.0),
                effective_score: Score::new(85),
                weight: Weight::new(0.5),
                trajectory: None,
            },
        );
        domains.insert(
            "b".to_string(),
            DomainScore {
                domain: "b".to_string(),
                raw_score: Score::new(70),
                confidence: Confidence::new(60.0),
                effective_score: Score::new(70),
                weight: Weight::new(0.3),
                trajectory: None,
            },
        );
        domains.insert(
            "c".to_string(),
            DomainScore {
                domain: "c".to_string(),
                raw_score: Score::new(50),
                confidence: Confidence::new(40.0),
                effective_score: Score::new(50),
                weight: Weight::new(0.2),
                trajectory: None,
            },
        );
        let result = unified_confidence(&domains);
        assert_eq!(result.value(), 66);
    }

    #[test]
    fn unified_confidence_empty_domains_returns_zero() {
        // No domains → no signal to aggregate → 0 (honest unknown).
        let domains: HashMap<String, DomainScore> = HashMap::new();
        let result = unified_confidence(&domains);
        assert_eq!(result.value(), 0);
    }

    #[test]
    fn unified_confidence_only_advisory_domains_returns_zero() {
        // All-advisory (weight=0.0) registries have nothing to weight by.
        // Spec §16.3 supply-chain campaign declared all three sensors at
        // weight 0.0 for v1; this case must return 0 (matches unified_score
        // semantics — advisory-only Brains have unified_score = 0 too).
        let mut domains = HashMap::new();
        domains.insert(
            "advisory-only".to_string(),
            DomainScore {
                domain: "advisory-only".to_string(),
                raw_score: Score::new(75),
                confidence: Confidence::new(85.0),
                effective_score: Score::new(75),
                weight: Weight::new(0.0),
                trajectory: None,
            },
        );
        let result = unified_confidence(&domains);
        assert_eq!(result.value(), 0);
    }

    #[test]
    fn unified_confidence_excludes_advisory_in_aggregate() {
        // Mixed scored + advisory: only the scored domain contributes.
        // Without the filter, naive mean = (90+10)/2 = 50; with the
        // filter, only the scored domain (90) counts → 90.
        let mut domains = HashMap::new();
        domains.insert(
            "scored".to_string(),
            DomainScore {
                domain: "scored".to_string(),
                raw_score: Score::new(80),
                confidence: Confidence::new(90.0),
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
                confidence: Confidence::new(10.0),
                effective_score: Score::new(10),
                weight: Weight::new(0.0),
                trajectory: None,
            },
        );
        let result = unified_confidence(&domains);
        assert_eq!(result.value(), 90);
    }
}
