//! E-B2-6 C3 — Subprocess integration tests for the
//! `neurogrim disposition record` CLI.
//!
//! Unit tests in `commands/disposition.rs` cover clap parsing
//! (closed-set vocabulary, required-arg checks, no-`--note`
//! structural pin). These integration tests cover the runtime
//! behavior: ledger writes round-trip into a real
//! `<project_root>/.claude/brain/invocation-ledger.jsonl`; missing
//! operator identity raises an error; the recursion-guard rejects
//! `operator_calibration:*` invocation IDs at parse time;
//! append-only invariant is preserved across a pre-existing skill
//! row.
//!
//! Pattern follows `domain_calibration_cli.rs` and `cli_smoke.rs`:
//! locate the binary via `env!("CARGO_BIN_EXE_neurogrim")`, spawn as
//! a subprocess, assert exit status + side effects (ledger contents
//! parsed back as JSON).

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn neurogrim_bin() -> &'static str {
    env!("CARGO_BIN_EXE_neurogrim")
}

/// Spawn `neurogrim <args>` with the given env overrides. Returns
/// `(exit_code, stdout, stderr)`. Panics on spawn failure.
fn run(args: &[&str], cwd: &Path, env: &[(&str, Option<&str>)]) -> (i32, String, String) {
    let mut cmd = Command::new(neurogrim_bin());
    cmd.args(args).current_dir(cwd);
    for (k, v) in env {
        match v {
            Some(value) => {
                cmd.env(k, value);
            }
            None => {
                cmd.env_remove(k);
            }
        }
    }
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn neurogrim {args:?}: {e}"));
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Read `<root>/.claude/brain/invocation-ledger.jsonl` as a Vec of
/// parsed JSON values, one per non-empty line. Returns empty Vec if
/// the file doesn't exist (used by negative-path assertions).
fn read_ledger(project_root: &Path) -> Vec<Value> {
    let path = project_root
        .join(".claude")
        .join("brain")
        .join("invocation-ledger.jsonl");
    if !path.exists() {
        return Vec::new();
    }
    let text = std::fs::read_to_string(&path).expect("read ledger");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("parse JSONL line"))
        .collect()
}

/// Empty tempdir — no `.claude/` exists yet, so the CLI must create
/// `.claude/brain/` itself.
fn empty_project() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

// ─── Test cases ───────────────────────────────────────────────────────

/// Closed-set vocabulary regression guard: typo'd kinds must error
/// at clap parse time, before any file I/O. The
/// `PossibleValuesParser` declared on the `--kind` arg is the
/// structural enforcement (Q1 lock, spec §17.12.2).
#[test]
fn closed_set_kind_rejected_at_parse_time() {
    let dir = empty_project();
    let (code, _stdout, stderr) = run(
        &[
            "disposition",
            "record",
            "--invocation-id",
            "tu_abc123",
            "--kind",
            "invalid_kind",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_ne!(code, 0, "invalid --kind must error at parse time");
    // clap's standard message names the bad value AND lists the valid
    // possibilities. Either substring is sufficient evidence the
    // PossibleValuesParser fired.
    assert!(
        stderr.contains("invalid_kind") || stderr.contains("invalid value"),
        "stderr should reference the bad value or 'invalid value'; got: {stderr}"
    );
    assert!(
        stderr.contains("accepted") || stderr.contains("possible values"),
        "stderr should list valid kinds or use 'possible values'; got: {stderr}"
    );
    // No ledger should have been written when parsing fails.
    assert!(
        read_ledger(dir.path()).is_empty(),
        "ledger must be untouched when --kind parse fails"
    );
}

/// Operator-identity discipline (§17.6): when neither `--operator`
/// nor `NEUROGRIM_OPERATOR` is set, the writer rejects with a clear
/// error. No "unknown" fallback — a ledger row with no audit trail
/// is a bug not a feature.
#[test]
fn operator_identity_required_when_neither_flag_nor_env() {
    let dir = empty_project();
    let (code, _stdout, stderr) = run(
        &[
            "disposition",
            "record",
            "--invocation-id",
            "tu_abc123",
            "--kind",
            "accepted",
        ],
        dir.path(),
        // Explicitly REMOVE the env var so a host-set
        // NEUROGRIM_OPERATOR doesn't silently satisfy the guard.
        &[("NEUROGRIM_OPERATOR", None)],
    );
    assert_ne!(code, 0, "missing operator must error");
    assert!(
        stderr.contains("operator identity"),
        "stderr should reference 'operator identity'; got: {stderr}"
    );
    assert!(
        read_ledger(dir.path()).is_empty(),
        "ledger must be untouched when operator identity is missing"
    );
}

/// Operator-identity falls back to NEUROGRIM_OPERATOR env when
/// --operator is absent. The successful row carries the env value
/// in `human_operator`.
#[test]
fn operator_identity_via_env() {
    let dir = empty_project();
    let (code, stdout, stderr) = run(
        &[
            "disposition",
            "record",
            "--invocation-id",
            "tu_xyz789",
            "--kind",
            "rejected",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_eq!(
        code, 0,
        "valid record should exit 0; stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("recorded disposition") && stdout.contains("alice"),
        "stdout should confirm the record + name the operator; got: {stdout}"
    );

    let entries = read_ledger(dir.path());
    assert_eq!(entries.len(), 1, "exactly one disposition row expected");
    assert_eq!(entries[0]["human_operator"], "alice");
    assert_eq!(entries[0]["entry_kind"], "disposition");
    assert_eq!(entries[0]["disposition_kind"], "rejected");
    assert_eq!(entries[0]["invocation_id"], "tu_xyz789");
}

/// Recursion-guard MUST (Q6 lock, spec §17.12.5): invocation IDs
/// starting with `operator_calibration:` are rejected at parse time.
/// The carve-out closes the recursion loop (operator dispositions a
/// sensor finding → re-counted in sensor's score) structurally.
#[test]
fn recursion_guard_rejects_operator_calibration_invocation_id() {
    let dir = empty_project();
    let (code, _stdout, stderr) = run(
        &[
            "disposition",
            "record",
            "--invocation-id",
            "operator_calibration:low_confidence",
            "--kind",
            "rejected",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_ne!(code, 0, "recursion-guard must reject");
    assert!(
        stderr.contains("recursion guard") || stderr.contains("§17.12.5"),
        "stderr should explain the recursion guard or cite §17.12.5; got: {stderr}"
    );
    assert!(
        read_ledger(dir.path()).is_empty(),
        "ledger must be untouched when recursion-guard fires"
    );
}

/// Successful end-to-end record: a valid invocation produces exactly
/// one JSONL row matching the schema's `DispositionEntry` shape.
///
/// **Q5 privacy regression pin (CLI integration layer):** the row
/// must contain ZERO of the forbidden free-text fields. If a future
/// agent regresses the schema or the writer struct, this assertion
/// catches it before merge.
#[test]
fn successful_record_appends_jsonl_row() {
    let dir = empty_project();
    let (code, stdout, stderr) = run(
        &[
            "disposition",
            "record",
            "--invocation-id",
            "tu_modified_test",
            "--kind",
            "modified",
            "--operator",
            "alice",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", None)],
    );
    assert_eq!(
        code, 0,
        "valid record should exit 0; stdout: {stdout}, stderr: {stderr}"
    );

    let entries = read_ledger(dir.path());
    assert_eq!(entries.len(), 1, "exactly one disposition row expected");

    let row = &entries[0];

    // Required schema fields per
    // invocation-ledger-v1.schema.json#/definitions/DispositionEntry.
    assert_eq!(row["schema_version"], "1");
    assert_eq!(row["entry_kind"], "disposition");
    assert_eq!(row["invocation_id"], "tu_modified_test");
    assert_eq!(row["disposition_kind"], "modified");
    assert_eq!(row["human_operator"], "alice");

    // ts must be ISO-8601 UTC (ends with 'Z'). This guards against
    // a future writer accidentally emitting a +00:00 form or a
    // local-time form.
    let ts = row["ts"].as_str().expect("ts is string");
    assert!(
        ts.ends_with('Z'),
        "ts must end with 'Z' (UTC); got: {ts}"
    );
    assert!(
        ts.contains('T'),
        "ts must be ISO-8601 (date'T'time); got: {ts}"
    );

    // Q5 privacy regression pin — the forbidden free-text fields
    // MUST NOT appear in any written row. The schema's
    // additionalProperties: false should make this impossible at
    // serialization time, but pin it explicitly at the integration
    // layer as a defense-in-depth check.
    for forbidden in ["note", "justification", "comment", "body", "text", "reason"] {
        assert!(
            row.get(forbidden).is_none(),
            "forbidden field {forbidden:?} must not appear (Q5 lock); row: {row}"
        );
    }
}

/// Append-only invariant: the writer must NOT clobber pre-existing
/// rows. We seed the ledger with a SkillEntry (mirroring what
/// `record-skill-invocation.sh` would write), then append a
/// disposition row, then verify both rows are present in order.
#[test]
fn append_only_preserves_existing_rows() {
    let dir = empty_project();

    // Seed the ledger with one SkillEntry row, mirroring the
    // record-skill-invocation.sh format (schema_version + ts + type +
    // name + session_id + invocation_id).
    let brain_dir = dir.path().join(".claude").join("brain");
    std::fs::create_dir_all(&brain_dir).expect("mkdir .claude/brain");
    let ledger_path = brain_dir.join("invocation-ledger.jsonl");
    let seed_row = r#"{"schema_version":"1","ts":"2026-04-27T19:00:00Z","type":"skill","name":"plan-critic","session_id":"sess-123","invocation_id":"tu_seed_skill"}"#;
    std::fs::write(&ledger_path, format!("{seed_row}\n")).expect("seed ledger");

    // Append a disposition row.
    let (code, _stdout, stderr) = run(
        &[
            "disposition",
            "record",
            "--invocation-id",
            "tu_seed_skill",
            "--kind",
            "accepted",
            "--operator",
            "alice",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", None)],
    );
    assert_eq!(code, 0, "record must succeed; stderr: {stderr}");

    // BOTH rows present, in original order. The skill row is row 0
    // (untouched); the disposition row is row 1 (newly appended).
    let entries = read_ledger(dir.path());
    assert_eq!(
        entries.len(),
        2,
        "ledger must contain seed skill row + new disposition row, in order"
    );
    assert_eq!(entries[0]["type"], "skill", "first row must be the skill seed");
    assert_eq!(
        entries[0]["invocation_id"], "tu_seed_skill",
        "skill row must be untouched"
    );
    assert_eq!(
        entries[0]["name"], "plan-critic",
        "skill row's name field must be preserved verbatim"
    );
    assert_eq!(
        entries[1]["entry_kind"], "disposition",
        "second row must be the new disposition"
    );
    assert_eq!(entries[1]["disposition_kind"], "accepted");
    assert_eq!(entries[1]["invocation_id"], "tu_seed_skill");
    assert_eq!(entries[1]["human_operator"], "alice");
}
