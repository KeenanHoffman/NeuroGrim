//! BB #9 — Pipeline Catalog.
//!
//! YAML loader + schema validation at load + precondition predicate DSL.
//!
//! ## Wave 2 design decisions
//!
//! 1. **Predicate DSL (BROKER-SPEC-GAPS.md gap #7 resolution).** Minimal
//!    dotted-path lookup against the broker's Overlay snapshot, with optional
//!    `=` / `!=` comparison + `{param_name}` substitution from dispatch
//!    params. Examples:
//!    - `"active_work"` — Overlay has this field (any value)
//!    - `"active_work.0.status = ready"` — field equals literal value
//!    - `"active_work.{work_unit_id}.ready"` — field exists, with param sub
//!    - `"broker_status != errored"` — inequality
//!
//!    Chosen over JSONPath for simplicity (S0-T MVP); rich JSONPath +
//!    numeric comparison + filter expressions land in S1-T per spec gap #7
//!    decision.
//!
//! 2. **Cross-broker sub_pipeline rejection (ultra-pass U12).** Loader
//!    rejects any `Step::SubPipeline` whose `sub_pipeline` field references
//!    a pipeline owned by a different broker (`<other_broker_id>/<...>`).
//!    Error: `"cross-broker composition requires BB #27, not yet in MVP"`.

use crate::pipeline::{ParamMap, Pipeline, Step};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog file not found: {0}")]
    FileNotFound(String),

    #[error("catalog YAML parse failed: {0}")]
    YamlParseFailed(#[from] serde_yaml::Error),

    #[error("io error reading catalog: {0}")]
    IoError(#[from] std::io::Error),

    #[error("pipeline `{pipeline_id}` invalid: {reason}")]
    PipelineInvalid {
        pipeline_id: String,
        reason: String,
    },

    #[error("cross-broker composition requires BB #27 (not yet in MVP): pipeline `{caller}` references `{callee}` in another broker")]
    CrossBrokerComposition { caller: String, callee: String },

    #[error("duplicate pipeline id in catalog: {0}")]
    DuplicatePipelineId(String),
}

/// Load a per-broker pipeline catalog from a YAML file.
/// The YAML file must contain a top-level list of `Pipeline` structs.
pub fn load_catalog(path: &Path) -> Result<Vec<Pipeline>, CatalogError> {
    let contents = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CatalogError::FileNotFound(path.display().to_string())
        } else {
            CatalogError::IoError(e)
        }
    })?;
    let pipelines: Vec<Pipeline> = serde_yaml::from_str(&contents)?;
    Ok(pipelines)
}

/// Validate a pipeline catalog against the broker's id. Rejects:
/// - Pipelines whose id doesn't start with `<broker_id>/`
/// - Cross-broker `sub_pipeline:` references (per BB #27 deferral)
/// - Duplicate pipeline IDs within the catalog
pub fn validate_catalog(pipelines: &[Pipeline], broker_id: &str) -> Result<(), CatalogError> {
    let prefix = format!("{}/", broker_id);
    let mut seen_ids = std::collections::HashSet::new();

    for pipeline in pipelines {
        // ID format check
        if !pipeline.id.starts_with(&prefix) {
            return Err(CatalogError::PipelineInvalid {
                pipeline_id: pipeline.id.clone(),
                reason: format!(
                    "pipeline id must start with `{}` (broker_id prefix)",
                    prefix
                ),
            });
        }

        // Duplicate detection
        if !seen_ids.insert(pipeline.id.clone()) {
            return Err(CatalogError::DuplicatePipelineId(pipeline.id.clone()));
        }

        // Cross-broker sub_pipeline rejection (ultra-pass U12)
        validate_steps_intra_broker(&pipeline.steps, &pipeline.id, broker_id)?;
    }

    Ok(())
}

fn validate_steps_intra_broker(
    steps: &[Step],
    caller_id: &str,
    broker_id: &str,
) -> Result<(), CatalogError> {
    let prefix = format!("{}/", broker_id);
    for step in steps {
        match step {
            Step::SubPipeline { sub_pipeline, .. } => {
                if !sub_pipeline.starts_with(&prefix) {
                    return Err(CatalogError::CrossBrokerComposition {
                        caller: caller_id.to_string(),
                        callee: sub_pipeline.clone(),
                    });
                }
            }
            Step::Guard { inner, .. } => {
                validate_steps_intra_broker(
                    std::slice::from_ref(inner),
                    caller_id,
                    broker_id,
                )?;
            }
            Step::Branch {
                if_true, if_false, ..
            } => {
                validate_steps_intra_broker(
                    std::slice::from_ref(if_true),
                    caller_id,
                    broker_id,
                )?;
                validate_steps_intra_broker(
                    std::slice::from_ref(if_false),
                    caller_id,
                    broker_id,
                )?;
            }
            Step::Leaf { .. } => {}
        }
    }
    Ok(())
}

/// Evaluate a precondition expression against an Overlay snapshot + dispatch
/// params. Returns `Ok(true)` if the precondition holds; `Ok(false)` if it
/// doesn't. `Err` only on malformed expressions.
///
/// Syntax (Wave 2 MVP):
/// - `<path>` — true if `<path>` exists in overlay
/// - `<path> = <value>` — true if path's value equals literal `<value>`
/// - `<path> != <value>` — true if path's value differs from `<value>`
/// - `{param_name}` substitution from `params`
///
/// Path syntax: dotted segments. Numeric segments index into arrays;
/// non-numeric segments index into objects.
pub fn evaluate_precondition(
    expression: &str,
    overlay: &serde_json::Value,
    params: &ParamMap,
) -> Result<bool, CatalogError> {
    let resolved = substitute_params(expression, params);

    // Try inequality first (longer operator); then equality; then plain path
    if let Some((path, value)) = resolved.split_once("!=") {
        let lhs = lookup_path(overlay, path.trim());
        match lhs {
            Some(v) => Ok(!values_equal(v, value.trim())),
            None => Ok(true), // missing != value is "they don't match"
        }
    } else if let Some((path, value)) = resolved.split_once('=') {
        let lhs = lookup_path(overlay, path.trim());
        match lhs {
            Some(v) => Ok(values_equal(v, value.trim())),
            None => Ok(false),
        }
    } else {
        Ok(lookup_path(overlay, resolved.trim()).is_some())
    }
}

/// Substitute `{param_name}` tokens with values from `params` (stringified).
fn substitute_params(expression: &str, params: &ParamMap) -> String {
    let mut result = expression.to_string();
    for (key, value) in params {
        let placeholder = format!("{{{}}}", key);
        let replacement = match value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        result = result.replace(&placeholder, &replacement);
    }
    result
}

/// Look up a dotted path against a JSON value.
fn lookup_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.').filter(|s| !s.is_empty()) {
        if let Ok(idx) = segment.parse::<usize>() {
            current = current.get(idx)?;
        } else {
            current = current.get(segment)?;
        }
    }
    Some(current)
}

/// Compare a JSON value against a literal string. Number/bool/string-coerced.
fn values_equal(lhs: &serde_json::Value, rhs_literal: &str) -> bool {
    match lhs {
        serde_json::Value::String(s) => s == rhs_literal,
        serde_json::Value::Number(n) => n.to_string() == rhs_literal,
        serde_json::Value::Bool(b) => b.to_string() == rhs_literal,
        serde_json::Value::Null => rhs_literal == "null",
        _ => lhs.to_string() == rhs_literal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{AuditClass, EffectClass, Tunability, Visibility};

    fn fixture_pipeline(id: &str, steps: Vec<Step>) -> Pipeline {
        Pipeline {
            id: id.to_string(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorOnly,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::HotStoreUpdate,
            params: serde_json::json!({}),
            preconditions: vec![],
            steps,
            description: String::new(),
            when_to_use: String::new(),
        }
    }

    #[test]
    fn validate_catalog_accepts_well_formed() {
        let pipelines = vec![fixture_pipeline(
            "work-broker/dispatch-work-unit",
            vec![Step::Leaf {
                leaf_op: "claim".to_string(),
            }],
        )];
        validate_catalog(&pipelines, "work-broker").unwrap();
    }

    #[test]
    fn validate_catalog_rejects_wrong_prefix() {
        let pipelines = vec![fixture_pipeline(
            "OTHER-broker/dispatch-work-unit",
            vec![],
        )];
        let err = validate_catalog(&pipelines, "work-broker").unwrap_err();
        assert!(matches!(err, CatalogError::PipelineInvalid { .. }));
    }

    #[test]
    fn validate_catalog_rejects_duplicates() {
        let pipelines = vec![
            fixture_pipeline("work-broker/x", vec![]),
            fixture_pipeline("work-broker/x", vec![]),
        ];
        let err = validate_catalog(&pipelines, "work-broker").unwrap_err();
        assert!(matches!(err, CatalogError::DuplicatePipelineId(_)));
    }

    #[test]
    fn validate_catalog_rejects_cross_broker_sub_pipeline() {
        let pipelines = vec![fixture_pipeline(
            "work-broker/parent",
            vec![Step::SubPipeline {
                sub_pipeline: "OTHER-broker/child".to_string(),
                params: ParamMap::new(),
            }],
        )];
        let err = validate_catalog(&pipelines, "work-broker").unwrap_err();
        assert!(matches!(err, CatalogError::CrossBrokerComposition { .. }));
    }

    #[test]
    fn validate_catalog_allows_intra_broker_sub_pipeline() {
        let pipelines = vec![
            fixture_pipeline(
                "work-broker/parent",
                vec![Step::SubPipeline {
                    sub_pipeline: "work-broker/child".to_string(),
                    params: ParamMap::new(),
                }],
            ),
            fixture_pipeline("work-broker/child", vec![]),
        ];
        validate_catalog(&pipelines, "work-broker").unwrap();
    }

    #[test]
    fn validate_catalog_rejects_cross_broker_in_guard_step() {
        let pipelines = vec![fixture_pipeline(
            "work-broker/p",
            vec![Step::Guard {
                predicate: "x".to_string(),
                inner: Box::new(Step::SubPipeline {
                    sub_pipeline: "EVIL/sub".to_string(),
                    params: ParamMap::new(),
                }),
            }],
        )];
        let err = validate_catalog(&pipelines, "work-broker").unwrap_err();
        assert!(matches!(err, CatalogError::CrossBrokerComposition { .. }));
    }

    #[test]
    fn evaluate_precondition_field_existence() {
        let overlay = serde_json::json!({"active_work": [{"id": "B-12"}]});
        let params = ParamMap::new();
        assert!(evaluate_precondition("active_work", &overlay, &params).unwrap());
        assert!(!evaluate_precondition("missing", &overlay, &params).unwrap());
    }

    #[test]
    fn evaluate_precondition_path_traversal() {
        let overlay = serde_json::json!({
            "active_work": [{"id": "B-12", "status": "ready"}]
        });
        let params = ParamMap::new();
        assert!(evaluate_precondition("active_work.0.id", &overlay, &params).unwrap());
        assert!(!evaluate_precondition("active_work.1.id", &overlay, &params).unwrap());
    }

    #[test]
    fn evaluate_precondition_equality() {
        let overlay = serde_json::json!({
            "broker_status": "active",
            "queue_depth": 5
        });
        let params = ParamMap::new();
        assert!(evaluate_precondition(
            "broker_status = active",
            &overlay,
            &params
        )
        .unwrap());
        assert!(!evaluate_precondition(
            "broker_status = errored",
            &overlay,
            &params
        )
        .unwrap());
        assert!(evaluate_precondition("queue_depth = 5", &overlay, &params).unwrap());
    }

    #[test]
    fn evaluate_precondition_inequality() {
        let overlay = serde_json::json!({"broker_status": "active"});
        let params = ParamMap::new();
        assert!(evaluate_precondition(
            "broker_status != errored",
            &overlay,
            &params
        )
        .unwrap());
        assert!(!evaluate_precondition(
            "broker_status != active",
            &overlay,
            &params
        )
        .unwrap());
    }

    #[test]
    fn evaluate_precondition_param_substitution() {
        let overlay = serde_json::json!({
            "active_work": {
                "B-12": {"status": "ready"},
                "B-13": {"status": "blocked"}
            }
        });
        let mut params = ParamMap::new();
        params.insert(
            "work_unit_id".to_string(),
            serde_json::Value::String("B-12".to_string()),
        );
        assert!(evaluate_precondition(
            "active_work.{work_unit_id}.status = ready",
            &overlay,
            &params
        )
        .unwrap());

        params.insert(
            "work_unit_id".to_string(),
            serde_json::Value::String("B-13".to_string()),
        );
        assert!(!evaluate_precondition(
            "active_work.{work_unit_id}.status = ready",
            &overlay,
            &params
        )
        .unwrap());
    }

    #[test]
    fn load_catalog_from_yaml() {
        let yaml = r#"
- id: work-broker/test-pipeline
  visibility: surfaced
  tunability: operator-only
  audit_class: capability
  effect_class: hot-store-update
  preconditions: ["active_work"]
  steps:
    - step_type: leaf
      leaf_op: claim
  description: "test pipeline"
  when_to_use: "for testing"
"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipelines = load_catalog(tmp.path()).unwrap();
        assert_eq!(pipelines.len(), 1);
        assert_eq!(pipelines[0].id, "work-broker/test-pipeline");
        validate_catalog(&pipelines, "work-broker").unwrap();
    }
}
