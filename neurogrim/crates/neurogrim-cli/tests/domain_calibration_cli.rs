//! E-B2-2 C6 — Subprocess integration tests for the
//! `neurogrim domain-calibration {list, triage, manual}` CLI.
//!
//! The unit tests in `commands/domain_calibration.rs` cover clap
//! parsing (decision-enum validation, required-arg checks, default
//! values). These integration tests cover the runtime behavior:
//! ledger writes round-trip through `read_all` + `fold`; unknown
//! domains are rejected against the registry; missing operator
//! identity raises an error; the triage path requires a matching
//! pending entry.
//!
//! Pattern follows `cli_smoke.rs` and `dual_brain_pair.rs`: locate
//! the binary via `env!("CARGO_BIN_EXE_neurogrim")`, spawn as a
//! subprocess, assert exit status + side effects (ledger contents).

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

/// Build a tempdir-backed project root with `.claude/brain-registry.json`
/// containing one declared domain (`test-health`). Returns the TempDir
/// (caller-owned; cleaned up on drop).
fn make_project(domains: &[&str]) -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).expect("mkdir .claude");

    let mut weights = serde_json::Map::new();
    let mut defs = serde_json::Map::new();
    for d in domains {
        weights.insert(d.to_string(), Value::from(0.0));
        defs.insert(
            d.to_string(),
            serde_json::json!({
                "scoring_source": { "type": "cmdb", "path": format!(".claude/{}-cmdb.json", d) }
            }),
        );
    }
    let registry = serde_json::json!({
        "meta": {
            "schema_version": "2",
            "description": "C6 integration test fixture",
            "updated_by": "test-suite"
        },
        "tools": {},
        "data_sources": {},
        "config": {
            "domain_weights": Value::Object(weights),
            "advisory_domains": domains,
            "domain_definitions": Value::Object(defs),
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
    });
    std::fs::write(
        claude.join("brain-registry.json"),
        serde_json::to_string_pretty(&registry).unwrap(),
    )
    .expect("write registry");
    dir
}

/// Read `<root>/.claude/brain/<domain>-calibration-ledger.jsonl`
/// and return its lines as parsed JSON. Returns empty Vec if the
/// file doesn't exist (used by negative-path assertions).
fn read_ledger_entries(project_root: &Path, domain: &str) -> Vec<Value> {
    let path = project_root
        .join(".claude")
        .join("brain")
        .join(format!("{domain}-calibration-ledger.jsonl"));
    if !path.exists() {
        return Vec::new();
    }
    let text = std::fs::read_to_string(&path).expect("read ledger");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse JSONL line"))
        .collect()
}

// ─── Test cases ───────────────────────────────────────────────────────

#[test]
fn manual_then_triage_round_trips_through_ledger() {
    let dir = make_project(&["test-health"]);
    let env = [("NEUROGRIM_OPERATOR", Some("alice"))];

    // Manual: append a pending entry.
    let (code, stdout, stderr) = run(
        &[
            "domain-calibration",
            "manual",
            "--domain",
            "test-health",
            "--actual-score",
            "45",
            "--signal",
            "manual:operator-spotted",
            "--context",
            "saw a regression in nightly",
        ],
        dir.path(),
        &env,
    );
    assert_eq!(
        code, 0,
        "manual should exit 0; stdout: {stdout}, stderr: {stderr}"
    );
    let entries = read_ledger_entries(dir.path(), "test-health");
    assert_eq!(entries.len(), 1, "ledger should have one pending entry");
    assert_eq!(entries[0]["entry_kind"], "pending");
    assert_eq!(entries[0]["domain"], "test-health");
    assert_eq!(entries[0]["actual_score"], 45);
    assert_eq!(entries[0]["trigger_signal_kind"], "manual:operator-spotted");
    assert_eq!(entries[0]["context_notes"], "saw a regression in nightly");

    // Triage: supersede the pending entry.
    let pending_ts = entries[0]["ts"].as_f64().expect("ts is float");
    let (code, _, stderr) = run(
        &[
            "domain-calibration",
            "triage",
            "--domain",
            "test-health",
            "--pending-ts",
            &pending_ts.to_string(),
            "--decision",
            "no-action",
            "--note",
            "Score drop was a deliberate test-suite restructure.",
        ],
        dir.path(),
        &env,
    );
    assert_eq!(code, 0, "triage should exit 0; stderr: {stderr}");

    let entries = read_ledger_entries(dir.path(), "test-health");
    assert_eq!(entries.len(), 2, "ledger should have pending + triaged");
    assert_eq!(entries[1]["entry_kind"], "triaged");
    assert_eq!(entries[1]["triage_decision"], "no-action");
    assert_eq!(entries[1]["human_operator"], "alice");
    assert_eq!(
        entries[1]["human_notes"],
        "Score drop was a deliberate test-suite restructure."
    );
    let supersedes = entries[1]["supersedes_ts"].as_f64().expect("supersedes_ts");
    assert!(
        (supersedes - pending_ts).abs() < 1e-6,
        "supersedes_ts must match pending ts (got {supersedes} vs {pending_ts})"
    );
}

#[test]
fn manual_rejects_unknown_domain() {
    // Registry only declares `test-health`; `code-quality` should be rejected.
    let dir = make_project(&["test-health"]);
    let (code, _, stderr) = run(
        &[
            "domain-calibration",
            "manual",
            "--domain",
            "code-quality",
            "--actual-score",
            "70",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_ne!(code, 0, "unknown domain must error");
    assert!(
        stderr.contains("code-quality") && stderr.contains("not declared"),
        "error should name the bad domain + reference the registry; got: {stderr}"
    );
    // No ledger should have been written.
    let cq_entries = read_ledger_entries(dir.path(), "code-quality");
    assert!(
        cq_entries.is_empty(),
        "code-quality ledger should not exist after rejection"
    );
}

#[test]
fn triage_rejects_unknown_domain() {
    let dir = make_project(&["test-health"]);
    let (code, _, stderr) = run(
        &[
            "domain-calibration",
            "triage",
            "--domain",
            "code-quality",
            "--pending-ts",
            "1745000000.123",
            "--decision",
            "confirmed",
            "--note",
            "x",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_ne!(code, 0, "unknown domain must error");
    assert!(
        stderr.contains("code-quality") && stderr.contains("not declared"),
        "error should name the bad domain; got: {stderr}"
    );
}

#[test]
fn triage_requires_operator_identity() {
    // Both NEUROGRIM_OPERATOR unset AND --operator absent → error
    // (no "unknown" fallback; per §17.6 audit-rationale discipline).
    let dir = make_project(&["test-health"]);
    let (code, _, stderr) = run(
        &[
            "domain-calibration",
            "triage",
            "--domain",
            "test-health",
            "--pending-ts",
            "1.0",
            "--decision",
            "confirmed",
            "--note",
            "x",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", None)],
    );
    assert_ne!(code, 0, "missing operator must error");
    assert!(
        stderr.contains("operator identity") || stderr.contains("NEUROGRIM_OPERATOR"),
        "error should name the env var or operator-identity requirement; got: {stderr}"
    );
}

#[test]
fn triage_rejects_no_matching_pending_entry() {
    // Ledger has no pending entry at the given ts → triage fails.
    let dir = make_project(&["test-health"]);
    let (code, _, stderr) = run(
        &[
            "domain-calibration",
            "triage",
            "--domain",
            "test-health",
            "--pending-ts",
            "9999999999.999",
            "--decision",
            "confirmed",
            "--note",
            "x",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_ne!(code, 0, "no matching pending should error");
    assert!(
        stderr.contains("no OPEN pending") || stderr.contains("not found"),
        "error should explain the missing-pending case; got: {stderr}"
    );
}

#[test]
fn manual_requires_operator_identity() {
    let dir = make_project(&["test-health"]);
    let (code, _, stderr) = run(
        &[
            "domain-calibration",
            "manual",
            "--domain",
            "test-health",
            "--actual-score",
            "30",
        ],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", None)],
    );
    assert_ne!(code, 0, "missing operator must error");
    assert!(
        stderr.contains("operator") || stderr.contains("NEUROGRIM_OPERATOR"),
        "error should reference operator identity; got: {stderr}"
    );
}

#[test]
fn list_on_empty_brain_dir_exits_clean() {
    // No `.claude/brain/` exists yet; list should NOT crash + should
    // signal "no entries" gracefully.
    let dir = make_project(&["test-health"]);
    let (code, stdout, stderr) = run(
        &["domain-calibration", "list"],
        dir.path(),
        &[("NEUROGRIM_OPERATOR", Some("alice"))],
    );
    assert_eq!(code, 0, "list on empty must exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("no entries") || stdout.contains("clean"),
        "stdout should indicate clean state; got: {stdout}"
    );
}

#[test]
fn list_after_manual_shows_pending() {
    let dir = make_project(&["test-health"]);
    let env = [("NEUROGRIM_OPERATOR", Some("alice"))];

    // Append one pending entry.
    let (code, _, _) = run(
        &[
            "domain-calibration",
            "manual",
            "--domain",
            "test-health",
            "--actual-score",
            "55",
        ],
        dir.path(),
        &env,
    );
    assert_eq!(code, 0);

    // List should show OPEN PENDING (1).
    let (code, stdout, stderr) = run(
        &[
            "domain-calibration",
            "list",
            "--domain",
            "test-health",
        ],
        dir.path(),
        &env,
    );
    assert_eq!(code, 0, "list should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("OPEN PENDING") || stdout.contains("test-health"),
        "list output should show the pending entry; got: {stdout}"
    );
}
