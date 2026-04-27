//! E-B2-6 C4 — `analyze_operator_calibration` behavioral tests.
//!
//! Mirrors the E-B2-4 `trust_budget_sensor_behavior.rs` shape:
//! synthetic-`TempDir` fixtures populated programmatically (JSONL written
//! line-by-line), then observed via the public `analyze_operator_calibration`
//! entry point's CMDB envelope output.
//!
//! Coverage (11 cases including the BR-5 privacy regression pin and the
//! Q6 recursion-guard pin):
//!
//!  1. `missing_ledger_returns_advisory_floor` — no ledger file at all;
//!     `score == 100`, `low_confidence == true`, no `:declaration:missing`-style
//!     finding (legitimate absence, not an error).
//!  2. `empty_ledger_low_confidence` — empty `.jsonl` file present.
//!  3. `skill_only_no_dispositions_low_confidence` — 5 skill rows, 0
//!     disposition rows; below the no-dispositions-yet threshold so no
//!     finding fires.
//!  4. `skill_only_lots_emits_no_dispositions_yet_finding` — 60 skill
//!     rows, 0 disposition rows; the `no_dispositions_yet` advisory
//!     fires.
//!  5. `few_dispositions_below_n_min_score_null` — 30 skill rows + 5
//!     disposition rows; `score == null` per Q9, single
//!     `:low_confidence` finding.
//!  6. `enough_dispositions_score_computed` — 50 skill + 25 disposition
//!     rows (20 accepted, 3 rejected, 2 modified); `score == 80`,
//!     `low_confidence == false`, no findings.
//!  7. `disposition_with_unknown_kind_skipped` — disposition row with
//!     `disposition_kind == "ignored"`; sensor doesn't crash; row
//!     counted toward `dispositioned_count` but NOT toward any
//!     closed-set per-kind counter.
//!  8. `malformed_jsonl_lines_skipped_with_summary` — 3 valid skill +
//!     2 invalid lines; sensor returns successfully;
//!     `total_invocations == 3`; `:malformed_lines` summary finding
//!     emitted with detail mentioning `2`.
//!  9. `aggregation_only_export_q5_privacy_pin` — **THE BR-5 PRIVACY
//!     REGRESSION PIN**. 30 disposition rows for various
//!     invocation_ids; the output JSON contains breakdown counters
//!     ONLY, no per-invocation data. Search the serialized output for
//!     the literal `invocation_id` strings; assert ZERO matches.
//! 10. `recursion_guard_no_command_in_validator_span` — Q6 lock.
//!     File-level grep for forbidden shell-out patterns; mirrors
//!     `trust_budget_sensor_behavior.rs:recursion_guard_no_command_in_validator_span`.
//! 11. `live_neurogrim_smoke` — invoke the sensor against the actual
//!     `D:/Brains/NeuroGrim/` project root; structural sanity (counts
//!     non-negative, low_confidence true, output JSON well-formed).

use serde_json::Value;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use neurogrim_sensory::operator_calibration::analyze_operator_calibration;

// ── Fixture helpers ─────────────────────────────────────────────────────────

fn make_brain_root() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

/// Create the `.claude/brain/` directory at `root` and return the path
/// to the invocation-ledger JSONL file (without writing it). Caller
/// chooses whether to write content or leave it absent.
fn ledger_path(root: &Path) -> PathBuf {
    let dir = root.join(".claude").join("brain");
    std::fs::create_dir_all(&dir).expect("create .claude/brain dir");
    dir.join("invocation-ledger.jsonl")
}

/// Write the given lines as a JSONL ledger at the canonical location.
/// Joins lines with `\n`; the writer adds a trailing newline.
fn write_ledger(root: &Path, lines: &[&str]) {
    let path = ledger_path(root);
    let mut buf = String::new();
    for line in lines {
        buf.push_str(line);
        buf.push('\n');
    }
    std::fs::write(&path, buf).expect("write invocation-ledger.jsonl");
}

/// Author a synthetic `SkillEntry` JSON line. The ts is fixed (the sensor
/// doesn't bucket by time at v1 — counts only).
fn skill_row(name: &str, invocation_id: &str) -> String {
    format!(
        r#"{{"schema_version":"1","ts":"2026-04-26T12:00:00Z","type":"skill","name":"{name}","session_id":"sess-test","invocation_id":"{invocation_id}"}}"#
    )
}

/// Author a synthetic `DispositionEntry` JSON line.
fn disposition_row(invocation_id: &str, kind: &str) -> String {
    format!(
        r#"{{"schema_version":"1","ts":"2026-04-26T13:00:00Z","entry_kind":"disposition","invocation_id":"{invocation_id}","disposition_kind":"{kind}","human_operator":"keenan"}}"#
    )
}

/// Filter findings by name prefix.
fn findings_by_prefix<'a>(result: &'a Value, prefix: &str) -> Vec<&'a Value> {
    result["findings"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|f| {
                    f["name"]
                        .as_str()
                        .map(|s| s.starts_with(prefix))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// All `operator_calibration:*` findings.
fn op_cal_findings(result: &Value) -> Vec<&Value> {
    findings_by_prefix(result, "operator_calibration:")
}

/// Locate the `operator_calibration.rs` source file from the crate's
/// manifest dir. Used by the recursion-guard test.
fn locate_operator_calibration_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("operator_calibration.rs")
}

// ── 1. Missing ledger → advisory floor ──────────────────────────────────────

#[tokio::test]
async fn missing_ledger_returns_advisory_floor() {
    let tmp = make_brain_root();
    // Do NOT create .claude/brain/invocation-ledger.jsonl.

    let result = analyze_operator_calibration(tmp.path().to_str().unwrap()).await;

    // Advisory floor: 100 (no judgment yet, so no negative signal).
    assert_eq!(result["score"], 100, "missing ledger must return advisory floor 100");

    // No declaration-missing-style finding for legitimate absence.
    let all = op_cal_findings(&result);
    assert!(
        all.is_empty() || all.iter().all(|f| f["points"] == 0),
        "missing ledger findings must be either empty or all advisory; got: {:?}",
        all
    );
    let no_dispositions = findings_by_prefix(&result, "operator_calibration:no_dispositions_yet");
    assert!(
        no_dispositions.is_empty(),
        "no_dispositions_yet must NOT fire on a missing ledger (no skill invocations either)"
    );

    // Breakdown shape.
    let bd = &result["operator_calibration_breakdown"];
    assert!(bd.is_object(), "breakdown must be present");
    assert_eq!(bd["ledger_present"], false);
    assert_eq!(bd["total_invocations"], 0);
    assert_eq!(bd["dispositioned_count"], 0);
    assert_eq!(bd["low_confidence"], true);
    assert_eq!(bd["has_ever_dispositioned"], false);
    assert_eq!(bd["n_min"], 20);
    // ledger_path should be null when absent.
    assert!(bd["ledger_path"].is_null(), "ledger_path must be JSON null when absent");

    // Envelope canonical fields.
    assert_eq!(result["meta"]["updated_by"], "operator-calibration");
    assert_eq!(result["meta"]["schema_version"], "1");
}

// ── 2. Empty ledger file → low-confidence advisory floor ────────────────────

#[tokio::test]
async fn empty_ledger_low_confidence() {
    let tmp = make_brain_root();
    write_ledger(tmp.path(), &[]);

    let result = analyze_operator_calibration(tmp.path().to_str().unwrap()).await;

    assert_eq!(result["score"], 100, "empty ledger == advisory floor (no judgment yet)");
    let bd = &result["operator_calibration_breakdown"];
    assert_eq!(bd["ledger_present"], true);
    assert_eq!(bd["total_invocations"], 0);
    assert_eq!(bd["dispositioned_count"], 0);
    assert_eq!(bd["low_confidence"], true);
    assert_eq!(bd["has_ever_dispositioned"], false);
}

// ── 3. Skills only, low total → low-confidence floor, no finding ────────────

#[tokio::test]
async fn skill_only_no_dispositions_low_confidence() {
    let tmp = make_brain_root();
    let lines: Vec<String> = (0..5)
        .map(|i| skill_row("plan-critic", &format!("inv-{i}")))
        .collect();
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_operator_calibration(tmp.path().to_str().unwrap()).await;

    // Advisory floor: dispositioned_count == 0 path.
    assert_eq!(result["score"], 100);
    let bd = &result["operator_calibration_breakdown"];
    assert_eq!(bd["total_invocations"], 5);
    assert_eq!(bd["dispositioned_count"], 0);
    assert_eq!(bd["low_confidence"], true);

    // Below the no-dispositions-yet threshold (50) — no finding.
    let no_dispositions = findings_by_prefix(&result, "operator_calibration:no_dispositions_yet");
    assert!(
        no_dispositions.is_empty(),
        "5 invocations < 50 threshold; no_dispositions_yet should NOT fire"
    );
    // No low_confidence finding either (that fires on 1 ≤ N < N_MIN, not on N == 0).
    let low_conf = findings_by_prefix(&result, "operator_calibration:low_confidence");
    assert!(
        low_conf.is_empty(),
        "low_confidence finding should NOT fire when dispositioned_count == 0"
    );
}

// ── 4. Many skills, no dispositions → no_dispositions_yet finding ───────────

#[tokio::test]
async fn skill_only_lots_emits_no_dispositions_yet_finding() {
    let tmp = make_brain_root();
    let lines: Vec<String> = (0..60)
        .map(|i| skill_row("plan-critic", &format!("inv-{i}")))
        .collect();
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_operator_calibration(tmp.path().to_str().unwrap()).await;

    assert_eq!(result["score"], 100);
    let bd = &result["operator_calibration_breakdown"];
    assert_eq!(bd["total_invocations"], 60);
    assert_eq!(bd["dispositioned_count"], 0);

    let nudge = findings_by_prefix(&result, "operator_calibration:no_dispositions_yet");
    assert_eq!(
        nudge.len(),
        1,
        "expected exactly 1 no_dispositions_yet finding when total > 50 and dispositioned == 0; got: {:?}",
        op_cal_findings(&result)
    );
    assert_eq!(nudge[0]["points"], 0);
    assert_eq!(nudge[0]["status"], "neutral");
}

// ── 5. Below N_MIN dispositions → score = null ──────────────────────────────

#[tokio::test]
async fn few_dispositions_below_n_min_score_null() {
    let tmp = make_brain_root();
    let mut lines: Vec<String> = (0..30)
        .map(|i| skill_row("plan-critic", &format!("inv-{i}")))
        .collect();
    // Mix of disposition kinds, 5 total — below N_MIN=20.
    lines.push(disposition_row("inv-0", "accepted"));
    lines.push(disposition_row("inv-1", "rejected"));
    lines.push(disposition_row("inv-2", "modified"));
    lines.push(disposition_row("inv-3", "superseded"));
    lines.push(disposition_row("inv-4", "accepted"));
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_operator_calibration(tmp.path().to_str().unwrap()).await;

    // Q9 lock — score is JSON null, not 0 or 100.
    assert!(
        result["score"].is_null(),
        "score must be JSON null below N_MIN per Q9 lock; got: {:?}",
        result["score"]
    );
    let bd = &result["operator_calibration_breakdown"];
    assert_eq!(bd["total_invocations"], 30);
    assert_eq!(bd["dispositioned_count"], 5);
    assert_eq!(bd["accepted_count"], 2);
    assert_eq!(bd["rejected_count"], 1);
    assert_eq!(bd["modified_count"], 1);
    assert_eq!(bd["superseded_count"], 1);
    assert_eq!(bd["low_confidence"], true);
    assert_eq!(bd["has_ever_dispositioned"], true);

    // Exactly one low_confidence finding.
    let low_conf = findings_by_prefix(&result, "operator_calibration:low_confidence");
    assert_eq!(
        low_conf.len(),
        1,
        "expected exactly 1 low_confidence finding; got: {:?}",
        op_cal_findings(&result)
    );
    assert_eq!(low_conf[0]["points"], 0);
    let detail = low_conf[0]["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("dispositioned_count=5") && detail.contains("N_MIN=20"),
        "low_confidence detail should mention sample-size and threshold; got: `{detail}`"
    );

    // no_dispositions_yet must NOT fire on this branch.
    let nudge = findings_by_prefix(&result, "operator_calibration:no_dispositions_yet");
    assert!(
        nudge.is_empty(),
        "no_dispositions_yet must not fire when dispositioned_count > 0"
    );
}

// ── 6. ≥ N_MIN dispositions → score computed ────────────────────────────────

#[tokio::test]
async fn enough_dispositions_score_computed() {
    let tmp = make_brain_root();
    let mut lines: Vec<String> = (0..50)
        .map(|i| skill_row("plan-critic", &format!("inv-{i}")))
        .collect();
    // 25 dispositions: 20 accepted, 3 rejected, 2 modified — 0 superseded.
    for i in 0..20 {
        lines.push(disposition_row(&format!("inv-{i}"), "accepted"));
    }
    for i in 20..23 {
        lines.push(disposition_row(&format!("inv-{i}"), "rejected"));
    }
    for i in 23..25 {
        lines.push(disposition_row(&format!("inv-{i}"), "modified"));
    }
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_operator_calibration(tmp.path().to_str().unwrap()).await;

    // 20/25 × 100 = 80.
    assert_eq!(result["score"], 80, "score must be round(20/25*100) = 80");
    let bd = &result["operator_calibration_breakdown"];
    assert_eq!(bd["total_invocations"], 50);
    assert_eq!(bd["dispositioned_count"], 25);
    assert_eq!(bd["accepted_count"], 20);
    assert_eq!(bd["rejected_count"], 3);
    assert_eq!(bd["modified_count"], 2);
    assert_eq!(bd["superseded_count"], 0);
    assert_eq!(bd["low_confidence"], false);
    assert_eq!(bd["has_ever_dispositioned"], true);

    // Clean state: no findings.
    let all = op_cal_findings(&result);
    assert!(
        all.is_empty(),
        "clean meaningful-sample-size state should emit zero findings; got: {:?}",
        all
    );
}

// ── 7. Unknown disposition_kind silently tolerated ──────────────────────────

#[tokio::test]
async fn disposition_with_unknown_kind_skipped() {
    let tmp = make_brain_root();
    // 1 skill row + 1 disposition row with an unknown kind ("ignored",
    // which is deliberately NOT in the closed set per Q1).
    let lines = vec![
        skill_row("plan-critic", "inv-0"),
        // Author the disposition row by hand so we can use a kind value
        // outside the closed set.
        r#"{"schema_version":"1","ts":"2026-04-26T13:00:00Z","entry_kind":"disposition","invocation_id":"inv-0","disposition_kind":"ignored","human_operator":"keenan"}"#.to_string(),
    ];
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_operator_calibration(tmp.path().to_str().unwrap()).await;

    // The sensor MUST NOT panic. The row counts toward dispositioned_count
    // (it IS a disposition row by entry_kind) but NOT toward any of the
    // closed-set per-kind counters.
    let bd = &result["operator_calibration_breakdown"];
    assert_eq!(bd["total_invocations"], 1);
    assert_eq!(bd["dispositioned_count"], 1);
    assert_eq!(bd["accepted_count"], 0);
    assert_eq!(bd["rejected_count"], 0);
    assert_eq!(bd["modified_count"], 0);
    assert_eq!(bd["superseded_count"], 0);
    // Score is null because dispositioned_count (1) < N_MIN (20).
    assert!(result["score"].is_null());
}

// ── 8. Malformed JSONL lines counted via summary finding ────────────────────

#[tokio::test]
async fn malformed_jsonl_lines_skipped_with_summary() {
    let tmp = make_brain_root();
    let lines = vec![
        skill_row("plan-critic", "inv-0"),
        skill_row("plan-critic", "inv-1"),
        "not json".to_string(),
        skill_row("plan-critic", "inv-2"),
        "{broken".to_string(),
    ];
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_operator_calibration(tmp.path().to_str().unwrap()).await;

    let bd = &result["operator_calibration_breakdown"];
    assert_eq!(
        bd["total_invocations"], 3,
        "valid skill rows are counted; malformed lines silently skipped"
    );
    assert_eq!(bd["dispositioned_count"], 0);

    let malformed_findings = findings_by_prefix(&result, "operator_calibration:malformed_lines");
    assert_eq!(
        malformed_findings.len(),
        1,
        "expected exactly 1 malformed_lines summary finding; got: {:?}",
        op_cal_findings(&result)
    );
    let detail = malformed_findings[0]["detail"]
        .as_str()
        .unwrap_or("");
    assert!(
        detail.contains('2'),
        "malformed_lines detail should mention the count `2`; got: `{detail}`"
    );
}

// ── 9. Aggregation-only export — BR-5 PRIVACY REGRESSION PIN ────────────────

// E6-1 BR-5 mitigation: the operator_calibration sensor's CMDB output MUST
// emit aggregate totals only. If a future change leaks per-invocation rows,
// this test catches it. Re-opening per-invocation export requires a
// charter-level BR-5 conversation.
#[tokio::test]
async fn aggregation_only_export_q5_privacy_pin() {
    let tmp = make_brain_root();

    // Author 30 disposition rows referencing 30 distinct, recognizable
    // invocation_ids. We then assert ZERO occurrences of those literal
    // strings in the serialized output.
    let mut lines: Vec<String> = (0..30)
        .map(|i| skill_row("plan-critic", &format!("inv-{i}")))
        .collect();
    let mut sensitive_ids: Vec<String> = Vec::new();
    for i in 0..30 {
        let id = format!("PRIVATE_INVOCATION_ID_{i}");
        sensitive_ids.push(id.clone());
        // Pick varied kinds so none of the per-kind counters are zero.
        let kind = match i % 4 {
            0 => "accepted",
            1 => "rejected",
            2 => "modified",
            _ => "superseded",
        };
        lines.push(disposition_row(&id, kind));
    }
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_operator_calibration(tmp.path().to_str().unwrap()).await;

    // Serialize the full envelope to a string and search for sensitive
    // invocation_id values. ZERO matches required.
    let serialized = serde_json::to_string(&result).expect("serialize result");
    for id in &sensitive_ids {
        assert!(
            !serialized.contains(id.as_str()),
            "BR-5 privacy regression: per-invocation id `{id}` leaked into the operator_calibration \
             CMDB output. The sensor MUST emit aggregate totals only. Re-opening per-invocation \
             export requires a charter-level BR-5 conversation."
        );
    }

    // Sanity check that we DID see the dispositions in aggregate form.
    let bd = &result["operator_calibration_breakdown"];
    assert_eq!(bd["dispositioned_count"], 30);
    // Score is computed — 30 ≥ N_MIN, accepted = 8 (i % 4 == 0 for 0..30 → 8 ids).
    // 8/30 × 100 ≈ 26.67 → rounds to 27.
    assert_eq!(result["score"], 27);
}

// ── 10. Recursion guard — Q6 hard rule ──────────────────────────────────────

/// The validator MUST be pure file-read + JSONL-parse + count + JSON-output.
/// NO shell-out. NO `std::process::Command`. NO `Stdio`. NO `std::process`.
/// This test reads the source of `operator_calibration.rs` and grep-checks
/// for forbidden patterns within the entire file (since the sensor logic
/// lives in helper functions called from `analyze_operator_calibration_path`,
/// the file-level scan IS the validator-span scan — same shape as
/// `trust_budget_sensor_behavior.rs::recursion_guard_no_command_in_validator_span`).
#[test]
fn recursion_guard_no_command_in_validator_span() {
    let path = locate_operator_calibration_source();
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let forbidden = [
        "std::process::Command",
        "process::Command",
        "Command::new",
        "Stdio",
        "std::process",
        "duct::cmd",
        "subprocess::",
    ];
    for pat in forbidden.iter() {
        assert!(
            !source.contains(pat),
            "Q6 recursion-guard violated: `operator_calibration.rs` contains forbidden \
             shell-execution pattern `{pat}`. The sensor must be pure file-read + \
             JSONL-parse + count + JSON-output. See E-B2-6 plan Q6."
        );
    }

    // Defense-in-depth: also reject patterns that suggest the sensor is
    // donning a hat or invoking the Skill / Bash / Edit / Write tool
    // surface — same defensive posture as
    // `hat_contract_sensor_behavior.rs::recursion_guard_no_command_in_validator_span`.
    let suspicious = ["Skill::", "execute_skill", "invoke_tool"];
    for pat in suspicious.iter() {
        assert!(
            !source.contains(pat),
            "Q6 recursion-guard tripped on suspicious pattern `{pat}` inside \
             `operator_calibration.rs`. The sensor should not invoke tools or skills \
             — surface the observation to the operator instead."
        );
    }
}

// ── 11. Live NeuroGrim smoke ────────────────────────────────────────────────

/// Locate the live NeuroGrim Brain root from the crate's manifest dir.
/// Mirrors `trust_budget_sensor_behavior.rs::locate_neurogrim_brain_root`.
fn locate_neurogrim_brain_root() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let brain_root = manifest_dir.join("../../..").canonicalize().ok()?;
    if brain_root.join(".claude").is_dir() {
        Some(brain_root)
    } else {
        None
    }
}

/// Smoke-test against the live NeuroGrim Brain root. The actual NeuroGrim
/// ledger may have skill rows (PostToolUse hook is enabled there) but no
/// disposition rows yet (CLI just shipped this session). We don't pin
/// specific finding counts — the live ledger is a moving target. We
/// assert structural sanity only: counts non-negative, low_confidence
/// true, output JSON well-formed.
#[tokio::test]
async fn live_neurogrim_smoke() {
    let Some(brain_root) = locate_neurogrim_brain_root() else {
        eprintln!("skip: live NeuroGrim brain root not reachable");
        return;
    };

    let result = analyze_operator_calibration(brain_root.to_str().unwrap()).await;

    // Envelope is well-formed.
    assert_eq!(result["meta"]["updated_by"], "operator-calibration");
    assert_eq!(result["meta"]["schema_version"], "1");

    // Score is either an integer in [0, 100] or JSON null (Q9 path).
    let score_ok = result["score"].is_null()
        || (result["score"].is_number()
            && result["score"].as_u64().map(|n| n <= 100).unwrap_or(false));
    assert!(
        score_ok,
        "live smoke: score must be a number in [0,100] or JSON null; got: {:?}",
        result["score"]
    );

    let bd = &result["operator_calibration_breakdown"];
    assert!(bd.is_object(), "breakdown must be present");
    let total = bd["total_invocations"].as_u64().expect("total_invocations is a number");
    let dispositioned = bd["dispositioned_count"].as_u64().expect("dispositioned_count is a number");

    // Per the brief: dispositions haven't been recorded yet, so
    // dispositioned_count == 0 and low_confidence == true. We assert
    // this expected state, but allow the test to soft-skip if a
    // future operator has actually dispositioned things — the
    // structural shape still holds.
    assert_eq!(
        dispositioned, 0,
        "expected zero dispositions on live NeuroGrim ledger (CLI just shipped this session); \
         got: {dispositioned}"
    );
    assert_eq!(
        bd["low_confidence"], true,
        "expected low_confidence == true (dispositioned_count < N_MIN)"
    );

    eprintln!(
        "live_neurogrim_smoke: total_invocations={total}, dispositioned_count={dispositioned}, \
         low_confidence={}",
        bd["low_confidence"]
    );
    let bd_present = bd["ledger_present"].as_bool().unwrap_or(false);
    eprintln!("  ledger_present = {bd_present}");
}
