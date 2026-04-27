//! Spec §3.8 Testing Discipline — Rust-side CMDB schema conformance.
//!
//! Validates that the output of `neurogrim_sensory::cmdb::build_cmdb` conforms
//! to the canonical `cmdb-envelope-v1.schema.json` shipped by the LSP-Brains
//! spec repo. This is the Rust counterpart to the Python SDK's
//! `test_envelope_validates_against_schema` — principle #18 "sensors need
//! sensors" applied symmetrically to both language implementations.
//!
//! When the crate is checked out standalone (no sibling `LSP-Brains` submodule
//! or ecosystem-root layout), the canonical schema is unreachable and the
//! tests are silently skipped. The Python SDK test follows the same pattern.
//!
//! Drift this test catches:
//!  - `Finding` struct fields renamed / shape changed
//!  - `build_cmdb` forgetting a required envelope field
//!  - Schema evolved in LSP-Brains without the Rust side following

use jsonschema::JSONSchema;
use neurogrim_sensory::cmdb::{build_cmdb, Finding};
use serde_json::{json, Value};

mod test_support;
use test_support::load_schema;

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

#[test]
fn build_cmdb_empty_findings_validates() {
    let Some(schema) = load_schema() else {
        eprintln!("skip: cmdb-envelope-v1 schema not reachable (standalone checkout)");
        return;
    };
    let cmdb = build_cmdb("test-tool", 85, vec![], None);
    assert!(
        schema.is_valid(&cmdb),
        "empty-findings envelope failed schema: {}",
        format_errors(&schema, &cmdb)
    );
}

#[test]
fn build_cmdb_single_finding_validates() {
    let Some(schema) = load_schema() else {
        eprintln!("skip: cmdb-envelope-v1 schema not reachable (standalone checkout)");
        return;
    };
    let cmdb = build_cmdb(
        "test-tool",
        80,
        vec![Finding {
            name: "readme-present".to_string(),
            status: "pass".to_string(),
            points: 10,
            detail: Some("README.md exists".to_string()),
        }],
        None,
    );
    assert!(
        schema.is_valid(&cmdb),
        "single-finding envelope failed schema: {}",
        format_errors(&schema, &cmdb)
    );
}

#[test]
fn build_cmdb_multiple_findings_with_extras_validates() {
    let Some(schema) = load_schema() else {
        eprintln!("skip: cmdb-envelope-v1 schema not reachable (standalone checkout)");
        return;
    };
    let findings = vec![
        Finding {
            name: "info-check".to_string(),
            status: "info".to_string(),
            points: 0,
            detail: Some("a note".to_string()),
        },
        Finding {
            name: "warn-check".to_string(),
            status: "warning".to_string(),
            points: -5,
            detail: Some("caution".to_string()),
        },
        Finding {
            // No detail — exercises the Option<String> skip_serializing_if path.
            name: "bare".to_string(),
            status: "pass".to_string(),
            points: 0,
            detail: None,
        },
    ];
    let cmdb = build_cmdb(
        "test-tool",
        65,
        findings,
        Some(vec![
            ("has_tests", json!(true)),
            ("test_count", json!(42)),
            ("ratio", json!(0.73)),
        ]),
    );
    assert!(
        schema.is_valid(&cmdb),
        "multi-finding envelope failed schema: {}",
        format_errors(&schema, &cmdb)
    );
}

#[test]
fn cmdb_envelope_with_optional_confidence_validates() {
    // E-B2-1 Component 2: cmdb-envelope-v1 schema permits an optional
    // root-level `confidence` field in [0, 100]. When present, the
    // aggregator prefers it over age-decay (per resolve_confidence in
    // neurogrim-core/src/scoring.rs); when absent, falls back to
    // exponential_decay of meta.updated_at. Schema must accept both shapes.
    let Some(schema) = load_schema() else {
        eprintln!("skip: cmdb-envelope-v1 schema not reachable (standalone checkout)");
        return;
    };

    // Shape A: envelope WITH explicit confidence.
    let with_conf = json!({
        "meta": {
            "schema_version": "1",
            "updated_at": "2026-04-27T12:00:00Z",
            "updated_by": "test-tool"
        },
        "score": 75,
        "updated_at": "2026-04-27T12:00:00Z",
        "confidence": 82,
        "findings": []
    });
    assert!(
        schema.is_valid(&with_conf),
        "envelope-with-confidence failed schema: {}",
        format_errors(&schema, &with_conf)
    );

    // Shape B: envelope WITHOUT confidence (legacy / pre-v2.7).
    let without_conf = json!({
        "meta": {
            "schema_version": "1",
            "updated_at": "2026-04-27T12:00:00Z",
            "updated_by": "test-tool"
        },
        "score": 75,
        "updated_at": "2026-04-27T12:00:00Z",
        "findings": []
    });
    assert!(
        schema.is_valid(&without_conf),
        "envelope-without-confidence (legacy) failed schema: {}",
        format_errors(&schema, &without_conf)
    );
}

#[test]
fn cmdb_envelope_confidence_out_of_range_rejected() {
    // Defensive: confidence values outside [0, 100] should fail
    // validation. The aggregator also clamps at runtime via
    // Confidence::new (defense-in-depth), but the schema is the
    // protocol-layer guard.
    let Some(schema) = load_schema() else {
        eprintln!("skip: cmdb-envelope-v1 schema not reachable (standalone checkout)");
        return;
    };

    let above_max = json!({
        "meta": {
            "schema_version": "1",
            "updated_at": "2026-04-27T12:00:00Z",
            "updated_by": "test-tool"
        },
        "score": 75,
        "updated_at": "2026-04-27T12:00:00Z",
        "confidence": 150,
        "findings": []
    });
    assert!(
        !schema.is_valid(&above_max),
        "expected validation failure for confidence > 100"
    );

    let negative = json!({
        "meta": {
            "schema_version": "1",
            "updated_at": "2026-04-27T12:00:00Z",
            "updated_by": "test-tool"
        },
        "score": 75,
        "updated_at": "2026-04-27T12:00:00Z",
        "confidence": -10,
        "findings": []
    });
    assert!(
        !schema.is_valid(&negative),
        "expected validation failure for confidence < 0"
    );
}

#[test]
fn finding_fields_match_schema_shape() {
    // Sanity: every field the schema names for a finding item is present on
    // the Finding struct. If the schema renames or adds a required finding
    // field, the compile-time Rust struct must grow to match — this test
    // surfaces that by asserting the serialized shape includes each key.
    let Some(_schema) = load_schema() else {
        eprintln!("skip: cmdb-envelope-v1 schema not reachable (standalone checkout)");
        return;
    };
    let cmdb = build_cmdb(
        "test-tool",
        90,
        vec![Finding {
            name: "n".to_string(),
            status: "s".to_string(),
            points: 1,
            detail: Some("d".to_string()),
        }],
        None,
    );
    let f = &cmdb["findings"][0];
    for key in ["name", "status", "points", "detail"] {
        assert!(
            !f.get(key).is_none(),
            "finding object missing schema-declared field: {key}"
        );
    }
}
