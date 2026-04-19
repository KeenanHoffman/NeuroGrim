//! CLI smoke tests — the user-facing `neurogrim` command surface.
//!
//! The A2A end-to-end path is already covered by `dual_brain_pair.rs`
//! (subprocess spawn + HTTP round-trip). These smoke tests cover the
//! remaining subcommands that a user interacts with directly: `--version`,
//! `--help`, `validate`, and one `sensory` invocation.
//!
//! Pattern follows `dual_brain_pair.rs`: locate the binary via
//! `env!("CARGO_BIN_EXE_neurogrim")` (Cargo exports this for integration
//! tests of binary crates), spawn as a subprocess, assert exit status and
//! output. No external deps — plain `std::process::Command`.

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn neurogrim_bin() -> &'static str {
    env!("CARGO_BIN_EXE_neurogrim")
}

/// Run `neurogrim <args>` and return (exit_code, stdout, stderr).
/// Panics on spawn failure (means the binary is missing — a real test bug).
fn run(args: &[&str], cwd: Option<&Path>) -> (i32, String, String) {
    let mut cmd = Command::new(neurogrim_bin());
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
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

#[test]
fn version_prints_semver() {
    let (code, stdout, _) = run(&["--version"], None);
    assert_eq!(code, 0, "neurogrim --version should exit 0");
    // Output should contain a version string matching `major.minor.patch`.
    // Clap's default format is "neurogrim 0.1.0".
    assert!(
        stdout.contains('.'),
        "version output should include a dotted semver, got: {stdout:?}"
    );
}

#[test]
fn aliases_resolve_to_primary_commands() {
    // Grimoire-themed aliases wired in via `clap`'s `visible_alias`.
    // Each should accept --help and exit 0. If an alias is ever removed
    // from main.rs, this fires.
    for alias in [
        "scry", "divine", "plan", "seal", "summon", "cast", "conjure", "commune",
    ] {
        let (code, _stdout, stderr) = run(&[alias, "--help"], None);
        assert_eq!(
            code, 0,
            "alias {alias:?} should resolve to a command (--help exit 0); stderr={stderr}"
        );
    }
}

#[test]
fn help_lists_core_subcommands() {
    let (code, stdout, _) = run(&["--help"], None);
    assert_eq!(code, 0, "neurogrim --help should exit 0");
    // Every subcommand in main.rs should show up in --help. If someone
    // renames or removes a command without updating docs, this test fires.
    for sub in [
        "score",
        "agent",
        "health",
        "trend",
        "validate",
        "serve",
        "sensory",
        "init",
        "awareness",
        "a2a-serve",
        "a2a-invoke",
        "a2a-discover",
    ] {
        assert!(
            stdout.contains(sub),
            "--help should mention subcommand {sub:?}; got:\n{stdout}"
        );
    }
}

#[test]
fn validate_accepts_minimal_registry() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    // Minimum valid brain-registry.json per brain-registry-v2 schema.
    // Mirrors the shape of the real registries at .claude/brain-registry.json
    // in each of the three Brains — `meta`, `tools`, `data_sources`, `config`
    // with `domain_weights` + `advisory_domains` + `domain_definitions`.
    let registry = serde_json::json!({
        "meta": {
            "schema_version": "2",
            "updated_by": "smoke-test",
            "project": "smoke-test-fixture"
        },
        "tools": {},
        "data_sources": {},
        "config": {
            "domain_weights": { "smoke": 1.0 },
            "advisory_domains": [],
            "domain_definitions": {
                "smoke": {
                    "scoring_source": {
                        "type": "cmdb",
                        "path": ".claude/smoke-cmdb.json"
                    }
                }
            }
        }
    });
    let registry_path = claude_dir.join("brain-registry.json");
    std::fs::write(
        &registry_path,
        serde_json::to_string_pretty(&registry).unwrap(),
    )
    .unwrap();
    let (code, stdout, stderr) = run(
        &["validate", "--registry", registry_path.to_str().unwrap()],
        None,
    );
    assert_eq!(
        code, 0,
        "validate should accept a minimal valid registry. stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn validate_rejects_malformed_json() {
    let tmp = TempDir::new().unwrap();
    let registry_path = tmp.path().join("bad.json");
    std::fs::write(&registry_path, "{ this is not json }").unwrap();
    let (code, _stdout, stderr) = run(
        &["validate", "--registry", registry_path.to_str().unwrap()],
        None,
    );
    assert_ne!(code, 0, "validate should fail on broken JSON");
    assert!(
        !stderr.is_empty() || code != 0,
        "validate should report the error somewhere"
    );
}

#[test]
fn sensory_deploy_readiness_produces_cmdb_json() {
    let tmp = TempDir::new().unwrap();
    let (code, stdout, stderr) = run(
        &[
            "sensory",
            "deploy-readiness",
            "--project-root",
            tmp.path().to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(
        code, 0,
        "`sensory deploy-readiness` should exit 0. stdout={stdout} stderr={stderr}"
    );
    // Stdout is the pretty-printed CMDB envelope (see main.rs run_sensory).
    let parsed: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}; raw={stdout}"));
    assert!(parsed.get("score").is_some(), "CMDB missing `score`");
    assert!(parsed.get("meta").is_some(), "CMDB missing `meta`");
    assert!(parsed.get("findings").is_some(), "CMDB missing `findings`");
    let score = parsed["score"].as_u64().expect("score not integer");
    assert!(score <= 100, "score {score} out of range");
}

#[test]
fn score_outputs_numeric_score_for_minimal_project() {
    // End-to-end smoke: write a minimal registry + a CMDB the registry
    // points at, then run `neurogrim score`. Exercises the full path
    // through BrainContext::load + scoring pipeline + display, which
    // is otherwise only covered by unit tests per-module.
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    // One CMDB with a known score.
    std::fs::write(
        claude_dir.join("smoke-cmdb.json"),
        r#"{
          "meta": {
            "schema_version": "1",
            "updated_by": "fixture",
            "updated_at": "2026-04-17T00:00:00Z"
          },
          "score": 85,
          "updated_at": "2026-04-17T00:00:00Z",
          "findings": []
        }"#,
    )
    .unwrap();

    let registry = serde_json::json!({
        "meta": {
            "schema_version": "2",
            "updated_by": "smoke-test",
            "project": "smoke-test-fixture"
        },
        "tools": {},
        "data_sources": {},
        "config": {
            "domain_weights": { "smoke": 1.0 },
            "advisory_domains": [],
            "domain_definitions": {
                "smoke": {
                    "scoring_source": {
                        "type": "cmdb",
                        "path": ".claude/smoke-cmdb.json"
                    }
                }
            }
        }
    });
    let registry_path = claude_dir.join("brain-registry.json");
    std::fs::write(
        &registry_path,
        serde_json::to_string_pretty(&registry).unwrap(),
    )
    .unwrap();

    let (code, stdout, stderr) = run(
        &[
            "score",
            "--plain",
            "--registry",
            registry_path.to_str().unwrap(),
        ],
        Some(tmp.path()),
    );
    assert_eq!(
        code, 0,
        "`neurogrim score` should exit 0 with a valid registry. stdout={stdout} stderr={stderr}"
    );
    // The display layer owns exact formatting; we just assert that
    // SOMETHING numeric (the score itself, probably 85 here) made it to
    // stdout. A regression that dropped the score from the output would
    // produce all-letters or empty stdout and fail this.
    assert!(
        stdout.chars().any(|c| c.is_ascii_digit()),
        "score output should contain digits; got stdout={stdout:?}"
    );
}

#[test]
fn score_appends_to_score_history() {
    // Two invocations of `neurogrim score` against the same registry
    // should produce a score-history.json with 2 entries. Regression
    // guard for Session 6's principle #12 wire-up: the history file
    // lives at `.claude/brain/score-history.json` and grows on each
    // score/health invocation.
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    // Minimum valid CMDB so there's something to score
    std::fs::write(
        claude_dir.join("smoke-cmdb.json"),
        r#"{
          "meta": {
            "schema_version": "1",
            "updated_by": "fixture",
            "updated_at": "2026-04-17T00:00:00Z"
          },
          "score": 85,
          "updated_at": "2026-04-17T00:00:00Z",
          "findings": []
        }"#,
    )
    .unwrap();

    let registry = serde_json::json!({
        "meta": {
            "schema_version": "2",
            "updated_by": "smoke-test",
            "project": "smoke-test-fixture"
        },
        "tools": {},
        "data_sources": {},
        "config": {
            "domain_weights": { "smoke": 1.0 },
            "advisory_domains": [],
            "domain_definitions": {
                "smoke": {
                    "scoring_source": {
                        "type": "cmdb",
                        "path": ".claude/smoke-cmdb.json"
                    }
                }
            }
        }
    });
    let registry_path = claude_dir.join("brain-registry.json");
    std::fs::write(
        &registry_path,
        serde_json::to_string_pretty(&registry).unwrap(),
    )
    .unwrap();

    // First score invocation
    let (code1, _, stderr1) = run(
        &[
            "score",
            "--plain",
            "--registry",
            registry_path.to_str().unwrap(),
        ],
        Some(tmp.path()),
    );
    assert_eq!(code1, 0, "first score exit failed. stderr={stderr1}");

    // Second invocation — should append, not overwrite
    let (code2, _, stderr2) = run(
        &[
            "score",
            "--plain",
            "--registry",
            registry_path.to_str().unwrap(),
        ],
        Some(tmp.path()),
    );
    assert_eq!(code2, 0, "second score exit failed. stderr={stderr2}");

    // Assert the history file exists and has 2 entries
    let history_path = claude_dir.join("brain").join("score-history.json");
    assert!(
        history_path.is_file(),
        "expected {history_path:?} to exist after two score invocations"
    );
    let history_raw = std::fs::read_to_string(&history_path).unwrap();
    let history: Vec<Value> = serde_json::from_str(&history_raw)
        .unwrap_or_else(|e| panic!("history not parseable: {e}; raw={history_raw}"));
    assert_eq!(
        history.len(),
        2,
        "expected 2 snapshot entries; got {}. raw={history_raw}",
        history.len()
    );
    // Each entry should have score + scored_at + domains
    for (i, entry) in history.iter().enumerate() {
        assert!(
            entry.get("score").is_some(),
            "entry {i} missing score field: {entry}"
        );
        assert!(
            entry.get("scored_at").is_some(),
            "entry {i} missing scored_at field: {entry}"
        );
        assert!(
            entry.get("domains").is_some(),
            "entry {i} missing domains field: {entry}"
        );
    }
}

#[test]
fn score_appends_to_proposal_ledger_with_linked_pre_score() {
    // Two invocations of `neurogrim score` should produce a
    // proposal-ledger.json with 2 entries. The second entry's
    // `pre_score` should equal the first entry's `post_score` —
    // proving the linked-list that gives compute_all_effectiveness
    // cause→effect signal (principle #4, closing the learning loop).
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    std::fs::write(
        claude_dir.join("smoke-cmdb.json"),
        r#"{
          "meta": {
            "schema_version": "1",
            "updated_by": "fixture",
            "updated_at": "2026-04-17T00:00:00Z"
          },
          "score": 85,
          "updated_at": "2026-04-17T00:00:00Z",
          "findings": []
        }"#,
    )
    .unwrap();

    let registry = serde_json::json!({
        "meta": {
            "schema_version": "2",
            "updated_by": "smoke-test",
            "project": "smoke-test-fixture"
        },
        "tools": {},
        "data_sources": {},
        "config": {
            "domain_weights": { "smoke": 1.0 },
            "advisory_domains": [],
            "domain_definitions": {
                "smoke": {
                    "scoring_source": {
                        "type": "cmdb",
                        "path": ".claude/smoke-cmdb.json"
                    }
                }
            }
        }
    });
    let registry_path = claude_dir.join("brain-registry.json");
    std::fs::write(
        &registry_path,
        serde_json::to_string_pretty(&registry).unwrap(),
    )
    .unwrap();

    // Run score twice
    for _ in 0..2 {
        let (code, _, stderr) = run(
            &[
                "score",
                "--plain",
                "--registry",
                registry_path.to_str().unwrap(),
            ],
            Some(tmp.path()),
        );
        assert_eq!(code, 0, "score exit failed. stderr={stderr}");
    }

    let ledger_path = claude_dir.join("brain").join("proposal-ledger.json");
    assert!(
        ledger_path.is_file(),
        "expected {ledger_path:?} after two score invocations"
    );
    let ledger_raw = std::fs::read_to_string(&ledger_path).unwrap();
    let ledger: Vec<Value> = serde_json::from_str(&ledger_raw)
        .unwrap_or_else(|e| panic!("ledger not parseable: {e}; raw={ledger_raw}"));
    assert_eq!(
        ledger.len(),
        2,
        "expected 2 ledger entries; got {}",
        ledger.len()
    );
    // Each entry must have the core fields
    for (i, entry) in ledger.iter().enumerate() {
        assert!(
            entry.get("timestamp").is_some(),
            "entry {i} missing timestamp"
        );
        assert!(
            entry.get("post_score").is_some(),
            "entry {i} missing post_score"
        );
        assert!(
            entry.get("proposals").is_some(),
            "entry {i} missing proposals array"
        );
    }
    // The second entry's pre_score should equal the first entry's
    // post_score — the linked-list invariant that drives effectiveness
    // math.
    let first_post = ledger[0]["post_score"].as_i64().unwrap();
    let second_pre = ledger[1]["pre_score"].as_i64().unwrap();
    assert_eq!(
        second_pre, first_post,
        "linked-list broken: second entry pre_score ({second_pre}) != first entry post_score ({first_post})"
    );
}
