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

use std::path::Path;

use serde_json::Value;
use tempfile::TempDir;

mod test_support;
use test_support::load_schema;

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
        neurogrim_sensory::code_quality::analyze_code_quality(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("code_quality", &env);
}

#[tokio::test]
async fn test_results_runs_on_empty_project() {
    let tmp = TempDir::new().unwrap();
    let env =
        neurogrim_sensory::test_results::analyze_test_health(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("test_results", &env);
}

#[tokio::test]
async fn deploy_readiness_runs_on_empty_project() {
    let tmp = TempDir::new().unwrap();
    let env = neurogrim_sensory::deploy_readiness::analyze_deploy_readiness(
        tmp.path().to_str().unwrap(),
    )
    .await;
    assert_envelope_healthy("deploy_readiness", &env);
}

#[tokio::test]
async fn security_standards_runs_on_empty_project() {
    let tmp = TempDir::new().unwrap();
    let env = neurogrim_sensory::security_standards::analyze_security_standards(
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
    let env = neurogrim_sensory::coherence::analyze_coherence(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("coherence", &env);
}

/// V5-MOD-2 Phase 4.5 sentinel: registry declares correlations but
/// zero peer CMDBs are reachable → degraded envelope with `score: 0`
/// + a `coherence:no_peer_cmdbs_found` finding. Without the sentinel,
/// the false-positive-green path returns score 100 because every
/// correlation's `evaluate_condition` falls through with "no
/// variables" (returns false → no fires → no deductions → score 100).
#[tokio::test]
async fn coherence_sentinel_fires_on_zero_peer_cmdbs() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    // Registry with one correlation that references two domains.
    // Neither domain has a CMDB file → sentinel must fire.
    std::fs::write(
        claude_dir.join("brain-registry.json"),
        r#"{
          "config": {
            "correlations": [
              {
                "name": "test-correlation",
                "domains": ["domain-a", "domain-b"],
                "condition": "domain-a:flag == true && domain-b:flag == true",
                "severity": "warning",
                "description": "fixture"
              }
            ],
            "domain_definitions": {
              "domain-a": {
                "weight": 1.0,
                "scoring_source": { "type": "cmdb", "path": ".claude/domain-a-cmdb.json" }
              },
              "domain-b": {
                "weight": 1.0,
                "scoring_source": { "type": "cmdb", "path": ".claude/domain-b-cmdb.json" }
              }
            }
          }
        }"#,
    )
    .unwrap();

    let env = neurogrim_sensory::coherence::analyze_coherence(
        tmp.path().to_str().unwrap()
    ).await;

    // Score must be 0 (degraded envelope, not 100).
    let score = env.get("score").and_then(Value::as_u64).unwrap();
    assert_eq!(
        score, 0,
        "sentinel must produce score 0 when correlations declared \
         but zero peer CMDBs reachable; got {score}"
    );

    // Finding must include the sentinel name.
    let findings = env.get("findings").and_then(Value::as_array).unwrap();
    let has_sentinel = findings.iter().any(|f| {
        f.get("name").and_then(Value::as_str)
            == Some("coherence:no_peer_cmdbs_found")
    });
    assert!(
        has_sentinel,
        "sentinel finding 'coherence:no_peer_cmdbs_found' must be \
         present; findings = {findings:?}"
    );

    // Envelope still validates against cmdb-envelope-v1.
    assert_envelope_healthy("coherence_sentinel", &env);
}

#[tokio::test]
async fn human_comms_runs_with_empty_manifest() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("human-comms.yaml"), "").unwrap();
    let env =
        neurogrim_sensory::human_comms::analyze_human_comms(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("human_comms", &env);
}

#[tokio::test]
async fn secret_refs_runs_with_empty_manifest() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("secret-refs.yaml"), "").unwrap();
    let env =
        neurogrim_sensory::secret_refs::analyze_secret_refs(tmp.path().to_str().unwrap()).await;
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
    match neurogrim_sensory::git_health::analyze_git_health(tmp.path().to_str().unwrap()).await {
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
        neurogrim_sensory::git_health::analyze_git_health(tmp.path().to_str().unwrap()).await;
    // Either an Ok envelope or a typed anyhow error — NOT a panic. If the
    // sensor's contract is "error on missing .git", that's legitimate; we
    // just assert it didn't abort the process.
    match result {
        Ok(env) => assert_envelope_healthy("git_health (no .git)", &env),
        Err(e) => eprintln!("git_health returned typed error on missing .git (acceptable): {e}"),
    }
}

// =============================================================================
// Tier B — state-specific behavioral assertions.
//
// Each pair of tests below sets up two fixture states (one "worse", one
// "better") and asserts the relative ordering of their scores. We avoid
// absolute-score assertions because implementation details (exact point
// deductions) should be free to change — behavioral guarantees are what
// the sensor actually promises: "dirty repo scores lower than clean
// repo", not "dirty repo scores exactly 82".
//
// This file covers three sensors as the pattern-setter: git_health,
// code_quality, deploy_readiness. Remaining five sensors (test_results,
// security_standards, coherence, human_comms, secret_refs) are pattern-
// replication follow-on work.
// =============================================================================

fn git_init(dir: &std::path::Path) {
    let status = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(dir)
        .status()
        .expect("git must be on PATH for Tier-B git_health fixtures");
    assert!(status.success(), "git init failed in {}", dir.display());
    // Minimum identity so `git commit` works in CI environments that
    // don't have a global user.name / user.email configured.
    for (k, v) in [
        ("user.email", "test@example.com"),
        ("user.name", "Tier-B Fixture"),
    ] {
        let s = std::process::Command::new("git")
            .args(["config", k, v])
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(s.success(), "git config {k} failed");
    }
}

fn git_commit_all(dir: &std::path::Path, message: &str) {
    let s = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(s.success(), "git add failed");
    let s = std::process::Command::new("git")
        .args(["commit", "-q", "-m", message])
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(s.success(), "git commit failed");
}

#[tokio::test]
async fn git_health_dirty_repo_scores_below_clean() {
    // Dirty fixture: git init, commit a file, then MODIFY the tracked
    // file so git reports it as dirty (modified tracked file, not
    // untracked — git_dirty_count filters `??` untracked entries out).
    // The untracked-file penalty doesn't kick in until > 5 files, so
    // modifying a tracked file is what actually moves the score.
    let dirty = TempDir::new().unwrap();
    git_init(dirty.path());
    std::fs::write(dirty.path().join("seed.txt"), "seed").unwrap();
    git_commit_all(dirty.path(), "seed");
    // Modify the already-committed file — this produces a "dirty" entry
    // (` M seed.txt`) that git_dirty_count counts.
    std::fs::write(dirty.path().join("seed.txt"), "seed modified").unwrap();

    // Clean fixture: git init + one committed file, nothing modified.
    let clean = TempDir::new().unwrap();
    git_init(clean.path());
    std::fs::write(clean.path().join("README.md"), "hello").unwrap();
    git_commit_all(clean.path(), "initial");

    let env_dirty =
        neurogrim_sensory::git_health::analyze_git_health(dirty.path().to_str().unwrap())
            .await
            .expect("git_health on dirty repo");
    let env_clean =
        neurogrim_sensory::git_health::analyze_git_health(clean.path().to_str().unwrap())
            .await
            .expect("git_health on clean repo");

    let score_dirty = env_dirty["score"].as_u64().unwrap();
    let score_clean = env_clean["score"].as_u64().unwrap();
    assert!(
        score_dirty < score_clean,
        "expected dirty < clean; got dirty={score_dirty} clean={score_clean}"
    );
}

#[tokio::test]
async fn code_quality_rich_project_scores_above_bare() {
    // Bare: empty tempdir. Sensor has no files to credit.
    let bare = TempDir::new().unwrap();

    // Rich: README + .gitignore + LICENSE + .editorconfig — each worth
    // points in the code_quality scorecard. Deliberately include the
    // full set so the score is comfortably higher than the bare
    // baseline without being flaky around a single file.
    let rich = TempDir::new().unwrap();
    for (name, content) in [
        ("README.md", "# Fixture"),
        (".gitignore", "target/"),
        ("LICENSE", "MIT"),
        (".editorconfig", "root = true"),
        ("rustfmt.toml", ""),
    ] {
        std::fs::write(rich.path().join(name), content).unwrap();
    }

    let env_bare =
        neurogrim_sensory::code_quality::analyze_code_quality(bare.path().to_str().unwrap())
            .await;
    let env_rich =
        neurogrim_sensory::code_quality::analyze_code_quality(rich.path().to_str().unwrap())
            .await;

    let score_bare = env_bare["score"].as_u64().unwrap();
    let score_rich = env_rich["score"].as_u64().unwrap();
    assert!(
        score_rich > score_bare,
        "expected rich > bare; got rich={score_rich} bare={score_bare}"
    );
}

#[tokio::test]
async fn deploy_readiness_with_dockerfile_and_ci_scores_higher() {
    // Bare: empty tempdir. deploy_readiness starts from 0 and adds for
    // every present indicator.
    let bare = TempDir::new().unwrap();

    // Readier: has Dockerfile + a CI workflow file + .gitignore + .git.
    // Each contributes points in the deploy_readiness scorecard.
    let readier = TempDir::new().unwrap();
    std::fs::write(readier.path().join("Dockerfile"), "FROM scratch\n").unwrap();
    std::fs::create_dir_all(readier.path().join(".github/workflows")).unwrap();
    std::fs::write(
        readier.path().join(".github/workflows/ci.yml"),
        "name: ci\non: push\njobs: {}\n",
    )
    .unwrap();
    std::fs::write(readier.path().join(".gitignore"), "target/\n").unwrap();
    std::fs::create_dir_all(readier.path().join(".git")).unwrap();

    let env_bare = neurogrim_sensory::deploy_readiness::analyze_deploy_readiness(
        bare.path().to_str().unwrap(),
    )
    .await;
    let env_readier = neurogrim_sensory::deploy_readiness::analyze_deploy_readiness(
        readier.path().to_str().unwrap(),
    )
    .await;

    let score_bare = env_bare["score"].as_u64().unwrap();
    let score_readier = env_readier["score"].as_u64().unwrap();
    assert!(
        score_readier > score_bare,
        "expected readier > bare; got readier={score_readier} bare={score_bare}"
    );
    // Bare project with nothing deployable should score 0 — the base.
    assert_eq!(
        score_bare, 0,
        "empty tempdir should score 0 on deploy_readiness"
    );
}

// ═════════════════════════════════════════════════════════════════════════
// supply_chain_sca (E-SC-2 Step 10) — full-pipeline integration tests
// ═════════════════════════════════════════════════════════════════════════
//
// These tests exercise the end-to-end sensor: lockfile parse → OSV
// query → RustSec-local cross-reference → accepted-advisory filter →
// scoring → CMDB envelope.
//
// Offline robustness: every test uses fabricated package names that OSV
// will return empty vulns for (so no real advisory flakes the test),
// AND pre-seeds the OSV cache via the `_testing_seed_cache` helper so
// the sensor doesn't need network to satisfy its OSV calls. Advisory
// flow is fully driven by a synthetic `vendor/rustsec-advisory-db/`
// under each test's TempDir.

use neurogrim_sensory::supply_chain_sca::{
    analyze_supply_chain_sca,
    osv::{_testing_seed_cache, ECOSYSTEM_CRATES_IO},
    Package,
};

/// Write a Cargo.lock at `dir` containing the given crates.io-sourced
/// packages.
fn write_scs_lockfile(dir: &Path, pkgs: &[(&str, &str)]) {
    let mut body = String::from("# This file is automatically @generated by Cargo.\nversion = 3\n\n");
    for (i, (name, version)) in pkgs.iter().enumerate() {
        // Deterministic hex checksum (64 chars) — value doesn't matter;
        // cargo-lock just needs it to be well-formed hex.
        let hex = format!("{:0<64}", i);
        body.push_str(&format!(
            "[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n\
             source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\
             checksum = \"{hex}\"\n\n"
        ));
    }
    std::fs::write(dir.join("Cargo.lock"), body).unwrap();
}

/// Write a RustSec advisory file under
/// `<project_root>/vendor/rustsec-advisory-db/crates/<package>/<filename>`.
/// `toml_body` is the TOML frontmatter without the fence markers.
fn write_scs_rustsec_advisory(
    project_root: &Path,
    package: &str,
    filename: &str,
    toml_body: &str,
) {
    let dir = project_root
        .join("vendor")
        .join("rustsec-advisory-db")
        .join("crates")
        .join(package);
    std::fs::create_dir_all(&dir).unwrap();
    let md = format!(
        "```toml\n{toml_body}\n```\n\n# {package} advisory (test fixture)\n"
    );
    std::fs::write(dir.join(filename), md).unwrap();
}

/// Pre-populate the OSV cache with an empty-vulns entry for each
/// package — so the sensor's OSV call short-circuits (all cache hits)
/// and tests never depend on api.osv.dev reachability.
fn seed_empty_osv_cache(project_root: &Path, pkgs: &[(&str, &str)]) {
    let cache_dir = project_root
        .join(".claude")
        .join("brain")
        .join("cache")
        .join("osv");
    std::fs::create_dir_all(&cache_dir).unwrap();
    for (name, version) in pkgs {
        let pkg = Package::crates_io(*name, *version);
        _testing_seed_cache(&cache_dir, ECOSYSTEM_CRATES_IO, &pkg, &[])
            .expect("seed cache");
    }
}

#[tokio::test]
async fn supply_chain_sca_empty_lockfile_scores_100() {
    // Zero packages → no OSV work → 0 findings → 100.
    let tmp = TempDir::new().unwrap();
    write_scs_lockfile(tmp.path(), &[]);

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:empty", &env);

    assert_eq!(env["score"].as_u64().unwrap(), 100);
    assert_eq!(env["total_packages_scanned"].as_u64().unwrap(), 0);
    assert_eq!(env["advisories_found"].as_u64().unwrap(), 0);
    let findings = env["findings"].as_array().unwrap();
    assert!(findings.is_empty(), "expected no findings, got {findings:?}");
}

#[tokio::test]
async fn supply_chain_sca_flags_rustsec_local_advisory() {
    // Fabricated package name → OSV returns empty vulns (cache-seeded
    // to short-circuit network). RustSec-local supplies the advisory.
    let tmp = TempDir::new().unwrap();
    let pkgs = [("neurogrim-test-fixture-xyz", "0.1.0")];
    write_scs_lockfile(tmp.path(), &pkgs);
    seed_empty_osv_cache(tmp.path(), &pkgs);

    write_scs_rustsec_advisory(
        tmp.path(),
        "neurogrim-test-fixture-xyz",
        "RUSTSEC-NG-TEST-0001.md",
        r#"[advisory]
id = "RUSTSEC-NG-TEST-0001"
package = "neurogrim-test-fixture-xyz"
informational = "unmaintained"

[versions]
patched = []"#,
    );

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:advisory", &env);

    assert_eq!(env["score"].as_u64().unwrap(), 75, "1 unaccepted advisory → rubric 75");
    assert_eq!(env["advisories_found"].as_u64().unwrap(), 1);
    assert_eq!(env["advisories_unaccepted"].as_u64().unwrap(), 1);
    assert_eq!(env["advisories_accepted"].as_u64().unwrap(), 0);

    let findings = env["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["name"].as_str().unwrap(), "RUSTSEC-NG-TEST-0001");
    assert_eq!(findings[0]["status"].as_str().unwrap(), "advisory");
    assert_eq!(findings[0]["points"].as_i64().unwrap(), -25);
    let detail = findings[0]["detail"].as_str().unwrap();
    assert!(detail.contains("neurogrim-test-fixture-xyz"));
    assert!(detail.contains("unmaintained"));
}

#[tokio::test]
async fn supply_chain_sca_respects_accepted_advisory() {
    let tmp = TempDir::new().unwrap();
    let pkgs = [("neurogrim-test-fixture-xyz", "0.1.0")];
    write_scs_lockfile(tmp.path(), &pkgs);
    seed_empty_osv_cache(tmp.path(), &pkgs);

    write_scs_rustsec_advisory(
        tmp.path(),
        "neurogrim-test-fixture-xyz",
        "RUSTSEC-NG-TEST-0001.md",
        r#"[advisory]
id = "RUSTSEC-NG-TEST-0001"
package = "neurogrim-test-fixture-xyz"
informational = "unmaintained"

[versions]
patched = []"#,
    );

    // Accept the advisory with an operator note.
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(
        claude_dir.join("supply-chain-accepted-advisories.toml"),
        r#"
[[accepted]]
id = "RUSTSEC-NG-TEST-0001"
note = "Test fixture acceptance — integration test only."
"#,
    )
    .unwrap();

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:accepted", &env);

    assert_eq!(env["score"].as_u64().unwrap(), 100, "accepted advisory must not deduct");
    assert_eq!(env["advisories_found"].as_u64().unwrap(), 1);
    assert_eq!(env["advisories_accepted"].as_u64().unwrap(), 1);
    assert_eq!(env["advisories_unaccepted"].as_u64().unwrap(), 0);

    let findings = env["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["status"].as_str().unwrap(), "accepted");
    assert_eq!(findings[0]["points"].as_i64().unwrap(), 0);
    let detail = findings[0]["detail"].as_str().unwrap();
    assert!(
        detail.contains("accepted by operator"),
        "detail should mark acceptance; got: {detail}"
    );
}

#[tokio::test]
async fn supply_chain_sca_hygiene_rejects_unannotated_acceptance() {
    // An acceptance entry without a non-empty `note` must NOT count.
    // The advisory continues to deduct from the score.
    let tmp = TempDir::new().unwrap();
    let pkgs = [("neurogrim-test-fixture-xyz", "0.1.0")];
    write_scs_lockfile(tmp.path(), &pkgs);
    seed_empty_osv_cache(tmp.path(), &pkgs);

    write_scs_rustsec_advisory(
        tmp.path(),
        "neurogrim-test-fixture-xyz",
        "RUSTSEC-NG-TEST-0001.md",
        r#"[advisory]
id = "RUSTSEC-NG-TEST-0001"
package = "neurogrim-test-fixture-xyz"
informational = "unmaintained"

[versions]
patched = []"#,
    );

    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    // Acceptance entry with NO note — hygiene filter should reject.
    std::fs::write(
        claude_dir.join("supply-chain-accepted-advisories.toml"),
        r#"
[[accepted]]
id = "RUSTSEC-NG-TEST-0001"
"#,
    )
    .unwrap();

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:hygiene", &env);

    // Score should still be 75 — acceptance without rationale gets
    // filtered, advisory still deducts.
    assert_eq!(env["score"].as_u64().unwrap(), 75);
    assert_eq!(env["advisories_unaccepted"].as_u64().unwrap(), 1);
    assert_eq!(env["advisories_accepted"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn supply_chain_sca_four_unaccepted_clamps_to_zero() {
    // 4+ unaccepted advisories → score 0.
    let tmp = TempDir::new().unwrap();
    let pkgs = [
        ("ng-test-a", "0.1.0"),
        ("ng-test-b", "0.1.0"),
        ("ng-test-c", "0.1.0"),
        ("ng-test-d", "0.1.0"),
    ];
    write_scs_lockfile(tmp.path(), &pkgs);
    seed_empty_osv_cache(tmp.path(), &pkgs);
    for (name, _) in &pkgs {
        write_scs_rustsec_advisory(
            tmp.path(),
            name,
            "RUSTSEC-NG-TEST-9999.md",
            &format!(
                r#"[advisory]
id = "RUSTSEC-NG-TEST-{name}"
package = "{name}"
informational = "unmaintained"

[versions]
patched = []"#
            ),
        );
    }

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:four", &env);

    assert_eq!(env["score"].as_u64().unwrap(), 0, "4 unaccepted → 0");
    assert_eq!(env["advisories_unaccepted"].as_u64().unwrap(), 4);
}

#[tokio::test]
async fn supply_chain_sca_missing_lockfile_yields_error_finding() {
    // Cargo.lock absent → sensor emits a well-formed CMDB with
    // sensor_status=lockfile_unreadable + score 0. Must NOT panic.
    let tmp = TempDir::new().unwrap();
    // Deliberately do not write Cargo.lock.

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:no_lockfile", &env);

    assert_eq!(env["score"].as_u64().unwrap(), 0);
    assert_eq!(
        env["sensor_status"].as_str().unwrap(),
        "lockfile_unreadable"
    );
    let findings = env["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["name"].as_str().unwrap(), "lockfile_read_failed");
    assert_eq!(findings[0]["status"].as_str().unwrap(), "error");
}

#[tokio::test]
async fn supply_chain_sca_patched_version_is_not_affected() {
    // Real-world shape: RUSTSEC-2026-0104 on rustls-webpki patches at
    // 0.103.13. A package at 0.103.13 should NOT be flagged.
    let tmp = TempDir::new().unwrap();
    let pkgs = [("neurogrim-test-patched", "0.103.13")];
    write_scs_lockfile(tmp.path(), &pkgs);
    seed_empty_osv_cache(tmp.path(), &pkgs);

    write_scs_rustsec_advisory(
        tmp.path(),
        "neurogrim-test-patched",
        "RUSTSEC-NG-TEST-0002.md",
        r#"[advisory]
id = "RUSTSEC-NG-TEST-0002"
package = "neurogrim-test-patched"
date = "2026-04-22"
categories = ["denial-of-service"]

[versions]
patched = [">= 0.103.13, < 0.104.0-alpha.1"]"#,
    );

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:patched", &env);

    assert_eq!(env["score"].as_u64().unwrap(), 100, "patched version not affected");
    assert_eq!(env["advisories_found"].as_u64().unwrap(), 0);
}

// =========================================================================
// supply_chain_sca (E-SC-3) — multi-ecosystem integration tests
// =========================================================================

/// Write a uv.lock at `dir/uv.lock` listing PyPI-sourced packages.
fn write_scs_uv_lock(dir: &Path, pkgs: &[(&str, &str)]) {
    let mut body = String::from("version = 1\n\n");
    for (name, version) in pkgs {
        body.push_str(&format!(
            "[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n\
             source = {{ registry = \"https://pypi.org/simple\" }}\n\n"
        ));
    }
    std::fs::write(dir.join("uv.lock"), body).unwrap();
}

/// Write a `requirements-lock.txt` with `==X.Y.Z` pins.
fn write_scs_requirements_txt(dir: &Path, name: &str, pkgs: &[(&str, &str)]) {
    let body: String = pkgs
        .iter()
        .map(|(n, v)| format!("{n}=={v}\n"))
        .collect();
    std::fs::write(dir.join(name), body).unwrap();
}

/// Pre-seed empty OSV cache entries for PyPI packages.
fn seed_empty_pypi_cache(project_root: &Path, pkgs: &[(&str, &str)]) {
    let cache_dir = project_root
        .join(".claude")
        .join("brain")
        .join("cache")
        .join("osv");
    std::fs::create_dir_all(&cache_dir).unwrap();
    for (name, version) in pkgs {
        let pkg = Package::pypi(*name, *version);
        _testing_seed_cache(&cache_dir, "PyPI", &pkg, &[])
            .expect("seed pypi cache");
    }
}

#[tokio::test]
async fn supply_chain_sca_pypi_uv_lock_clean() {
    let tmp = TempDir::new().unwrap();
    let pkgs = [("anyio", "4.6.0"), ("httpx", "0.27.0")];
    write_scs_uv_lock(tmp.path(), &pkgs);
    seed_empty_pypi_cache(tmp.path(), &pkgs);

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:pypi_uv_clean", &env);
    assert_eq!(env["score"].as_u64().unwrap(), 100);
    assert_eq!(env["total_packages_scanned"].as_u64().unwrap(), 2);
    let ecosystems = env["ecosystems_scanned"].as_array().unwrap();
    let names: Vec<&str> = ecosystems.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(names, vec!["PyPI"]);
    assert_eq!(env["packages_by_ecosystem"]["PyPI"].as_u64().unwrap(), 2);
}

#[tokio::test]
async fn supply_chain_sca_pypi_requirements_txt_clean() {
    let tmp = TempDir::new().unwrap();
    let pkgs = [("requests", "2.32.3"), ("urllib3", "2.2.3")];
    write_scs_requirements_txt(tmp.path(), "requirements-lock.txt", &pkgs);
    seed_empty_pypi_cache(tmp.path(), &pkgs);

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:pypi_req_clean", &env);
    assert_eq!(env["score"].as_u64().unwrap(), 100);
    assert_eq!(env["packages_by_ecosystem"]["PyPI"].as_u64().unwrap(), 2);
}

#[tokio::test]
async fn supply_chain_sca_mixed_rust_python() {
    // Both Cargo.lock and uv.lock at the same root → both ecosystems
    // appear in packages_by_ecosystem.
    let tmp = TempDir::new().unwrap();
    let rust_pkgs = [("ng-test-rust", "0.1.0")];
    let py_pkgs = [("ng-test-py", "1.0.0")];
    write_scs_lockfile(tmp.path(), &rust_pkgs);
    seed_empty_osv_cache(tmp.path(), &rust_pkgs);
    write_scs_uv_lock(tmp.path(), &py_pkgs);
    seed_empty_pypi_cache(tmp.path(), &py_pkgs);

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:mixed", &env);
    assert_eq!(env["score"].as_u64().unwrap(), 100);
    let ecos = env["ecosystems_scanned"].as_array().unwrap();
    let mut names: Vec<&str> = ecos.iter().map(|v| v.as_str().unwrap()).collect();
    names.sort();
    assert_eq!(names, vec!["PyPI", "crates.io"]);
    assert_eq!(env["packages_by_ecosystem"]["crates.io"].as_u64().unwrap(), 1);
    assert_eq!(env["packages_by_ecosystem"]["PyPI"].as_u64().unwrap(), 1);
    assert_eq!(env["total_packages_scanned"].as_u64().unwrap(), 2);
}

// =========================================================================
// supply_chain_sca (E-SC-4) — npm/yarn/pnpm integration tests
// =========================================================================

/// Pre-seed empty OSV cache entries for npm packages.
fn seed_empty_npm_cache(project_root: &Path, pkgs: &[(&str, &str)]) {
    let cache_dir = project_root
        .join(".claude")
        .join("brain")
        .join("cache")
        .join("osv");
    std::fs::create_dir_all(&cache_dir).unwrap();
    for (name, version) in pkgs {
        let pkg = Package::npm(*name, *version);
        _testing_seed_cache(&cache_dir, "npm", &pkg, &[])
            .expect("seed npm cache");
    }
}

#[tokio::test]
async fn supply_chain_sca_npm_package_lock_v3() {
    let tmp = TempDir::new().unwrap();
    let pkgs = [("ng-test-npm-pkg", "1.0.0")];
    seed_empty_npm_cache(tmp.path(), &pkgs);

    std::fs::write(
        tmp.path().join("package-lock.json"),
        r#"{
            "name": "fixture",
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "fixture", "version": "1.0.0"},
                "node_modules/ng-test-npm-pkg": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/ng-test-npm-pkg/-/ng-test-npm-pkg-1.0.0.tgz"
                }
            }
        }"#,
    )
    .unwrap();

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:npm", &env);
    assert_eq!(env["score"].as_u64().unwrap(), 100);
    assert_eq!(env["packages_by_ecosystem"]["npm"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn supply_chain_sca_yarn_lock() {
    let tmp = TempDir::new().unwrap();
    let pkgs = [("ng-test-yarn-pkg", "2.1.0")];
    seed_empty_npm_cache(tmp.path(), &pkgs);

    std::fs::write(
        tmp.path().join("yarn.lock"),
        r#"# THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.
# yarn lockfile v1


ng-test-yarn-pkg@^2.0.0:
  version "2.1.0"
  resolved "https://registry.npmjs.org/ng-test-yarn-pkg/-/ng-test-yarn-pkg-2.1.0.tgz#abc"
  integrity sha512-XXXXX==
"#,
    )
    .unwrap();

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:yarn", &env);
    assert_eq!(env["score"].as_u64().unwrap(), 100);
    assert_eq!(env["packages_by_ecosystem"]["npm"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn supply_chain_sca_pnpm_lock() {
    let tmp = TempDir::new().unwrap();
    let pkgs = [("ng-test-pnpm-pkg", "3.0.0")];
    seed_empty_npm_cache(tmp.path(), &pkgs);

    std::fs::write(
        tmp.path().join("pnpm-lock.yaml"),
        r#"lockfileVersion: '9.0'

packages:
  ng-test-pnpm-pkg@3.0.0:
    resolution: {integrity: sha512-aaa}
"#,
    )
    .unwrap();

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:pnpm", &env);
    assert_eq!(env["score"].as_u64().unwrap(), 100);
    assert_eq!(env["packages_by_ecosystem"]["npm"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn supply_chain_sca_full_three_ecosystem_dogfood() {
    // Cargo + uv + npm — the maximal multi-ecosystem case the
    // sensor is designed for (Tauri-style apps with both Rust
    // and JS frontends + Python tooling).
    let tmp = TempDir::new().unwrap();
    let rust_pkgs = [("ng-rust-fixture", "0.1.0")];
    let py_pkgs = [("ng-py-fixture", "1.0.0")];
    let npm_pkgs = [("ng-npm-fixture", "2.0.0")];
    write_scs_lockfile(tmp.path(), &rust_pkgs);
    seed_empty_osv_cache(tmp.path(), &rust_pkgs);
    write_scs_uv_lock(tmp.path(), &py_pkgs);
    seed_empty_pypi_cache(tmp.path(), &py_pkgs);
    seed_empty_npm_cache(tmp.path(), &npm_pkgs);
    std::fs::write(
        tmp.path().join("package-lock.json"),
        r#"{
            "name": "x",
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "x", "version": "1.0.0"},
                "node_modules/ng-npm-fixture": {
                    "version": "2.0.0",
                    "resolved": "https://registry.npmjs.org/ng-npm-fixture/-/ng-npm-fixture-2.0.0.tgz"
                }
            }
        }"#,
    )
    .unwrap();

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:three_ecosystem", &env);
    assert_eq!(env["score"].as_u64().unwrap(), 100);
    let mut ecos: Vec<&str> = env["ecosystems_scanned"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    ecos.sort();
    assert_eq!(ecos, vec!["PyPI", "crates.io", "npm"]);
    assert_eq!(env["packages_by_ecosystem"]["crates.io"].as_u64().unwrap(), 1);
    assert_eq!(env["packages_by_ecosystem"]["PyPI"].as_u64().unwrap(), 1);
    assert_eq!(env["packages_by_ecosystem"]["npm"].as_u64().unwrap(), 1);
    assert_eq!(env["total_packages_scanned"].as_u64().unwrap(), 3);
}

#[tokio::test]
async fn supply_chain_sca_npm_skips_git_resolution() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("package-lock.json"),
        r#"{
            "name": "x",
            "lockfileVersion": 3,
            "packages": {
                "": {"name": "x", "version": "1.0.0"},
                "node_modules/from-git": {
                    "version": "1.0.0",
                    "resolved": "git+https://github.com/x/y#abc"
                }
            }
        }"#,
    )
    .unwrap();

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:npm_git", &env);
    // git deps don't get queried; sensor still produces a valid
    // envelope with score 100 (no advisories, 0 packages scanned).
    assert_eq!(env["score"].as_u64().unwrap(), 100);
    assert_eq!(env["total_packages_scanned"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn supply_chain_sca_no_lockfile_found_emits_error_finding() {
    // E-SC-3 generalizes the missing-Cargo.lock error to any
    // lockfile-missing case. Empty TempDir → finding + score 0.
    let tmp = TempDir::new().unwrap();
    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:no_lockfile_v3", &env);
    assert_eq!(env["score"].as_u64().unwrap(), 0);
    assert_eq!(env["sensor_status"].as_str().unwrap(), "lockfile_unreadable");
    let findings = env["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["name"].as_str().unwrap(), "lockfile_read_failed");
    let detail = findings[0]["detail"].as_str().unwrap();
    assert!(
        detail.contains("Cargo.lock") && detail.contains("uv.lock"),
        "error message should list supported lockfile formats; got: {detail}"
    );
}

#[tokio::test]
async fn supply_chain_sca_withdrawn_advisory_is_ignored() {
    // RustSec `withdrawn` field → advisory treated as inactive.
    // Mirrors OSV's behavior (it filters withdrawn from batch).
    // Real-world shape: RUSTSEC-2025-0007 on ring was withdrawn
    // 2025-02-22 when briansmith resumed maintenance.
    let tmp = TempDir::new().unwrap();
    let pkgs = [("neurogrim-test-withdrawn", "0.17.14")];
    write_scs_lockfile(tmp.path(), &pkgs);
    seed_empty_osv_cache(tmp.path(), &pkgs);

    write_scs_rustsec_advisory(
        tmp.path(),
        "neurogrim-test-withdrawn",
        "RUSTSEC-NG-TEST-0003.md",
        r#"[advisory]
id = "RUSTSEC-NG-TEST-0003"
package = "neurogrim-test-withdrawn"
informational = "unmaintained"
withdrawn = "2025-02-22"

[versions]
patched = []"#,
    );

    let env = analyze_supply_chain_sca(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_sca:withdrawn", &env);

    assert_eq!(env["score"].as_u64().unwrap(), 100, "withdrawn advisory ignored");
    assert_eq!(env["advisories_found"].as_u64().unwrap(), 0);
}

// =========================================================================
// supply_chain_vigilance (E-SC-5) — Tier-A schema-conformance tests
//
// Added 2026-04-26 PRE-RELEASE B11+B16 fix. Spec §16.3 requires Layer 2
// findings MUST conform to cmdb-envelope-v1.schema.json; previously no test
// asserted the contract. These tests run on a minimal fixture with no
// lockfile so they don't hit the network.
// =========================================================================

#[tokio::test]
async fn supply_chain_vigilance_no_lockfile_yields_valid_envelope() {
    let tmp = TempDir::new().unwrap();
    let env = neurogrim_sensory::supply_chain_vigilance::analyze_supply_chain_vigilance(
        tmp.path().to_str().unwrap(),
    )
    .await;
    assert_envelope_healthy("supply_chain_vigilance:no_lockfile", &env);
}

#[tokio::test]
async fn supply_chain_vigilance_empty_cargo_lock_yields_valid_envelope() {
    let tmp = TempDir::new().unwrap();
    // Empty Cargo.lock — parses to zero deps; no registry calls fire.
    std::fs::write(
        tmp.path().join("Cargo.lock"),
        "version = 4\n",
    )
    .unwrap();
    let env = neurogrim_sensory::supply_chain_vigilance::analyze_supply_chain_vigilance(
        tmp.path().to_str().unwrap(),
    )
    .await;
    assert_envelope_healthy("supply_chain_vigilance:empty_lock", &env);
}

// =========================================================================
// supply_chain_review (E-SC-6) — Tier-A schema-conformance tests
//
// Added 2026-04-26 PRE-RELEASE B11+B16 fix. Spec §16.4 requires Layer 3
// CMDB output MUST conform to cmdb-envelope-v1.schema.json. Tests run on
// an empty .claude/ fixture (no tickets, no ledger) — no network, fast.
// =========================================================================

#[tokio::test]
async fn supply_chain_review_empty_yields_valid_envelope() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
    let env = neurogrim_sensory::supply_chain_review::analyze_supply_chain_review(
        tmp.path().to_str().unwrap(),
    )
    .await;
    assert_envelope_healthy("supply_chain_review:empty", &env);
    assert_eq!(
        env["score"].as_u64().unwrap(),
        100,
        "empty review state must score 100 (no open tickets)"
    );
}

#[tokio::test]
async fn supply_chain_review_with_one_open_ticket_validates() {
    use neurogrim_sensory::supply_chain_review::{cli_create, analyze_supply_chain_review};
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
    cli_create(
        tmp.path(),
        "PyPI",
        "litellm",
        Some("1.82.7"),
        "manual:operator-spotted",
        "high-base64-payload count noticed in upstream diff",
        "alice",
    )
    .unwrap();
    let env = analyze_supply_chain_review(tmp.path().to_str().unwrap()).await;
    assert_envelope_healthy("supply_chain_review:one_open", &env);
    // 100 - 1×10 = 90 per the v1 score model.
    assert_eq!(env["score"].as_u64().unwrap(), 90);
    assert_eq!(env["tickets_open"].as_u64().unwrap(), 1);
}
