//! Spec §17 / E-B2-2 C1 — Domain-calibration-ledger schema conformance.
//!
//! Validates synthetic calibration-ledger entries against
//! `domain-calibration-ledger-v1.schema.json`. The schema enforces
//! the 2-phase Pending → Triaged shape across the Brains-2.0
//! ecosystem; this test pins three concerns:
//!
//!  1. **Pending happy path** — a well-formed pending entry validates.
//!  2. **Triaged happy path** — a well-formed triaged entry validates.
//!  3. **Required-field discipline** — missing required fields fail.
//!
//! Plus pinned regressions:
//!  - `additionalProperties: false` rejects unknown fields at root.
//!  - Out-of-range scores rejected (defense-in-depth alongside Rust
//!    Confidence::new clamping).
//!  - `triage_decision` values restricted to the four-class enum.
//!  - `human_notes` minLength 1 (operator-rationale discipline).
//!
//! When the LSP-Brains submodule isn't reachable (standalone
//! checkout), tests skip with an eprintln — same convention as
//! the cmdb-envelope and agent-output schema-conformance suites.

use jsonschema::JSONSchema;
use serde_json::{json, Value};

mod test_support;
use test_support::load_calibration_ledger_schema;

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

/// Build a minimal valid `pending` entry. Override individual fields
/// via `extra` to exercise edge cases (missing required, wrong type, etc.).
fn pending_entry(extra: Option<Vec<(&str, Value)>>) -> Value {
    let mut entry = json!({
        "ts": 1_777_310_000.0_f64,
        "schema_version": "1",
        "entry_kind": "pending",
        "domain": "test-health",
        "domain_family": "domain-calibration",
        "trigger_signal_kind": "out-of-range",
        "actual_score": 30
    });
    if let Some(overrides) = extra {
        if let Some(obj) = entry.as_object_mut() {
            for (k, v) in overrides {
                obj.insert(k.to_string(), v);
            }
        }
    }
    entry
}

/// Build a minimal valid `triaged` entry that supersedes a hypothetical
/// pending at ts=1_777_310_000.0.
fn triaged_entry(extra: Option<Vec<(&str, Value)>>) -> Value {
    let mut entry = json!({
        "ts": 1_777_310_500.0_f64,
        "schema_version": "1",
        "entry_kind": "triaged",
        "domain": "test-health",
        "domain_family": "domain-calibration",
        "supersedes_ts": 1_777_310_000.0_f64,
        "triage_decision": "no-action",
        "human_operator": "test-operator",
        "human_notes": "Score drop was a deliberate test-suite restructure; recalibrate next sprint."
    });
    if let Some(overrides) = extra {
        if let Some(obj) = entry.as_object_mut() {
            for (k, v) in overrides {
                obj.insert(k.to_string(), v);
            }
        }
    }
    entry
}

// ─── Happy paths ──────────────────────────────────────────────────────

#[test]
fn pending_minimal_validates() {
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let entry = pending_entry(None);
    assert!(
        schema.is_valid(&entry),
        "minimal pending entry failed schema: {}",
        format_errors(&schema, &entry)
    );
}

#[test]
fn pending_with_optional_fields_validates() {
    // All optional fields populated — exercises the schema's full
    // pending shape (expected_score_lower/upper, context_notes,
    // context_artifacts).
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let entry = pending_entry(Some(vec![
        ("expected_score_lower", json!(70)),
        ("expected_score_upper", json!(100)),
        (
            "context_notes",
            json!("Score dropped from 85 to 30 — flagged because expected range is [70, 100]."),
        ),
        (
            "context_artifacts",
            json!([
                ".claude/test-health-cmdb.json#snapshot-2026-04-27",
                "https://example.com/scoring-history-fragment"
            ]),
        ),
    ]));
    assert!(
        schema.is_valid(&entry),
        "pending entry with optional fields failed schema: {}",
        format_errors(&schema, &entry)
    );
}

#[test]
fn triaged_minimal_validates() {
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let entry = triaged_entry(None);
    assert!(
        schema.is_valid(&entry),
        "minimal triaged entry failed schema: {}",
        format_errors(&schema, &entry)
    );
}

#[test]
fn triaged_with_audit_artifacts_validates() {
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let entry = triaged_entry(Some(vec![(
        "audit_artifacts",
        json!([".claude/snapshots/test-health-2026-04-27.json"]),
    )]));
    assert!(
        schema.is_valid(&entry),
        "triaged entry with audit_artifacts failed schema: {}",
        format_errors(&schema, &entry)
    );
}

// ─── Required-field discipline ────────────────────────────────────────

#[test]
fn pending_missing_required_field_rejected() {
    // Drop trigger_signal_kind — pending REQUIRES it.
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let mut entry = pending_entry(None);
    entry
        .as_object_mut()
        .expect("entry is object")
        .remove("trigger_signal_kind");
    assert!(
        !schema.is_valid(&entry),
        "expected pending without trigger_signal_kind to fail schema; got valid"
    );
}

#[test]
fn triaged_missing_supersedes_ts_rejected() {
    // Triaged MUST link to its pending predecessor.
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let mut entry = triaged_entry(None);
    entry
        .as_object_mut()
        .expect("entry is object")
        .remove("supersedes_ts");
    assert!(
        !schema.is_valid(&entry),
        "expected triaged without supersedes_ts to fail schema; got valid"
    );
}

#[test]
fn triaged_missing_human_notes_rejected() {
    // human_notes is the auditability lever — must be present.
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let mut entry = triaged_entry(None);
    entry
        .as_object_mut()
        .expect("entry is object")
        .remove("human_notes");
    assert!(
        !schema.is_valid(&entry),
        "expected triaged without human_notes to fail schema; got valid"
    );
}

#[test]
fn triaged_empty_human_notes_rejected() {
    // minLength: 1 enforced — empty string is silent acceptance.
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let entry = triaged_entry(Some(vec![("human_notes", json!(""))]));
    assert!(
        !schema.is_valid(&entry),
        "expected triaged with empty human_notes to fail schema; got valid"
    );
}

// ─── Defensive / out-of-range ─────────────────────────────────────────

#[test]
fn pending_actual_score_out_of_range_rejected() {
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let above = pending_entry(Some(vec![("actual_score", json!(150))]));
    assert!(
        !schema.is_valid(&above),
        "expected actual_score > 100 to fail schema; got valid"
    );
    let below = pending_entry(Some(vec![("actual_score", json!(-10))]));
    assert!(
        !schema.is_valid(&below),
        "expected actual_score < 0 to fail schema; got valid"
    );
}

#[test]
fn triaged_decision_outside_enum_rejected() {
    // The four-class enum is intentionally coarse; finer
    // categorization belongs in human_notes. Unknown enum values
    // would silently lose semantic meaning for downstream readers.
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let entry = triaged_entry(Some(vec![("triage_decision", json!("escalate"))]));
    assert!(
        !schema.is_valid(&entry),
        "expected unknown triage_decision value to fail schema; got valid"
    );
}

#[test]
fn unknown_root_field_rejected() {
    // additionalProperties: false discipline. If a future contributor
    // wants to add a field, they should add it to the schema's
    // properties block (and version-bump if breaking).
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let entry = pending_entry(Some(vec![("speculative_extension_field", json!(true))]));
    assert!(
        !schema.is_valid(&entry),
        "expected unknown root field to fail schema (additionalProperties: false); got valid"
    );
}

#[test]
fn wrong_entry_kind_rejected() {
    // entry_kind is the oneOf discriminator. A novel value wouldn't
    // match either branch.
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let entry = pending_entry(Some(vec![("entry_kind", json!("draft"))]));
    assert!(
        !schema.is_valid(&entry),
        "expected unknown entry_kind to fail schema; got valid"
    );
}

#[test]
fn unknown_domain_family_rejected() {
    // v1 enum: domain-calibration only. Future families add an enum
    // value here AND a per-family definitions block. Unknown family
    // values are caught here (defense alongside the writer's
    // registry-validation check).
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let entry = pending_entry(Some(vec![("domain_family", json!("self-coherence"))]));
    assert!(
        !schema.is_valid(&entry),
        "expected unknown domain_family to fail schema; got valid"
    );
}

#[test]
fn pending_and_triaged_link_via_supersedes_ts() {
    // Schema-level invariant: a triaged's supersedes_ts MAY equal a
    // pending's ts. Schema can't validate the cross-entry reference
    // (writer's job) — this test just confirms both shapes coexist
    // cleanly when their numeric ts values align.
    let Some(schema) = load_calibration_ledger_schema() else {
        eprintln!("skip: domain-calibration-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let pending = pending_entry(None);
    let triaged = triaged_entry(None);
    let pending_ts = pending["ts"].as_f64().expect("pending has ts");
    let supersedes_ts = triaged["supersedes_ts"]
        .as_f64()
        .expect("triaged has supersedes_ts");
    assert_eq!(
        pending_ts, supersedes_ts,
        "fixture invariant: triaged.supersedes_ts must reference pending.ts"
    );
    assert!(schema.is_valid(&pending), "pending fixture must validate");
    assert!(schema.is_valid(&triaged), "triaged fixture must validate");
}
