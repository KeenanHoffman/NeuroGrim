//! Spec §16.8 / E-B2-4 C1+C2 — Trust-budget schema conformance.
//!
//! Validates the per-Brain trust-budget schema and its four on-disk
//! TOML fixtures (per Q5 — repo-root committed; per Q1 — closed-set
//! `Ecosystem` and `TrustPosture` vocabularies). This file pins five
//! concerns:
//!
//!  A. **Valid fixture** — fully populated trust-budget validates;
//!     `schema_version == "1"` is pinned to catch any regression in
//!     const enforcement (per E4-7 forward-compat).
//!  B. **Missing-required fixture** — TOML lacks `schema_version`;
//!     validation fails with an error path that mentions
//!     `schema_version` (distinguishes from C and D).
//!  C. **Additional-properties fixture** — TOML declares an unknown
//!     top-level field; validation fails because of
//!     `additionalProperties: false`. The error path must NOT mention
//!     `schema_version` or any required field; it must mention the
//!     unknown property.
//!  D. **Unknown-trust-posture fixture** — TOML declares a
//!     `trust_posture` value outside the closed enum (per Q1);
//!     validation fails on the closed-set vocabulary, distinct from B
//!     and C.
//!  E. **Closed-set discipline pin** — schema's
//!     `definitions.Ecosystem.enum` has exactly the 4 locked
//!     ecosystem values, and `definitions.TrustPosture.enum` has
//!     exactly the 4 locked trust-posture values. Mirror of E-B2-3 C2
//!     Test D `schema_tool_name_enum_has_exactly_eight_entries` —
//!     vocabulary changes surface as deliberate test edits, not
//!     silent drift.
//!
//! When the LSP-Brains submodule isn't reachable (standalone
//! checkout), tests skip with an eprintln — same convention as the
//! cmdb-envelope, agent-output, calibration-ledger, and hat-contract
//! schema-conformance suites.

use jsonschema::JSONSchema;
use serde_json::Value;

mod test_support;
use test_support::{
    load_trust_budget_schema, locate_trust_budget_fixture, read_trust_budget_schema_value,
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

/// Parse a fixture TOML file and convert it to a `serde_json::Value`
/// so the jsonschema validator (which is JSON-typed) can consume it.
/// TOML's data model is a strict superset of JSON's for our schema's
/// needs: strings, arrays, booleans, and nested tables only.
fn read_trust_budget_fixture_as_json(name: &str) -> Value {
    let path = locate_trust_budget_fixture(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let toml_value: toml::Value = toml::from_str(&raw)
        .unwrap_or_else(|e| panic!("parse fixture {} as TOML: {e}", path.display()));
    serde_json::to_value(toml_value)
        .unwrap_or_else(|e| panic!("convert fixture {} to JSON: {e}", path.display()))
}

// ─── Test case A — valid fixture ──────────────────────────────────────

#[test]
fn fixture_valid_validates() {
    let Some(schema) = load_trust_budget_schema() else {
        eprintln!("skip: trust-budget-v1 schema not reachable (standalone checkout)");
        return;
    };
    let instance = read_trust_budget_fixture_as_json("trust-budget-valid.toml");
    assert!(
        schema.is_valid(&instance),
        "valid fixture failed schema: {}",
        format_errors(&schema, &instance)
    );

    // E4-7 forward-compat: pin `schema_version == "1"` so a future
    // const-bump regression surfaces here rather than silently
    // passing.
    assert_eq!(
        instance["schema_version"], "1",
        "valid fixture must declare schema_version = \"1\" (E4-7 forward-compat)"
    );

    // Belt-and-braces: pin a load-bearing surface so a fixture edit
    // that drops the populated declared_* arrays surfaces here.
    assert!(
        instance["declared_crates"]
            .as_array()
            .expect("declared_crates is array")
            .iter()
            .any(|v| v["name"] == "serde" && v["ecosystem"] == "cargo"),
        "valid fixture must declare serde/cargo in declared_crates"
    );
    assert!(
        instance["declared_external_services"]
            .as_array()
            .expect("declared_external_services is array")
            .iter()
            .any(|v| v["fqdn"] == "osv.dev" && v["trust_posture"] == "api_only"),
        "valid fixture must declare osv.dev/api_only in declared_external_services"
    );
}

// ─── Test case B — missing-required fixture ───────────────────────────

#[test]
fn fixture_missing_required_rejected() {
    let Some(schema) = load_trust_budget_schema() else {
        eprintln!("skip: trust-budget-v1 schema not reachable (standalone checkout)");
        return;
    };
    let instance = read_trust_budget_fixture_as_json("trust-budget-missing-required.toml");

    // Per E4-7 + Q5: schema_version is the only required top-level
    // field. Validation must fail when it's missing.
    assert!(
        !schema.is_valid(&instance),
        "expected missing-required fixture to fail schema; got valid"
    );

    // Distinguish the failure mode from C (additionalProperties) and
    // D (unknown trust_posture) by inspecting the error text. The
    // jsonschema crate reports missing-required errors mentioning the
    // missing field name.
    let errors_text = format_errors(&schema, &instance);
    assert!(
        errors_text.contains("schema_version"),
        "expected validation error to mention `schema_version` (the missing required field); got:\n{}",
        errors_text
    );
}

// ─── Test case C — additional-properties fixture ──────────────────────

#[test]
fn fixture_additional_properties_rejected() {
    let Some(schema) = load_trust_budget_schema() else {
        eprintln!("skip: trust-budget-v1 schema not reachable (standalone checkout)");
        return;
    };
    let instance = read_trust_budget_fixture_as_json("trust-budget-additional-properties.toml");

    // Per E4-7 + culture-manifest-v1 template: top-level unknown
    // fields are rejected. A future contributor wanting a new field
    // must version-bump (additive) or use the explicit `extensions`
    // forward-compat surface — not smuggle it in at the top level.
    assert!(
        !schema.is_valid(&instance),
        "expected additional-properties fixture to fail schema; got valid"
    );

    // Distinguish from B (missing schema_version) and D (unknown
    // trust_posture): the error must mention the offending unknown
    // property and must NOT mention schema_version (which IS present
    // in this fixture) or trust_posture (no service has a bad
    // posture in this fixture).
    let errors_text = format_errors(&schema, &instance);
    assert!(
        errors_text.contains("secretly_allowed_field"),
        "expected validation error to mention the unknown property `secretly_allowed_field`; got:\n{}",
        errors_text
    );
    assert!(
        !errors_text.contains("trust_posture"),
        "expected additionalProperties failure to be distinct from trust_posture enum failure; got:\n{}",
        errors_text
    );
    // The error path for additionalProperties violations does NOT
    // include `/schema_version` — this fixture has schema_version
    // present and valid. If the error text references it, the failure
    // mode has been confused with case B.
    assert!(
        !errors_text.contains("/schema_version"),
        "expected additionalProperties failure path NOT to mention /schema_version (case B); got:\n{}",
        errors_text
    );
}

// ─── Test case D — unknown-trust-posture fixture ──────────────────────

#[test]
fn fixture_unknown_trust_posture_rejected() {
    let Some(schema) = load_trust_budget_schema() else {
        eprintln!("skip: trust-budget-v1 schema not reachable (standalone checkout)");
        return;
    };
    let instance = read_trust_budget_fixture_as_json("trust-budget-unknown-trust-posture.toml");

    // Per Q1: closed-set vocabulary. The fixture's `trust_posture` is
    // `trust_me_bro`, which is not in the 4-entry locked enum
    // (api_only / official_registry / operator_audited /
    // vendor_attested). Validation must fail.
    assert!(
        !schema.is_valid(&instance),
        "expected unknown-trust-posture fixture to fail schema; got valid"
    );

    // Distinguish from B (missing schema_version — not the case here,
    // it's present) and C (additionalProperties — not the case here,
    // every property is in the schema). The error path must touch
    // `/declared_external_services` and the bad value must surface.
    let errors_text = format_errors(&schema, &instance);
    assert!(
        errors_text.contains("trust_posture") || errors_text.contains("trust_me_bro"),
        "expected validation error path to mention trust_posture or the bad value `trust_me_bro` (closed-set enum mismatch); got:\n{}",
        errors_text
    );
}

// ─── Test case E — closed-set discipline pin ──────────────────────────

#[test]
fn closed_set_enums_have_exactly_locked_entries() {
    // Per Q1: the closed vocabularies have exactly the locked sizes.
    // Future schema edits that add vocabulary (additive only per
    // charter rollout discipline #2) will fail this test until the
    // expected sets are updated explicitly — same discipline as
    // `culture-manifest-v1.values` and `hat-contract-v1.ToolName`.
    let Some(schema_value) = read_trust_budget_schema_value() else {
        eprintln!("skip: trust-budget-v1 schema not reachable (standalone checkout)");
        return;
    };

    // ── Ecosystem enum ────────────────────────────────────────────
    let ecosystem_values = schema_value
        .pointer("/definitions/Ecosystem/enum")
        .and_then(|v| v.as_array())
        .expect("schema must define /definitions/Ecosystem/enum as an array");

    let expected_ecosystems: Vec<&str> = vec!["cargo", "pypi", "npm", "system"];
    assert_eq!(
        ecosystem_values.len(),
        expected_ecosystems.len(),
        "Ecosystem closed-set vocabulary must have exactly {} entries (Q1); got {}: {:?}",
        expected_ecosystems.len(),
        ecosystem_values.len(),
        ecosystem_values
    );
    let actual_ecosystems: Vec<String> = ecosystem_values
        .iter()
        .map(|v| {
            v.as_str()
                .expect("Ecosystem enum entry must be a string")
                .to_string()
        })
        .collect();
    for term in &expected_ecosystems {
        assert!(
            actual_ecosystems.iter().any(|a| a == term),
            "Ecosystem closed-set vocabulary must contain `{}` (Q1); got {:?}",
            term,
            actual_ecosystems
        );
    }

    // ── TrustPosture enum ─────────────────────────────────────────
    let posture_values = schema_value
        .pointer("/definitions/TrustPosture/enum")
        .and_then(|v| v.as_array())
        .expect("schema must define /definitions/TrustPosture/enum as an array");

    let expected_postures: Vec<&str> = vec![
        "api_only",
        "official_registry",
        "operator_audited",
        "vendor_attested",
    ];
    assert_eq!(
        posture_values.len(),
        expected_postures.len(),
        "TrustPosture closed-set vocabulary must have exactly {} entries (Q1); got {}: {:?}",
        expected_postures.len(),
        posture_values.len(),
        posture_values
    );
    let actual_postures: Vec<String> = posture_values
        .iter()
        .map(|v| {
            v.as_str()
                .expect("TrustPosture enum entry must be a string")
                .to_string()
        })
        .collect();
    for term in &expected_postures {
        assert!(
            actual_postures.iter().any(|a| a == term),
            "TrustPosture closed-set vocabulary must contain `{}` (Q1); got {:?}",
            term,
            actual_postures
        );
    }

    // Defense-in-depth: assert obvious near-miss values are NOT in
    // the vocabulary. `trust_me_bro` appears in fixture D and must
    // remain firmly outside the closed set; `trusted` is the kind of
    // ambiguous one-word posture this discipline deliberately rejects.
    assert!(
        !actual_postures.iter().any(|a| a == "trust_me_bro"),
        "TrustPosture closed-set vocabulary MUST NOT contain `trust_me_bro` (test fixture D); got {:?}",
        actual_postures
    );
    assert!(
        !actual_postures.iter().any(|a| a == "trusted"),
        "TrustPosture closed-set vocabulary MUST NOT contain ambiguous `trusted` (Q1 — discriminate HOW, not just IS-trusted); got {:?}",
        actual_postures
    );
}
