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

/// Compute envelope-supplied confidence from a sensor's cache age in
/// seconds, anchored to a TTL in days (E-B2-1 C12).
///
/// Sensors that maintain their own cached data — supply-chain-sca's OSV
/// cache (24h TTL), supply-chain-vigilance's registry-metadata cache
/// (7d TTL) — MAY use this helper to translate their freshest-cache-
/// entry age into an envelope-supplied `confidence` value to embed in
/// the CMDB envelope's optional `confidence` field (spec §3.1, v2.7+).
///
/// The curve matches `exponential_decay`'s shape: age=0 → 100;
/// age=`ttl_days` → 25; asymptotic to 1 beyond. The TTL's role here is
/// the "very_stale_days" anchor — within the TTL window, confidence
/// drops gracefully from 100 to 25, signaling cache-driven staleness
/// to operators while we still use the cached value internally.
///
/// Returns `None` when `cache_age_seconds` is `None` — meaning no cache
/// reads occurred (all-live or no-queries case). The Brain's aggregator
/// then falls back to age-decay of `meta.updated_at` (which will be
/// ~now → confidence ~100 — accurate semantics: "this run did fresh
/// work").
pub fn confidence_from_cache_age(cache_age_seconds: Option<u64>, ttl_days: f64) -> Option<u8> {
    let age_seconds = cache_age_seconds?;
    let age_days = age_seconds as f64 / 86400.0;
    // Defensive: clamp ttl_days to a positive minimum so a buggy caller
    // with ttl_days=0 doesn't divide-by-zero. Operationally a TTL of 0
    // is meaningless (cache would always miss), but we don't crash.
    let ttl_safe = ttl_days.max(0.001);
    let lambda = (4.0_f64).ln() / ttl_safe;
    let conf = 100.0 * (-lambda * age_days).exp();
    Some(conf.round().clamp(1.0, 100.0) as u8)
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

    // E-B2-1 C12: confidence_from_cache_age tests.
    // The curve mirrors exponential_decay but parameterizes "very_stale"
    // as ttl_days. age=0 → 100; age=ttl_days → 25; age>>ttl_days → 1.

    #[test]
    fn cache_age_none_returns_none() {
        // No cache hits → no signal → aggregator's age-decay handles.
        assert_eq!(confidence_from_cache_age(None, 1.0), None);
    }

    #[test]
    fn cache_age_zero_returns_100() {
        // Cache just refreshed → full confidence.
        assert_eq!(confidence_from_cache_age(Some(0), 1.0), Some(100));
        assert_eq!(confidence_from_cache_age(Some(0), 7.0), Some(100));
    }

    #[test]
    fn cache_age_at_ttl_returns_25() {
        // Cache at TTL boundary → 25 (matches exponential_decay's
        // very_stale_days anchor).
        let one_day_secs = 86400u64;
        assert_eq!(
            confidence_from_cache_age(Some(one_day_secs), 1.0),
            Some(25)
        );

        let seven_days_secs = 7 * 86400u64;
        assert_eq!(
            confidence_from_cache_age(Some(seven_days_secs), 7.0),
            Some(25)
        );
    }

    #[test]
    fn cache_age_beyond_ttl_drops_low() {
        // Past TTL (cache would normally be evicted): asymptotic to 1.
        let two_days_secs = 2 * 86400u64;
        let conf = confidence_from_cache_age(Some(two_days_secs), 1.0).unwrap();
        // e^(-ln(4)*2) = 1/16 ≈ 6
        assert!(conf <= 10, "expected ≤10 at 2×TTL, got {conf}");
    }

    #[test]
    fn cache_age_very_old_clamps_to_one() {
        // 30 days at TTL=1 day: confidence = 100 * 4^-30 ≈ 9e-17. Clamped to 1.
        let thirty_days_secs = 30 * 86400u64;
        assert_eq!(
            confidence_from_cache_age(Some(thirty_days_secs), 1.0),
            Some(1)
        );
    }

    #[test]
    fn cache_age_within_ttl_decays_smoothly() {
        // Half-TTL → ~50 (e^(-ln(4)*0.5) = e^(-ln(2)) = 0.5).
        let half_day_secs = 86400u64 / 2;
        let conf = confidence_from_cache_age(Some(half_day_secs), 1.0).unwrap();
        assert!(
            (conf as i16 - 50).abs() <= 1,
            "expected ~50 at 0.5×TTL, got {conf}"
        );
    }

    #[test]
    fn cache_age_zero_ttl_does_not_panic() {
        // Defensive: a buggy caller passing ttl_days=0 must not crash.
        // Result is implementation-defined; we just need it to not panic.
        let _ = confidence_from_cache_age(Some(86400), 0.0);
        let _ = confidence_from_cache_age(Some(0), 0.0);
        let _ = confidence_from_cache_age(Some(0), -1.0);
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
