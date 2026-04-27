//! Spec §5.4.1 / E-B2-3 C1+C3 — Hat-contract schema conformance.
//!
//! Validates the persona-hat contract schema and its three on-disk
//! fixtures (per Q11). Persona-hat (Hat-model B per spec §5.4.1, Q5 = 5c)
//! — distinct from the registry-hat (Hat-model A; embedded in
//! `brain-registry-v2.schema.json:413-433`). This file pins five
//! concerns:
//!
//!  A. **Valid fixture** — fully populated frontmatter validates.
//!  B. **Invalid-vocabulary fixture** — frontmatter declares an unknown
//!     `forbidden_tools` value; validation fails specifically because of
//!     the closed-set enum mismatch (per Q1).
//!  C. **Missing-frontmatter fixture** — no `---` fences; detected as
//!     "no contract present" per Q4 — distinct from invalid-vocabulary
//!     (no schema-target object exists, so validation isn't applied).
//!  D. **Closed-set discipline** — schema's `definitions.ToolName.enum`
//!     contains exactly the 8 entries from Q1. Future schema edits that
//!     add vocabulary surface explicitly via this test.
//!  E. **`additionalProperties: false` discipline** — top-level unknown
//!     fields rejected; matches the `culture-manifest-v1` template Q1
//!     references.
//!
//! When the LSP-Brains submodule isn't reachable (standalone checkout),
//! tests skip with an eprintln — same convention as the cmdb-envelope,
//! agent-output, and calibration-ledger schema-conformance suites.

use jsonschema::JSONSchema;
use serde_json::{json, Value};

mod test_support;
use test_support::{
    load_hat_contract_schema, read_hat_contract_fixture, read_hat_contract_schema_value,
};

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

/// Result of attempting to extract YAML frontmatter from a markdown file.
///
/// The semantic distinction between `Missing` (no fences) and `Invalid`
/// (fences present but body fails the schema) is load-bearing per Q4 +
/// the §5.4.1 spec: missing frontmatter is a neutral "not yet promoted"
/// state, NOT a schema-validation failure.
#[derive(Debug)]
enum FrontmatterExtraction {
    /// File starts with `---` and contains a closing `---`; raw YAML
    /// body returned. The caller then YAML-parses + schema-validates.
    Found(String),
    /// File does not start with `---`. No schema-target object exists.
    Missing,
    /// File starts with `---` but no closing `---` was found before EOF.
    /// This is a malformed file rather than a missing-frontmatter file;
    /// the schema-conformance test currently treats it as a separate
    /// failure mode (none of the three fixtures exercise it).
    #[allow(dead_code)]
    Unterminated,
}

/// Extract YAML frontmatter delimited by `---` lines at the start of a
/// markdown file. Per Q4 + §5.4.1, the missing-frontmatter case is
/// distinct from the invalid-frontmatter case — surfaced as
/// `FrontmatterExtraction::Missing` rather than as a parse error.
fn extract_frontmatter(markdown: &str) -> FrontmatterExtraction {
    // Normalize line endings — fixtures may be checked out with CRLF on
    // Windows. We treat \r\n and \n identically for fence detection.
    let mut lines = markdown.split_inclusive('\n');
    let first = match lines.next() {
        Some(l) => l.trim_end(),
        None => return FrontmatterExtraction::Missing,
    };
    if first != "---" {
        return FrontmatterExtraction::Missing;
    }
    let mut yaml = String::new();
    for line in lines {
        if line.trim_end() == "---" {
            return FrontmatterExtraction::Found(yaml);
        }
        yaml.push_str(line);
    }
    FrontmatterExtraction::Unterminated
}

/// Convert a `serde_yaml::Value` into a `serde_json::Value` so the
/// jsonschema validator (which is JSON-typed) can consume it. YAML's
/// data model is a strict superset of JSON's for our schema's needs:
/// strings, arrays, and nested mappings only.
fn yaml_to_json(yaml: &str) -> Result<Value, String> {
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| format!("yaml parse: {e}"))?;
    serde_json::to_value(yaml_value).map_err(|e| format!("yaml→json: {e}"))
}

// ─── Test case A — valid fixture ──────────────────────────────────────

#[test]
fn fixture_valid_validates() {
    let Some(schema) = load_hat_contract_schema() else {
        eprintln!("skip: hat-contract-v1 schema not reachable (standalone checkout)");
        return;
    };
    let raw = read_hat_contract_fixture("hat-contract-valid.md");
    let yaml = match extract_frontmatter(&raw) {
        FrontmatterExtraction::Found(y) => y,
        other => panic!(
            "valid fixture must have well-formed frontmatter; got {:?}",
            other
        ),
    };
    let instance = yaml_to_json(&yaml).expect("valid fixture frontmatter parses as YAML");
    assert!(
        schema.is_valid(&instance),
        "valid fixture failed schema: {}",
        format_errors(&schema, &instance)
    );

    // Pin the load-bearing fields so a fixture edit that drops them
    // surfaces here rather than silently passing.
    assert_eq!(instance["name"], "supply-chain-auditor");
    assert!(
        instance["forbidden_tools"]
            .as_array()
            .expect("forbidden_tools is array")
            .iter()
            .any(|v| v == "package_install"),
        "valid fixture must declare package_install in forbidden_tools (Q4 nuance — populated, not permissive default)"
    );
}

// ─── Test case B — invalid-vocabulary fixture ─────────────────────────

#[test]
fn fixture_invalid_vocabulary_rejected_with_enum_mismatch() {
    let Some(schema) = load_hat_contract_schema() else {
        eprintln!("skip: hat-contract-v1 schema not reachable (standalone checkout)");
        return;
    };
    let raw = read_hat_contract_fixture("hat-contract-invalid-vocabulary.md");
    let yaml = match extract_frontmatter(&raw) {
        FrontmatterExtraction::Found(y) => y,
        other => panic!(
            "invalid-vocabulary fixture must have frontmatter (failure mode is the enum, not the fences); got {:?}",
            other
        ),
    };
    let instance =
        yaml_to_json(&yaml).expect("invalid-vocabulary fixture frontmatter parses as YAML");

    // Per Q1: closed-set enum rejection. The fixture's `forbidden_tools`
    // contains `assassinate_prod`, which is not in the 8-entry vocabulary.
    assert!(
        !schema.is_valid(&instance),
        "expected invalid-vocabulary fixture to fail schema; got valid"
    );

    // Distinguish the failure mode from missing-frontmatter (case C) and
    // additionalProperties (case E) by inspecting where the validator
    // complained. The error path must touch /forbidden_tools.
    let errors_text = format_errors(&schema, &instance);
    assert!(
        errors_text.contains("/forbidden_tools"),
        "expected validation error path to mention /forbidden_tools (closed-set enum mismatch); got:\n{}",
        errors_text
    );
}

// ─── Test case C — missing-frontmatter fixture ────────────────────────

#[test]
fn fixture_missing_frontmatter_detected_as_no_contract() {
    // Per Q4: a hat without frontmatter is a neutral "no contract
    // present" state, NOT a schema-validation failure. The
    // schema-conformance test pins this distinction by asserting we
    // never even reach the JSON-schema validator for this fixture.
    let raw = read_hat_contract_fixture("hat-contract-missing-frontmatter.md");
    match extract_frontmatter(&raw) {
        FrontmatterExtraction::Missing => {}
        other => panic!(
            "missing-frontmatter fixture must be detected as Missing (NOT validated against schema); got {:?}",
            other
        ),
    }

    // Belt-and-braces: assert the fixture genuinely has no `---` fence
    // at the top, beyond what `extract_frontmatter` already returned.
    assert!(
        !raw.trim_start().starts_with("---"),
        "missing-frontmatter fixture must not begin with `---`"
    );
}

// ─── Test case D — closed-set discipline pin ──────────────────────────

#[test]
fn schema_tool_name_enum_has_exactly_eight_entries() {
    // Per Q1: the closed vocabulary has exactly 8 entries. Future
    // schema edits that add vocabulary (additive only per charter
    // rollout discipline #2) will fail this test until the expected
    // set is updated explicitly — same discipline as
    // `culture-manifest-v1.values`. Ensures vocabulary changes are a
    // visible, deliberate event rather than a silent drift.
    let Some(schema_value) = read_hat_contract_schema_value() else {
        eprintln!("skip: hat-contract-v1 schema not reachable (standalone checkout)");
        return;
    };

    let enum_values = schema_value
        .pointer("/definitions/ToolName/enum")
        .and_then(|v| v.as_array())
        .expect("schema must define /definitions/ToolName/enum as an array");

    let expected: Vec<&str> = vec![
        "Bash",
        "Write",
        "Edit",
        "WebFetch",
        "WebSearch",
        "network_egress",
        "mcp:*",
        "package_install",
    ];

    assert_eq!(
        enum_values.len(),
        expected.len(),
        "closed-set vocabulary must have exactly {} entries (Q1); got {}: {:?}",
        expected.len(),
        enum_values.len(),
        enum_values
    );

    let actual: Vec<String> = enum_values
        .iter()
        .map(|v| v.as_str().expect("enum entry must be a string").to_string())
        .collect();
    for term in &expected {
        assert!(
            actual.iter().any(|a| a == term),
            "closed-set vocabulary must contain `{}` (Q1); got {:?}",
            term,
            actual
        );
    }

    // Defense-in-depth: assert `Read` is NOT in the vocabulary. The
    // master plan stub had it as an example value; Q1 rejected abstract
    // tool surfaces and `Read` (a Claude Code read-only tool) is
    // intentionally NOT a forbiddable tool — read-only operations are
    // never the load-bearing anti-capability claim.
    assert!(
        !actual.iter().any(|a| a == "Read"),
        "closed-set vocabulary MUST NOT contain `Read` (Q1 + plan brief — read-only is not the load-bearing anti-capability axis); got {:?}",
        actual
    );
}

// ─── Test case E — additionalProperties: false discipline ─────────────

#[test]
fn unknown_top_level_field_rejected() {
    // Per Q1 + culture-manifest-v1 template: top-level unknown fields
    // are rejected. A future contributor wanting a new field must
    // version-bump (additive) rather than smuggle it in.
    let Some(schema) = load_hat_contract_schema() else {
        eprintln!("skip: hat-contract-v1 schema not reachable (standalone checkout)");
        return;
    };
    let instance = json!({
        "name": "smuggled-hat",
        "description": "Test fixture — additionalProperties: false discipline.",
        "secretly_allowed": true
    });
    assert!(
        !schema.is_valid(&instance),
        "expected unknown top-level field `secretly_allowed` to fail schema (additionalProperties: false); got valid"
    );

    // Distinguish from case B (forbidden_tools enum mismatch): the
    // error path here must NOT touch /forbidden_tools.
    let errors_text = format_errors(&schema, &instance);
    assert!(
        !errors_text.contains("/forbidden_tools"),
        "expected additionalProperties failure to be distinct from forbidden_tools enum failure; got:\n{}",
        errors_text
    );
}
