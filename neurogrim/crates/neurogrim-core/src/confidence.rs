use crate::types::Confidence;
use chrono::{DateTime, Utc};

/// Configuration for confidence decay calculation.
#[derive(Debug, Clone)]
pub struct ConfidenceConfig {
    /// Age in days below which data is fresh (default: 1.0).
    pub fresh_days: f64,
    /// Age in days at which data is stale (default: 3.0).
    pub stale_days: f64,
    /// Age in days at which data is very stale. Anchor for lambda (default: 7.0).
    pub very_stale_days: f64,
}

impl Default for ConfidenceConfig {
    fn default() -> Self {
        ConfidenceConfig {
            fresh_days: 1.0,
            stale_days: 3.0,
            very_stale_days: 7.0,
        }
    }
}

/// Compute confidence using continuous exponential decay (spec Section 4.4).
///
/// Formula: confidence = round(100 * e^(-lambda * age_days))
/// Where:  lambda = ln(4) / very_stale_days
///
/// This produces:
/// - Age 0: confidence 100
/// - Age very_stale_days: confidence 25
/// - Missing data: confidence 0
pub fn exponential_decay(
    updated_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    config: &ConfidenceConfig,
) -> Confidence {
    let updated_at = match updated_at {
        Some(ts) => ts,
        None => return Confidence::zero(), // Missing data = 0 confidence
    };

    let age_days = (now - updated_at).num_milliseconds() as f64 / (1000.0 * 60.0 * 60.0 * 24.0);

    if age_days < 0.0 {
        // Future timestamp — treat as fresh
        return Confidence::full();
    }

    let lambda = (4.0_f64).ln() / config.very_stale_days;
    let confidence = 100.0 * (-lambda * age_days).exp();

    // Minimum confidence for present data is 1 (to distinguish stale from missing)
    let clamped = confidence.round().max(1.0).min(100.0);
    Confidence::new(clamped)
}

/// Compute freshness multiplier for ecosystem aggregation (spec Section 4.8).
///
/// Step function:
/// - <= 1 day: 1.0
/// - <= 3 days: 0.75
/// - <= 7 days: 0.5
/// - > 7 days: 0.25
pub fn freshness_multiplier(updated_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> f64 {
    let updated_at = match updated_at {
        Some(ts) => ts,
        None => return 0.25,
    };

    let age_days = (now - updated_at).num_milliseconds() as f64 / (1000.0 * 60.0 * 60.0 * 24.0);

    if age_days <= 1.0 {
        1.0
    } else if age_days <= 3.0 {
        0.75
    } else if age_days <= 7.0 {
        0.5
    } else {
        0.25
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn config() -> ConfidenceConfig {
        ConfidenceConfig::default()
    }

    #[test]
    fn missing_data_returns_zero() {
        let now = Utc::now();
        assert_eq!(exponential_decay(None, now, &config()).value(), 0);
    }

    #[test]
    fn fresh_data_returns_100() {
        let now = Utc::now();
        assert_eq!(exponential_decay(Some(now), now, &config()).value(), 100);
    }

    #[test]
    fn one_day_old_returns_approximately_82() {
        let now = Utc::now();
        let one_day_ago = now - Duration::days(1);
        let conf = exponential_decay(Some(one_day_ago), now, &config());
        // Spec says ~82 at 1 day with default config
        assert!(
            (conf.value() as i16 - 82).abs() <= 1,
            "Expected ~82, got {}",
            conf.value()
        );
    }

    #[test]
    fn three_days_old_returns_approximately_55() {
        let now = Utc::now();
        let three_days_ago = now - Duration::days(3);
        let conf = exponential_decay(Some(three_days_ago), now, &config());
        assert!(
            (conf.value() as i16 - 55).abs() <= 1,
            "Expected ~55, got {}",
            conf.value()
        );
    }

    #[test]
    fn seven_days_old_returns_25() {
        let now = Utc::now();
        let seven_days_ago = now - Duration::days(7);
        let conf = exponential_decay(Some(seven_days_ago), now, &config());
        assert_eq!(conf.value(), 25);
    }

    #[test]
    fn fourteen_days_old_returns_low_but_nonzero() {
        let now = Utc::now();
        let fourteen_days_ago = now - Duration::days(14);
        let conf = exponential_decay(Some(fourteen_days_ago), now, &config());
        // At 14 days: confidence = 100 * e^(-ln(4)/7 * 14) = 100 * e^(-2*ln(4)) = 100/16 ≈ 6
        assert!(
            conf.value() >= 1 && conf.value() <= 10,
            "Expected ~6, got {}",
            conf.value()
        );
    }

    #[test]
    fn very_old_data_returns_minimum_1() {
        let now = Utc::now();
        let ancient = now - Duration::days(365);
        let conf = exponential_decay(Some(ancient), now, &config());
        // Present data always gets at least 1
        assert_eq!(conf.value(), 1);
    }

    #[test]
    fn future_timestamp_returns_100() {
        let now = Utc::now();
        let future = now + Duration::hours(1);
        assert_eq!(exponential_decay(Some(future), now, &config()).value(), 100);
    }

    #[test]
    fn half_day_returns_approximately_91() {
        let now = Utc::now();
        let half_day_ago = now - Duration::hours(12);
        let conf = exponential_decay(Some(half_day_ago), now, &config());
        // e^(-ln(4)/7 * 0.5) ≈ 0.905 → ~91
        assert!(
            (conf.value() as i16 - 91).abs() <= 1,
            "Expected ~91, got {}",
            conf.value()
        );
    }

    #[test]
    fn freshness_multiplier_thresholds() {
        let now = Utc::now();

        assert_eq!(freshness_multiplier(Some(now), now), 1.0);
        assert_eq!(
            freshness_multiplier(Some(now - Duration::hours(12)), now),
            1.0
        );
        assert_eq!(
            freshness_multiplier(Some(now - Duration::days(2)), now),
            0.75
        );
        assert_eq!(
            freshness_multiplier(Some(now - Duration::days(5)), now),
            0.5
        );
        assert_eq!(
            freshness_multiplier(Some(now - Duration::days(10)), now),
            0.25
        );
        assert_eq!(freshness_multiplier(None, now), 0.25);
    }
}
