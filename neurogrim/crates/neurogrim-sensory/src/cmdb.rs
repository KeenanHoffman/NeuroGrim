//! CMDB envelope builder — shared by all sensory tools.
//!
//! Produces JSON conforming to cmdb-envelope-v1.schema.json.

use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

/// A finding from a sensory observation.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub name: String,
    pub status: String,
    pub points: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Build a CMDB-envelope JSON value.
///
/// `confidence` is the sensor-supplied freshness signal in `[0, 100]`
/// (E-B2-1, spec §3.8 + cmdb-envelope-v1 schema). When `Some(n)`, the
/// envelope's root `confidence` field is set to `n` (clamped to ≤100).
/// When `None`, the field is omitted and the Brain's aggregator falls
/// back to age-decay of `meta.updated_at` (existing behavior).
///
/// Most sensors should pass `None` — the aggregator's age-decay is the
/// canonical confidence source. Only sensors with their own freshness
/// signal independent of clock-skew (e.g., `supply_chain_vigilance`'s
/// cache-age data, `supply_chain_sca`'s OSV cache age) should opt-in
/// to pass `Some(n)`. See E-B2-1 C12 for the selective-opt-in epic.
pub fn build_cmdb(
    tool_name: &str,
    score: u8,
    findings: Vec<Finding>,
    extra_fields: Option<Vec<(&str, Value)>>,
    confidence: Option<u8>,
) -> Value {
    let now = Utc::now().to_rfc3339();
    let score = score.min(100);

    let mut cmdb = serde_json::json!({
        "meta": {
            "schema_version": "1",
            "updated_at": now,
            "updated_by": tool_name,
        },
        "score": score,
        "updated_at": now,
        "findings": findings,
    });

    // Add extra domain-specific fields
    if let Some(fields) = extra_fields {
        if let Some(obj) = cmdb.as_object_mut() {
            for (key, value) in fields {
                obj.insert(key.to_string(), value);
            }
        }
    }

    // Optional sensor-supplied confidence (E-B2-1). Clamped at the
    // builder boundary as defense-in-depth alongside the schema's
    // [0, 100] constraint and the aggregator's Confidence::new clamp.
    if let Some(c) = confidence {
        if let Some(obj) = cmdb.as_object_mut() {
            obj.insert(
                "confidence".to_string(),
                Value::from(c.min(100)),
            );
        }
    }

    cmdb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmdb_has_required_fields() {
        let cmdb = build_cmdb("test-tool", 85, vec![], None, None);
        assert_eq!(cmdb["score"], 85);
        assert!(cmdb["updated_at"].is_string());
        assert_eq!(cmdb["meta"]["schema_version"], "1");
        assert_eq!(cmdb["meta"]["updated_by"], "test-tool");
    }

    #[test]
    fn cmdb_clamps_score() {
        let cmdb = build_cmdb("test", 150, vec![], None, None);
        assert_eq!(cmdb["score"], 100);
    }

    #[test]
    fn cmdb_includes_findings() {
        let findings = vec![Finding {
            name: "readme".to_string(),
            status: "found".to_string(),
            points: 10,
            detail: Some("README.md exists".to_string()),
        }];
        let cmdb = build_cmdb("test", 80, findings, None, None);
        assert_eq!(cmdb["findings"][0]["name"], "readme");
    }

    #[test]
    fn cmdb_includes_extra_fields() {
        let extras = vec![
            ("has_tests", Value::Bool(true)),
            ("test_count", Value::from(42)),
        ];
        let cmdb = build_cmdb("test", 80, vec![], Some(extras), None);
        assert_eq!(cmdb["has_tests"], true);
        assert_eq!(cmdb["test_count"], 42);
    }

    #[test]
    fn cmdb_omits_confidence_when_none() {
        // Default path: aggregator-as-source-of-truth. No `confidence`
        // key in the emitted envelope; aggregator falls back to
        // exponential_decay on meta.updated_at.
        let cmdb = build_cmdb("test", 80, vec![], None, None);
        assert!(
            cmdb.get("confidence").is_none(),
            "expected no `confidence` key when confidence is None, got {cmdb}"
        );
    }

    #[test]
    fn cmdb_includes_confidence_when_some() {
        // Selective opt-in path: sensor supplies an explicit confidence
        // value; envelope carries it. Aggregator's resolve_confidence
        // honors envelope-Some over age-decay.
        let cmdb = build_cmdb("test", 80, vec![], None, Some(85));
        assert_eq!(cmdb["confidence"], 85);
    }

    #[test]
    fn cmdb_includes_confidence_zero_when_some_zero() {
        // Sensor explicitly recorded zero confidence — must NOT be
        // confused with `None` (which would omit the key entirely).
        let cmdb = build_cmdb("test", 80, vec![], None, Some(0));
        assert_eq!(cmdb["confidence"], 0);
    }

    #[test]
    fn cmdb_clamps_confidence() {
        // Defense-in-depth: even if a buggy sensor passes >100,
        // build_cmdb clamps at the builder boundary. The schema also
        // rejects out-of-range values; the aggregator also clamps via
        // Confidence::new.
        let cmdb = build_cmdb("test", 80, vec![], None, Some(200));
        assert_eq!(cmdb["confidence"], 100);
    }

    #[test]
    fn cmdb_confidence_coexists_with_extras() {
        // Both confidence and extra_fields land at the root of the
        // same envelope without clobbering each other.
        let extras = vec![("has_tests", Value::Bool(true))];
        let cmdb = build_cmdb("test", 80, vec![], Some(extras), Some(75));
        assert_eq!(cmdb["confidence"], 75);
        assert_eq!(cmdb["has_tests"], true);
    }
}
