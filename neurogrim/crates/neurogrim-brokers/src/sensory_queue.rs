//! A.2.5 — Sensory Queue enforcer V1 (BB #18).
//!
//! Per `docs/BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md` Gate 3:
//!
//! - **V1 scope**: rate limit per source (sliding window) + schema
//!   validation against cmdb-envelope-v1 structural contract.
//! - **V2 scope (deferred)**: redaction (operator-configurable secret
//!   pattern stripping; PII detection). Schema-aware scrubbing.
//!
//! ## Trust boundary contract (per BROKER-CONTRACT.md)
//!
//! Every custom sensor (Tier 1 declarative + Tier 2 operator-Rust) that
//! writes a CMDB MUST flow through this enforcer. **Built-in sensors**
//! (the 26 in `neurogrim-sensory`) are **pre-trusted** and bypass the
//! enforcer — they're shipped code, not operator-authored.
//!
//! ## Rate limit semantics
//!
//! Sliding window per source-id (broker_id). Defaults: 12 writes per
//! 60-second window. Operators override via cluster.toml `[sensory_queue.
//! default_limits]` (per-cluster) or `[sensory_queue.per_source.<id>]`
//! (per-source). V1 in-substrate ships with hardcoded defaults; operator
//! override wiring lands as a small follow-on.
//!
//! ## Schema validation semantics
//!
//! V1 ships a hand-rolled structural check that validates:
//! - `meta` is an object
//! - `meta.schema_version` is the string "1"
//! - `score` is an integer in [0, 100]
//! - `findings` (if present) is an array
//!
//! This is the structural floor cmdb-envelope-v1 requires. Full
//! JSON Schema Draft 2020-12 validation against the vendored
//! `cmdb-envelope-v1.schema.json` is V2 — adds a `jsonschema` crate
//! dep that's out of scope for V1.
//!
//! ## Why this enforcer matters
//!
//! Without it, an operator-authored sensor extension could:
//! - Spam writes (denial of service against the materializer + disk I/O)
//! - Emit malformed CMDB envelopes that crash the scoring engine
//! - (V2) Leak secrets through fact values

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Rate-limit config — sliding window per source.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub window: Duration,
    pub max_writes_per_window: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            window: Duration::from_secs(60),
            max_writes_per_window: 12,
        }
    }
}

/// One sliding-window record per source-id.
#[derive(Debug, Default)]
struct SlidingWindow {
    write_timestamps: Vec<Instant>,
}

impl SlidingWindow {
    /// Returns true if a new write would fit within the limit; pruning
    /// stale entries as a side effect.
    fn try_write(&mut self, now: Instant, config: &RateLimitConfig) -> bool {
        let cutoff = now.checked_sub(config.window).unwrap_or(now);
        self.write_timestamps.retain(|&ts| ts > cutoff);
        if self.write_timestamps.len() < config.max_writes_per_window as usize {
            self.write_timestamps.push(now);
            true
        } else {
            false
        }
    }

    fn current_count(&self, now: Instant, config: &RateLimitConfig) -> usize {
        let cutoff = now.checked_sub(config.window).unwrap_or(now);
        self.write_timestamps
            .iter()
            .filter(|&&ts| ts > cutoff)
            .count()
    }
}

#[derive(Debug, Clone)]
pub enum RefusalReason {
    RateLimit {
        window_secs: u64,
        max: u32,
        current: usize,
    },
    SchemaInvalid {
        field: String,
        message: String,
    },
    /// V2 placeholder — V1 does not redact, so this variant is unused
    /// today. Documented in the trait for forward-compat with V2
    /// consumers reading `RefusalReason`.
    RedactionRequired {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct EnforceResult {
    pub allowed: bool,
    pub refusal_reason: Option<RefusalReason>,
    pub source_id: String,
}

impl EnforceResult {
    pub fn allowed(source_id: impl Into<String>) -> Self {
        Self {
            allowed: true,
            refusal_reason: None,
            source_id: source_id.into(),
        }
    }
    pub fn refused(source_id: impl Into<String>, reason: RefusalReason) -> Self {
        Self {
            allowed: false,
            refusal_reason: Some(reason),
            source_id: source_id.into(),
        }
    }
}

/// V1 enforcer — rate limit + structural schema validation. Stateful
/// (holds per-source sliding windows in a Mutex). Safe to share via
/// `Arc<SensoryQueueEnforcerV1>` across async tasks.
pub struct SensoryQueueEnforcerV1 {
    default_limits: RateLimitConfig,
    per_source_limits: HashMap<String, RateLimitConfig>,
    windows: Mutex<HashMap<String, SlidingWindow>>,
}

impl SensoryQueueEnforcerV1 {
    pub fn new(default_limits: RateLimitConfig) -> Self {
        Self {
            default_limits,
            per_source_limits: HashMap::new(),
            windows: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_default_limits() -> Self {
        Self::new(RateLimitConfig::default())
    }

    /// Set a per-source rate-limit override (e.g., "trust sensor X with
    /// 60 writes/min while others get 12").
    pub fn set_per_source_limits(&mut self, source_id: impl Into<String>, config: RateLimitConfig) {
        self.per_source_limits.insert(source_id.into(), config);
    }

    /// V1 enforce: rate-limit check + schema validation. Built-in sensors
    /// bypass by NOT calling this method (their SensorBackedBroker is
    /// constructed without `.with_enforcer(...)`).
    pub fn enforce(&self, source_id: &str, payload: &serde_json::Value) -> EnforceResult {
        // Step 1: rate limit
        let config = self
            .per_source_limits
            .get(source_id)
            .cloned()
            .unwrap_or_else(|| self.default_limits.clone());
        let now = Instant::now();
        let mut windows = self.windows.lock().unwrap();
        let window = windows.entry(source_id.to_string()).or_default();
        if !window.try_write(now, &config) {
            let current = window.current_count(now, &config);
            return EnforceResult::refused(
                source_id,
                RefusalReason::RateLimit {
                    window_secs: config.window.as_secs(),
                    max: config.max_writes_per_window,
                    current,
                },
            );
        }
        drop(windows);

        // Step 2: schema validation (structural floor for cmdb-envelope-v1)
        if let Err((field, message)) = validate_envelope_v1(payload) {
            return EnforceResult::refused(
                source_id,
                RefusalReason::SchemaInvalid { field, message },
            );
        }

        EnforceResult::allowed(source_id)
    }
}

/// V1 structural validation — hand-rolled floor for cmdb-envelope-v1.
/// Returns `Err((field_path, message))` on the first failure.
///
/// Full JSON Schema validation lands in V2 (adds `jsonschema` crate dep).
fn validate_envelope_v1(payload: &serde_json::Value) -> Result<(), (String, String)> {
    let obj = payload.as_object().ok_or((
        "(root)".to_string(),
        "payload must be a JSON object".to_string(),
    ))?;

    // meta.schema_version == "1"
    let meta = obj.get("meta").and_then(|v| v.as_object()).ok_or((
        "meta".to_string(),
        "missing or non-object `meta`".to_string(),
    ))?;
    let schema_version = meta.get("schema_version").and_then(|v| v.as_str()).ok_or((
        "meta.schema_version".to_string(),
        "missing or non-string `meta.schema_version`".to_string(),
    ))?;
    if schema_version != "1" {
        return Err((
            "meta.schema_version".to_string(),
            format!("expected `1`, got `{}`", schema_version),
        ));
    }

    // score is integer 0..=100
    let score = obj.get("score").and_then(|v| v.as_u64()).ok_or((
        "score".to_string(),
        "missing or non-integer `score`".to_string(),
    ))?;
    if score > 100 {
        return Err((
            "score".to_string(),
            format!("score must be in [0, 100], got {}", score),
        ));
    }

    // findings (if present) must be array
    if let Some(findings) = obj.get("findings") {
        if !findings.is_array() {
            return Err((
                "findings".to_string(),
                "`findings` must be a JSON array".to_string(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_envelope() -> serde_json::Value {
        json!({
            "meta": {
                "schema_version": "1",
                "updated_at": "2026-06-26T00:00:00Z",
                "updated_by": "test"
            },
            "score": 75,
            "updated_at": "2026-06-26T00:00:00Z",
            "findings": []
        })
    }

    #[test]
    fn valid_envelope_passes_schema() {
        assert!(validate_envelope_v1(&valid_envelope()).is_ok());
    }

    #[test]
    fn non_object_root_rejected() {
        let err = validate_envelope_v1(&json!("not an object")).unwrap_err();
        assert_eq!(err.0, "(root)");
    }

    #[test]
    fn missing_meta_rejected() {
        let err = validate_envelope_v1(&json!({"score": 50})).unwrap_err();
        assert_eq!(err.0, "meta");
    }

    #[test]
    fn wrong_schema_version_rejected() {
        let mut envelope = valid_envelope();
        envelope["meta"]["schema_version"] = json!("99");
        let err = validate_envelope_v1(&envelope).unwrap_err();
        assert_eq!(err.0, "meta.schema_version");
        assert!(err.1.contains("99"));
    }

    #[test]
    fn missing_score_rejected() {
        let err = validate_envelope_v1(&json!({
            "meta": {"schema_version": "1"}
        }))
        .unwrap_err();
        assert_eq!(err.0, "score");
    }

    #[test]
    fn out_of_range_score_rejected() {
        let mut envelope = valid_envelope();
        envelope["score"] = json!(150);
        let err = validate_envelope_v1(&envelope).unwrap_err();
        assert_eq!(err.0, "score");
        assert!(err.1.contains("[0, 100]"));
    }

    #[test]
    fn non_array_findings_rejected() {
        let mut envelope = valid_envelope();
        envelope["findings"] = json!({"not": "an array"});
        let err = validate_envelope_v1(&envelope).unwrap_err();
        assert_eq!(err.0, "findings");
    }

    #[test]
    fn enforcer_allows_under_rate_limit() {
        let enforcer = SensoryQueueEnforcerV1::with_default_limits();
        let envelope = valid_envelope();
        for _ in 0..5 {
            let result = enforcer.enforce("test-sensor", &envelope);
            assert!(result.allowed, "should allow under limit");
        }
    }

    #[test]
    fn enforcer_refuses_over_rate_limit() {
        let mut enforcer = SensoryQueueEnforcerV1::with_default_limits();
        enforcer.set_per_source_limits(
            "spammer",
            RateLimitConfig {
                window: Duration::from_secs(60),
                max_writes_per_window: 3,
            },
        );
        let envelope = valid_envelope();
        // 3 writes pass
        for _ in 0..3 {
            assert!(enforcer.enforce("spammer", &envelope).allowed);
        }
        // 4th refused
        let result = enforcer.enforce("spammer", &envelope);
        assert!(!result.allowed);
        match result.refusal_reason {
            Some(RefusalReason::RateLimit { max, current, .. }) => {
                assert_eq!(max, 3);
                assert_eq!(current, 3);
            }
            other => panic!("expected RateLimit, got {:?}", other),
        }
    }

    #[test]
    fn enforcer_refuses_malformed_payload() {
        let enforcer = SensoryQueueEnforcerV1::with_default_limits();
        let malformed = json!({"not": "an envelope"});
        let result = enforcer.enforce("test-sensor", &malformed);
        assert!(!result.allowed);
        match result.refusal_reason {
            Some(RefusalReason::SchemaInvalid { .. }) => {}
            other => panic!("expected SchemaInvalid, got {:?}", other),
        }
    }

    #[test]
    fn enforcer_per_source_limits_are_independent() {
        let enforcer = SensoryQueueEnforcerV1::with_default_limits();
        let envelope = valid_envelope();
        // Source A maxes out (12 writes)
        for _ in 0..12 {
            assert!(enforcer.enforce("source-a", &envelope).allowed);
        }
        // Source B is unaffected
        assert!(enforcer.enforce("source-b", &envelope).allowed);
        // Source A's 13th is refused
        assert!(!enforcer.enforce("source-a", &envelope).allowed);
    }
}
