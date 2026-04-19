//! Correlation engine (spec Section 8).
//!
//! Evaluates condition trees against domain variables, fires incident patterns,
//! and manages the incident ledger.

use crate::types::ScoreSnapshot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Domain variables: a flat map of "domain:variable" → value.
pub type DomainVariables = HashMap<String, Value>;

/// Extract domain variables from CMDB data.
///
/// For each domain, extracts:
/// 1. Variables declared in `exported_variables` config
/// 2. All top-level boolean/number fields as fallback
pub fn extract_domain_variables(
    cmdb_data: &HashMap<String, Value>,
    exported_vars: &HashMap<String, HashMap<String, crate::registry::ExportedVariable>>,
) -> DomainVariables {
    let mut vars = DomainVariables::new();

    for (domain, cmdb) in cmdb_data {
        // Check for explicit exported_variables config
        if let Some(exports) = exported_vars.get(domain) {
            for (var_name, export) in exports {
                if let Some(val) = cmdb.get(&export.field) {
                    vars.insert(var_name.clone(), val.clone());
                }
            }
        }

        // Fallback: extract all top-level boolean/number fields
        if let Some(obj) = cmdb.as_object() {
            for (field, value) in obj {
                if field == "meta" || field == "findings" {
                    continue; // Skip structural fields
                }
                match value {
                    Value::Bool(_) | Value::Number(_) => {
                        let key = format!("{}:{}", domain, field);
                        vars.entry(key).or_insert_with(|| value.clone());
                    }
                    _ => {}
                }
            }
        }
    }

    vars
}

/// Evaluate a condition tree node against domain variables.
///
/// Supports: and, or, not, comparison operators (>=, >, ==, <, <=, !=),
/// and temporal operators (duration_above, delta_in_window, recurrence_count).
pub fn evaluate_condition(node: &Value, vars: &DomainVariables, history: &[ScoreSnapshot]) -> bool {
    if node.is_null() {
        return false;
    }

    let obj = match node.as_object() {
        Some(o) => o,
        None => return false,
    };

    // Branch operators
    if let Some(children) = obj.get("and") {
        return match children.as_array() {
            Some(arr) => arr.iter().all(|c| evaluate_condition(c, vars, history)),
            None => false,
        };
    }

    if let Some(children) = obj.get("or") {
        return match children.as_array() {
            Some(arr) => arr.iter().any(|c| evaluate_condition(c, vars, history)),
            None => false,
        };
    }

    if let Some(child) = obj.get("not") {
        return !evaluate_condition(child, vars, history);
    }

    // Temporal operators
    if let Some(config) = obj.get("duration_above") {
        return eval_duration_above(config, vars, history);
    }
    if let Some(config) = obj.get("delta_in_window") {
        return eval_delta_in_window(config, vars, history);
    }
    if let Some(config) = obj.get("recurrence_count") {
        return eval_recurrence_count(config, vars, history);
    }

    // Comparison operators
    for op in &[">=", ">", "==", "<", "<=", "!="] {
        if let Some(args) = obj.get(*op) {
            return eval_comparison(op, args, vars);
        }
    }

    false
}

/// Evaluate a comparison operator: { ">=": ["var_name", value] }
fn eval_comparison(op: &str, args: &Value, vars: &DomainVariables) -> bool {
    let arr = match args.as_array() {
        Some(a) if a.len() == 2 => a,
        _ => return false,
    };

    let var_name = match arr[0].as_str() {
        Some(s) => s,
        None => return false,
    };

    let var_val = match vars.get(var_name) {
        Some(v) => v,
        None => return false,
    };

    // Try numeric comparison first
    if let (Some(lhs), Some(rhs)) = (as_f64(var_val), as_f64(&arr[1])) {
        return match op {
            ">=" => lhs >= rhs,
            ">" => lhs > rhs,
            "==" => (lhs - rhs).abs() < f64::EPSILON,
            "<" => lhs < rhs,
            "<=" => lhs <= rhs,
            "!=" => (lhs - rhs).abs() >= f64::EPSILON,
            _ => false,
        };
    }

    // Fall back to string/bool comparison for == and !=
    match op {
        "==" => var_val == &arr[1],
        "!=" => var_val != &arr[1],
        _ => false,
    }
}

/// duration_above: check if variable stayed >= threshold across entire time window.
fn eval_duration_above(config: &Value, _vars: &DomainVariables, history: &[ScoreSnapshot]) -> bool {
    let var_name = config.get("var").and_then(|v| v.as_str()).unwrap_or("");
    let threshold = config
        .get("threshold")
        .and_then(|v| as_f64(v))
        .unwrap_or(0.0);
    let hours = config.get("hours").and_then(|v| as_f64(v)).unwrap_or(0.0);

    if history.is_empty() {
        return false;
    }

    let cutoff = Utc::now() - chrono::Duration::hours(hours as i64);
    let in_window: Vec<&ScoreSnapshot> = history.iter().filter(|s| s.scored_at >= cutoff).collect();

    if in_window.is_empty() {
        return false;
    }

    // Check if variable stayed >= threshold across all snapshots in window
    // Note: domain variables aren't stored per-snapshot in current schema,
    // so we check raw scores as proxy
    for snap in &in_window {
        let val = extract_var_from_snapshot(snap, var_name);
        if val < threshold {
            return false;
        }
    }

    true
}

/// delta_in_window: check if variable changed by required delta over N snapshots.
fn eval_delta_in_window(config: &Value, vars: &DomainVariables, history: &[ScoreSnapshot]) -> bool {
    let var_name = config.get("var").and_then(|v| v.as_str()).unwrap_or("");
    let delta = config.get("delta").and_then(|v| as_f64(v)).unwrap_or(0.0);
    let snapshots = config
        .get("snapshots")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    if history.len() < snapshots {
        return false;
    }

    let current_val = match vars.get(var_name).and_then(|v| as_f64(v)) {
        Some(v) => v,
        None => return false,
    };

    let hist_idx = history.len().saturating_sub(snapshots);
    let hist_val = extract_var_from_snapshot(&history[hist_idx], var_name);

    let actual_delta = current_val - hist_val;

    if delta >= 0.0 {
        actual_delta >= delta
    } else {
        actual_delta <= delta
    }
}

/// recurrence_count: check if pattern has recurred N times in window.
fn eval_recurrence_count(
    config: &Value,
    _vars: &DomainVariables,
    _history: &[ScoreSnapshot],
) -> bool {
    // This would check the incident ledger for recurrence count.
    // For now, returns false (no ledger access in pure core).
    let _pattern_id = config.get("pattern_id").and_then(|v| v.as_str());
    let _min_count = config.get("min_count").and_then(|v| v.as_u64());
    false
}

/// Extract a variable value from a snapshot by parsing "domain:field" format.
fn extract_var_from_snapshot(snap: &ScoreSnapshot, var_name: &str) -> f64 {
    let parts: Vec<&str> = var_name.splitn(2, ':').collect();
    if parts.len() == 2 {
        let domain = parts[0];
        let field = parts[1];
        if let Some(ds) = snap.domains.get(domain) {
            if field == "score" {
                return ds.score as f64;
            }
            if field == "confidence" {
                return ds.confidence as f64;
            }
        }
    }
    0.0
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Incident pattern match result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentMatch {
    pub id: String,
    pub name: String,
    pub severity: String,
    pub recurrence_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hypothesis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_remediation: Option<String>,
}

/// Incident ledger entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentLedgerEntry {
    pub timestamp: String,
    pub pattern_id: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

/// Evaluate all incident patterns against current domain variables.
pub fn evaluate_incident_patterns(
    patterns: &[Value],
    vars: &DomainVariables,
    history: &[ScoreSnapshot],
    existing_ledger: &[IncidentLedgerEntry],
    severity_config: &crate::registry::SeverityConfig,
) -> (Vec<IncidentMatch>, Vec<String>) {
    let mut matched = Vec::new();
    let mut skipped_temporal = Vec::new();

    for pattern in patterns {
        let id = pattern.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let name = pattern.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let hypothesis = pattern.get("hypothesis").and_then(|v| v.as_str());
        let skill = pattern.get("skill_remediation").and_then(|v| v.as_str());
        let severity_base = pattern
            .get("severity_base")
            .and_then(|v| v.as_str())
            .unwrap_or("info");

        // Check if pattern has temporal operators requiring history
        let condition = pattern
            .get("condition_tree")
            .or_else(|| pattern.get("conditions"));

        let has_temporal = condition.map(|c| has_temporal_operator(c)).unwrap_or(false);

        if has_temporal && history.is_empty() {
            skipped_temporal.push(id.to_string());
            continue;
        }

        // Evaluate the condition
        let fired = match condition {
            Some(cond)
                if !cond.is_null() && cond.as_object().map(|o| !o.is_empty()).unwrap_or(false) =>
            {
                evaluate_condition(cond, vars, history)
            }
            _ => {
                // Empty conditions = always fires (matches current PS behavior)
                true
            }
        };

        if fired {
            let recurrence =
                count_recurrence(id, existing_ledger, severity_config.recurrence_window_days);
            let total = recurrence + 1;
            let severity = escalate_severity(severity_base, total, severity_config);

            matched.push(IncidentMatch {
                id: id.to_string(),
                name: name.to_string(),
                severity,
                recurrence_count: total,
                hypothesis: hypothesis.map(|s| s.to_string()),
                skill_remediation: skill.map(|s| s.to_string()),
            });
        }
    }

    (matched, skipped_temporal)
}

/// Check if a condition tree contains temporal operators.
fn has_temporal_operator(node: &Value) -> bool {
    if let Some(obj) = node.as_object() {
        if obj.contains_key("duration_above")
            || obj.contains_key("delta_in_window")
            || obj.contains_key("recurrence_count")
        {
            return true;
        }
        if let Some(children) = obj.get("and").or_else(|| obj.get("or")) {
            if let Some(arr) = children.as_array() {
                return arr.iter().any(has_temporal_operator);
            }
        }
        if let Some(child) = obj.get("not") {
            return has_temporal_operator(child);
        }
    }
    false
}

/// Count how many times a pattern has fired within the recurrence window.
fn count_recurrence(pattern_id: &str, ledger: &[IncidentLedgerEntry], window_days: u32) -> u32 {
    let cutoff = Utc::now() - chrono::Duration::days(window_days as i64);
    ledger
        .iter()
        .filter(|e| {
            e.pattern_id == pattern_id
                && e.timestamp
                    .parse::<DateTime<Utc>>()
                    .map(|ts| ts >= cutoff)
                    .unwrap_or(false)
        })
        .count() as u32
}

/// Escalate severity based on recurrence count.
fn escalate_severity(
    base: &str,
    total_count: u32,
    config: &crate::registry::SeverityConfig,
) -> String {
    if total_count >= config.critical_count {
        "critical".to_string()
    } else if total_count >= config.warning_count {
        "warning".to_string()
    } else {
        base.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vars(pairs: &[(&str, Value)]) -> DomainVariables {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    // --- Condition Tree Tests ---

    #[test]
    fn null_condition_returns_false() {
        assert!(!evaluate_condition(
            &Value::Null,
            &DomainVariables::new(),
            &[]
        ));
    }

    #[test]
    fn comparison_gte() {
        let vars = make_vars(&[("test-health:score", Value::from(80))]);
        let node = serde_json::json!({">=": ["test-health:score", 75]});
        assert!(evaluate_condition(&node, &vars, &[]));

        let node2 = serde_json::json!({">=": ["test-health:score", 90]});
        assert!(!evaluate_condition(&node2, &vars, &[]));
    }

    #[test]
    fn comparison_eq() {
        let vars = make_vars(&[("flag", Value::Bool(true))]);
        let node = serde_json::json!({"==": ["flag", true]});
        assert!(evaluate_condition(&node, &vars, &[]));
    }

    #[test]
    fn comparison_neq() {
        let vars = make_vars(&[("test-health:score", Value::from(30))]);
        let node = serde_json::json!({"!=": ["test-health:score", 100]});
        assert!(evaluate_condition(&node, &vars, &[]));
    }

    #[test]
    fn and_all_true() {
        let vars = make_vars(&[("a", Value::from(10)), ("b", Value::from(20))]);
        let node = serde_json::json!({
            "and": [
                {">=": ["a", 5]},
                {">=": ["b", 15]}
            ]
        });
        assert!(evaluate_condition(&node, &vars, &[]));
    }

    #[test]
    fn and_one_false() {
        let vars = make_vars(&[("a", Value::from(10)), ("b", Value::from(5))]);
        let node = serde_json::json!({
            "and": [
                {">=": ["a", 5]},
                {">=": ["b", 15]}
            ]
        });
        assert!(!evaluate_condition(&node, &vars, &[]));
    }

    #[test]
    fn or_one_true() {
        let vars = make_vars(&[("a", Value::from(10)), ("b", Value::from(5))]);
        let node = serde_json::json!({
            "or": [
                {">=": ["a", 5]},
                {">=": ["b", 15]}
            ]
        });
        assert!(evaluate_condition(&node, &vars, &[]));
    }

    #[test]
    fn or_none_true() {
        let vars = make_vars(&[("a", Value::from(1)), ("b", Value::from(2))]);
        let node = serde_json::json!({
            "or": [
                {">=": ["a", 5]},
                {">=": ["b", 15]}
            ]
        });
        assert!(!evaluate_condition(&node, &vars, &[]));
    }

    #[test]
    fn not_inverts() {
        let vars = make_vars(&[("a", Value::from(10))]);
        let node = serde_json::json!({"not": {">=": ["a", 5]}});
        assert!(!evaluate_condition(&node, &vars, &[]));

        let node2 = serde_json::json!({"not": {">=": ["a", 50]}});
        assert!(evaluate_condition(&node2, &vars, &[]));
    }

    #[test]
    fn nested_and_or_not() {
        let vars = make_vars(&[
            ("a", Value::from(10)),
            ("b", Value::from(20)),
            ("c", Value::from(5)),
        ]);
        // (a >= 5 AND b >= 15) OR NOT(c >= 10)
        let node = serde_json::json!({
            "or": [
                {"and": [{">=": ["a", 5]}, {">=": ["b", 15]}]},
                {"not": {">=": ["c", 10]}}
            ]
        });
        assert!(evaluate_condition(&node, &vars, &[]));
    }

    #[test]
    fn missing_variable_returns_false() {
        let vars = DomainVariables::new();
        let node = serde_json::json!({">=": ["nonexistent", 5]});
        assert!(!evaluate_condition(&node, &vars, &[]));
    }

    // --- Domain Variable Extraction Tests ---

    #[test]
    fn extract_fallback_variables() {
        let mut cmdb_data = HashMap::new();
        cmdb_data.insert(
            "test-health".to_string(),
            serde_json::json!({
                "meta": {"schema_version": "1"},
                "score": 85,
                "updated_at": "2026-04-11T00:00:00Z",
                "has_tests": true,
                "test_count": 42
            }),
        );

        let vars = extract_domain_variables(&cmdb_data, &HashMap::new());
        assert_eq!(vars.get("test-health:score"), Some(&Value::from(85)));
        assert_eq!(vars.get("test-health:has_tests"), Some(&Value::Bool(true)));
        assert_eq!(vars.get("test-health:test_count"), Some(&Value::from(42)));
        // meta and findings should be excluded
        assert!(!vars.contains_key("test-health:meta"));
    }

    // --- Severity Escalation Tests ---

    #[test]
    fn severity_escalation() {
        let config = crate::registry::SeverityConfig {
            warning_count: 3,
            critical_count: 5,
            recurrence_window_days: 7,
        };
        assert_eq!(escalate_severity("info", 1, &config), "info");
        assert_eq!(escalate_severity("info", 3, &config), "warning");
        assert_eq!(escalate_severity("info", 5, &config), "critical");
        assert_eq!(escalate_severity("warning", 2, &config), "warning");
    }

    // --- Empty Conditions Tests ---

    #[test]
    fn empty_conditions_always_fire() {
        let patterns = vec![serde_json::json!({
            "id": "test",
            "name": "Test Pattern",
            "conditions": {},
            "severity_base": "info"
        })];
        let config = crate::registry::SeverityConfig::default();
        let (matched, _) =
            evaluate_incident_patterns(&patterns, &DomainVariables::new(), &[], &[], &config);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, "test");
    }
}
