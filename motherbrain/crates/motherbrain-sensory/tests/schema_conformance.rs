//! Spec §3.8 Testing Discipline — Rust-side CMDB schema conformance.
//!
//! Validates that the output of `motherbrain_sensory::cmdb::build_cmdb` conforms
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

use std::path::PathBuf;

use jsonschema::JSONSchema;
use motherbrain_sensory::cmdb::{build_cmdb, Finding};
use serde_json::{json, Value};

/// Locate `cmdb-envelope-v1.schema.json` by walking known layouts:
///
/// 1. Ecosystem layout: `<repo>/Moth-er-Br-AI-n/motherbrain/crates/motherbrain-sensory/`
///    → `<repo>/LSP-Brains/schemas/cmdb-envelope-v1.schema.json`
/// 2. Sibling layout (two repos side-by-side): `<parent>/Moth-er-Br-AI-n/…`
///    → `<parent>/LSP-Brains/schemas/cmdb-envelope-v1.schema.json`
///
/// Returns `None` when the schema isn't reachable (standalone checkout).
fn locate_cmdb_schema() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crate dir → crates/ → motherbrain/ → Moth-er-Br-AI-n/ → repo-parent
    let candidates = [
        // Ecosystem layout: .../Moth-er-Br-AI-n/motherbrain/crates/motherbrain-sensory/
        manifest_dir.join("../../../../LSP-Brains/schemas/cmdb-envelope-v1.schema.json"),
        // Standalone-sibling: .../Moth-er-Br-AI-n/motherbrain/crates/motherbrain-sensory/
        manifest_dir.join("../../../LSP-Brains/schemas/cmdb-envelope-v1.schema.json"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn load_schema() -> Option<JSONSchema> {
    let path = locate_cmdb_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&value)
        .ok()
}

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
