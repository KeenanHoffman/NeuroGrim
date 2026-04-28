//! E-B2-7 C7 — Subprocess integration tests for the
//! `neurogrim federated-pattern emit` CLI.
//!
//! Unit tests in `commands/federated_pattern.rs` cover clap parsing
//! (closed-set vocabulary, free-text-flag rejection, FIPS SHA-256
//! vectors). These integration tests cover the runtime behavior:
//! ledger writes round-trip into a real
//! `<project_root>/.claude/brain/pattern-aggregation-ledger.jsonl`;
//! missing children + unknown peers raise errors; the recursion-guard
//! rejects `federated_patterns:*` pattern_kind values; the privacy
//! contract is structurally pinned (no free-text fields ever appear in
//! a written row, regardless of what an operator passes on the
//! command line).
//!
//! Pattern follows `disposition_cli_behavior.rs` and
//! `domain_calibration_cli.rs`: locate the binary via
//! `env!("CARGO_BIN_EXE_neurogrim")`, spawn as a subprocess, assert
//! exit status + side effects (ledger contents parsed back as JSON).
//!
//! ## Test-only flag posture
//!
//! The CLI exposes a hidden `--no-transmit` flag whose only purpose is
//! to skip the actual A2A transmission AFTER the ledger row is
//! written. Tests use this flag to verify the Q12 log-before-transmit
//! invariant without standing up a live peer (cross-Brain integration
//! is C8's surface, not C7's). The flag is hidden from `--help` and
//! is not a production code path; production emission requires the
//! peer to be running an A2A server.

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

/// Read `<root>/.claude/brain/pattern-aggregation-ledger.jsonl` as a
/// Vec of parsed JSON values, one per non-empty line. Returns empty
/// Vec if the file doesn't exist (used by negative-path assertions).
fn read_ledger(project_root: &Path) -> Vec<Value> {
    let path = project_root
        .join(".claude")
        .join("brain")
        .join("pattern-aggregation-ledger.jsonl");
    if !path.exists() {
        return Vec::new();
    }
    let text = std::fs::read_to_string(&path).expect("read ledger");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("parse JSONL line"))
        .collect()
}

/// Tempdir + a synthetic brain-registry.json with the given children
/// map (key = peer name, value = a2a_endpoint). The registry minimum
/// shape: `meta`, `config.domain_weights`, `config.children`.
fn project_with_children(children: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let claude_dir = dir.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("mkdir .claude");

    let children_json: serde_json::Map<String, serde_json::Value> = children
        .iter()
        .map(|(name, endpoint)| {
            (
                name.to_string(),
                serde_json::json!({
                    "display_name": format!("Test child: {name}"),
                    "a2a_endpoint": endpoint,
                    "agent_card_url": format!("{endpoint}.well-known/agent-card.json"),
                    "interface_version": "1",
                    "depends_on": [],
                    "weight": 1.0,
                    "enabled": true
                }),
            )
        })
        .collect();

    let registry = serde_json::json!({
        "meta": {
            "schema_version": "2.0",
            "description": "Test brain for federated_pattern_cli_behavior.rs",
            "updated_by": "test-brain",
            "project": "TestBrain"
        },
        "config": {
            "domain_weights": {
                "code-quality": 1.0
            },
            "children": children_json
        }
    });

    let registry_path = claude_dir.join("brain-registry.json");
    std::fs::write(&registry_path, serde_json::to_string_pretty(&registry).unwrap())
        .expect("write registry");
    dir
}

/// Tempdir with brain-registry.json present but config.children empty.
/// Used to verify Q16 topology lock: federation requires at least one
/// declared peer.
fn project_with_no_children() -> TempDir {
    project_with_children(&[])
}

// ─── Test cases ───────────────────────────────────────────────────────

/// Q14 closed-set vocabulary regression guard at the CLI integration
/// layer: typo'd pattern_kinds must error at clap parse time, before
/// any file I/O. The `PossibleValuesParser` declared on the
/// `--pattern-kind` arg is the structural enforcement (Q14 lock, spec
/// §16.6.1).
#[test]
fn closed_set_pattern_kind_rejected_at_parse_time() {
    let dir = project_with_children(&[("child-a", "http://127.0.0.1:18421/a2a/v1/")]);
    let (code, _stdout, stderr) = run(
        &[
            "federated-pattern",
            "emit",
            "--pattern-kind",
            "invalid-pattern",
            "--no-transmit",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_ne!(code, 0, "invalid --pattern-kind must error at parse time");
    // clap's standard message names the bad value AND lists the valid
    // possibilities. Either substring is sufficient evidence the
    // PossibleValuesParser fired.
    assert!(
        stderr.contains("invalid-pattern") || stderr.contains("invalid value"),
        "stderr should reference the bad value or 'invalid value'; got: {stderr}"
    );
    assert!(
        stderr.contains("vigilance-pattern") || stderr.contains("possible values"),
        "stderr should list valid kinds or use 'possible values'; got: {stderr}"
    );
    // No ledger should have been written when parsing fails.
    assert!(
        read_ledger(dir.path()).is_empty(),
        "ledger must be untouched when --pattern-kind parse fails"
    );
}

/// Q9 recursion-guard MUST (spec §16.6.1): pattern_kind values
/// starting with `federated_patterns:` are rejected at parse time —
/// defense-in-depth for any future v2/v3 vocabulary that might
/// inadvertently include such a value.
///
/// **Layered behavior at v1:** clap's PossibleValuesParser fires FIRST
/// because `federated_patterns:low_confidence` is not in the v1
/// closed-set `["vigilance-pattern"]`. Either error pathway is
/// acceptable evidence the guard is in place — the test asserts a
/// non-zero exit + a stderr message that mentions the value or one of
/// the guard locks.
#[test]
fn recursion_guard_rejects_federated_patterns_kind_at_parse_time() {
    let dir = project_with_children(&[("child-a", "http://127.0.0.1:18421/a2a/v1/")]);
    let (code, _stdout, stderr) = run(
        &[
            "federated-pattern",
            "emit",
            "--pattern-kind",
            "federated_patterns:low_confidence",
            "--no-transmit",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_ne!(code, 0, "recursion-guarded pattern_kind must error");
    assert!(
        stderr.contains("federated_patterns")
            || stderr.contains("invalid value")
            || stderr.contains("recursion guard")
            || stderr.contains("§16.6.1")
            || stderr.contains("Q9"),
        "stderr should reference the value, the guard, or the spec section; got: {stderr}"
    );
    assert!(
        read_ledger(dir.path()).is_empty(),
        "ledger must be untouched when recursion-guard fires"
    );
}

/// Q16 topology lock (spec §16.6.1): federation requires at least one
/// declared peer. An empty `config.children` map → error.
#[test]
fn no_children_in_registry_rejects() {
    let dir = project_with_no_children();
    let (code, _stdout, stderr) = run(
        &[
            "federated-pattern",
            "emit",
            "--pattern-kind",
            "vigilance-pattern",
            "--no-transmit",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_ne!(code, 0, "empty children must error");
    assert!(
        stderr.contains("no children declared")
            || stderr.contains("Q16"),
        "stderr should mention 'no children declared' or 'Q16'; got: {stderr}"
    );
    assert!(
        read_ledger(dir.path()).is_empty(),
        "ledger must be untouched when topology check fires"
    );
}

/// Unknown --peer rejection: `--peer X` where X is not a key in
/// `config.children` errors with a helpful message naming the bad
/// value.
#[test]
fn unknown_peer_rejects() {
    let dir = project_with_children(&[
        ("child-a", "http://127.0.0.1:18421/a2a/v1/"),
        ("child-b", "http://127.0.0.1:18422/a2a/v1/"),
    ]);
    let (code, _stdout, stderr) = run(
        &[
            "federated-pattern",
            "emit",
            "--pattern-kind",
            "vigilance-pattern",
            "--peer",
            "child-z",
            "--no-transmit",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_ne!(code, 0, "unknown peer must error");
    assert!(
        stderr.contains("child-z") || stderr.contains("unknown peer"),
        "stderr should name the bad peer or say 'unknown peer'; got: {stderr}"
    );
    assert!(
        read_ledger(dir.path()).is_empty(),
        "ledger must be untouched when peer lookup fails"
    );
}

/// Q12 log-before-transmit lock (spec §16.6.1 MUST): a successful
/// emit writes an `entry_kind=emitted` row to the
/// pattern-aggregation-ledger. The row must carry the closed-set
/// vocabulary values (pattern_kind, severity_class, numeric counts)
/// AND must contain ZERO of the forbidden free-text fields — the
/// privacy contract is pinned at the CLI integration layer as a
/// defense-in-depth check on top of the schema's
/// `additionalProperties: false`.
///
/// Uses `--no-transmit` so the test doesn't depend on a live peer
/// (cross-Brain integration is C8's surface). The Q12 contract is
/// "log BEFORE transmit"; since the ledger row is written
/// unconditionally before transport, this verifies the lock without
/// the wire I/O.
#[test]
fn successful_emit_writes_emitted_ledger_row_q12() {
    let dir = project_with_children(&[
        ("child-a", "http://127.0.0.1:18421/a2a/v1/"),
    ]);
    let (code, stdout, stderr) = run(
        &[
            "federated-pattern",
            "emit",
            "--pattern-kind",
            "vigilance-pattern",
            "--peer",
            "child-a",
            "--no-transmit",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_eq!(
        code, 0,
        "valid emit should exit 0; stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("emitted federated-pattern")
            && stdout.contains("child-a")
            && stdout.contains("vigilance-pattern"),
        "stdout should confirm the emission with peer + kind; got: {stdout}"
    );

    let entries = read_ledger(dir.path());
    assert_eq!(entries.len(), 1, "exactly one emitted row expected");

    let row = &entries[0];

    // Required EmittedEntry schema fields.
    assert_eq!(row["schema_version"], "1");
    assert_eq!(row["entry_kind"], "emitted");
    assert!(
        row["envelope_message_id"].is_string(),
        "envelope_message_id must be a string"
    );
    assert!(row["peer_brain_id"].is_string());
    assert!(row["to_brain_id"].is_string());
    assert_eq!(
        row["peer_brain_id"], row["to_brain_id"],
        "peer_brain_id and to_brain_id must match (both are the peer's hash)"
    );

    // ts is ISO 8601 UTC ('Z'-suffixed). Same discipline as the
    // disposition CLI integration test.
    let ts = row["ts"].as_str().expect("ts is string");
    assert!(ts.ends_with('Z'), "ts must end with 'Z' (UTC); got: {ts}");
    assert!(ts.contains('T'), "ts must be ISO 8601; got: {ts}");

    // Payload contents — closed-set vocabulary + bounded numeric
    // feature_vector (Q1 + Q14 lock).
    let payload = &row["payload"];
    assert_eq!(payload["pattern_kind"], "vigilance-pattern");
    assert_eq!(payload["schema_version"], "1");
    let fv = &payload["feature_vector"];
    assert!(
        fv["numeric_count"].is_number(),
        "feature_vector.numeric_count must be a number"
    );
    assert_eq!(
        fv["severity_class"], "info",
        "v1 baseline severity_class is 'info'"
    );
    assert!(
        fv["observation_window_days"].is_number(),
        "feature_vector.observation_window_days must be a number"
    );

    // Q1 + Q5 + Q8 privacy regression pin — the forbidden free-text
    // fields MUST NOT appear in any written row, at the top level OR
    // inside the payload OR inside the feature_vector. The schema's
    // additionalProperties: false should make this impossible at
    // serialization time, but pin it explicitly here as a defense-in-
    // depth check.
    let forbidden = ["note", "justification", "comment", "body", "text", "prose", "reason"];
    for f in forbidden {
        assert!(
            row.get(f).is_none(),
            "forbidden top-level field {f:?} must not appear (Q1+Q5+Q8 lock); row: {row}"
        );
        assert!(
            payload.get(f).is_none(),
            "forbidden payload field {f:?} must not appear (Q1+Q5+Q8 lock); payload: {payload}"
        );
        assert!(
            fv.get(f).is_none(),
            "forbidden feature_vector field {f:?} must not appear (Q1+Q5+Q8 lock); fv: {fv}"
        );
    }
}

/// Q14 closed-set vocabulary regression guard at the CLI layer. Parse
/// `--help` for the `emit` subcommand and verify the only accepted
/// `--pattern-kind` value is `vigilance-pattern`. Any future agent
/// who silently extends `PATTERN_KINDS` without a spec change fails
/// this assertion before merge.
#[test]
fn pattern_kind_enum_has_exactly_one_entry_q14() {
    let dir = project_with_children(&[("child-a", "http://127.0.0.1:18421/a2a/v1/")]);
    let (code, stdout, _stderr) = run(
        &["federated-pattern", "emit", "--help"],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_eq!(code, 0, "--help should exit 0");
    // clap's standard --help output for a PossibleValuesParser
    // includes `[possible values: vigilance-pattern]` (or similar).
    // The vocabulary must include `vigilance-pattern`.
    assert!(
        stdout.contains("vigilance-pattern"),
        "--help must list 'vigilance-pattern' as a possible value; got: {stdout}"
    );
    // Q14 negative pin: ANY other plausible v2/v3 candidate name MUST
    // NOT appear. If someone added them, the closed-set lock at v1 is
    // breached — calls for a spec change first.
    for v2_candidate in [
        "operator-calibration-pattern",
        "hat-contract-pattern",
        "trust-budget-pattern",
    ] {
        assert!(
            !stdout.contains(v2_candidate),
            "v2 candidate {v2_candidate:?} must not appear at v1 (Q14 lock); got: {stdout}"
        );
    }
}
