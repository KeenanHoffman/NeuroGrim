//! Integration tests for `neurogrim domain new` (v3.2 Phase C.2).
//!
//! Subprocess-based: spawn the actual `neurogrim` binary against a
//! tmpdir and assert on the resulting filesystem state. Heavier-weight
//! than the unit tests in `commands/domain.rs`, but exercises the full
//! pipeline (CLI parsing → registry mutation → CMDB write → optional
//! sensor scaffolding).
//!
//! Mirrors `init_template_behavior.rs` shape exactly.

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn neurogrim_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_neurogrim"))
}

/// Spawn `neurogrim init --template abstract-project` in `dir` to set
/// up a minimal Brain registry against which `domain new` can mutate.
/// Returns the registry path.
fn init_abstract(dir: &TempDir, name: &str) -> PathBuf {
    let registry_path = dir.path().join(".claude/brain-registry.json");
    let out = Command::new(neurogrim_bin())
        .args([
            "init",
            "--project-root",
            dir.path().to_str().unwrap(),
            "--output",
            registry_path.to_str().unwrap(),
            "--template",
            "abstract-project",
            "--name",
            name,
            "--yes",
        ])
        .output()
        .expect("failed to spawn neurogrim init");
    assert!(
        out.status.success(),
        "init failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    registry_path
}

fn run_domain_new(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(neurogrim_bin())
        .arg("domain")
        .arg("new")
        .args(args)
        .output()
        .expect("failed to spawn neurogrim domain new");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn domain_new_stub_creates_registry_entry_and_cmdb() {
    let tmp = TempDir::new().unwrap();
    init_abstract(&tmp, "stub-test");

    let (ok, _stdout, stderr) = run_domain_new(&[
        "test-coverage",
        "--description",
        "Test Coverage Tracking",
        "--directory",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(ok, "domain new failed: {stderr}");

    let registry: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".claude/brain-registry.json")).unwrap(),
    )
    .unwrap();
    let cfg = &registry["config"];
    assert_eq!(cfg["domain_weights"]["test-coverage"], 0.0);
    assert_eq!(cfg["principle_map"]["test-coverage"], "Test Coverage Tracking");
    assert_eq!(
        cfg["domain_definitions"]["test-coverage"]["scoring_source"]["type"],
        "cmdb"
    );

    let cmdb_path = tmp.path().join(".claude/test-coverage-cmdb.json");
    assert!(cmdb_path.is_file());
    let cmdb: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cmdb_path).unwrap()).unwrap();
    assert_eq!(cmdb["score"], 50);
    assert!(cmdb["meta"]["updated_at"].is_string());
}

#[test]
fn domain_new_python_scaffolds_sensor() {
    let tmp = TempDir::new().unwrap();
    init_abstract(&tmp, "python-test");

    let (ok, _stdout, stderr) = run_domain_new(&[
        "my-coverage",
        "--type",
        "python",
        "--directory",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(ok, "domain new --type python failed: {stderr}");

    let sensor = tmp.path().join("sensory/check_my_coverage.py");
    assert!(sensor.is_file());
    let py = std::fs::read_to_string(&sensor).unwrap();
    assert!(py.contains("def analyze("));
    assert!(py.contains("schema_version"));
    assert!(py.contains("check-my-coverage"));
}

#[test]
fn domain_new_refuses_duplicate_without_force() {
    let tmp = TempDir::new().unwrap();
    init_abstract(&tmp, "dup-test");

    let (ok1, _, _) = run_domain_new(&[
        "dup",
        "--directory",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(ok1);

    let (ok2, _, stderr2) = run_domain_new(&[
        "dup",
        "--directory",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(!ok2, "second invocation should refuse");
    assert!(
        stderr2.contains("already registered") || stderr2.contains("--force"),
        "stderr should explain refusal: {stderr2}"
    );
}

#[test]
fn domain_new_force_overwrites() {
    let tmp = TempDir::new().unwrap();
    init_abstract(&tmp, "force-test");

    let (ok1, _, _) = run_domain_new(&[
        "fo",
        "--description",
        "Original",
        "--directory",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(ok1);
    let (ok2, _, stderr2) = run_domain_new(&[
        "fo",
        "--description",
        "Renamed",
        "--directory",
        tmp.path().to_str().unwrap(),
        "--force",
    ]);
    assert!(ok2, "second invocation with --force failed: {stderr2}");

    let registry: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".claude/brain-registry.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(registry["config"]["principle_map"]["fo"], "Renamed");
}

#[test]
fn domain_new_validates_kebab_case() {
    let tmp = TempDir::new().unwrap();
    init_abstract(&tmp, "kebab-test");

    let (ok, _stdout, stderr) = run_domain_new(&[
        "BadName",
        "--directory",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(!ok, "BadName should be rejected");
    assert!(
        stderr.contains("kebab-case") || stderr.contains("lowercase"),
        "stderr should explain naming rule: {stderr}"
    );
}

#[test]
fn domain_new_then_validate_succeeds() {
    let tmp = TempDir::new().unwrap();
    let registry_path = init_abstract(&tmp, "validate-test");

    let (ok, _, stderr) = run_domain_new(&[
        "post-validate",
        "--directory",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(ok, "domain new failed: {stderr}");

    let validate = Command::new(neurogrim_bin())
        .args(["validate", "--registry", registry_path.to_str().unwrap()])
        .output()
        .expect("validate spawn");
    assert!(
        validate.status.success(),
        "validate failed: {}",
        String::from_utf8_lossy(&validate.stderr)
    );
    let stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(stdout.contains("Result: VALID"), "got: {stdout}");
}

#[test]
fn domain_new_then_agent_prose_lists_domain() {
    let tmp = TempDir::new().unwrap();
    let registry_path = init_abstract(&tmp, "prose-test");

    let (ok, _, _) = run_domain_new(&[
        "freshly-added",
        "--description",
        "Fresh New Thing",
        "--directory",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(ok);

    let prose = Command::new(neurogrim_bin())
        .args([
            "agent",
            "--prose",
            "--plain",
            "--registry",
            registry_path.to_str().unwrap(),
        ])
        .output()
        .expect("agent --prose spawn");
    assert!(prose.status.success(), "agent --prose failed");
    let stdout = String::from_utf8_lossy(&prose.stdout);
    assert!(
        stdout.contains("Fresh New Thing") || stdout.contains("freshly-added"),
        "agent --prose should mention the new domain; got:\n{stdout}"
    );
}
