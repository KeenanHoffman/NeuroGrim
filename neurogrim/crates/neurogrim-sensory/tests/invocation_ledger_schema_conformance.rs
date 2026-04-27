//! Spec §17.x / E-B2-6 C1+C2 — Invocation-ledger schema conformance.
//!
//! Validates the v1 invocation-ledger schema and its six on-disk JSONL
//! fixtures (per Q1 closed-set + Q3 oneOf row-kind discrimination + Q5
//! privacy lock + Q6 operator-identity discipline). The schema is the
//! FIRST machine-checkable contract for `.claude/brain/invocation-
//! ledger.jsonl` — previously the format was defined only by
//! `record-skill-invocation.sh:56-58`'s fixed printf line + the prose
//! at `docs/invocation-ledger.md:26-39`. This file pins seven
//! concerns:
//!
//!  A. **Valid skill-only fixture** — a row matching the writer's
//!     existing printf shape validates as `SkillEntry`.
//!  B. **Valid with disposition fixture** — the optional `disposition`
//!     forward-compat field on `SkillEntry` validates when present.
//!  C. **Valid mixed-rows fixture** — both row kinds (`SkillEntry`
//!     with `type:"skill"` and `DispositionEntry` with
//!     `entry_kind:"disposition"`) validate against the `oneOf` schema
//!     within the same JSONL file.
//!  D. **Missing-required disposition `human_operator` fixture** —
//!     validation FAILS with an error path that mentions
//!     `human_operator` (distinguishes from E and F).
//!  E. **Additional-properties fixture (Q5 PRIVACY REGRESSION PIN)**
//!     — validation FAILS specifically because of
//!     `additionalProperties: false` on `DispositionEntry`. The
//!     fixture smuggles a `note` field; the schema's structural
//!     enforcement is the v1 lock for "no free-text justification."
//!     If a future change relaxes this, that change must explicitly
//!     re-open the BR-5 conversation (charter-level).
//!  F. **Unknown-disposition-kind fixture** — validation FAILS on the
//!     closed-set enum mismatch (per Q1). Distinguishes from D and E
//!     semantically.
//!  G. **Closed-set discipline pin** — schema's
//!     `definitions.Disposition.enum` has exactly the 4 locked entries
//!     from Q1: `accepted`, `rejected`, `modified`, `superseded`.
//!     Mirrors E-B2-3 C2 Test D + E-B2-4 C2 Test E — vocabulary
//!     changes surface as deliberate test edits, not silent drift.
//!
//! When the LSP-Brains submodule isn't reachable (standalone
//! checkout), tests skip with an eprintln — same convention as the
//! cmdb-envelope, agent-output, calibration-ledger, hat-contract, and
//! trust-budget schema-conformance suites.

use jsonschema::JSONSchema;
use serde_json::Value;

mod test_support;
use test_support::{
    load_invocation_ledger_schema, locate_invocation_ledger_fixture,
    read_invocation_ledger_schema_value,
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

/// Read a JSONL fixture file, split on lines, skip empty lines, parse
/// each non-empty line as a `serde_json::Value`. The schema validates
/// ONE row at a time (per Q3 — `oneOf` at the top level), so callers
/// validate each returned row independently.
fn read_invocation_ledger_fixture_rows(name: &str) -> Vec<Value> {
    let path = locate_invocation_ledger_fixture(name);
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

// ─── Test case A — valid skill-only fixture ───────────────────────────

#[test]
fn fixture_valid_skill_only_validates() {
    let Some(schema) = load_invocation_ledger_schema() else {
        eprintln!("skip: invocation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let rows = read_invocation_ledger_fixture_rows("invocation-ledger-valid-skill-only.jsonl");
    assert!(
        !rows.is_empty(),
        "valid-skill-only fixture must contain at least one row"
    );

    for (idx, row) in rows.iter().enumerate() {
        assert!(
            schema.is_valid(row),
            "valid-skill-only fixture row {} failed schema: {}\nrow: {}",
            idx,
            format_errors(&schema, row),
            row
        );
    }

    // Pin: at least one row must have `type == "skill"` and validate
    // as a `SkillEntry`. Mirrors the writer's existing printf shape at
    // `record-skill-invocation.sh:56-58`.
    assert!(
        rows.iter().any(|r| r["type"] == "skill"),
        "valid-skill-only fixture must contain at least one row with type == \"skill\" (the existing writer's contract)"
    );
}

// ─── Test case B — valid with disposition fixture ─────────────────────

#[test]
fn fixture_valid_with_disposition_field_validates() {
    let Some(schema) = load_invocation_ledger_schema() else {
        eprintln!("skip: invocation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let rows = read_invocation_ledger_fixture_rows("invocation-ledger-valid-with-disposition.jsonl");
    assert!(
        !rows.is_empty(),
        "valid-with-disposition fixture must contain at least one row"
    );

    for (idx, row) in rows.iter().enumerate() {
        assert!(
            schema.is_valid(row),
            "valid-with-disposition fixture row {} failed schema: {}\nrow: {}",
            idx,
            format_errors(&schema, row),
            row
        );
    }

    // Pin: the row contains the optional `disposition` field set to
    // `"accepted"`. Q3 forward-compat — writers MAY include the
    // disposition inline on a `SkillEntry`; v1 readers tolerate this
    // shape. The canonical disposition path is a separate
    // `DispositionEntry` row; this test pins that the inline form is
    // ALSO schema-valid.
    assert!(
        rows.iter().any(|r| r["disposition"] == "accepted"),
        "valid-with-disposition fixture must contain a row with disposition == \"accepted\" (Q3 forward-compat)"
    );
}

// ─── Test case C — valid mixed rows fixture ───────────────────────────

#[test]
fn fixture_valid_mixed_rows_validates() {
    let Some(schema) = load_invocation_ledger_schema() else {
        eprintln!("skip: invocation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let rows = read_invocation_ledger_fixture_rows("invocation-ledger-valid-mixed-rows.jsonl");
    assert_eq!(
        rows.len(),
        2,
        "valid-mixed-rows fixture must contain exactly 2 rows (skill + disposition)"
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

    // Pin: row 1 has `type == "skill"` (matches `SkillEntry`).
    assert_eq!(
        rows[0]["type"], "skill",
        "valid-mixed-rows row 0 must be a SkillEntry (type == \"skill\")"
    );
    // Pin: row 2 has `entry_kind == "disposition"` (matches `DispositionEntry`).
    assert_eq!(
        rows[1]["entry_kind"], "disposition",
        "valid-mixed-rows row 1 must be a DispositionEntry (entry_kind == \"disposition\")"
    );
    // Pin: both rows reference the same `invocation_id` — the
    // disposition row attaches to the prior skill row by id.
    assert_eq!(
        rows[0]["invocation_id"], rows[1]["invocation_id"],
        "valid-mixed-rows: disposition row's invocation_id must match the skill row's invocation_id (referential integrity)"
    );
}

// ─── Test case D — missing-required disposition human_operator ────────

#[test]
fn fixture_missing_required_disposition_human_operator_rejected() {
    let Some(schema) = load_invocation_ledger_schema() else {
        eprintln!("skip: invocation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let rows = read_invocation_ledger_fixture_rows("invocation-ledger-missing-required.jsonl");
    assert_eq!(
        rows.len(),
        1,
        "missing-required fixture must contain exactly 1 row (the disposition row missing human_operator)"
    );
    let row = &rows[0];

    // Sanity: the fixture genuinely lacks `human_operator`. This is
    // load-bearing — a fixture edit that adds the field would silently
    // pass validation and erase the test's signal.
    assert!(
        row.get("human_operator").is_none(),
        "missing-required fixture MUST NOT contain `human_operator` (Q6 required field — its absence is what the test pins)"
    );
    // Sanity: the fixture is shaped like a `DispositionEntry` (it has
    // `entry_kind: \"disposition\"`), so the failure is specifically
    // a missing-required failure on the DispositionEntry branch — NOT
    // a row that accidentally matched neither branch for unrelated
    // reasons. This distinguishes from F (unknown disposition_kind —
    // present here as a valid `\"rejected\"`) and E (additional
    // property — none here).
    assert_eq!(
        row["entry_kind"], "disposition",
        "missing-required fixture must be a disposition row (entry_kind == \"disposition\")"
    );
    assert_eq!(
        row["disposition_kind"], "rejected",
        "missing-required fixture must have a valid disposition_kind (so the failure isolates to human_operator)"
    );
    assert!(
        row.get("note").is_none(),
        "missing-required fixture must NOT contain a note field (so the failure isolates to missing human_operator, not additionalProperties)"
    );

    // Per Q6 lock + DispositionEntry.required: `human_operator` is
    // REQUIRED (mirrors §17.6 NEUROGRIM_OPERATOR convention). Validation
    // must fail when it's missing.
    //
    // NOTE on error-text inspection: the top-level schema is `oneOf`
    // (SkillEntry XOR DispositionEntry). When a row fails neither
    // branch cleanly, the jsonschema crate's outermost validate()
    // reports the generic `"not valid under any of the schemas listed
    // in the 'oneOf' keyword"` message. To distinguish this fixture's
    // failure mode from E and F, we (a) pin the row's structural
    // shape above (it IS a disposition row, with valid kind, no
    // additional fields) — leaving only the missing human_operator as
    // the possible failure cause — and (b) directly validate against
    // the DispositionEntry sub-schema below to confirm the per-branch
    // error mentions the missing field. This is the tightest
    // semantic distinction the validator's error model supports.
    assert!(
        !schema.is_valid(row),
        "expected missing-required (human_operator) fixture to fail schema; got valid"
    );

    // Direct sub-schema validation: extract the DispositionEntry
    // definition and validate the row against it in isolation. This
    // surfaces the per-branch missing-required error that the
    // top-level oneOf wrapper hides.
    let Some(schema_value) = read_invocation_ledger_schema_value() else {
        eprintln!("skip: invocation-ledger-v1 schema raw value not reachable");
        return;
    };
    let disposition_entry_def = schema_value
        .pointer("/definitions/DispositionEntry")
        .expect("schema must define /definitions/DispositionEntry")
        .clone();
    let sub_schema = JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&disposition_entry_def)
        .expect("DispositionEntry sub-schema must compile in isolation");
    assert!(
        !sub_schema.is_valid(row),
        "DispositionEntry sub-schema must reject the missing-required row directly"
    );
    let sub_errors = format_errors(&sub_schema, row);
    assert!(
        sub_errors.contains("human_operator"),
        "expected DispositionEntry sub-schema validation error to mention `human_operator` (the missing required field); got:\n{}",
        sub_errors
    );
}

// ─── Test case E — Q5 PRIVACY REGRESSION PIN ──────────────────────────

#[test]
fn fixture_additional_properties_rejected_q5_privacy_pin() {
    // Q5 lock: free-text justification forbidden at v1; the schema's
    // additionalProperties:false on DispositionEntry is the structural
    // enforcement. If a future change relaxes this, that change must
    // explicitly re-open the BR-5 conversation.
    let Some(schema) = load_invocation_ledger_schema() else {
        eprintln!("skip: invocation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let rows =
        read_invocation_ledger_fixture_rows("invocation-ledger-additional-properties.jsonl");
    assert_eq!(
        rows.len(),
        1,
        "additional-properties fixture must contain exactly 1 row (the disposition row with smuggled `note`)"
    );
    let row = &rows[0];

    // Sanity: the fixture genuinely contains the `note` field — this
    // is what makes the test load-bearing as the privacy regression
    // pin. If a fixture edit drops the note, the test would still pass
    // by accident.
    assert!(
        row.get("note").is_some(),
        "Q5 privacy pin: fixture MUST contain a `note` field (this is what the schema's additionalProperties:false rejects)"
    );

    assert!(
        !schema.is_valid(row),
        "Q5 PRIVACY REGRESSION PIN: expected additional-properties (smuggled `note`) fixture to fail schema; got valid. \
         If this test passes, the privacy contract at docs/invocation-ledger.md:26-39 has been structurally relaxed — \
         that change must explicitly re-open the BR-5 conversation (charter-level)."
    );

    // Distinguish from D (missing human_operator — present here) and F
    // (unknown disposition_kind — `rejected` is a valid kind here).
    // The error text must mention the offending unknown property
    // `note` or "additional propert(y|ies)".
    let errors_text = format_errors(&schema, row);
    let mentions_note = errors_text.contains("note");
    let mentions_additional = errors_text.contains("additional propert");
    assert!(
        mentions_note || mentions_additional,
        "expected validation error to mention `note` or `additional propert*` (Q5 additionalProperties:false enforcement); got:\n{}",
        errors_text
    );
}

// ─── Test case F — unknown disposition_kind fixture ───────────────────

#[test]
fn fixture_unknown_disposition_kind_rejected() {
    let Some(schema) = load_invocation_ledger_schema() else {
        eprintln!("skip: invocation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };
    let rows =
        read_invocation_ledger_fixture_rows("invocation-ledger-unknown-disposition-kind.jsonl");
    assert_eq!(
        rows.len(),
        1,
        "unknown-disposition-kind fixture must contain exactly 1 row"
    );
    let row = &rows[0];

    // Sanity: the fixture's disposition_kind must literally be
    // `"ignored"` — Q1 explicitly rejected this value (encoding
    // absence-of-action would re-open the auto-inference trap from Q2).
    assert_eq!(
        row["disposition_kind"], "ignored",
        "fixture MUST set disposition_kind = \"ignored\" (the value Q1 explicitly rejected)"
    );

    // Per Q1: closed-set 4-entry vocabulary. `ignored` is NOT in the
    // locked enum (accepted / rejected / modified / superseded).
    // Validation must fail.
    assert!(
        !schema.is_valid(row),
        "expected unknown-disposition-kind fixture to fail schema; got valid"
    );

    // Distinguish from D (missing human_operator — present here) and E
    // (additionalProperties — every property is in the schema). The
    // error text must surface `disposition_kind` AND either the bad
    // value `ignored` OR the closed-set enum vocabulary.
    let errors_text = format_errors(&schema, row);
    assert!(
        errors_text.contains("disposition_kind"),
        "expected validation error path to mention `disposition_kind` (closed-set enum mismatch); got:\n{}",
        errors_text
    );
    assert!(
        errors_text.contains("ignored")
            || errors_text.contains("accepted")
            || errors_text.contains("rejected")
            || errors_text.contains("modified")
            || errors_text.contains("superseded"),
        "expected validation error to mention either the bad value `ignored` OR a member of the closed-set enum; got:\n{}",
        errors_text
    );
}

// ─── Test case G — closed-set discipline pin ──────────────────────────

#[test]
fn closed_set_disposition_enum_has_exactly_locked_entries() {
    // Per Q1: the closed `Disposition` vocabulary has exactly 4 entries.
    // Future schema edits that add vocabulary (additive only per
    // charter rollout discipline #2) will fail this test until the
    // expected set is updated explicitly — same discipline as
    // `culture-manifest-v1.values`, `hat-contract-v1.ToolName`, and
    // `trust-budget-v1.Ecosystem`. Mirror of E-B2-3 C2 Test D
    // `schema_tool_name_enum_has_exactly_eight_entries` and E-B2-4 C2
    // Test E `closed_set_enums_have_exactly_locked_entries` —
    // vocabulary changes surface as deliberate test edits, not
    // silent drift.
    let Some(schema_value) = read_invocation_ledger_schema_value() else {
        eprintln!("skip: invocation-ledger-v1 schema not reachable (standalone checkout)");
        return;
    };

    let disposition_values = schema_value
        .pointer("/definitions/Disposition/enum")
        .and_then(|v| v.as_array())
        .expect("schema must define /definitions/Disposition/enum as an array");

    let expected: Vec<&str> = vec!["accepted", "rejected", "modified", "superseded"];
    assert_eq!(
        disposition_values.len(),
        expected.len(),
        "Disposition closed-set vocabulary must have exactly {} entries (Q1); got {}: {:?}",
        expected.len(),
        disposition_values.len(),
        disposition_values
    );

    let actual: Vec<String> = disposition_values
        .iter()
        .map(|v| {
            v.as_str()
                .expect("Disposition enum entry must be a string")
                .to_string()
        })
        .collect();
    for term in &expected {
        assert!(
            actual.iter().any(|a| a == term),
            "Disposition closed-set vocabulary must contain `{}` (Q1); got {:?}",
            term,
            actual
        );
    }

    // Defense-in-depth: assert Q1-rejected values are NOT in the
    // vocabulary. `ignored` (default for everything not dispositioned)
    // and `deferred` (encoding indecision motivates synthetic-
    // disposition spam) were both explicitly rejected at Q1.
    assert!(
        !actual.iter().any(|a| a == "ignored"),
        "Disposition closed-set vocabulary MUST NOT contain `ignored` (Q1 explicitly rejected — would re-open the auto-inference trap from Q2); got {:?}",
        actual
    );
    assert!(
        !actual.iter().any(|a| a == "deferred"),
        "Disposition closed-set vocabulary MUST NOT contain `deferred` (Q1 explicitly rejected — encoding indecision as positive value motivates synthetic-disposition spam); got {:?}",
        actual
    );
}
