//! Spec §3.8 Testing Discipline — Tier-A behavioral coverage for all 8
//! Rust sensors.
//!
//! Each test:
//! 1. Creates a fresh `tempfile::TempDir` as the project root.
//! 2. Populates the minimum fixture state the sensor needs to run.
//! 3. Calls the sensor's `analyze_*` function.
//! 4. Asserts the output validates against `cmdb-envelope-v1.schema.json`
//!    (reusing the same loader as `schema_conformance.rs`) and that the
//!    score is in `[0, 100]`.
//!
//! Tier A is deliberately minimal: "the sensor runs, doesn't panic, and
//! produces a well-formed envelope." Tier B (state-specific behavioral
//! assertions — "dirty git repo → score ≤ 80") is future work. The value
//! of Tier A is catching regressions where a sensor crashes on missing
//! files, produces malformed JSON, or drops required envelope fields.
//!
//! When the schema file isn't reachable (standalone checkout with no
//! sibling `LSP-Brains/`), tests print a skip marker instead of failing —
//! same pattern as `schema_conformance.rs`.

use std::path::PathBuf;

use jsonschema::JSONSchema;
use serde_json::Value;
use tempfile::TempDir;

fn locate_cmdb_schema() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../../../LSP-Brains/schemas/cmdb-envelope-v1.schema.json"),
        manifest_dir.join("../../../LSP-Brains/schemas/cmdb-envelope-v1.schema.json"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn load_schema() -> Option<JSONSchema> {
    let path = locate_cmdb_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&value)
        .ok()
}

/// Validate an envelope against the canonical schema + score-range assertion.
/// When the schema isn't reachable, skips the schema check but still asserts
/// the score shape is right (cheap safety even without the schema).
fn assert_envelope_healthy(tag: &str, env: &Value) {
    // Score present and in [0, 100] regardless of schema reachability.
    let score = env
        .get("score")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("{tag}: score missing or not integer"));
    assert!(score <= 100, "{tag}: score {score} out of range");

    // Schema check when reachable.
    if let Some(schema) = load_schema() {
        let errs: Vec<_> = schema
            .validate(env)
            .err()
            .map(|it| it.map(|e| format!("{e} at {}", e.instance_path)).collect())
            .unwrap_or_default();
        assert!(
            errs.is_empty(),
            "{tag}: schema validation failed: {}",
            errs.join("; ")
        );
    } else {
        eprintln!("{tag}: skipping canonical-schema check (sibling LSP-Brains not reachable)");
    }
}

#[tokio::test]
async fn code_quality_runs_on_minimal_cargo_project() {
    let tmp = TempDir::new().unwrap();
    // Minimal Rust project so code_quality has something to inspect.
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
    let env =
        motherbrain_sensory::code_quality::analyze_code_quality(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("code_quality", &env);
}

#[tokio::test]
async fn test_results_runs_on_empty_project() {
    let tmp = TempDir::new().unwrap();
    let env =
        motherbrain_sensory::test_results::analyze_test_health(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("test_results", &env);
}

#[tokio::test]
async fn deploy_readiness_runs_on_empty_project() {
    let tmp = TempDir::new().unwrap();
    let env = motherbrain_sensory::deploy_readiness::analyze_deploy_readiness(
        tmp.path().to_str().unwrap(),
    )
    .await;
    assert_envelope_healthy("deploy_readiness", &env);
}

#[tokio::test]
async fn security_standards_runs_on_empty_project() {
    let tmp = TempDir::new().unwrap();
    let env = motherbrain_sensory::security_standards::analyze_security_standards(
        tmp.path().to_str().unwrap(),
    )
    .await;
    assert_envelope_healthy("security_standards", &env);
}

#[tokio::test]
async fn coherence_runs_with_one_stub_cmdb() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    // A minimum valid CMDB envelope the coherence sensor can read.
    std::fs::write(
        claude_dir.join("example-cmdb.json"),
        r#"{
          "meta": {
            "schema_version": "1",
            "updated_by": "fixture",
            "updated_at": "2026-04-17T00:00:00Z"
          },
          "score": 100,
          "updated_at": "2026-04-17T00:00:00Z",
          "findings": []
        }"#,
    )
    .unwrap();
    let env = motherbrain_sensory::coherence::analyze_coherence(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("coherence", &env);
}

#[tokio::test]
async fn human_comms_runs_with_empty_manifest() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("human-comms.yaml"), "").unwrap();
    let env =
        motherbrain_sensory::human_comms::analyze_human_comms(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("human_comms", &env);
}

#[tokio::test]
async fn secret_refs_runs_with_empty_manifest() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("secret-refs.yaml"), "").unwrap();
    let env =
        motherbrain_sensory::secret_refs::analyze_secret_refs(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("secret_refs", &env);
}

#[tokio::test]
async fn git_health_runs_on_initialized_repo() {
    let tmp = TempDir::new().unwrap();
    // Initialize a real git repo so the sensor has `.git/` to read. This
    // catches git-health regressions that would otherwise require a full
    // cargo project at the workspace level.
    let status = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(tmp.path())
        .status()
        .expect("`git` must be on PATH for git_health fixture");
    assert!(status.success(), "git init failed");
    match motherbrain_sensory::git_health::analyze_git_health(tmp.path().to_str().unwrap()).await {
        Ok(env) => assert_envelope_healthy("git_health", &env),
        Err(e) => panic!("git_health on a just-initialized repo should not error: {e}"),
    }
}

#[tokio::test]
async fn git_health_produces_envelope_without_git_dir() {
    // Regression guard: even when `.git/` is missing, the sensor should
    // produce an envelope (score likely 0), not panic. A sensor that
    // panics on missing state defeats the whole observability layer.
    let tmp = TempDir::new().unwrap();
    let result =
        motherbrain_sensory::git_health::analyze_git_health(tmp.path().to_str().unwrap()).await;
    // Either an Ok envelope or a typed anyhow error — NOT a panic. If the
    // sensor's contract is "error on missing .git", that's legitimate; we
    // just assert it didn't abort the process.
    match result {
        Ok(env) => assert_envelope_healthy("git_health (no .git)", &env),
        Err(e) => eprintln!("git_health returned typed error on missing .git (acceptable): {e}"),
    }
}
