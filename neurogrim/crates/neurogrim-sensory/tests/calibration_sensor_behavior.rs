//! Spec §17.9 / E-B2-2 C4 — Domain-calibration sensor behavior.
//!
//! End-to-end tests of the `analyze_domain_calibration` entry
//! point against synthetic ledger fixtures. Pins:
//!
//!  1. **Zero-ledger** — no `.claude/brain/` dir or empty dir →
//!     score 100, has_ever_fired=false, confidence 100. The "no
//!     signal yet" case the tuple-aware confidence (§17.9) was
//!     designed to distinguish from "recently triaged".
//!
//!  2. **Pending-only ledger** — has fired but operator hasn't
//!     triaged any → score reflects open count, confidence 50.
//!     The "calibration backlog unattended" case.
//!
//!  3. **Pending+triaged-recent** — fresh triage → high confidence
//!     decayed only slightly from the 7-day TTL anchor.
//!
//!  4. **Pending+triaged-ancient** — old triage (well past 7-day
//!     TTL) → confidence drops sharply.
//!
//!  5. **Multi-domain** — tests aggregation across multiple
//!     `<domain>-calibration-ledger.jsonl` files.

use serde_json::{json, Value};
use std::path::Path;
use tempfile::TempDir;

use neurogrim_sensory::domain_calibration::analyze_domain_calibration;

/// Build a project-root tempdir with a `.claude/brain/` subdir
/// ready to receive ledger files.
fn make_project_root() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let brain = dir.path().join(".claude").join("brain");
    std::fs::create_dir_all(&brain).expect("create brain dir");
    dir
}

/// Write a JSONL file at `<project_root>/.claude/brain/<domain>-calibration-ledger.jsonl`
/// containing the given entries (one per line).
fn write_ledger(project_root: &Path, domain: &str, entries: &[Value]) {
    let path = project_root
        .join(".claude")
        .join("brain")
        .join(format!("{domain}-calibration-ledger.jsonl"));
    let lines: Vec<String> = entries
        .iter()
        .map(|e| serde_json::to_string(e).expect("serialize"))
        .collect();
    std::fs::write(&path, lines.join("\n") + "\n").expect("write ledger");
}

fn pending_entry(ts: f64, domain: &str) -> Value {
    json!({
        "entry_kind": "pending",
        "ts": ts,
        "schema_version": "1",
        "domain": domain,
        "domain_family": "domain-calibration",
        "trigger_signal_kind": "out-of-range",
        "actual_score": 30
    })
}

fn triaged_entry(ts: f64, supersedes_ts: f64, domain: &str) -> Value {
    json!({
        "entry_kind": "triaged",
        "ts": ts,
        "schema_version": "1",
        "domain": domain,
        "domain_family": "domain-calibration",
        "supersedes_ts": supersedes_ts,
        "triage_decision": "no-action",
        "human_operator": "test-operator",
        "human_notes": "Score drop was a deliberate test-suite restructure."
    })
}

fn now_secs() -> f64 {
    chrono::Utc::now().timestamp() as f64
}

// ─── Test cases ───────────────────────────────────────────────────────

#[tokio::test]
async fn zero_ledger_yields_score_100_confidence_100() {
    // No ledgers exist (brain dir is empty). Per §17.9 tuple:
    // has_ever_fired=false → confidence=100. Score is 100 (nothing
    // open). The "no signal yet" case.
    let dir = make_project_root();
    let cmdb = analyze_domain_calibration(dir.path().to_str().unwrap()).await;
    assert_eq!(cmdb["score"], 100, "zero ledger → score 100");
    assert_eq!(cmdb["confidence"], 100, "zero ledger → confidence 100");
    assert_eq!(cmdb["has_ever_fired"], false);
    assert_eq!(cmdb["open_pending_total"], 0);
    assert_eq!(cmdb["triaged_total"], 0);
    let scanned = cmdb["domains_scanned"].as_array().expect("array");
    assert_eq!(scanned.len(), 0);
}

#[tokio::test]
async fn missing_brain_dir_yields_score_100() {
    // Edge: no .claude/ at all. Same effective output as empty
    // brain/ — sensor doesn't crash; reports clean.
    let dir = tempfile::tempdir().expect("tempdir");
    let cmdb = analyze_domain_calibration(dir.path().to_str().unwrap()).await;
    assert_eq!(cmdb["score"], 100);
    assert_eq!(cmdb["confidence"], 100);
    assert_eq!(cmdb["has_ever_fired"], false);
}

#[tokio::test]
async fn pending_only_ledger_low_confidence() {
    // Pending entries exist but none triaged. Per §17.9:
    // has_ever_fired=true AND last_triage_age=None → confidence 50
    // ("operator hasn't responded; calibration backlog unattended").
    // Score: 100 - 10 × 1 = 90 (one open pending).
    let dir = make_project_root();
    let now = now_secs();
    write_ledger(
        dir.path(),
        "test-health",
        &[pending_entry(now - 3600.0, "test-health")],
    );
    let cmdb = analyze_domain_calibration(dir.path().to_str().unwrap()).await;
    assert_eq!(cmdb["score"], 90, "one open pending → 100 - 10 × 1 = 90");
    assert_eq!(
        cmdb["confidence"], 50,
        "pending-only → confidence 50 (operator unresponsive)"
    );
    assert_eq!(cmdb["has_ever_fired"], true);
    assert_eq!(cmdb["open_pending_total"], 1);
    assert_eq!(cmdb["triaged_total"], 0);
}

#[tokio::test]
async fn pending_plus_recent_triaged_high_confidence() {
    // Triaged entry from ~1 hour ago → confidence decays only
    // slightly from full. The 7-day TTL anchor gives:
    // confidence = 100 × 4^(-1h/7d) ≈ 99.
    let dir = make_project_root();
    let now = now_secs();
    let pending_ts = now - 7200.0;
    let triaged_ts = now - 3600.0; // 1 hour ago
    write_ledger(
        dir.path(),
        "test-health",
        &[
            pending_entry(pending_ts, "test-health"),
            triaged_entry(triaged_ts, pending_ts, "test-health"),
        ],
    );
    let cmdb = analyze_domain_calibration(dir.path().to_str().unwrap()).await;
    // Both pending superseded → 0 open. Score = 100.
    assert_eq!(cmdb["score"], 100, "all pending triaged → 0 open → score 100");
    let conf = cmdb["confidence"].as_u64().unwrap();
    assert!(
        conf >= 95,
        "1h-old triage should yield confidence ≥ 95; got {conf}"
    );
    assert_eq!(cmdb["has_ever_fired"], true);
    assert_eq!(cmdb["open_pending_total"], 0);
    assert_eq!(cmdb["triaged_total"], 1);
}

#[tokio::test]
async fn pending_plus_ancient_triaged_decays_confidence() {
    // Triaged 30 days ago → far past the 7-day TTL anchor.
    // confidence = 100 × 4^(-30/7) ≈ 0.34, clamps to 1.
    let dir = make_project_root();
    let now = now_secs();
    let pending_ts = now - (30.0 + 1.0) * 86400.0; // 31 days ago
    let triaged_ts = now - 30.0 * 86400.0; // 30 days ago
    write_ledger(
        dir.path(),
        "test-health",
        &[
            pending_entry(pending_ts, "test-health"),
            triaged_entry(triaged_ts, pending_ts, "test-health"),
        ],
    );
    let cmdb = analyze_domain_calibration(dir.path().to_str().unwrap()).await;
    let conf = cmdb["confidence"].as_u64().unwrap();
    assert!(
        conf <= 5,
        "30-day-old triage should yield confidence ≤ 5; got {conf}"
    );
    assert_eq!(cmdb["score"], 100, "all triaged → no open → score 100");
}

#[tokio::test]
async fn multi_domain_aggregation() {
    // Two ledger files with different domains. Sensor aggregates
    // open counts across both; domains_scanned reports 2; triaged_total
    // counts both.
    let dir = make_project_root();
    let now = now_secs();
    let pending_th = now - 3600.0;
    let pending_cq = now - 1800.0;
    let triaged_cq = now - 600.0;
    write_ledger(
        dir.path(),
        "test-health",
        &[pending_entry(pending_th, "test-health")],
    );
    write_ledger(
        dir.path(),
        "code-quality",
        &[
            pending_entry(pending_cq, "code-quality"),
            triaged_entry(triaged_cq, pending_cq, "code-quality"),
        ],
    );
    let cmdb = analyze_domain_calibration(dir.path().to_str().unwrap()).await;
    // Open: only test-health's pending. Score = 100 - 10 = 90.
    assert_eq!(cmdb["score"], 90);
    let scanned = cmdb["domains_scanned"].as_array().unwrap();
    assert_eq!(scanned.len(), 2);
    assert_eq!(cmdb["open_pending_total"], 1);
    assert_eq!(cmdb["triaged_total"], 1);
    let domains_open = cmdb["domains_with_open_pending"].as_array().unwrap();
    assert_eq!(domains_open.len(), 1);
    assert_eq!(domains_open[0], "test-health");
    // Confidence: has_ever_fired=true, last_triage 10 minutes ago
    // → high (close to 100).
    let conf = cmdb["confidence"].as_u64().unwrap();
    assert!(
        conf >= 95,
        "fresh triage → high confidence; got {conf}"
    );
}

#[tokio::test]
async fn rotation_filename_recognized() {
    // §17.7 rotation: <domain>-calibration-ledger-<year>.jsonl.
    // Sensor reader globs *-calibration-ledger*.jsonl so rotated
    // files are read transparently.
    let dir = make_project_root();
    let now = now_secs();
    let path = dir
        .path()
        .join(".claude")
        .join("brain")
        .join("test-health-calibration-ledger-2025.jsonl");
    let entries = vec![pending_entry(now - 3600.0, "test-health")];
    let lines: Vec<String> = entries
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();

    let cmdb = analyze_domain_calibration(dir.path().to_str().unwrap()).await;
    // Sensor sees the rotated file, treats it as test-health's ledger.
    assert_eq!(cmdb["open_pending_total"], 1);
    assert_eq!(cmdb["has_ever_fired"], true);
    let scanned = cmdb["domains_scanned"].as_array().unwrap();
    assert_eq!(scanned.len(), 1);
    assert_eq!(scanned[0], "test-health");
}

#[tokio::test]
async fn malformed_lines_silently_skipped() {
    // §17.2 + writer convention: malformed lines are logged and
    // skipped, not propagated. Sensor must not crash on bad data.
    let dir = make_project_root();
    let now = now_secs();
    let path = dir
        .path()
        .join(".claude")
        .join("brain")
        .join("test-health-calibration-ledger.jsonl");
    let valid = serde_json::to_string(&pending_entry(now - 3600.0, "test-health")).unwrap();
    let content = format!("{}\nthis is not JSON {{\n", valid);
    std::fs::write(&path, content).unwrap();

    let cmdb = analyze_domain_calibration(dir.path().to_str().unwrap()).await;
    // Valid entry is counted; malformed line is silently dropped.
    assert_eq!(cmdb["open_pending_total"], 1);
}

// ─── E-B2-2 C8 — On-disk fixture sensor behavior ──────────────────────
//
// The synthetic tests above use `now_secs()`-anchored timestamps so
// confidence math is deterministic relative to "now". The fixtures are
// fixed-anchor (`1745000000.0`, ~2025-04) — older than any real "now"
// the test will run at, so confidence values are arbitrary. These
// fixture tests therefore only assert STRUCTURAL invariants
// (open_pending count, triaged count, has_ever_fired). Confidence is
// covered by the synthetic suite.

mod test_support;
use test_support::locate_calibration_fixture;

/// Stage a fixture file into a tempdir's
/// `<root>/.claude/brain/<domain>-calibration-ledger.jsonl`. Returns
/// the tempdir for the test to consume.
fn stage_fixture(fixture_name: &str, target_domain: &str) -> TempDir {
    let dir = make_project_root();
    let src = locate_calibration_fixture(fixture_name);
    let dst = dir
        .path()
        .join(".claude")
        .join("brain")
        .join(format!("{target_domain}-calibration-ledger.jsonl"));
    std::fs::copy(&src, &dst).unwrap_or_else(|e| {
        panic!(
            "stage fixture {} -> {}: {e}",
            src.display(),
            dst.display()
        )
    });
    dir
}

#[tokio::test]
async fn fixture_pending_only_yields_open_count_1() {
    // Fixture (a): one open pending entry.
    let dir = stage_fixture("calibration-ledger-pending-only.jsonl", "test-health");
    let cmdb = analyze_domain_calibration(dir.path().to_str().unwrap()).await;
    assert_eq!(cmdb["open_pending_total"], 1);
    assert_eq!(cmdb["triaged_total"], 0);
    assert_eq!(cmdb["has_ever_fired"], true);
    let scanned = cmdb["domains_scanned"].as_array().expect("array");
    assert_eq!(scanned.len(), 1);
    assert_eq!(scanned[0], "test-health");
    let domains_open = cmdb["domains_with_open_pending"].as_array().unwrap();
    assert_eq!(domains_open.len(), 1);
    assert_eq!(domains_open[0], "test-health");
}

#[tokio::test]
async fn fixture_pending_triaged_clears_open_count() {
    // Fixture (b): pending + triaged supersede pair → 0 open, 1 triaged.
    let dir = stage_fixture("calibration-ledger-pending-triaged.jsonl", "test-health");
    let cmdb = analyze_domain_calibration(dir.path().to_str().unwrap()).await;
    assert_eq!(
        cmdb["open_pending_total"], 0,
        "triaged entry should supersede the pending"
    );
    assert_eq!(cmdb["triaged_total"], 1);
    assert_eq!(cmdb["has_ever_fired"], true);
    let domains_open = cmdb["domains_with_open_pending"].as_array().unwrap();
    assert!(
        domains_open.is_empty(),
        "no open pending after triage; got {:?}",
        domains_open
    );
}

#[tokio::test]
async fn fixture_malformed_line_silently_skipped() {
    // Fixture (c): one valid pending + one corrupted line. §17.2
    // invariant — sensor MUST NOT crash; malformed line silently
    // skipped; valid entry counted.
    let dir = stage_fixture("calibration-ledger-malformed.jsonl", "test-health");
    let cmdb = analyze_domain_calibration(dir.path().to_str().unwrap()).await;
    assert_eq!(
        cmdb["open_pending_total"], 1,
        "valid entry counted; malformed line skipped"
    );
    assert_eq!(cmdb["has_ever_fired"], true);
}
