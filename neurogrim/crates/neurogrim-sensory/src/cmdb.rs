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
pub fn build_cmdb(
    tool_name: &str,
    score: u8,
    findings: Vec<Finding>,
    extra_fields: Option<Vec<(&str, Value)>>,
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

    cmdb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmdb_has_required_fields() {
        let cmdb = build_cmdb("test-tool", 85, vec![], None);
        assert_eq!(cmdb["score"], 85);
        assert!(cmdb["updated_at"].is_string());
        assert_eq!(cmdb["meta"]["schema_version"], "1");
        assert_eq!(cmdb["meta"]["updated_by"], "test-tool");
    }

    #[test]
    fn cmdb_clamps_score() {
        let cmdb = build_cmdb("test", 150, vec![], None);
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
        let cmdb = build_cmdb("test", 80, findings, None);
        assert_eq!(cmdb["findings"][0]["name"], "readme");
    }

    #[test]
    fn cmdb_includes_extra_fields() {
        let extras = vec![
            ("has_tests", Value::Bool(true)),
            ("test_count", Value::from(42)),
        ];
        let cmdb = build_cmdb("test", 80, vec![], Some(extras));
        assert_eq!(cmdb["has_tests"], true);
        assert_eq!(cmdb["test_count"], 42);
    }
}
