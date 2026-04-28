//! Integration tests for `neurogrim init --template <name>` (v3.1.1
//! init automation Phase 5).
//!
//! Subprocess-based end-to-end tests that invoke the actual `neurogrim`
//! binary against a tmpdir and assert on the resulting filesystem
//! state. These are heavier-weight than the unit tests in
//! `init_scaffold.rs` — they exercise the FULL pipeline (CLI parsing →
//! scaffolding → on-disk artifact production).
//!
//! The binary path is resolved via `CARGO_BIN_EXE_neurogrim`, the
//! standard cargo-set env var for integration tests.

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn neurogrim_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_neurogrim"))
}

/// Run `neurogrim init` with the given args; return (exit success, stdout, stderr).
fn run_init(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(neurogrim_bin())
        .arg("init")
        .args(args)
        .output()
        .expect("failed to spawn neurogrim init");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn abstract_project_init_produces_valid_brain() {
    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path().to_str().unwrap();
    let registry_path = tmp.path().join(".claude/brain-registry.json");
    let (ok, _stdout, stderr) = run_init(&[
        "--project-root",
        project_root,
        "--output",
        registry_path.to_str().unwrap(),
        "--template",
        "abstract-project",
        "--name",
        "smoketest-abstract",
        "--domains",
        "demo-a,demo-b",
        "--yes",
    ]);
    assert!(
        ok,
        "init should succeed; stderr was:\n{stderr}"
    );

    // All expected files materialized.
    let files = [
        ".claude/brain-registry.json",
        ".claude/culture.yaml",
        ".claude/demo-a-cmdb.json",
        ".claude/demo-b-cmdb.json",
        ".claude/human-comms-cmdb.json",
        ".claude/secret-refs-cmdb.json",
        ".claude/git-health-cmdb.json",
        ".claude/skills/hats/SKILL.md",
        ".claude/skills/hats/visionary.md",
        ".claude/skills/imagination-mode/SKILL.md",
        ".claude/skills/north-star/SKILL.md",
        ".claude/skills/rubber-duck/SKILL.md",
        ".claude/skills/human-comms/SKILL.md",
        ".claude/skills/write-skill/SKILL.md",
        ".claude/skills/neurogrim-onboarding/SKILL.md",
        ".claude/settings.local.json",
        "scripts/record-skill-invocation.sh",
        "CLAUDE.md",
        ".gitignore",
    ];
    for f in files {
        assert!(
            tmp.path().join(f).is_file(),
            "expected file {f} after init --template abstract-project"
        );
    }
}

#[test]
fn init_then_validate_succeeds() {
    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path().to_str().unwrap();
    let registry_path = tmp.path().join(".claude/brain-registry.json");
    run_init(&[
        "--project-root",
        project_root,
        "--output",
        registry_path.to_str().unwrap(),
        "--template",
        "abstract-project",
        "--name",
        "validate-test",
        "--yes",
    ]);

    let validate = Command::new(neurogrim_bin())
        .args(["validate", "--registry", registry_path.to_str().unwrap()])
        .output()
        .expect("validate spawn");
    assert!(
        validate.status.success(),
        "validate failed: stderr={}",
        String::from_utf8_lossy(&validate.stderr)
    );
    let stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(
        stdout.contains("Result: VALID"),
        "validate output should contain 'Result: VALID'; got:\n{stdout}"
    );
}

#[test]
fn replicate_job_hunt_smoke() {
    // Replicate the job-hunt B'1 Pilot #1 onboarding (commits cc74f3d /
    // 362ed3c) via `neurogrim init --template abstract-project` with
    // the same 5 abstract domains. Should produce equivalent structure.
    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path().to_str().unwrap();
    let registry_path = tmp.path().join(".claude/brain-registry.json");
    let (ok, _, stderr) = run_init(&[
        "--project-root",
        project_root,
        "--output",
        registry_path.to_str().unwrap(),
        "--template",
        "abstract-project",
        "--name",
        "job-hunt-replica",
        "--domains",
        "resume-coherence,application-pipeline-health,interview-prep-coverage,target-role-clarity,network-cadence",
        "--yes",
    ]);
    assert!(ok, "replicate-job-hunt init failed: {stderr}");

    // 8 stub CMDBs: 5 abstract + 3 generic.
    for domain in [
        "resume-coherence",
        "application-pipeline-health",
        "interview-prep-coverage",
        "target-role-clarity",
        "network-cadence",
        "human-comms",
        "secret-refs",
        "git-health",
    ] {
        let path = tmp.path().join(format!(".claude/{domain}-cmdb.json"));
        assert!(path.is_file(), "missing CMDB for {domain}");
    }

    // CLAUDE.md mentions the project name and all 8 domains.
    let claude_md = std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
    assert!(claude_md.contains("job-hunt-replica"));
    for d in [
        "resume-coherence",
        "application-pipeline-health",
        "interview-prep-coverage",
    ] {
        assert!(
            claude_md.contains(d),
            "CLAUDE.md should mention domain {d}"
        );
    }

    // .gitignore extended with marker.
    let gitignore = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains("LSP Brains runtime artifacts"));

    // Subsequent validate succeeds (registry is well-formed).
    let validate = Command::new(neurogrim_bin())
        .args(["validate", "--registry", registry_path.to_str().unwrap()])
        .output()
        .expect("validate spawn");
    assert!(
        validate.status.success(),
        "post-init validate failed: {}",
        String::from_utf8_lossy(&validate.stderr)
    );
}

#[test]
fn skill_new_then_init_workflow() {
    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path().to_str().unwrap();
    let registry_path = tmp.path().join(".claude/brain-registry.json");

    // 1. Init the project.
    run_init(&[
        "--project-root",
        project_root,
        "--output",
        registry_path.to_str().unwrap(),
        "--template",
        "abstract-project",
        "--name",
        "skill-workflow-test",
        "--yes",
    ]);

    // 2. Run `neurogrim skill new test-protocol` from the project root.
    let skill_new = Command::new(neurogrim_bin())
        .args([
            "skill",
            "new",
            "test-protocol",
            "--directory",
            project_root,
        ])
        .output()
        .expect("skill new spawn");
    assert!(
        skill_new.status.success(),
        "skill new failed: {}",
        String::from_utf8_lossy(&skill_new.stderr)
    );

    // 3. Verify the skeleton was written.
    let skill_md_path = tmp.path().join(".claude/skills/test-protocol/SKILL.md");
    assert!(skill_md_path.is_file());
    let content = std::fs::read_to_string(&skill_md_path).unwrap();
    assert!(content.contains("name: test-protocol"));
    assert!(content.contains("# Skill: Test Protocol"));
    assert!(content.contains("description: TODO"));
}

#[test]
fn federation_register_workflow() {
    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path().to_str().unwrap();
    let registry_path = tmp.path().join(".claude/brain-registry.json");

    // 1. Init an ecosystem-style project.
    run_init(&[
        "--project-root",
        project_root,
        "--output",
        registry_path.to_str().unwrap(),
        "--template",
        "abstract-project",
        "--name",
        "fed-test",
        "--yes",
    ]);

    // 2. Register two children: one read-only, one not.
    let r1 = Command::new(neurogrim_bin())
        .args([
            "federation",
            "register",
            "--name",
            "child-readonly",
            "--path",
            "../ro",
            "--read-only",
            "--registry",
            registry_path.to_str().unwrap(),
        ])
        .output()
        .expect("federation register #1 spawn");
    assert!(r1.status.success(), "federation register #1 failed");

    let r2 = Command::new(neurogrim_bin())
        .args([
            "federation",
            "register",
            "--name",
            "child-normal",
            "--path",
            "../normal",
            "--registry",
            registry_path.to_str().unwrap(),
        ])
        .output()
        .expect("federation register #2 spawn");
    assert!(r2.status.success(), "federation register #2 failed");

    // 3. Verify both children are present with correct shapes + auto-allocated ports.
    let registry: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&registry_path).unwrap()).unwrap();
    let children = registry["config"]["children"].as_object().unwrap();
    assert_eq!(children.len(), 2);
    let ro = &children["child-readonly"];
    assert_eq!(ro["read_only"], true);
    assert_eq!(ro["weight"], 0.0);
    let normal = &children["child-normal"];
    assert!(normal.get("read_only").is_none());
    assert_eq!(normal["weight"], 1.0);

    // 4. Ports auto-allocated: child-readonly got 8421, child-normal got 8422.
    assert!(ro["a2a_endpoint"]
        .as_str()
        .unwrap()
        .contains("localhost:8421"));
    assert!(normal["a2a_endpoint"]
        .as_str()
        .unwrap()
        .contains("localhost:8422"));
}

#[test]
fn unknown_template_rejected_at_clap_parse() {
    let tmp = TempDir::new().unwrap();
    let registry_path = tmp.path().join(".claude/brain-registry.json");
    let (ok, _, stderr) = run_init(&[
        "--project-root",
        tmp.path().to_str().unwrap(),
        "--output",
        registry_path.to_str().unwrap(),
        "--template",
        "nonexistent-template",
        "--yes",
    ]);
    assert!(!ok, "init should fail for unknown template");
    assert!(
        stderr.contains("invalid value") || stderr.contains("nonexistent-template"),
        "stderr should mention the bad value; got:\n{stderr}"
    );
}
