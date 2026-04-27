//! E-B2-3 C5 — `score_hat_contracts` behavioral tests.
//!
//! These tests exercise the persona-hat contract validator end-to-end
//! through the public `analyze_capability_hygiene` entry point. The
//! validator itself is module-private (`score_hat_contracts` is `fn`,
//! not `pub fn`) — keeping it that way is intentional per the C5 brief:
//! "Do NOT leak the validator into the public API of `lib.rs`." We
//! observe its behavior via the CMDB output's `capability_breakdown`
//! and `findings` fields.
//!
//! Coverage (5 behavioral cases + 1 recursion-guard pin):
//!
//!  1. **`zero_hats_no_findings`** — empty hats dir → 0 findings.
//!  2. **`well_formed_hats_no_error_findings`** — well-formed fixture
//!     hats validate cleanly → 0 error/vocabulary findings.
//!  3. **`invalid_vocabulary_emits_finding`** — closed-set rejection →
//!     exactly one `hat_contract:vocabulary:<hat>:<term>` finding.
//!  4. **`missing_frontmatter_emits_declaration_finding`** — no fences
//!     → exactly one `hat_contract:declaration:<hat>` finding.
//!  5. **`skill_md_is_not_treated_as_hat`** — the catalog `SKILL.md`
//!     is excluded from per-hat scanning (otherwise its 287-line prose
//!     would generate spurious findings).
//!  6. **`recursion_guard_no_command_in_validator_span`** (Q6) — the
//!     source of `capability_hygiene.rs` contains no `Command` /
//!     shell-execution references inside the `score_hat_contracts`
//!     function. Pins the Q6 hard rule against future refactors.
//!
//! Aggregate integration (E-B2-3 C6, 2026-04-27): hat_contracts now
//! contributes to the top-level `total_capabilities` +
//! `overall_compliant_count` rollups. The C5 wiring already exposed the
//! type via `capability_breakdown.hat_contracts`; C6 folded it into the
//! denominators so per-Brain "how many capabilities exist / validate
//! cleanly" counts are honest. The `earned`/`possible` axis is untouched
//! — hat_contracts is advisory per Q3 and does NOT influence the
//! hygiene score numerator/denominator. Tests below pin the rollup
//! behaviour explicitly: #1 (zero hats → both rollups stay 0).

use serde_json::Value;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use neurogrim_sensory::capability_hygiene::analyze_capability_hygiene;

// ── Fixture helpers ─────────────────────────────────────────────────

/// Make a tempdir that looks like a Brain root, with `.claude/skills/hats/`
/// pre-created.
fn make_brain_with_hats_dir() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let hats = dir.path().join(".claude").join("skills").join("hats");
    std::fs::create_dir_all(&hats).expect("create hats dir");
    dir
}

/// Write a hat file at `<root>/.claude/skills/hats/<name>.md`.
fn write_hat(root: &Path, name: &str, body: &str) {
    let hats = root.join(".claude").join("skills").join("hats");
    std::fs::create_dir_all(&hats).expect("create hats dir");
    std::fs::write(hats.join(format!("{name}.md")), body).expect("write hat");
}

/// Filter findings under the `hat_contract:` finding-name prefix.
fn hat_contract_findings(result: &Value) -> Vec<&Value> {
    result["findings"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|f| {
                    f["name"]
                        .as_str()
                        .map(|s| s.starts_with("hat_contract:"))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Locate the `capability_hygiene.rs` source file from the crate's
/// manifest dir. Used by the recursion-guard test to read source at
/// test time.
fn locate_capability_hygiene_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("capability_hygiene.rs")
}

// ── 1. Zero hats → no findings ──────────────────────────────────────

#[tokio::test]
async fn zero_hats_no_findings() {
    let tmp = make_brain_with_hats_dir();

    let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;

    let findings = hat_contract_findings(&result);
    assert!(
        findings.is_empty(),
        "empty hats dir must produce zero hat_contract findings; got: {:?}",
        findings
    );

    // The breakdown is still emitted (visible-but-non-aggregate).
    let breakdown = &result["capability_breakdown"]["hat_contracts"];
    assert_eq!(
        breakdown["total"], 0,
        "expected hat_contracts.total = 0; got: {breakdown}"
    );
    assert_eq!(
        breakdown["compliant"], 0,
        "expected hat_contracts.compliant = 0; got: {breakdown}"
    );

    // C6 rollup pin: empty hats dir contributes 0 + 0 to the rollups.
    // No other capabilities exist in this fixture either, so the
    // top-level totals stay at 0. Confirms the C6 aggregation handles
    // the empty-hats path without spurious increments.
    assert_eq!(
        result["total_capabilities"], 0,
        "C6 rollup: empty hats dir must contribute 0 to total_capabilities"
    );
    assert_eq!(
        result["overall_compliant_count"], 0,
        "C6 rollup: empty hats dir must contribute 0 to overall_compliant_count"
    );
}

// ── 2. Well-formed hats → no error findings ─────────────────────────

#[tokio::test]
async fn well_formed_hats_no_error_findings() {
    let tmp = make_brain_with_hats_dir();

    // The 8 well-formed hats from Component 4 (mirror of the live
    // hats — populated for supply-chain-auditor + source-reader, the
    // two hats with pre-existing prose MUST_NOT claims; permissive
    // default for the other six per Q4).
    let permissive_briefs: &[(&str, &str)] = &[
        (
            "adversary",
            "Adversarial reviewer who surfaces edge cases, missing rollback paths, and gate gaps.",
        ),
        (
            "architect",
            "Generative system designer who explores tradeoffs, layering, and extension points.",
        ),
        (
            "incident-commander",
            "Calm-and-decisive operator who stabilizes incidents before investigating root cause.",
        ),
        (
            "rubber-duck",
            "Socratic listener who asks clarifying questions instead of jumping to solutions.",
        ),
        (
            "security-auditor",
            "Paranoid reviewer of IAM, secrets, and access topology who minimizes surface area.",
        ),
        (
            "visionary",
            "Divergent thinker who explores multiple approaches before committing to specifics.",
        ),
    ];

    for (name, desc) in permissive_briefs {
        let body = format!(
            "---\n\
             name: {name}\n\
             description: {desc}\n\
             briefing: short briefing line.\n\
             ---\n\
             \n\
             Persona hat for testing — well-formed permissive default.\n"
        );
        write_hat(tmp.path(), name, &body);
    }

    // Populated hats — supply-chain-auditor (forbidden_tools +
    // network_targets) and source-reader (forbidden_tools).
    let supply_chain = "---\n\
        name: supply-chain-auditor\n\
        description: Read-only adversarial reviewer for dependency / package supply-chain risk.\n\
        briefing: Skeptical reviewer; surface upstream risk; never install or build untrusted packages.\n\
        forbidden_tools:\n  - Bash\n  - package_install\n\
        network_targets:\n  allowed:\n    - osv.dev\n  forbidden:\n    - \"*.npmjs.com\"\n\
        ---\n\
        \n\
        Persona hat for package-level supply-chain review.\n";
    write_hat(tmp.path(), "supply-chain-auditor", supply_chain);

    let source_reader = "---\n\
        name: source-reader\n\
        description: Read-only investigator for understanding existing code before editing.\n\
        briefing: Read code to understand intent; do not modify or shell out; quote line numbers when relevant.\n\
        forbidden_tools:\n  - Write\n  - Edit\n  - Bash\n\
        ---\n\
        \n\
        Persona hat for read-only code investigation.\n";
    write_hat(tmp.path(), "source-reader", source_reader);

    let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;

    let breakdown = &result["capability_breakdown"]["hat_contracts"];
    assert_eq!(
        breakdown["total"], 8,
        "expected 8 hats scanned; got breakdown: {breakdown}"
    );
    assert_eq!(
        breakdown["compliant"], 8,
        "expected all 8 hats compliant; got breakdown: {breakdown}"
    );

    // No vocabulary or declaration findings should be emitted.
    let findings = hat_contract_findings(&result);
    let error_findings: Vec<&Value> = findings
        .iter()
        .filter(|f| {
            let n = f["name"].as_str().unwrap_or("");
            n.contains(":vocabulary:") || n.contains(":declaration:")
        })
        .copied()
        .collect();
    assert!(
        error_findings.is_empty(),
        "well-formed hats must produce zero error findings; got: {:?}",
        error_findings
    );

    // C6 rollup pin (2026-04-27): hat_contracts now contributes to the
    // top-level rollups. The fixture sets up ONLY 8 hats — no skills,
    // subagents, tools, registry-hats, correlations, or personas — so
    // `total_capabilities` and `overall_compliant_count` must both equal
    // 8 (8 hat_contracts, all compliant). Pre-C6 baseline was 0 / 0
    // because hat_contracts was deliberately excluded; C6 lifts it.
    assert_eq!(
        result["total_capabilities"], 8,
        "C6 rollup: 8 well-formed hats must contribute +8 to total_capabilities; got: {result}"
    );
    assert_eq!(
        result["overall_compliant_count"], 8,
        "C6 rollup: 8 well-formed hats must contribute +8 to overall_compliant_count; got: {result}"
    );
}

// ── 3. Invalid vocabulary → vocabulary finding ──────────────────────

#[tokio::test]
async fn invalid_vocabulary_emits_finding() {
    let tmp = make_brain_with_hats_dir();

    // A hat declaring a vocabulary term outside the closed Q1 set.
    let body = "---\n\
        name: rogue-hat\n\
        description: Test fixture — declares an unknown vocabulary term to exercise the closed-set rejection path.\n\
        forbidden_tools:\n  - assassinate_prod\n\
        ---\n\
        \n\
        Test hat for invalid-vocabulary path.\n";
    write_hat(tmp.path(), "rogue-hat", body);

    let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
    let findings = hat_contract_findings(&result);

    // EXACTLY ONE finding, of vocabulary kind, naming the offending term.
    let vocab_findings: Vec<&Value> = findings
        .iter()
        .filter(|f| {
            f["name"]
                .as_str()
                .map(|n| n.starts_with("hat_contract:vocabulary:"))
                .unwrap_or(false)
        })
        .copied()
        .collect();
    assert_eq!(
        vocab_findings.len(),
        1,
        "expected exactly 1 vocabulary finding; got: {:?}",
        findings
    );
    assert_eq!(
        vocab_findings[0]["name"],
        "hat_contract:vocabulary:rogue-hat:assassinate_prod",
        "vocabulary finding name must follow the documented `<hat>:<term>` shape; got: {:?}",
        vocab_findings[0]
    );
    assert_eq!(
        vocab_findings[0]["status"], "error",
        "vocabulary finding must be flagged as error (advisory but error-status per Q3); got: {:?}",
        vocab_findings[0]
    );
    assert_eq!(
        vocab_findings[0]["points"], 0,
        "vocabulary finding must carry points: 0 (advisory per Q3)"
    );

    // No declaration finding for this hat (vocabulary is the specific
    // failure mode; we don't double-report).
    let declaration_for_this_hat: Vec<&Value> = findings
        .iter()
        .filter(|f| {
            f["name"]
                .as_str()
                .map(|n| n == "hat_contract:declaration:rogue-hat")
                .unwrap_or(false)
        })
        .copied()
        .collect();
    assert!(
        declaration_for_this_hat.is_empty(),
        "vocabulary failure must not also fire a declaration finding; got: {:?}",
        declaration_for_this_hat
    );
}

// ── 4. Missing frontmatter → declaration finding ────────────────────

#[tokio::test]
async fn missing_frontmatter_emits_declaration_finding() {
    let tmp = make_brain_with_hats_dir();

    // No `---` fences — pure prose.
    let body = "# unstructured-hat\n\
                \n\
                This file has no YAML frontmatter; per Q4 the validator emits a neutral \
                declaration finding rather than treating it as a schema-validation failure.\n";
    write_hat(tmp.path(), "unstructured-hat", body);

    let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
    let findings = hat_contract_findings(&result);

    // EXACTLY ONE finding, of declaration kind, naming the hat.
    assert_eq!(
        findings.len(),
        1,
        "expected exactly 1 hat_contract finding; got: {:?}",
        findings
    );
    assert_eq!(
        findings[0]["name"],
        "hat_contract:declaration:unstructured-hat",
        "declaration finding name must follow the documented `<hat>` shape; got: {:?}",
        findings[0]
    );
    assert_eq!(
        findings[0]["status"], "neutral",
        "missing-frontmatter declaration finding is neutral (Q4 — permissive default + advisory); got: {:?}",
        findings[0]
    );
    assert_eq!(
        findings[0]["points"], 0,
        "declaration finding must carry points: 0 (advisory per Q3)"
    );
}

// ── 5. SKILL.md must NOT be treated as a hat ────────────────────────

#[tokio::test]
async fn skill_md_is_not_treated_as_hat() {
    let tmp = make_brain_with_hats_dir();

    // Drop a SKILL.md catalog with prose that LOOKS like it might
    // collide with the validator (e.g., contains "MUST NOT" + lists of
    // tool names that resemble vocabulary). If the validator
    // mistakenly treated SKILL.md as a per-hat contract, we'd see
    // findings; we MUST NOT.
    let skill_md = "# Hats Catalog\n\n\
                    Persona-hat catalog index — operational checklists for the eight named hats.\n\n\
                    ## adversary\n\nMUST NOT skip the rollback-path question.\n\n\
                    ## supply-chain-auditor\n\nMUST NOT execute Bash. MUST NOT package_install.\n\n\
                    ## source-reader\n\nMUST NOT Write. MUST NOT Edit.\n";
    write_hat(tmp.path(), "SKILL", skill_md);

    let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;

    // No hat_contract findings: the catalog index was skipped, not
    // scanned as a per-hat contract.
    let findings = hat_contract_findings(&result);
    assert!(
        findings.is_empty(),
        "SKILL.md must NOT produce hat_contract findings; got: {:?}",
        findings
    );

    // total = 0 — SKILL.md was not counted as a hat.
    let breakdown = &result["capability_breakdown"]["hat_contracts"];
    assert_eq!(
        breakdown["total"], 0,
        "SKILL.md must NOT contribute to hat_contracts.total; got: {breakdown}"
    );
}

// ── Live-dogfood pin — validator runs cleanly against the 8 hats ────

/// Locate `D:/Brains/NeuroGrim/.claude/skills/hats/` from the crate's
/// manifest dir. The crate sits at
/// `<root>/NeuroGrim/neurogrim/crates/neurogrim-sensory/`; the live hats
/// live at `<root>/NeuroGrim/.claude/skills/hats/`. Three `..` hops up
/// from the manifest dir lands at `<root>/NeuroGrim/`. Returns `None`
/// when the live hats dir isn't reachable (standalone checkout) — same
/// skip-when-absent convention as the schema-conformance suites.
fn locate_neurogrim_brain_root() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let neurogrim_root = manifest_dir.join("../../..");
    let hats_dir = neurogrim_root.join(".claude").join("skills").join("hats");
    if hats_dir.is_dir() {
        Some(neurogrim_root)
    } else {
        None
    }
}

/// Component-5 verification line item (per the C5 brief): "point
/// `score_hat_contracts` at `D:/Brains/NeuroGrim/.claude/skills/hats/`
/// and assert: 8 hats scanned, 0 error findings (all should validate
/// cleanly post-Component 4)."
#[tokio::test]
async fn live_neurogrim_hats_validate_cleanly() {
    let Some(brain_root) = locate_neurogrim_brain_root() else {
        eprintln!("skip: live NeuroGrim hats dir not reachable");
        return;
    };

    let result = analyze_capability_hygiene(brain_root.to_str().unwrap()).await;

    let breakdown = &result["capability_breakdown"]["hat_contracts"];
    assert_eq!(
        breakdown["total"], 8,
        "expected exactly 8 hats scanned in live NeuroGrim Brain (the spec-named set: \
         adversary, architect, incident-commander, rubber-duck, security-auditor, \
         supply-chain-auditor, visionary, source-reader). Got breakdown: {breakdown}"
    );
    assert_eq!(
        breakdown["compliant"], 8,
        "expected all 8 live hats to validate cleanly post-Component 4. \
         Got breakdown: {breakdown}"
    );

    let findings = hat_contract_findings(&result);
    assert!(
        findings.is_empty(),
        "live NeuroGrim hats produced {} hat_contract findings — \
         Component 4 migration is incomplete or inconsistent. Findings: {:?}",
        findings.len(),
        findings
    );

    // C6 rollup pin (2026-04-27): the 8 live hats MUST contribute +8 to
    // both top-level rollups. The live NeuroGrim Brain has many other
    // capabilities (skills, subagents, tools, registry-hats, ...), so we
    // can't pin exact totals — but we can pin "at least 8" with a brief
    // failure message naming the C6 contract. The breakdown numbers above
    // already pinned hat_contracts.total = 8 + hat_contracts.compliant = 8.
    let total_caps = result["total_capabilities"]
        .as_u64()
        .expect("total_capabilities must be a number");
    let overall_compliant = result["overall_compliant_count"]
        .as_u64()
        .expect("overall_compliant_count must be a number");
    assert!(
        total_caps >= 8,
        "C6 rollup: live NeuroGrim total_capabilities must be ≥ 8 (the 8 hat_contracts \
         alone); got {total_caps}. Either C6 rollup integration regressed or the live \
         Brain's hats catalog is incomplete."
    );
    assert!(
        overall_compliant >= 8,
        "C6 rollup: live NeuroGrim overall_compliant_count must be ≥ 8 (the 8 \
         well-formed hat_contracts alone); got {overall_compliant}. Either C6 rollup \
         integration regressed or live hats stopped validating cleanly."
    );
}

// ── 6. Recursion guard — Q6 hard rule ────────────────────────────────

/// The validator MUST be pure file-read + JSON-Schema-validate. NO shell-
/// out. NO `std::process::Command`. NO `Bash`/`Edit`/`Write` invocations.
/// This test reads the source of `capability_hygiene.rs`, locates the
/// `score_hat_contracts` function span, and grep-checks for forbidden
/// patterns within the span.
///
/// Per the C5 brief: "if a future refactor inadvertently adds shell-out,
/// the test catches it." Pin Q6 against future drift.
#[test]
fn recursion_guard_no_command_in_validator_span() {
    let path = locate_capability_hygiene_source();
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    // Locate the `score_hat_contracts` function start. The signature
    // is `fn score_hat_contracts(root: &Path) -> TypeResult {`. We
    // search for a stable substring that uniquely identifies the
    // function's opening line.
    let signature = "fn score_hat_contracts(root: &Path) -> TypeResult {";
    let fn_start = source.find(signature).unwrap_or_else(|| {
        panic!(
            "could not locate `{}` in {}; recursion-guard test cannot validate",
            signature,
            path.display()
        )
    });

    // The validator's body extends to the matching closing brace at the
    // end of the function. We approximate the span by walking forward
    // from `fn_start` and counting `{` / `}` pairs starting from the
    // signature's opening brace. This is robust to nested control flow.
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut span_end: Option<usize> = None;
    let mut i = fn_start;
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut escape_next = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escape_next {
            escape_next = false;
            i += 1;
            continue;
        }
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_string {
            if b == b'\\' {
                escape_next = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        // Detect comment / string starts (best-effort; we don't model
        // raw strings or char literals — adequate for this source file).
        if b == b'/' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'/' {
                in_line_comment = true;
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'*' {
                in_block_comment = true;
                i += 2;
                continue;
            }
        }
        if b == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                span_end = Some(i + 1);
                break;
            }
        }
        i += 1;
    }
    let span_end = span_end.unwrap_or_else(|| {
        panic!(
            "could not find matching closing brace for score_hat_contracts in {}",
            path.display()
        )
    });

    let span = &source[fn_start..span_end];

    // Forbidden patterns — Q6 hard rule. The validator MUST NOT shell
    // out OR otherwise execute external code. This list is the closure
    // of "shell-execution surfaces" we know about; future surfaces
    // (e.g., `std::os::unix::process::CommandExt`) won't compile on
    // Windows but should still trip this check if introduced.
    let forbidden = [
        "std::process::Command",
        "process::Command",
        "Command::new",
        "duct::cmd",
        "subprocess::",
    ];
    for pat in forbidden.iter() {
        assert!(
            !span.contains(pat),
            "Q6 recursion-guard violated: `score_hat_contracts` contains forbidden \
             shell-execution pattern `{pat}`. The validator must be pure file-read + \
             JSON-Schema-validate. See spec §5.4.1 + plan E-B2-3 Q6."
        );
    }

    // Defense-in-depth: also reject patterns that suggest the validator
    // is donning a hat or invoking the Skill / Bash / Edit / Write tool
    // surface — those are signals of a recursion path that doesn't
    // exist today but could be smuggled in via a refactor.
    let suspicious = [
        "Skill::",
        "execute_skill",
        "invoke_tool",
    ];
    for pat in suspicious.iter() {
        assert!(
            !span.contains(pat),
            "Q6 recursion-guard tripped on suspicious pattern `{pat}` inside \
             `score_hat_contracts`. The validator should not invoke tools or \
             skills — surface the violation to the operator instead."
        );
    }

    // Cargo.toml dependency check — assert no shell-execution crates
    // were added to neurogrim-sensory. Mirrors the source-side check
    // at the dependency-graph level: even if the source is clean now,
    // a future refactor might introduce a wrapper crate.
    let cargo_toml_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let cargo_toml = std::fs::read_to_string(&cargo_toml_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", cargo_toml_path.display()));
    let forbidden_deps = [
        "\nwhich = ",
        "\nsubprocess = ",
        "\nduct = ",
        "\nshell-words = ",
    ];
    for dep in forbidden_deps.iter() {
        assert!(
            !cargo_toml.contains(dep),
            "Q6 recursion-guard violated at dependency level: Cargo.toml grew shell-\
             execution dependency `{}`. Validator must not link to shell-execution \
             crates. See spec §5.4.1 + plan E-B2-3 Q6.",
            dep.trim()
        );
    }
}
