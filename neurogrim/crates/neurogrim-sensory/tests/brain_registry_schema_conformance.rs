//! E-B2-2 C5 — Schema-conformance for the brain-registry-v2
//! additions: optional `enable_calibration_writes` flag at the
//! `config` block + optional `calibration_trigger` discriminated
//! union on each domain definition.
//!
//! These tests pin the JSON Schema additions independently of the
//! Rust serde round-trip tests in
//! `neurogrim-core/src/calibration_ledger.rs` (which only verify
//! Rust↔JSON via serde). Together they close the loop: serde
//! produces the right JSON shape → that JSON shape validates
//! against the schema → operators get JSON Schema-driven errors
//! at registry-load time.
//!
//! Skip-when-absent convention (matches the cmdb-envelope and
//! agent-output schema tests): when the LSP-Brains schemas dir
//! isn't reachable (standalone checkout), the test is silently
//! skipped — `load_brain_registry_schema()` returns `None`.

mod test_support;

use serde_json::{json, Value};
use test_support::load_brain_registry_schema;

/// Minimal valid v2 brain-registry skeleton; tests append the
/// fields under test (calibration_trigger, enable_calibration_writes)
/// onto this baseline.
fn minimal_registry() -> Value {
    json!({
        "meta": {
            "schema_version": "2",
            "description": "test fixture",
            "updated_by": "test-suite"
        },
        "tools": {},
        "data_sources": {},
        "config": {
            "domain_weights": {
                "domain-calibration": 0.0
            },
            "advisory_domains": ["domain-calibration"],
            "domain_definitions": {
                "domain-calibration": {
                    "scoring_source": {
                        "type": "cmdb",
                        "path": ".claude/domain-calibration-cmdb.json"
                    }
                }
            },
            "scoring": { "model": "multiplier" },
            "gate_tiers": {
                "before-merge": { "scoring_weight": 0.5, "priority_weight": 1.0 }
            },
            "confidence_thresholds": {
                "cmdb_fresh_days": 1,
                "cmdb_stale_days": 3,
                "cmdb_very_stale_days": 7
            },
            "autonomy": {
                "levels": {},
                "action_types": {},
                "safety_invariants": []
            }
        }
    })
}

/// Inject a `calibration_trigger` value onto the
/// `domain-calibration` domain definition. Returns the mutated
/// registry value for validation.
fn with_trigger(mut reg: Value, trigger: Value) -> Value {
    reg["config"]["domain_definitions"]["domain-calibration"]["calibration_trigger"] = trigger;
    reg
}

// ─── Happy paths: each discriminated-union variant validates ─────

#[test]
fn baseline_registry_validates() {
    let Some(schema) = load_brain_registry_schema() else { return };
    assert!(
        schema.is_valid(&minimal_registry()),
        "baseline registry without calibration fields must validate"
    );
}

#[test]
fn out_of_expected_range_variant_validates() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(
        minimal_registry(),
        json!({ "kind": "out-of-expected-range", "min": 70, "max": 95 }),
    );
    assert!(
        schema.is_valid(&reg),
        "out-of-expected-range trigger must validate; errors: {:?}",
        schema.validate(&reg).err().map(|e| e.collect::<Vec<_>>())
    );
}

#[test]
fn signal_class_fired_variant_validates() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(
        minimal_registry(),
        json!({
            "kind": "signal-class-fired",
            "signal_kinds": ["pattern:typosquat", "pattern:publish-cadence"]
        }),
    );
    assert!(
        schema.is_valid(&reg),
        "signal-class-fired trigger must validate"
    );
}

#[test]
fn manual_variant_validates() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(minimal_registry(), json!({ "kind": "manual" }));
    assert!(schema.is_valid(&reg), "manual trigger must validate");
}

#[test]
fn trajectory_swing_variant_validates() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(
        minimal_registry(),
        json!({ "kind": "trajectory-swing", "window_days": 14, "magnitude": 30 }),
    );
    assert!(
        schema.is_valid(&reg),
        "trajectory-swing trigger must validate (reserved v2 variant)"
    );
}

#[test]
fn enable_calibration_writes_true_validates() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let mut reg = minimal_registry();
    reg["config"]["enable_calibration_writes"] = json!(true);
    assert!(
        schema.is_valid(&reg),
        "enable_calibration_writes=true must validate"
    );
}

#[test]
fn enable_calibration_writes_false_validates() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let mut reg = minimal_registry();
    reg["config"]["enable_calibration_writes"] = json!(false);
    assert!(
        schema.is_valid(&reg),
        "enable_calibration_writes=false must validate (explicit default)"
    );
}

// ─── Required-field discipline rejects malformed payloads ────────

#[test]
fn out_of_expected_range_missing_min_rejected() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(
        minimal_registry(),
        json!({ "kind": "out-of-expected-range", "max": 95 }),
    );
    assert!(
        !schema.is_valid(&reg),
        "out-of-expected-range without min must be rejected"
    );
}

#[test]
fn out_of_expected_range_missing_max_rejected() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(
        minimal_registry(),
        json!({ "kind": "out-of-expected-range", "min": 70 }),
    );
    assert!(
        !schema.is_valid(&reg),
        "out-of-expected-range without max must be rejected"
    );
}

#[test]
fn signal_class_fired_missing_signal_kinds_rejected() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(
        minimal_registry(),
        json!({ "kind": "signal-class-fired" }),
    );
    assert!(
        !schema.is_valid(&reg),
        "signal-class-fired without signal_kinds must be rejected"
    );
}

#[test]
fn signal_class_fired_empty_signal_kinds_rejected() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(
        minimal_registry(),
        json!({ "kind": "signal-class-fired", "signal_kinds": [] }),
    );
    assert!(
        !schema.is_valid(&reg),
        "signal-class-fired with empty signal_kinds must be rejected (minItems: 1)"
    );
}

#[test]
fn trajectory_swing_missing_fields_rejected() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(
        minimal_registry(),
        json!({ "kind": "trajectory-swing", "window_days": 14 }),
    );
    assert!(
        !schema.is_valid(&reg),
        "trajectory-swing without magnitude must be rejected"
    );
}

// ─── Defensive: additional properties + unknown kinds rejected ───

#[test]
fn manual_with_extra_property_rejected() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(
        minimal_registry(),
        json!({ "kind": "manual", "extra_field": "should fail" }),
    );
    assert!(
        !schema.is_valid(&reg),
        "manual variant with additional property must be rejected (additionalProperties: false)"
    );
}

#[test]
fn unknown_kind_rejected() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(
        minimal_registry(),
        json!({ "kind": "unknown-kind" }),
    );
    assert!(
        !schema.is_valid(&reg),
        "unknown discriminator value must be rejected (no oneOf branch matches)"
    );
}

#[test]
fn out_of_expected_range_min_above_100_rejected() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = with_trigger(
        minimal_registry(),
        json!({ "kind": "out-of-expected-range", "min": 101, "max": 95 }),
    );
    assert!(
        !schema.is_valid(&reg),
        "min > 100 must be rejected (maximum: 100)"
    );
}

#[test]
fn enable_calibration_writes_wrong_type_rejected() {
    let Some(schema) = load_brain_registry_schema() else { return };
    let mut reg = minimal_registry();
    reg["config"]["enable_calibration_writes"] = json!("true");
    assert!(
        !schema.is_valid(&reg),
        "enable_calibration_writes as string must be rejected (type: boolean)"
    );
}

// ─── Backward compatibility: registry without new fields ─────────

#[test]
fn registry_without_either_field_still_validates() {
    // C5 invariant: both new fields are optional; existing v2
    // registries that pre-date the field additions must continue
    // to validate.
    let Some(schema) = load_brain_registry_schema() else { return };
    let reg = minimal_registry();
    assert!(
        reg["config"].get("enable_calibration_writes").is_none(),
        "baseline must omit enable_calibration_writes"
    );
    assert!(
        reg["config"]["domain_definitions"]["domain-calibration"]
            .get("calibration_trigger")
            .is_none(),
        "baseline must omit calibration_trigger"
    );
    assert!(schema.is_valid(&reg), "baseline must validate");
}
