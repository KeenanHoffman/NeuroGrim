//! Spec §6 / E-B2-1 C7 — Agent-output schema conformance.
//!
//! Validates that synthetic AgentOutput JSON shapes (both pre-v2.7
//! without `unified_confidence` and v2.7 with it) conform to the
//! canonical `agent-output-v1.schema.json` shipped by the LSP-Brains
//! spec repo. Companion to `schema_conformance.rs` which validates
//! the cmdb-envelope side.
//!
//! When the crate is checked out standalone (no sibling `LSP-Brains`
//! submodule or ecosystem-root layout), the canonical schema is
//! unreachable and the tests are silently skipped. Same pattern as
//! the CMDB conformance tests.
//!
//! Drift this test catches:
//!  - The v2.7 schema accidentally re-tightens `additionalProperties`
//!    or drops the `unified_confidence` property.
//!  - The schema accidentally requires `unified_confidence` (it
//!    MUST stay optional for v2.6 backward-compat).

use jsonschema::JSONSchema;
use serde_json::{json, Value};

mod test_support;
use test_support::load_agent_output_schema;

/// Render validation errors as a readable string for assertion output.
fn format_errors(schema: &JSONSchema, instance: &Value) -> String {
    let errors = schema.validate(instance);
    match errors {
        Ok(_) => String::from("(no errors)"),
        Err(errs) => errs
            .map(|e| format!("{} at {}", e, e.instance_path))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn minimal_agent_output(extra: Option<(&str, Value)>) -> Value {
    let mut output = json!({
        "schema_version": "1",
        "scored_at": "2026-04-27T12:00:00Z",
        "score": 75,
        "domains": {},
        "dirty_gates": [],
        "stale_artifacts": [],
        "domain_variables": {},
        "top_recommendations": [],
        "correlations_fired": [],
        "incident_patterns": [],
        "skipped_temporal": []
    });
    if let Some((key, value)) = extra {
        if let Some(obj) = output.as_object_mut() {
            obj.insert(key.to_string(), value);
        }
    }
    output
}

#[test]
fn agent_output_without_unified_confidence_validates() {
    // Pre-v2.7 shape (and v2.6 wire format from older A2A peers).
    // `unified_confidence` is omitted; schema MUST still accept the
    // payload — backward-compat for the four-Brain ecosystem upgrade.
    let Some(schema) = load_agent_output_schema() else {
        eprintln!("skip: agent-output-v1 schema not reachable (standalone checkout)");
        return;
    };
    let output = minimal_agent_output(None);
    assert!(
        schema.is_valid(&output),
        "v2.6-style AgentOutput (no unified_confidence) failed schema: {}",
        format_errors(&schema, &output)
    );
}

#[test]
fn agent_output_with_unified_confidence_validates() {
    // v2.7 shape — `unified_confidence` is present and well-formed.
    let Some(schema) = load_agent_output_schema() else {
        eprintln!("skip: agent-output-v1 schema not reachable (standalone checkout)");
        return;
    };
    let output = minimal_agent_output(Some(("unified_confidence", json!(82))));
    assert!(
        schema.is_valid(&output),
        "v2.7-style AgentOutput (unified_confidence=82) failed schema: {}",
        format_errors(&schema, &output)
    );
}

#[test]
fn agent_output_unified_confidence_zero_validates() {
    // The default-when-absent value (set by serde) is 0; round-trip
    // through the schema must accept zero without complaint.
    let Some(schema) = load_agent_output_schema() else {
        eprintln!("skip: agent-output-v1 schema not reachable (standalone checkout)");
        return;
    };
    let output = minimal_agent_output(Some(("unified_confidence", json!(0))));
    assert!(
        schema.is_valid(&output),
        "AgentOutput with unified_confidence=0 failed schema: {}",
        format_errors(&schema, &output)
    );
}

#[test]
fn agent_output_unified_confidence_above_100_rejected() {
    // Defensive: `>100` violates `maximum: 100`. Schema is the
    // protocol-layer guard; the aggregator's Confidence::new also
    // clamps at runtime.
    let Some(schema) = load_agent_output_schema() else {
        eprintln!("skip: agent-output-v1 schema not reachable (standalone checkout)");
        return;
    };
    let output = minimal_agent_output(Some(("unified_confidence", json!(200))));
    assert!(
        !schema.is_valid(&output),
        "expected validation failure for unified_confidence > 100"
    );
}

#[test]
fn agent_output_extra_unknown_field_validates() {
    // The v2.7 schema relaxed root `additionalProperties: false → true`
    // as a one-time forward-compat enabler. This test pins that
    // relaxation: any unknown field MUST be tolerated. If a future
    // contributor re-tightens additionalProperties, this test fires
    // and forces the discussion.
    let Some(schema) = load_agent_output_schema() else {
        eprintln!("skip: agent-output-v1 schema not reachable (standalone checkout)");
        return;
    };
    let output = minimal_agent_output(Some(("future_field_brains_2_5", json!("forward-compat"))));
    assert!(
        schema.is_valid(&output),
        "schema must tolerate unknown root fields per v2.7 additionalProperties relax: {}",
        format_errors(&schema, &output)
    );
}
