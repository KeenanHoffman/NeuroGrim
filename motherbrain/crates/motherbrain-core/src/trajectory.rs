//! Trajectory intelligence (spec Section 7).
//!
//! Computes velocity, acceleration, and classification from score history.
//! Uses RAW scores (not confidence-weighted) to prevent phantom trends.

use crate::registry::TrajectoryConfig;
use crate::types::{ScoreSnapshot, TrajectoryClassification, TrajectoryResult};
use std::collections::HashMap;

/// Compute trajectory from score history.
///
/// For per-domain trajectory, pass `Some(domain_name)`.
/// For unified trajectory, pass `None`.
pub fn compute_trajectory(
    history: &[ScoreSnapshot],
    config: &TrajectoryConfig,
    domain: Option<&str>,
    domain_weights: &HashMap<String, f64>,
) -> TrajectoryResult {
    let sample_count = history.len();

    if sample_count < config.min_samples_for_trend {
        return TrajectoryResult {
            velocity: 0.0,
            acceleration: 0.0,
            classification: TrajectoryClassification::NoData,
            samples: sample_count,
        };
    }

    // Extract raw scores
    let scores: Vec<f64> = history
        .iter()
        .map(|snap| extract_raw_score(snap, domain, domain_weights))
        .collect();

    let velocity = compute_velocity(&scores, config.velocity_window);
    let acceleration = compute_acceleration(&scores, config.velocity_window);
    let classification = classify(
        velocity,
        &scores,
        config.velocity_window,
        &config.classification_thresholds,
    );

    TrajectoryResult {
        velocity: round2(velocity),
        acceleration: round2(acceleration),
        classification,
        samples: sample_count,
    }
}

/// Extract raw score from a snapshot.
/// For domain-specific: use the domain's raw score.
/// For unified: compute weighted average of raw domain scores.
fn extract_raw_score(
    snap: &ScoreSnapshot,
    domain: Option<&str>,
    domain_weights: &HashMap<String, f64>,
) -> f64 {
    match domain {
        Some(d) => snap
            .domains
            .get(d)
            .map(|ds| ds.score as f64)
            .unwrap_or(0.0),
        None => {
            // Weighted average of raw domain scores
            let mut raw_sum = 0.0;
            let mut weight_sum = 0.0;
            for (key, weight) in domain_weights {
                if *weight <= 0.0 {
                    continue;
                }
                if let Some(ds) = snap.domains.get(key) {
                    raw_sum += ds.score as f64 * weight;
                    weight_sum += weight;
                }
            }
            if weight_sum > 0.0 {
                raw_sum / weight_sum
            } else {
                snap.score as f64
            }
        }
    }
}

/// Compute velocity: avg(last N) - avg(previous N).
/// N = min(velocity_window, floor(count / 2)), minimum 1.
fn compute_velocity(scores: &[f64], velocity_window: usize) -> f64 {
    let count = scores.len();
    let n = velocity_window.min(count / 2).max(1);

    let recent = &scores[count - n..];
    let previous = &scores[count - 2 * n..count - n];

    avg(recent) - avg(previous)
}

/// Compute acceleration: current_velocity - previous_velocity.
/// Requires 3*N samples; returns 0 otherwise.
fn compute_acceleration(scores: &[f64], velocity_window: usize) -> f64 {
    let count = scores.len();
    let n = velocity_window.min(count / 2).max(1);

    if count < 3 * n {
        return 0.0;
    }

    let recent = &scores[count - n..];
    let previous = &scores[count - 2 * n..count - n];
    let older = &scores[count - 3 * n..count - 2 * n];

    let current_velocity = avg(recent) - avg(previous);
    let prev_velocity = avg(previous) - avg(older);

    current_velocity - prev_velocity
}

/// Classify the trajectory (spec Section 7.5).
/// Evaluated in order: volatile → improving → degrading → stable.
fn classify(
    velocity: f64,
    scores: &[f64],
    velocity_window: usize,
    thresholds: &crate::registry::ClassificationThresholds,
) -> TrajectoryClassification {
    let count = scores.len();
    let n = velocity_window.min(count / 2).max(1);
    let window_start = count.saturating_sub(2 * n);
    let window = &scores[window_start..];

    let stddev = stddev(window);

    if stddev >= thresholds.volatile_stddev {
        TrajectoryClassification::Volatile
    } else if velocity >= thresholds.improving {
        TrajectoryClassification::Improving
    } else if velocity <= thresholds.degrading {
        TrajectoryClassification::Degrading
    } else {
        TrajectoryClassification::Stable
    }
}

fn avg(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn stddev(values: &[f64]) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    let mean = avg(values);
    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SnapshotDomain;
    use chrono::Utc;

    fn default_config() -> TrajectoryConfig {
        TrajectoryConfig::default()
    }

    fn default_weights() -> HashMap<String, f64> {
        let mut w = HashMap::new();
        w.insert("a".to_string(), 0.5);
        w.insert("b".to_string(), 0.5);
        w
    }

    fn make_snapshot(score: u8) -> ScoreSnapshot {
        ScoreSnapshot {
            scored_at: Utc::now(),
            score,
            domains: HashMap::new(),
            hat: None,
        }
    }

    fn make_domain_snapshot(domain: &str, score: u8) -> ScoreSnapshot {
        let mut domains = HashMap::new();
        domains.insert(
            domain.to_string(),
            SnapshotDomain {
                score,
                confidence: 100,
            },
        );
        ScoreSnapshot {
            scored_at: Utc::now(),
            score,
            domains,
            hat: None,
        }
    }

    #[test]
    fn no_data_with_few_samples() {
        let history: Vec<ScoreSnapshot> = (0..3).map(|i| make_snapshot(50 + i)).collect();
        let result = compute_trajectory(&history, &default_config(), None, &default_weights());
        assert_eq!(result.classification, TrajectoryClassification::NoData);
        assert_eq!(result.samples, 3);
    }

    #[test]
    fn stable_with_constant_scores() {
        let history: Vec<ScoreSnapshot> = (0..10).map(|_| make_snapshot(70)).collect();
        let result = compute_trajectory(&history, &default_config(), None, &default_weights());
        assert_eq!(result.classification, TrajectoryClassification::Stable);
        assert_eq!(result.velocity, 0.0);
        assert_eq!(result.acceleration, 0.0);
    }

    #[test]
    fn improving_with_rising_scores() {
        // Use small increments so stddev stays below volatile_stddev (10)
        // Window of 2N=10 scores: stddev of [60..69] ≈ 3.0
        let history: Vec<ScoreSnapshot> = vec![
            make_snapshot(60), make_snapshot(61), make_snapshot(62),
            make_snapshot(63), make_snapshot(64), make_snapshot(65),
            make_snapshot(66), make_snapshot(67), make_snapshot(68),
            make_snapshot(69),
        ];
        let result = compute_trajectory(&history, &default_config(), None, &default_weights());
        assert_eq!(result.classification, TrajectoryClassification::Improving);
        assert!(result.velocity > 0.0);
    }

    #[test]
    fn degrading_with_falling_scores() {
        let history: Vec<ScoreSnapshot> = vec![
            make_snapshot(69), make_snapshot(68), make_snapshot(67),
            make_snapshot(66), make_snapshot(65), make_snapshot(64),
            make_snapshot(63), make_snapshot(62), make_snapshot(61),
            make_snapshot(60),
        ];
        let result = compute_trajectory(&history, &default_config(), None, &default_weights());
        assert_eq!(result.classification, TrajectoryClassification::Degrading);
        assert!(result.velocity < 0.0);
    }

    #[test]
    fn volatile_with_oscillating_scores() {
        let history: Vec<ScoreSnapshot> = vec![
            make_snapshot(30),
            make_snapshot(80),
            make_snapshot(20),
            make_snapshot(90),
            make_snapshot(25),
            make_snapshot(85),
            make_snapshot(30),
            make_snapshot(80),
            make_snapshot(20),
            make_snapshot(90),
        ];
        let result = compute_trajectory(&history, &default_config(), None, &default_weights());
        assert_eq!(result.classification, TrajectoryClassification::Volatile);
    }

    #[test]
    fn per_domain_trajectory() {
        let history: Vec<ScoreSnapshot> = vec![
            make_domain_snapshot("a", 60),
            make_domain_snapshot("a", 61),
            make_domain_snapshot("a", 62),
            make_domain_snapshot("a", 65),
            make_domain_snapshot("a", 67),
            make_domain_snapshot("a", 69),
        ];
        let result = compute_trajectory(
            &history,
            &default_config(),
            Some("a"),
            &default_weights(),
        );
        assert_eq!(result.classification, TrajectoryClassification::Improving);
        assert!(result.velocity > 0.0);
    }

    #[test]
    fn velocity_formula_exact() {
        // 10 samples: [10,20,30,40,50,60,70,80,90,100]
        // N = min(5, 10/2) = 5
        // recent = [60,70,80,90,100] avg = 80
        // previous = [10,20,30,40,50] avg = 30
        // velocity = 80 - 30 = 50
        let history: Vec<ScoreSnapshot> = (1..=10).map(|i| make_snapshot(i * 10)).collect();
        let result = compute_trajectory(&history, &default_config(), None, &default_weights());
        assert_eq!(result.velocity, 50.0);
    }

    #[test]
    fn acceleration_formula_exact() {
        // 15 samples in 3 groups of 5:
        // older [10,20,30,40,50] avg=30
        // previous [50,50,50,50,50] avg=50
        // recent [50,60,70,80,90] avg=70
        // prev_velocity = 50 - 30 = 20
        // current_velocity = 70 - 50 = 20
        // acceleration = 20 - 20 = 0
        let history: Vec<ScoreSnapshot> = vec![
            make_snapshot(10), make_snapshot(20), make_snapshot(30), make_snapshot(40), make_snapshot(50),
            make_snapshot(50), make_snapshot(50), make_snapshot(50), make_snapshot(50), make_snapshot(50),
            make_snapshot(50), make_snapshot(60), make_snapshot(70), make_snapshot(80), make_snapshot(90),
        ];
        let result = compute_trajectory(&history, &default_config(), None, &default_weights());
        assert_eq!(result.acceleration, 0.0);
    }

    #[test]
    fn acceleration_positive_when_speeding_up() {
        // older: [10,10,10,10,10] avg=10
        // previous: [20,20,20,20,20] avg=20, prev_vel=10
        // recent: [50,50,50,50,50] avg=50, cur_vel=30
        // acceleration = 30 - 10 = 20
        let history: Vec<ScoreSnapshot> = vec![
            make_snapshot(10), make_snapshot(10), make_snapshot(10), make_snapshot(10), make_snapshot(10),
            make_snapshot(20), make_snapshot(20), make_snapshot(20), make_snapshot(20), make_snapshot(20),
            make_snapshot(50), make_snapshot(50), make_snapshot(50), make_snapshot(50), make_snapshot(50),
        ];
        let result = compute_trajectory(&history, &default_config(), None, &default_weights());
        assert_eq!(result.acceleration, 20.0);
        assert_eq!(result.velocity, 30.0);
    }
}
