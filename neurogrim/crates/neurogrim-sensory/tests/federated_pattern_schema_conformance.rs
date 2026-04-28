//! Spec §16.6 (v2.12 extension) / E-B2-7 C1+C2+C3 — Federated-pattern
//! A2A + pattern-aggregation-ledger schema conformance.
//!
//! Validates the v1 federated-pattern A2A payload schema and the v1
//! pattern-aggregation-ledger schema, plus the nine on-disk fixtures
//! that pin every locked decision in E-B2-7's Layer-2 plan (Q1, Q4, Q8,
//! Q11, Q14, Q15 + R-1 schema-name lock).
//!
//! E-B2-7 is the FIRST cross-Brain primitive in the Brains-2.0 campaign
//! — the wire format and the ledger structure together carry the
//! privacy-by-construction contract for federated patterns. This file
//! pins eleven concerns:
//!
//!  A. **Valid federated-pattern payload validates** — the shape from
//!     the locked Q1 + Q14 + Q15 design (closed-set `vigilance-pattern`
//!     + closed-set numeric-only feature_vector + opaque-hash
//!     anonymized_origin + 1-entry origin_set + ISO 8601
//!     discovered_at) validates cleanly against the schema.
//!  B. **Missing required `peer_brain_id` rejected** — Q1 lock: the
//!     receiver-from-the-sender identity is required so receivers can
//!     verify routing correctness.
//!  C. **Additional-properties rejected (R-1 + Q1 + Q8 PRIVACY PIN)**
//!     — `additionalProperties: false` on the federated-pattern payload
//!     structurally enforces privacy. A smuggled `note` field at the
//!     top level is rejected. If a future change relaxes this, that
//!     change must explicitly re-open the BR-6 + privacy-under-
//!     composition conversation (charter-level).
//!  D. **Unknown pattern_kind rejected** — Q14 closed-set discipline:
//!     v1 enum has exactly one entry (`vigilance-pattern`); any other
//!     value (e.g., `operator-calibration-pattern`) is rejected at
//!     v1.
//!  E. **Bounded feature_vector under composition (Q8 PRIVACY PIN)**
//!     — Q8 lock: feature_vector is closed-set numeric-only at v1. No
//!     strings, no FQDNs, no operator handles. Even smuggled INSIDE
//!     the feature_vector object, free-text MUST be rejected
//!     specifically on the FeatureVector's `additionalProperties:
//!     false`.
//!  F. **Q14 closed-set pattern_kind discipline pin** — schema's
//!     `definitions.PatternKind.enum` has exactly the locked 1-entry
//!     vocabulary from Q14: `["vigilance-pattern"]`. Vocabulary
//!     additions surface as deliberate test edits, not silent drift —
//!     same discipline as `culture-manifest-v1.values`,
//!     `hat-contract-v1.ToolName`, `trust-budget-v1.Ecosystem`,
//!     `invocation-ledger-v1.Disposition`.
//!  G. **Valid received-row ledger entry validates** — Q4 oneOf
//!     ReceivedEntry shape with required fields populated.
//!  H. **Valid emitted-row ledger entry validates** — Q4 oneOf
//!     EmittedEntry shape (Q12 sender-side audit trail).
//!  I. **Valid mixed-rows ledger validates** — both row kinds in the
//!     same JSONL file each validate against the `oneOf`.
//!  J. **Unknown dropped_reason rejected (Q4 closed-set pin)** —
//!     received entry with a value outside the closed 5-entry
//!     `DroppedReason` enum is rejected.
//!  K. **Q4+Q6+Q11+Q15 closed-set DroppedReason discipline pin** —
//!     schema's `definitions.DroppedReason.enum` has exactly the 5
//!     locked entries.
//!
//! When the LSP-Brains submodule isn't reachable (standalone checkout),
//! tests skip with an eprintln — same convention as the cmdb-envelope,
//! agent-output, calibration-ledger, hat-contract, trust-budget, and
//! invocation-ledger schema-conformance suites.

use jsonschema::JSONSchema;
use serde_json::Value;

mod test_support;
use test_support::{
    load_a2a_federated_pattern_schema, load_pattern_aggregation_ledger_schema,
    locate_federated_pattern_fixture, read_a2a_federated_pattern_schema_value,
    read_pattern_aggregation_ledger_schema_value,
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

/// Read a single-object JSON fixture (NOT JSONL). For payload fixtures
/// — the federated-pattern A2A payload validates ONE object.
fn read_payload_fixture(name: &str) -> Value {
    let path = locate_federated_pattern_fixture(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("parse fixture {} as JSON: {e}", path.display()))
}

/// Read a JSONL ledger fixture: split on lines, skip empty, parse each
/// non-empty line as a `serde_json::Value`. The schema validates ONE
/// row at a time (per Q4 — `oneOf` at the top level).
fn read_ledger_fixture_rows(name: &str) -> Vec<Value> {
    let path = locate_federated_pattern_fixture(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line).unwrap_or_else(|e| {
                panic!(
                    "parse fixture {} line as JSON: {e}\nline: {line}",
                    path.display()
                )
            })
        })
        .collect()
}

// ─── Test A — valid federated-pattern payload ─────────────────────────

#[test]
fn a2a_federated_pattern_valid_validates() {
    let Some(schema) = load_a2a_federated_pattern_schema() else {
        eprintln!("skip: a2a-federated-pattern-v1 schema not reachable (standalone checkout)");
        return;
    };
    let payload = read_payload_fixture("a2a-federated-pattern-valid.json");

    assert!(
        schema.is_valid(&payload),
        "valid federated-pattern fixture failed schema: {}\npayload: {}",
        format_errors(&schema, &payload),
        payload
    );

    // Pin: required Q14 + Q1 fields are present and structurally
    // correct. If a future fixture edit drops these, the test would
    // still pass (validation cannot enforce field-presence beyond
    // required), so pin them explicitly.
    assert_eq!(
        payload["pattern_kind"], "vigilance-pattern",
        "valid fixture must pin Q14 single-value pattern_kind"
    );
    assert!(
        payload["feature_vector"].is_object(),
        "valid fixture must contain feature_vector object (Q1 + Q8 closed-set numeric-only)"
    );
    assert_eq!(
        payload["feature_vector"]["severity_class"], "high",
        "valid fixture feature_vector must carry the closed-set severity_class"
    );
}

// ─── Test B — missing required peer_brain_id ──────────────────────────

#[test]
fn a2a_federated_pattern_missing_required_rejected() {
    let Some(schema) = load_a2a_federated_pattern_schema() else {
        eprintln!("skip: a2a-federated-pattern-v1 schema not reachable (standalone checkout)");
        return;
    };
    let payload = read_payload_fixture("a2a-federated-pattern-missing-required.json");

    // Sanity: the fixture genuinely lacks `peer_brain_id`. Load-bearing
    // — a fixture edit that adds the field would silently pass
    // validation and erase the test's signal.
    assert!(
        payload.get("peer_brain_id").is_none(),
        "missing-required fixture MUST NOT contain `peer_brain_id` (Q1 required field — its absence is what the test pins)"
    );

    assert!(
        !schema.is_valid(&payload),
        "expected missing-required (peer_brain_id) fixture to fail schema; got valid"
    );

    let errors = format_errors(&schema, &payload);
    assert!(
        errors.contains("peer_brain_id"),
        "expected validation error to mention `peer_brain_id` (the missing required field); got:\n{}",
        errors
    );
}

// ─── Test C — Q5 + Q8 PRIVACY REGRESSION PIN at top level ─────────────

#[test]
fn a2a_federated_pattern_additional_properties_q5_privacy_pin() {
    // R-1 + Q1 + Q8 lock: additionalProperties:false on the federated-
    // pattern payload structurally enforces privacy. If a future change
    // relaxes this, that change must explicitly re-open the BR-6 +
    // privacy-under-composition conversation.
    let Some(schema) = load_a2a_federated_pattern_schema() else {
        eprintln!("skip: a2a-federated-pattern-v1 schema not reachable (standalone checkout)");
        return;
    };
    let payload = read_payload_fixture("a2a-federated-pattern-additional-properties.json");

    // Sanity: the fixture genuinely contains the smuggled `note` field
    // at the top level — this is what makes the test load-bearing as
    // the privacy regression pin.
    assert!(
        payload.get("note").is_some(),
        "Q1+Q8 privacy pin: fixture MUST contain a top-level `note` field (this is what the schema's additionalProperties:false rejects)"
    );

    assert!(
        !schema.is_valid(&payload),
        "Q1+Q8 PRIVACY REGRESSION PIN: expected additional-properties (smuggled `note`) fixture to fail schema; got valid. \
         If this test passes, the privacy contract on the federated-pattern wire format has been structurally relaxed — \
         that change must explicitly re-open the BR-6 + privacy-under-composition conversation (charter-level)."
    );

    let errors_text = format_errors(&schema, &payload);
    let mentions_note = errors_text.contains("note");
    let mentions_additional = errors_text.contains("additional propert");
    assert!(
        mentions_note || mentions_additional,
        "expected validation error to mention `note` or `additional propert*` (Q1+Q8 additionalProperties:false enforcement); got:\n{}",
        errors_text
    );
}

// ─── Test D — unknown pattern_kind (Q14 closed-set) ───────────────────

#[test]
fn a2a_federated_pattern_unknown_pattern_kind_rejected() {
    let Some(schema) = load_a2a_federated_pattern_schema() else {
        eprintln!("skip: a2a-federated-pattern-v1 schema not reachable (standalone checkout)");
        return;
    };
    let payload = read_payload_fixture("a2a-federated-pattern-unknown-pattern-kind.json");

    // Sanity: the fixture's pattern_kind must literally be a non-v1
    // value — Q14 explicitly locks v1 to a single-entry enum.
    assert_eq!(
        payload["pattern_kind"], "operator-calibration-pattern",
        "fixture MUST set pattern_kind to a non-v1 value (the value Q14 explicitly excluded at v1)"
    );

    // Per Q14: closed-set 1-entry vocabulary at v1. Any other value
    // (e.g., `operator-calibration-pattern`) MUST be rejected.
    assert!(
        !schema.is_valid(&payload),
        "expected unknown-pattern-kind fixture to fail schema; got valid"
    );

    let errors_text = format_errors(&schema, &payload);
    assert!(
        errors_text.contains("pattern_kind") || errors_text.contains("vigilance-pattern"),
        "expected validation error to mention `pattern_kind` or the v1 enum value; got:\n{}",
        errors_text
    );
}

// ─── Test E — Q8 PRIVACY UNDER COMPOSITION PIN ────────────────────────

#[test]
fn a2a_federated_pattern_bounded_feature_vector_q8_privacy_pin() {
    // Q8 privacy lock: feature_vector is closed-set numeric-only at v1.
    // No strings, no FQDNs, no operator handles. Even smuggled INSIDE
    // the feature_vector object, free-text MUST be rejected.
    let Some(schema) = load_a2a_federated_pattern_schema() else {
        eprintln!("skip: a2a-federated-pattern-v1 schema not reachable (standalone checkout)");
        return;
    };
    let payload = read_payload_fixture("a2a-federated-pattern-bounded-feature-vector.json");

    // Sanity: the fixture genuinely smuggles `note` INSIDE the
    // feature_vector. This distinguishes test E from test C — C's
    // smuggle is at the top level, E's smuggle is one layer down.
    assert!(
        payload["feature_vector"].is_object(),
        "Q8 fixture must have a feature_vector object"
    );
    assert!(
        payload["feature_vector"].get("note").is_some(),
        "Q8 privacy-under-composition pin: fixture MUST smuggle a `note` field INSIDE feature_vector (this is what the FeatureVector definition's additionalProperties:false rejects)"
    );

    assert!(
        !schema.is_valid(&payload),
        "Q8 PRIVACY UNDER COMPOSITION PIN: expected bounded-feature-vector (smuggled `note` INSIDE feature_vector) fixture to fail schema; got valid. \
         If this test passes, the bounded-numeric-only feature_vector contract has been structurally relaxed — \
         that change must explicitly re-open the BR-6 + privacy-under-composition conversation (charter-level)."
    );

    // Distinguish from C: the failure must be specifically on the
    // FeatureVector's `additionalProperties: false`, not the top-level
    // one. Direct sub-schema validation against FeatureVector surfaces
    // the per-branch error.
    let Some(schema_value) = read_a2a_federated_pattern_schema_value() else {
        eprintln!("skip: a2a-federated-pattern-v1 raw schema value not reachable");
        return;
    };
    let feature_vector_def = schema_value
        .pointer("/definitions/FeatureVector")
        .expect("schema must define /definitions/FeatureVector")
        .clone();
    let sub_schema = JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&feature_vector_def)
        .expect("FeatureVector sub-schema must compile in isolation");
    assert!(
        !sub_schema.is_valid(&payload["feature_vector"]),
        "FeatureVector sub-schema must reject the smuggled-note feature_vector directly"
    );
    let sub_errors = format_errors(&sub_schema, &payload["feature_vector"]);
    let mentions_note = sub_errors.contains("note");
    let mentions_additional = sub_errors.contains("additional propert");
    assert!(
        mentions_note || mentions_additional,
        "expected FeatureVector sub-schema error to mention `note` or `additional propert*` (Q8 enforcement); got:\n{}",
        sub_errors
    );
}

// ─── Test F — Q14 closed-set pattern_kind discipline pin ──────────────

#[test]
fn a2a_federated_pattern_pattern_kind_enum_has_exactly_one_entry_q14() {
    // Per Q14: the closed `PatternKind` vocabulary has exactly 1 entry
    // at v1 (`vigilance-pattern`). Future schema edits that add
    // vocabulary (additive only per charter rollout discipline #2)
    // will fail this test until the expected set is updated explicitly
    // — same discipline as `culture-manifest-v1.values`,
    // `hat-contract-v1.ToolName`, `trust-budget-v1.Ecosystem`, and
    // `invocation-ledger-v1.Disposition`. Vocabulary changes surface
    // as deliberate test edits, not silent drift.
    let Some(schema_value) = read_a2a_federated_pattern_schema_value() else {
        eprintln!("skip: a2a-federated-pattern-v1 schema not reachable (standalone checkout)");
        return;
    };

    let pattern_kind_values = schema_value
        .pointer("/definitions/PatternKind/enum")
        .and_then(|v| v.as_array())
        .expect("schema must define /definitions/PatternKind/enum as an array");

    let expected: Vec<&str> = vec!["vigilance-pattern"];
    assert_eq!(
        pattern_kind_values.len(),
        expected.len(),
        "PatternKind closed-set vocabulary must have exactly {} entry at v1 (Q14); got {}: {:?}",
        expected.len(),
        pattern_kind_values.len(),
        pattern_kind_values
    );

    let actual: Vec<String> = pattern_kind_values
        .iter()
        .map(|v| {
            v.as_str()
                .expect("PatternKind enum entry must be a string")
                .to_string()
        })
        .collect();
    for term in &expected {
        assert!(
            actual.iter().any(|a| a == term),
            "PatternKind closed-set vocabulary must contain `{}` (Q14); got {:?}",
            term,
            actual
        );
    }

    // Defense-in-depth: assert v2/v3 candidates from BACKLOG B-23 are
    // NOT yet in the v1 vocabulary. They are deliberate v2 candidates
    // and require additive spec change + schema bump.
    for forbidden in [
        "operator-calibration-pattern",
        "hat-contract-pattern",
        "trust-budget-pattern",
    ] {
        assert!(
            !actual.iter().any(|a| a == forbidden),
            "PatternKind closed-set vocabulary MUST NOT contain `{}` at v1 (Q14 — v2/v3 candidate per BACKLOG B-23); got {:?}",
            forbidden,
            actual
        );
    }
}

// ─── Test G — valid received-row ledger entry ─────────────────────────

#[test]
fn pattern_aggregation_ledger_valid_received_validates() {
    let Some(schema) = load_pattern_aggregation_ledger_schema() else {
        eprintln!("skip: pattern-aggregation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let rows = read_ledger_fixture_rows("pattern-aggregation-ledger-valid-received.jsonl");
    assert_eq!(
        rows.len(),
        1,
        "valid-received fixture must contain exactly 1 row"
    );
    let row = &rows[0];

    assert!(
        schema.is_valid(row),
        "valid received-row ledger fixture failed schema: {}\nrow: {}",
        format_errors(&schema, row),
        row
    );
    assert_eq!(
        row["entry_kind"], "received",
        "valid-received fixture must pin entry_kind == \"received\""
    );
}

// ─── Test H — valid emitted-row ledger entry ──────────────────────────

#[test]
fn pattern_aggregation_ledger_valid_emitted_validates() {
    let Some(schema) = load_pattern_aggregation_ledger_schema() else {
        eprintln!("skip: pattern-aggregation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let rows = read_ledger_fixture_rows("pattern-aggregation-ledger-valid-emitted.jsonl");
    assert_eq!(
        rows.len(),
        1,
        "valid-emitted fixture must contain exactly 1 row"
    );
    let row = &rows[0];

    assert!(
        schema.is_valid(row),
        "valid emitted-row ledger fixture failed schema: {}\nrow: {}",
        format_errors(&schema, row),
        row
    );
    assert_eq!(
        row["entry_kind"], "emitted",
        "valid-emitted fixture must pin entry_kind == \"emitted\""
    );
}

// ─── Test I — valid mixed rows ────────────────────────────────────────

#[test]
fn pattern_aggregation_ledger_valid_mixed_rows_validate() {
    let Some(schema) = load_pattern_aggregation_ledger_schema() else {
        eprintln!("skip: pattern-aggregation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let rows = read_ledger_fixture_rows("pattern-aggregation-ledger-valid-mixed-rows.jsonl");
    assert_eq!(
        rows.len(),
        2,
        "valid-mixed-rows fixture must contain exactly 2 rows (one received + one emitted)"
    );

    for (idx, row) in rows.iter().enumerate() {
        assert!(
            schema.is_valid(row),
            "valid-mixed-rows fixture row {} failed schema: {}\nrow: {}",
            idx,
            format_errors(&schema, row),
            row
        );
    }

    // Pin: row 0 is `received`, row 1 is `emitted` — exercises BOTH
    // oneOf branches in the same JSONL stream (Q4 lock).
    assert_eq!(
        rows[0]["entry_kind"], "received",
        "mixed-rows row 0 must be a ReceivedEntry (entry_kind == \"received\")"
    );
    assert_eq!(
        rows[1]["entry_kind"], "emitted",
        "mixed-rows row 1 must be an EmittedEntry (entry_kind == \"emitted\")"
    );
}

// ─── Test J — unknown dropped_reason (Q4 closed-set) ──────────────────

#[test]
fn pattern_aggregation_ledger_unknown_dropped_reason_rejected_q4() {
    let Some(schema) = load_pattern_aggregation_ledger_schema() else {
        eprintln!("skip: pattern-aggregation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let rows = read_ledger_fixture_rows("pattern-aggregation-ledger-unknown-dropped-reason.jsonl");
    assert_eq!(
        rows.len(),
        1,
        "unknown-dropped-reason fixture must contain exactly 1 row"
    );
    let row = &rows[0];

    // Sanity: the fixture's dropped_reason must literally be a non-Q4
    // value — Q4 explicitly locks the closed-set to 5 entries.
    assert_eq!(
        row["dropped_reason"], "operator-rejected",
        "fixture MUST set dropped_reason to a non-Q4 value (the value Q4 explicitly excluded)"
    );

    // Per Q4: closed-set 5-entry vocabulary. `operator-rejected` is
    // NOT in the locked enum (rate-limit-exceeded / recursion-guard /
    // schema-validation-failed / hop-limit-exceeded /
    // unknown-pattern-kind). Validation must fail.
    assert!(
        !schema.is_valid(row),
        "expected unknown-dropped-reason fixture to fail schema; got valid"
    );

    let errors_text = format_errors(&schema, row);
    assert!(
        errors_text.contains("dropped_reason")
            || errors_text.contains("rate-limit-exceeded")
            || errors_text.contains("recursion-guard")
            || errors_text.contains("schema-validation-failed")
            || errors_text.contains("hop-limit-exceeded")
            || errors_text.contains("unknown-pattern-kind")
            || errors_text.contains("operator-rejected"),
        "expected validation error to mention `dropped_reason`, the bad value, OR a member of the closed-set enum; got:\n{}",
        errors_text
    );
}

// ─── Test K — Q4 closed-set DroppedReason discipline pin ──────────────

#[test]
fn pattern_aggregation_ledger_dropped_reason_enum_has_exactly_locked_entries() {
    // Per Q4 + Q6 + Q11 + Q15 locks: the closed `DroppedReason`
    // vocabulary has exactly 5 entries. Future schema edits that add
    // vocabulary (additive only per charter rollout discipline #2)
    // will fail this test until the expected set is updated explicitly
    // — same discipline as `invocation-ledger-v1.Disposition`,
    // `hat-contract-v1.ToolName`, `trust-budget-v1.Ecosystem`, and
    // `culture-manifest-v1.values`.
    let Some(schema_value) = read_pattern_aggregation_ledger_schema_value() else {
        eprintln!("skip: pattern-aggregation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };

    let dropped_values = schema_value
        .pointer("/definitions/DroppedReason/enum")
        .and_then(|v| v.as_array())
        .expect("schema must define /definitions/DroppedReason/enum as an array");

    let expected: Vec<&str> = vec![
        "rate-limit-exceeded",
        "recursion-guard",
        "schema-validation-failed",
        "hop-limit-exceeded",
        "unknown-pattern-kind",
    ];
    assert_eq!(
        dropped_values.len(),
        expected.len(),
        "DroppedReason closed-set vocabulary must have exactly {} entries (Q4+Q6+Q11+Q15); got {}: {:?}",
        expected.len(),
        dropped_values.len(),
        dropped_values
    );

    let actual: Vec<String> = dropped_values
        .iter()
        .map(|v| {
            v.as_str()
                .expect("DroppedReason enum entry must be a string")
                .to_string()
        })
        .collect();
    for term in &expected {
        assert!(
            actual.iter().any(|a| a == term),
            "DroppedReason closed-set vocabulary must contain `{}` (Q4+Q6+Q11+Q15); got {:?}",
            term,
            actual
        );
    }

    // Defense-in-depth: assert NOT-IN-CLOSED-SET sentinel values are
    // absent. `operator-rejected` would imply reputation gating
    // semantics (BACKLOG B-23 v3 candidate) — explicitly out of scope
    // at v1.
    for forbidden in ["operator-rejected", "peer-banned", "trust-too-low"] {
        assert!(
            !actual.iter().any(|a| a == forbidden),
            "DroppedReason closed-set vocabulary MUST NOT contain `{}` at v1 (out-of-scope per Q7 reputation lock + BACKLOG B-23); got {:?}",
            forbidden,
            actual
        );
    }
}
