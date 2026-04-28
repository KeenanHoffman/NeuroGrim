//! E-B2-7 C6 — `analyze_federated_patterns` behavioral tests.
//!
//! Mirrors the E-B2-6 `operator_calibration_sensor_behavior.rs` shape:
//! synthetic-`TempDir` fixtures populated programmatically (JSONL written
//! line-by-line + brain-registry.json synthesized when needed), then observed
//! via the public `analyze_federated_patterns` entry point's CMDB envelope
//! output.
//!
//! Coverage (12 cases including the BR-5 privacy regression pin and the Q9
//! recursion-guard pin):
//!
//!  1. `missing_ledger_returns_advisory_floor` — no ledger file at all;
//!     `score == 100`, `low_confidence == true`, all counters 0, no error
//!     finding (legitimate absence).
//!  2. `empty_ledger_low_confidence` — empty `.jsonl` file present.
//!  3. `only_received_rows_aggregates_correctly` — 5 received rows from 2
//!     peers; `total_received_count == 5`, `peer_breakdown.len() == 2`.
//!  4. `only_emitted_rows_aggregates_correctly` — 3 emitted rows;
//!     `total_emitted_count == 3`.
//!  5. `mixed_received_and_emitted` — both row kinds; correct
//!     disambiguation by `entry_kind`.
//!  6. `dropped_received_rows_counted_in_breakdown` — 3 dropped received
//!     rows with mixed dropped_reason values; `total_dropped_count == 3`,
//!     `dropped_by_reason.<reason>` correct.
//!  7. `high_drop_rate_finding_fires_q17` — 10 received rows with 6
//!     dropped → `federated_patterns:high_drop_rate` finding present.
//!  8. `peer_inactive_30d_finding_fires` — registry declares a peer +
//!     ledger has NO activity for that peer within 30 days →
//!     `federated_patterns:peer_inactive_30d:<peer-hash>` fires.
//!  9. `malformed_jsonl_lines_skipped_with_summary` — valid + invalid
//!     lines; sensor returns successfully; summary finding mentions count.
//! 10. `aggregation_only_export_privacy_pin` — **THE PRIVACY REGRESSION
//!     PIN.** Per-row payload data must NOT leak into the breakdown's
//!     top-level fields.
//! 11. `recursion_guard_no_command_in_validator_span` — Q9 source-level
//!     lock. File-level grep for forbidden shell-out patterns.
//! 12. `live_neurogrim_smoke` — invoke against actual `D:/Brains/NeuroGrim/`;
//!     verify it doesn't panic. Ledger is absent — expect
//!     `low_confidence: true`, `total_received_count == 0`.

use serde_json::Value;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use neurogrim_sensory::federated_patterns::analyze_federated_patterns;

// ── Fixture helpers ─────────────────────────────────────────────────────────

fn make_brain_root() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

/// Create the `.claude/brain/` directory at `root` and return the path to
/// the pattern-aggregation-ledger JSONL file (without writing it).
fn ledger_path(root: &Path) -> PathBuf {
    let dir = root.join(".claude").join("brain");
    std::fs::create_dir_all(&dir).expect("create .claude/brain dir");
    dir.join("pattern-aggregation-ledger.jsonl")
}

/// Write the given lines as a JSONL ledger at the canonical location. Joins
/// lines with `\n`; the writer adds a trailing newline.
fn write_ledger(root: &Path, lines: &[&str]) {
    let path = ledger_path(root);
    let mut buf = String::new();
    for line in lines {
        buf.push_str(line);
        buf.push('\n');
    }
    std::fs::write(&path, buf).expect("write pattern-aggregation-ledger.jsonl");
}

/// Synthesize a minimal `brain-registry.json` with the given child names.
/// Mirrors the shape of `D:/Brains/NeuroGrim/.claude/brain-registry.json:children`
/// — each child is an object keyed by name. Used to drive the
/// `peer_inactive_30d` and `no_active_peers` paths.
fn write_registry_with_children(root: &Path, child_names: &[&str]) {
    let claude_dir = root.join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("create .claude dir");
    let path = claude_dir.join("brain-registry.json");

    let mut children = serde_json::Map::new();
    for name in child_names {
        children.insert(
            (*name).to_string(),
            serde_json::json!({
                "display_name": format!("synthetic peer {name}"),
                "a2a_endpoint": "http://127.0.0.1:9999/a2a/v1/",
                "interface_version": "1",
                "weight": 1.0,
                "enabled": true
            }),
        );
    }
    let registry = serde_json::json!({
        "meta": {
            "schema_version": "2.0",
            "updated_by": "test-fixture",
        },
        "config": {
            "domain_weights": {},
            "domain_definitions": {},
            "children": serde_json::Value::Object(children)
        }
    });
    std::fs::write(&path, registry.to_string()).expect("write brain-registry.json");
}

/// Author a synthetic ReceivedEntry JSON line. `ts` lets the caller pin a
/// row inside or outside the 7/30-day windows.
fn received_row(
    ts: &str,
    from_brain_id: &str,
    pattern_kind: &str,
    invocation_id: &str,
) -> String {
    format!(
        r#"{{"schema_version":"1","entry_kind":"received","ts":"{ts}","peer_brain_id":"local-brain-hash","from_brain_id":"{from_brain_id}","envelope_message_id":"env-{invocation_id}","payload":{{"pattern_kind":"{pattern_kind}","invocation_id":"{invocation_id}"}}}}"#
    )
}

/// Author a synthetic ReceivedEntry JSON line with a `dropped_reason`. Used
/// to exercise the high-drop-rate finding + per-reason breakdown.
fn dropped_received_row(
    ts: &str,
    from_brain_id: &str,
    pattern_kind: &str,
    invocation_id: &str,
    dropped_reason: &str,
) -> String {
    format!(
        r#"{{"schema_version":"1","entry_kind":"received","ts":"{ts}","peer_brain_id":"local-brain-hash","from_brain_id":"{from_brain_id}","envelope_message_id":"env-{invocation_id}","payload":{{"pattern_kind":"{pattern_kind}","invocation_id":"{invocation_id}"}},"dropped_reason":"{dropped_reason}"}}"#
    )
}

/// Author a synthetic EmittedEntry JSON line.
fn emitted_row(
    ts: &str,
    to_brain_id: &str,
    pattern_kind: &str,
    invocation_id: &str,
) -> String {
    format!(
        r#"{{"schema_version":"1","entry_kind":"emitted","ts":"{ts}","peer_brain_id":"local-brain-hash","to_brain_id":"{to_brain_id}","envelope_message_id":"env-{invocation_id}","payload":{{"pattern_kind":"{pattern_kind}","invocation_id":"{invocation_id}"}}}}"#
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

/// All `federated_patterns:*` findings.
fn fp_findings(result: &Value) -> Vec<&Value> {
    findings_by_prefix(result, "federated_patterns:")
}

/// A timestamp string for "now-ish" — well within the 7-day window. Uses
/// chrono so the fixture moves with the actual sensor's window cutoff.
fn ts_recent() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// A timestamp string for ~45 days ago — outside both 7-day and 30-day
/// windows. Used by `peer_inactive_30d_finding_fires`.
fn ts_forty_five_days_ago() -> String {
    (chrono::Utc::now() - chrono::Duration::days(45)).to_rfc3339()
}

/// Locate the `federated_patterns.rs` source file from the crate's manifest
/// dir. Used by the recursion-guard test.
fn locate_federated_patterns_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("federated_patterns.rs")
}

// ── 1. Missing ledger → advisory floor ──────────────────────────────────────

#[tokio::test]
async fn missing_ledger_returns_advisory_floor() {
    let tmp = make_brain_root();
    // Do NOT create .claude/brain/pattern-aggregation-ledger.jsonl.

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;

    // Score is ALWAYS 100 per Q10 — federation is INFORMATION, not health.
    assert_eq!(result["score"], 100, "missing ledger must return advisory floor 100");

    // Breakdown shape.
    let bd = &result["federated_patterns_breakdown"];
    assert!(bd.is_object(), "breakdown must be present");
    assert_eq!(bd["ledger_present"], false);
    assert_eq!(bd["total_received_count"], 0);
    assert_eq!(bd["total_emitted_count"], 0);
    assert_eq!(bd["total_dropped_count"], 0);
    assert_eq!(bd["low_confidence"], true);
    assert_eq!(bd["declared_peer_count"], 0);
    assert_eq!(bd["active_peer_count"], 0);
    assert!(bd["ledger_path"].is_null(), "ledger_path must be JSON null when absent");
    // dropped_by_reason should be an empty object, peer_breakdown / pattern_kind_breakdown
    // empty arrays.
    assert!(bd["dropped_by_reason"].is_object());
    assert_eq!(bd["dropped_by_reason"].as_object().unwrap().len(), 0);
    assert!(bd["peer_breakdown"].is_array());
    assert_eq!(bd["peer_breakdown"].as_array().unwrap().len(), 0);
    assert!(bd["pattern_kind_breakdown"].is_array());

    // Envelope canonical fields.
    assert_eq!(result["meta"]["updated_by"], "federated-patterns");
    assert_eq!(result["meta"]["schema_version"], "1");

    // Only the low_confidence finding should fire (no peers declared, no
    // activity, so no_active_peers does NOT fire — Q17 lock requires
    // received+emitted > 0 to fire that finding).
    let low_conf = findings_by_prefix(&result, "federated_patterns:low_confidence");
    assert_eq!(low_conf.len(), 1, "expected 1 low_confidence finding on missing ledger");
    let no_active = findings_by_prefix(&result, "federated_patterns:no_active_peers");
    assert!(
        no_active.is_empty(),
        "no_active_peers must NOT fire when there's no activity (advisory absence)"
    );

    // No high_drop_rate, no peer_inactive_30d, no malformed_lines.
    assert!(findings_by_prefix(&result, "federated_patterns:high_drop_rate").is_empty());
    assert!(findings_by_prefix(&result, "federated_patterns:peer_inactive_30d").is_empty());
    assert!(findings_by_prefix(&result, "federated_patterns:malformed_lines").is_empty());
}

// ── 2. Empty ledger file → low-confidence advisory floor ────────────────────

#[tokio::test]
async fn empty_ledger_low_confidence() {
    let tmp = make_brain_root();
    write_ledger(tmp.path(), &[]);

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;

    assert_eq!(result["score"], 100, "empty ledger → advisory floor (federation is observability)");
    let bd = &result["federated_patterns_breakdown"];
    assert_eq!(bd["ledger_present"], true);
    assert_eq!(bd["total_received_count"], 0);
    assert_eq!(bd["total_emitted_count"], 0);
    assert_eq!(bd["low_confidence"], true);

    // low_confidence finding must fire.
    let low_conf = findings_by_prefix(&result, "federated_patterns:low_confidence");
    assert_eq!(low_conf.len(), 1);
    assert_eq!(low_conf[0]["points"], 0);
    assert_eq!(low_conf[0]["status"], "neutral");
}

// ── 3. Received-only rows aggregate correctly ───────────────────────────────

#[tokio::test]
async fn only_received_rows_aggregates_correctly() {
    let tmp = make_brain_root();
    let recent = ts_recent();
    let lines = vec![
        received_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-0"),
        received_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-1"),
        received_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-2"),
        received_row(&recent, "peer-beta-hash", "vigilance-pattern", "inv-3"),
        received_row(&recent, "peer-beta-hash", "vigilance-pattern", "inv-4"),
    ];
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;

    let bd = &result["federated_patterns_breakdown"];
    assert_eq!(bd["total_received_count"], 5);
    assert_eq!(bd["total_emitted_count"], 0);
    assert_eq!(bd["total_dropped_count"], 0);
    assert_eq!(bd["low_confidence"], false);
    assert_eq!(bd["active_peer_count"], 2);

    let peer_breakdown = bd["peer_breakdown"].as_array().expect("peer_breakdown is an array");
    assert_eq!(peer_breakdown.len(), 2, "expected 2 distinct peers in breakdown");
    // BTreeMap order: peer-alpha-hash before peer-beta-hash alphabetically.
    let alpha = peer_breakdown.iter().find(|p| p["peer_brain_id"] == "peer-alpha-hash").unwrap();
    assert_eq!(alpha["received_count"], 3);
    assert_eq!(alpha["emitted_count"], 0);
    let beta = peer_breakdown.iter().find(|p| p["peer_brain_id"] == "peer-beta-hash").unwrap();
    assert_eq!(beta["received_count"], 2);
    assert_eq!(beta["emitted_count"], 0);

    // pattern_kind_breakdown: single entry for vigilance-pattern.
    let pkb = bd["pattern_kind_breakdown"].as_array().expect("pattern_kind_breakdown is an array");
    assert_eq!(pkb.len(), 1);
    assert_eq!(pkb[0]["pattern_kind"], "vigilance-pattern");
    assert_eq!(pkb[0]["received_count"], 5);
    assert_eq!(pkb[0]["emitted_count"], 0);
}

// ── 4. Emitted-only rows aggregate correctly ────────────────────────────────

#[tokio::test]
async fn only_emitted_rows_aggregates_correctly() {
    let tmp = make_brain_root();
    let recent = ts_recent();
    let lines = vec![
        emitted_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-0"),
        emitted_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-1"),
        emitted_row(&recent, "peer-beta-hash", "vigilance-pattern", "inv-2"),
    ];
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;

    let bd = &result["federated_patterns_breakdown"];
    assert_eq!(bd["total_received_count"], 0);
    assert_eq!(bd["total_emitted_count"], 3);
    assert_eq!(bd["low_confidence"], false);
    assert_eq!(bd["active_peer_count"], 2);

    let peer_breakdown = bd["peer_breakdown"].as_array().unwrap();
    let alpha = peer_breakdown.iter().find(|p| p["peer_brain_id"] == "peer-alpha-hash").unwrap();
    assert_eq!(alpha["emitted_count"], 2);
    let beta = peer_breakdown.iter().find(|p| p["peer_brain_id"] == "peer-beta-hash").unwrap();
    assert_eq!(beta["emitted_count"], 1);
}

// ── 5. Mixed received + emitted disambiguation ──────────────────────────────

#[tokio::test]
async fn mixed_received_and_emitted() {
    let tmp = make_brain_root();
    let recent = ts_recent();
    let lines = vec![
        received_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-0"),
        emitted_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-1"),
        received_row(&recent, "peer-beta-hash", "vigilance-pattern", "inv-2"),
        emitted_row(&recent, "peer-beta-hash", "vigilance-pattern", "inv-3"),
        received_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-4"),
    ];
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;

    let bd = &result["federated_patterns_breakdown"];
    assert_eq!(bd["total_received_count"], 3);
    assert_eq!(bd["total_emitted_count"], 2);
    assert_eq!(bd["active_peer_count"], 2);

    let peer_breakdown = bd["peer_breakdown"].as_array().unwrap();
    let alpha = peer_breakdown.iter().find(|p| p["peer_brain_id"] == "peer-alpha-hash").unwrap();
    assert_eq!(alpha["received_count"], 2);
    assert_eq!(alpha["emitted_count"], 1);
    let beta = peer_breakdown.iter().find(|p| p["peer_brain_id"] == "peer-beta-hash").unwrap();
    assert_eq!(beta["received_count"], 1);
    assert_eq!(beta["emitted_count"], 1);

    // pattern_kind_breakdown: vigilance-pattern with received=3, emitted=2.
    let pkb = bd["pattern_kind_breakdown"].as_array().unwrap();
    assert_eq!(pkb.len(), 1);
    assert_eq!(pkb[0]["pattern_kind"], "vigilance-pattern");
    assert_eq!(pkb[0]["received_count"], 3);
    assert_eq!(pkb[0]["emitted_count"], 2);
}

// ── 6. Dropped received rows counted in breakdown ───────────────────────────

#[tokio::test]
async fn dropped_received_rows_counted_in_breakdown() {
    let tmp = make_brain_root();
    let recent = ts_recent();
    let lines = vec![
        dropped_received_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-0", "rate-limit-exceeded"),
        dropped_received_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-1", "recursion-guard"),
        dropped_received_row(&recent, "peer-beta-hash", "vigilance-pattern", "inv-2", "rate-limit-exceeded"),
    ];
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;

    let bd = &result["federated_patterns_breakdown"];
    assert_eq!(bd["total_received_count"], 3, "all 3 dropped rows still count toward received_count");
    assert_eq!(bd["total_dropped_count"], 3);
    assert_eq!(bd["dropped_by_reason"]["rate-limit-exceeded"], 2);
    assert_eq!(bd["dropped_by_reason"]["recursion-guard"], 1);
}

// ── 7. high_drop_rate finding fires (Q17) ──────────────────────────────────

#[tokio::test]
async fn high_drop_rate_finding_fires_q17() {
    let tmp = make_brain_root();
    let recent = ts_recent();

    // 10 received rows total. 6 of them dropped (60%, > 50% threshold).
    let mut lines: Vec<String> = Vec::new();
    // 4 accepted received rows.
    for i in 0..4 {
        lines.push(received_row(
            &recent,
            "peer-alpha-hash",
            "vigilance-pattern",
            &format!("inv-good-{i}"),
        ));
    }
    // 6 dropped received rows.
    for i in 0..6 {
        lines.push(dropped_received_row(
            &recent,
            "peer-alpha-hash",
            "vigilance-pattern",
            &format!("inv-drop-{i}"),
            "rate-limit-exceeded",
        ));
    }
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;

    let bd = &result["federated_patterns_breakdown"];
    assert_eq!(bd["total_received_count"], 10);
    assert_eq!(bd["total_dropped_count"], 6);

    // high_drop_rate finding must fire.
    let hdr = findings_by_prefix(&result, "federated_patterns:high_drop_rate");
    assert_eq!(
        hdr.len(),
        1,
        "expected exactly 1 high_drop_rate finding when drop ratio > 0.5; got: {:?}",
        fp_findings(&result)
    );
    assert_eq!(hdr[0]["points"], 0);
    assert_eq!(hdr[0]["status"], "neutral");
    let detail = hdr[0]["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("60") || detail.contains("6 dropped"),
        "high_drop_rate detail should mention the ratio and counts; got: `{detail}`"
    );
}

// ── 8. peer_inactive_30d finding fires ──────────────────────────────────────

#[tokio::test]
async fn peer_inactive_30d_finding_fires() {
    let tmp = make_brain_root();

    // Registry declares one peer.
    write_registry_with_children(tmp.path(), &["python-starter"]);

    // Ledger has only OLD activity (45 days ago) for that peer — outside the
    // 30-day window. Plus a fresh row from a different peer to keep
    // low_confidence false.
    let recent = ts_recent();
    let old = ts_forty_five_days_ago();
    let lines = vec![
        received_row(&old, "python-starter", "vigilance-pattern", "inv-old-0"),
        received_row(&recent, "some-other-peer", "vigilance-pattern", "inv-fresh"),
    ];
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;

    // peer_inactive_30d finding must fire for the declared peer (python-starter)
    // — last_seen is 45 days ago, last_emit is None (never emitted to).
    let inactive = findings_by_prefix(&result, "federated_patterns:peer_inactive_30d");
    assert_eq!(
        inactive.len(),
        1,
        "expected exactly 1 peer_inactive_30d finding for declared peer; got: {:?}",
        fp_findings(&result)
    );
    let name = inactive[0]["name"].as_str().unwrap_or("");
    assert!(
        name.starts_with("federated_patterns:peer_inactive_30d:"),
        "finding name must include peer-id discriminator; got: `{name}`"
    );
    // The trailing token should be derived from `python-starter` (or a
    // truncated form) — short_peer_token strips non-alnum-/_, takes 12 chars.
    assert!(
        name.contains("python-start"),
        "finding name should embed the (short) peer id; got: `{name}`"
    );

    // Sanity: declared_peer_count == 1, active_peer_count == 2.
    let bd = &result["federated_patterns_breakdown"];
    assert_eq!(bd["declared_peer_count"], 1);
    assert_eq!(bd["active_peer_count"], 2);
}

// ── 9. Malformed JSONL lines counted via summary finding ────────────────────

#[tokio::test]
async fn malformed_jsonl_lines_skipped_with_summary() {
    let tmp = make_brain_root();
    let recent = ts_recent();
    let lines = vec![
        received_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-0"),
        emitted_row(&recent, "peer-alpha-hash", "vigilance-pattern", "inv-1"),
        "not json".to_string(),
        received_row(&recent, "peer-beta-hash", "vigilance-pattern", "inv-2"),
        "{broken".to_string(),
    ];
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;

    let bd = &result["federated_patterns_breakdown"];
    assert_eq!(
        bd["total_received_count"], 2,
        "valid received rows are counted; malformed lines silently skipped"
    );
    assert_eq!(bd["total_emitted_count"], 1);

    let malformed = findings_by_prefix(&result, "federated_patterns:malformed_lines");
    assert_eq!(
        malformed.len(),
        1,
        "expected exactly 1 malformed_lines summary finding; got: {:?}",
        fp_findings(&result)
    );
    let detail = malformed[0]["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains('2'),
        "malformed_lines detail should mention the count `2`; got: `{detail}`"
    );
}

// ── 10. Aggregation-only export — BR-5 PRIVACY REGRESSION PIN ───────────────

// Q5+E6-1 + Q8 echo: aggregation-only export. If a future change leaks
// per-row payload data into the breakdown's top-level (e.g., adding
// "received_payloads" array), this test catches it.
//
// This is the privacy-mitigation regression pin for the federated-patterns
// sensor. The CMDB output emits AGGREGATE TOTALS + per-peer breakdowns +
// per-pattern-kind breakdowns ONLY. Per-row payload data (like
// `invocation_id` values stored inside the payload) MUST NOT appear in any
// top-level breakdown field; the only place the test invocation_ids are
// allowed to appear is via the aggregate-grouping dimensions (e.g.,
// peer_brain_id keying). This test searches for embedded test invocation_ids
// in the serialized envelope and asserts ZERO matches — the test ids are
// chosen to be distinct, recognizable, and impossible to confuse with any
// legitimate aggregation key.
#[tokio::test]
async fn aggregation_only_export_privacy_pin() {
    let tmp = make_brain_root();
    let recent = ts_recent();

    // Author 30 received + emitted rows referencing 30 distinct test
    // invocation_ids. The invocation_ids are stored INSIDE the payload
    // object — exactly the place a future change might be tempted to
    // pass through into the breakdown.
    let mut lines: Vec<String> = Vec::new();
    let mut sensitive_ids: Vec<String> = Vec::new();
    for i in 0..15 {
        let id = format!("PRIVATE_RECEIVED_INVOCATION_ID_{i}");
        sensitive_ids.push(id.clone());
        lines.push(received_row(
            &recent,
            "peer-alpha-hash",
            "vigilance-pattern",
            &id,
        ));
    }
    for i in 0..15 {
        let id = format!("PRIVATE_EMITTED_INVOCATION_ID_{i}");
        sensitive_ids.push(id.clone());
        lines.push(emitted_row(
            &recent,
            "peer-beta-hash",
            "vigilance-pattern",
            &id,
        ));
    }
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_ledger(tmp.path(), &line_refs);

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;

    // Serialize the full envelope to a string and search for sensitive
    // invocation_id values. ZERO matches required.
    let serialized = serde_json::to_string(&result).expect("serialize result");
    for id in &sensitive_ids {
        assert!(
            !serialized.contains(id.as_str()),
            "BR-5 privacy regression: per-row payload id `{id}` leaked into the federated_patterns \
             CMDB output. The sensor MUST emit aggregate totals + per-peer + per-pattern-kind \
             breakdowns only. Re-opening per-row export requires a charter-level BR-5 \
             conversation."
        );
    }

    // Sanity: the sensor DID see the rows in aggregate form.
    let bd = &result["federated_patterns_breakdown"];
    assert_eq!(bd["total_received_count"], 15);
    assert_eq!(bd["total_emitted_count"], 15);
    assert_eq!(bd["active_peer_count"], 2);
}

// ── 11. Recursion guard — Q9 source-level lock ──────────────────────────────

/// The validator MUST be pure file-read + JSONL-parse + count + JSON-output.
/// NO shell-out. NO `std::process::Command`. NO `Stdio`. NO `std::process`.
/// This test reads the source of `federated_patterns.rs` and grep-checks for
/// forbidden patterns within the entire file. Mirrors the recursion-guard
/// pin in `operator_calibration_sensor_behavior.rs` and
/// `trust_budget_sensor_behavior.rs`.
#[test]
fn recursion_guard_no_command_in_validator_span() {
    let path = locate_federated_patterns_source();
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
            "Q9 recursion-guard violated: `federated_patterns.rs` contains forbidden \
             shell-execution pattern `{pat}`. The sensor must be pure file-read + \
             JSONL-parse + count + JSON-output. See E-B2-7 plan Q9."
        );
    }

    // Defense-in-depth: also reject patterns that suggest the sensor is
    // donning a hat or invoking the Skill / Bash / Edit / Write tool surface
    // — same defensive posture as `hat_contract_sensor_behavior.rs`.
    let suspicious = ["Skill::", "execute_skill", "invoke_tool"];
    for pat in suspicious.iter() {
        assert!(
            !source.contains(pat),
            "Q9 recursion-guard tripped on suspicious pattern `{pat}` inside \
             `federated_patterns.rs`. The sensor should not invoke tools or skills — \
             surface the observation to the operator instead."
        );
    }
}

// ── 12. Live NeuroGrim smoke ────────────────────────────────────────────────

/// Locate the live NeuroGrim Brain root from the crate's manifest dir.
/// Mirrors `operator_calibration_sensor_behavior.rs::locate_neurogrim_brain_root`.
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
/// pattern-aggregation-ledger does NOT exist yet (no federation has run).
/// Expect `low_confidence: true`, `total_received_count == 0`. Don't pin
/// finding counts on anything beyond structural sanity — the live ledger is
/// a moving target once federation is exercised.
#[tokio::test]
async fn live_neurogrim_smoke() {
    let Some(brain_root) = locate_neurogrim_brain_root() else {
        eprintln!("skip: live NeuroGrim brain root not reachable");
        return;
    };

    let result = analyze_federated_patterns(brain_root.to_str().unwrap()).await;

    // Envelope is well-formed.
    assert_eq!(result["meta"]["updated_by"], "federated-patterns");
    assert_eq!(result["meta"]["schema_version"], "1");

    // Score is ALWAYS 100 per Q10.
    assert_eq!(result["score"], 100);

    let bd = &result["federated_patterns_breakdown"];
    assert!(bd.is_object(), "breakdown must be present");
    let received = bd["total_received_count"].as_u64().expect("total_received_count is a number");
    let emitted = bd["total_emitted_count"].as_u64().expect("total_emitted_count is a number");

    // Per the brief: federation hasn't run yet, so received_count == 0,
    // emitted_count == 0, low_confidence == true.
    assert_eq!(
        received, 0,
        "expected zero received on live NeuroGrim ledger (no federation run yet); got: {received}"
    );
    assert_eq!(
        emitted, 0,
        "expected zero emitted on live NeuroGrim ledger (no federation run yet); got: {emitted}"
    );
    assert_eq!(
        bd["low_confidence"], true,
        "expected low_confidence == true (no federation activity)"
    );

    eprintln!(
        "live_neurogrim_smoke: received={received}, emitted={emitted}, low_confidence={}, \
         declared_peer_count={}",
        bd["low_confidence"], bd["declared_peer_count"]
    );
    let bd_present = bd["ledger_present"].as_bool().unwrap_or(false);
    eprintln!("  ledger_present = {bd_present}");
}

// ── 13. Cross-peer co-occurrence (v3.1 E-V31-E E1.2) ────────────────────────

/// Author a received row carrying a populated feature_vector. Distinct from
/// `received_row` which produces a payload without feature_vector — the
/// cross-peer detection requires both `severity_class` and
/// `observation_window_days` to be present.
fn received_row_with_feature_vector(
    ts: &str,
    from_brain_id: &str,
    severity_class: &str,
    observation_window_days: u64,
    invocation_id: &str,
) -> String {
    format!(
        r#"{{"schema_version":"1","entry_kind":"received","ts":"{ts}","peer_brain_id":"local-brain-hash","from_brain_id":"{from_brain_id}","envelope_message_id":"env-{invocation_id}","payload":{{"pattern_kind":"vigilance-pattern","invocation_id":"{invocation_id}","feature_vector":{{"numeric_count":3,"severity_class":"{severity_class}","observation_window_days":{observation_window_days}}}}}}}"#
    )
}

/// POSITIVE case: ≥2 distinct anonymized origins emit vigilance-pattern
/// findings sharing a feature_vector signature within the 7-day window.
/// Expectation: a `federated_patterns:cross_peer_co_occurrence` finding
/// fires; its detail mentions the peer count and severity class
/// aggregately (no per-peer hashes).
#[tokio::test]
async fn cross_peer_co_occurrence_fires_with_two_distinct_peers() {
    let tmp = make_brain_root();
    let recent = ts_recent();
    write_ledger(
        tmp.path(),
        &[
            &received_row_with_feature_vector(&recent, "peer-alpha-hash", "medium", 7, "i1"),
            &received_row_with_feature_vector(&recent, "peer-beta-hash", "medium", 7, "i2"),
        ],
    );

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;
    let findings = findings_by_prefix(&result, "federated_patterns:cross_peer_co_occurrence");
    assert_eq!(
        findings.len(),
        1,
        "expected exactly one cross-peer co-occurrence finding; got {findings:?}"
    );
    let detail = findings[0]["detail"].as_str().expect("detail present");
    assert!(
        detail.contains("2 distinct anonymized origins"),
        "detail must reference peer count aggregately; got: {detail}"
    );
    assert!(
        detail.contains("medium"),
        "detail must reference shared severity_class; got: {detail}"
    );
}

/// NEGATIVE case: only one distinct origin emits multiple rows. The finding
/// MUST NOT fire — co-occurrence requires ≥2 distinct origins.
#[tokio::test]
async fn cross_peer_co_occurrence_does_not_fire_with_single_peer() {
    let tmp = make_brain_root();
    let recent = ts_recent();
    write_ledger(
        tmp.path(),
        &[
            &received_row_with_feature_vector(&recent, "peer-alpha-hash", "medium", 7, "i1"),
            &received_row_with_feature_vector(&recent, "peer-alpha-hash", "medium", 7, "i2"),
            &received_row_with_feature_vector(&recent, "peer-alpha-hash", "medium", 7, "i3"),
        ],
    );

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;
    let findings = findings_by_prefix(&result, "federated_patterns:cross_peer_co_occurrence");
    assert_eq!(
        findings.len(),
        0,
        "single-peer activity must not fire cross-peer; got {findings:?}"
    );
}

/// NEGATIVE case: two distinct origins but rows are OUTSIDE the 7-day
/// window. The finding MUST NOT fire — only recent activity counts.
#[tokio::test]
async fn cross_peer_co_occurrence_does_not_fire_outside_window() {
    let tmp = make_brain_root();
    let old = ts_forty_five_days_ago();
    write_ledger(
        tmp.path(),
        &[
            &received_row_with_feature_vector(&old, "peer-alpha-hash", "medium", 7, "i1"),
            &received_row_with_feature_vector(&old, "peer-beta-hash", "medium", 7, "i2"),
        ],
    );

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;
    let findings = findings_by_prefix(&result, "federated_patterns:cross_peer_co_occurrence");
    assert_eq!(
        findings.len(),
        0,
        "old activity must not fire cross-peer (rolling 7-day window); got {findings:?}"
    );
}

/// NEGATIVE case: two distinct origins emit findings but with DIFFERENT
/// feature_vector signatures (different severity classes). The finding
/// MUST NOT fire — co-occurrence requires shared signature.
#[tokio::test]
async fn cross_peer_co_occurrence_groups_by_signature() {
    let tmp = make_brain_root();
    let recent = ts_recent();
    write_ledger(
        tmp.path(),
        &[
            &received_row_with_feature_vector(&recent, "peer-alpha-hash", "medium", 7, "i1"),
            &received_row_with_feature_vector(&recent, "peer-beta-hash", "critical", 7, "i2"),
        ],
    );

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;
    let findings = findings_by_prefix(&result, "federated_patterns:cross_peer_co_occurrence");
    assert_eq!(
        findings.len(),
        0,
        "different severity_class values are different signatures; got {findings:?}"
    );
}

/// PRIVACY REGRESSION: the cross-peer finding's detail string MUST NOT
/// contain raw `from_brain_id` hashes. Only aggregate counts + closed-set
/// enum names + integer windows. Mirrors the
/// `aggregation_only_export_privacy_pin` test for the new finding kind.
#[tokio::test]
async fn cross_peer_co_occurrence_aggregate_only_no_peer_hash_leak() {
    let tmp = make_brain_root();
    let recent = ts_recent();
    write_ledger(
        tmp.path(),
        &[
            &received_row_with_feature_vector(
                &recent,
                "PRIVATE_PEER_HASH_ALPHA",
                "medium",
                7,
                "i1",
            ),
            &received_row_with_feature_vector(
                &recent,
                "PRIVATE_PEER_HASH_BETA",
                "medium",
                7,
                "i2",
            ),
            &received_row_with_feature_vector(
                &recent,
                "PRIVATE_PEER_HASH_GAMMA",
                "medium",
                7,
                "i3",
            ),
        ],
    );

    let result = analyze_federated_patterns(tmp.path().to_str().unwrap()).await;
    let findings = findings_by_prefix(&result, "federated_patterns:cross_peer_co_occurrence");
    assert_eq!(
        findings.len(),
        1,
        "expected exactly one co-occurrence finding for 3 distinct peers; got {findings:?}"
    );
    let detail = findings[0]["detail"].as_str().expect("detail present");

    // Detail should mention the COUNT (3) but never any specific peer hash.
    assert!(
        detail.contains("3 distinct anonymized origins"),
        "detail must report aggregate peer count; got: {detail}"
    );
    for hash in [
        "PRIVATE_PEER_HASH_ALPHA",
        "PRIVATE_PEER_HASH_BETA",
        "PRIVATE_PEER_HASH_GAMMA",
    ] {
        assert!(
            !detail.contains(hash),
            "BR-5 privacy regression: cross-peer co-occurrence finding leaked \
             individual peer hash `{hash}` into detail. The finding MUST emit \
             aggregate counts + severity + window only.",
        );
    }
}
