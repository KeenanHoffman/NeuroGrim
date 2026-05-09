//! External-content safety advisory domain (v2-Feature 6 Phase 6.4, 2026-05-09).
//!
//! Reads recent `category=external_content` rows from
//! `<project>/.claude/audit.jsonl` (the IDE writes these every time
//! `external_content_scan` runs). Aggregates by severity and emits a
//! CMDB whose score reflects the operator's recent injection-attempt
//! exposure.
//!
//! ## Scope
//!
//! - Phase 1: read existing audit rows. The IDE's
//!   `external_content_scan` Tauri command (Phase 6.1) is the producer;
//!   this is the read-side.
//! - Phase 2 (deferred): operator allowlist (per-pattern mute) lives
//!   in `.claude/external-content-allowlist.yaml` per the plan; the
//!   sensor will pre-filter findings against it.
//!
//! ## Score formula
//!
//! Start from 100 and subtract:
//! - 5 points per `block`-severity scan in the lookback window (cap 50)
//! - 1 point per `warn`-severity scan (cap 20)
//! - 0 points per `info`-severity scan (informational only)
//! - 0 points per `clean` scan (these RAISE the denominator but don't
//!   penalize the score)
//!
//! ## Why advisory (weight 0.0) for v1
//!
//! Heuristic detection has unknown false-positive rates without
//! operator data. Promotion to weighted needs ≥30 days of audit
//! history showing operators consistently confirm `block`-severity
//! findings as actual injection attempts (vs benign content that
//! merely discusses injection). Per LSP-Brains spec §15.5.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cmdb::{build_cmdb, Finding};

/// One audit row parsed from `audit.jsonl`. We only deserialize the
/// fields we use; serde tolerates extras.
#[derive(Debug, Clone, Deserialize)]
struct AuditRow {
    category: String,
    kind: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct SafetyReport {
    pub total_scans: usize,
    pub clean: usize,
    pub info: usize,
    pub warn: usize,
    pub block: usize,
    /// Top 5 most-frequent finding patterns across the window. Useful
    /// for surfacing "this operator keeps seeing zero-width characters
    /// in fetched content" without dumping every match.
    pub top_patterns: Vec<(String, usize)>,
}

/// Pure analyzer — drives off in-memory rows. The caller is
/// responsible for I/O + filtering by time window.
pub fn build_report(rows: &[AuditRow]) -> SafetyReport {
    let mut total = 0usize;
    let mut clean = 0usize;
    let mut info = 0usize;
    let mut warn = 0usize;
    let mut block = 0usize;
    let mut pattern_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for row in rows {
        if row.category != "external_content" {
            continue;
        }
        total += 1;
        match row.kind.as_str() {
            "scan_clean" => clean += 1,
            "scan_info" => info += 1,
            "scan_warn" => warn += 1,
            "scan_block" => block += 1,
            _ => {} // ignore unrecognized kinds defensively
        }
        if let Some(arr) = row.payload.get("patterns").and_then(|v| v.as_array()) {
            for p in arr {
                if let Some(s) = p.as_str() {
                    *pattern_counts.entry(s.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    let mut top: Vec<(String, usize)> = pattern_counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top.truncate(5);

    SafetyReport {
        total_scans: total,
        clean,
        info,
        warn,
        block,
        top_patterns: top,
    }
}

/// Score the report. 100 = no concerning activity; lower = more
/// recent flags. See module-level rustdoc for the formula.
pub fn score_report(report: &SafetyReport) -> u8 {
    let block_penalty = (report.block as u32 * 5).min(50);
    let warn_penalty = report.warn as u32; // already capped via .min below
    let warn_penalty = warn_penalty.min(20);
    let total_penalty = block_penalty + warn_penalty;
    let score = 100i32 - total_penalty as i32;
    score.clamp(0, 100) as u8
}

/// Top-level analyzer: read JSONL, build report, emit CMDB.
///
/// Missing `audit.jsonl` is NOT an error — it just means the IDE
/// never recorded an external-content scan in this project. The
/// sensor returns score 100 with `total_scans: 0` so the brain
/// aggregator can decide what that means.
pub fn analyze_external_content_safety(project_root: &Path) -> Value {
    let audit_path = project_root.join(".claude").join("audit.jsonl");
    let rows = read_audit_rows(&audit_path);
    let report = build_report(&rows);
    let score = score_report(&report);

    let extras = vec![
        ("total_scans", Value::from(report.total_scans)),
        ("clean", Value::from(report.clean)),
        ("info", Value::from(report.info)),
        ("warn", Value::from(report.warn)),
        ("block", Value::from(report.block)),
        (
            "top_patterns",
            serde_json::to_value(&report.top_patterns).unwrap_or(Value::Null),
        ),
    ];

    let findings = build_findings(&report);

    build_cmdb(
        "external-content-safety",
        score,
        findings,
        Some(extras),
        None,
    )
}

fn read_audit_rows(path: &Path) -> Vec<AuditRow> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in contents.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(row) = serde_json::from_str::<AuditRow>(line) {
            out.push(row);
        }
    }
    out
}

fn build_findings(report: &SafetyReport) -> Vec<Finding> {
    let mut findings = Vec::new();
    if report.block > 0 {
        findings.push(Finding {
            name: "block_severity_scans".to_string(),
            status: "flagged".to_string(),
            points: -((report.block as i32 * 5).min(50)),
            detail: Some(format!(
                "{} block-severity external-content scan(s) in window",
                report.block
            )),
        });
    }
    if report.warn > 0 {
        findings.push(Finding {
            name: "warn_severity_scans".to_string(),
            status: "flagged".to_string(),
            points: -((report.warn as i32).min(20)),
            detail: Some(format!(
                "{} warn-severity external-content scan(s) in window",
                report.warn
            )),
        });
    }
    if report.total_scans == 0 {
        findings.push(Finding {
            name: "no_scans_recorded".to_string(),
            status: "info".to_string(),
            points: 0,
            detail: Some(
                "No external-content scans found in audit.jsonl — \
                 IDE may not have processed any external content yet."
                    .to_string(),
            ),
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn row(kind: &str, patterns: &[&str]) -> AuditRow {
        AuditRow {
            category: "external_content".to_string(),
            kind: kind.to_string(),
            payload: json!({ "patterns": patterns }),
        }
    }

    #[test]
    fn empty_rows_yield_perfect_score_with_zero_scans() {
        let report = build_report(&[]);
        assert_eq!(report.total_scans, 0);
        assert_eq!(score_report(&report), 100);
    }

    #[test]
    fn ignores_rows_from_other_categories() {
        let rows = vec![AuditRow {
            category: "agent".to_string(),
            kind: "scan_block".to_string(),
            payload: Value::Null,
        }];
        let report = build_report(&rows);
        assert_eq!(report.total_scans, 0);
        assert_eq!(score_report(&report), 100);
    }

    #[test]
    fn block_scans_drop_score() {
        let rows = vec![
            row("scan_block", &["phrase:ignore previous instructions"]),
            row("scan_clean", &[]),
        ];
        let report = build_report(&rows);
        assert_eq!(report.block, 1);
        assert_eq!(report.clean, 1);
        // 100 - (1 * 5) = 95
        assert_eq!(score_report(&report), 95);
    }

    #[test]
    fn block_penalty_caps_at_50() {
        let rows: Vec<AuditRow> = (0..30).map(|_| row("scan_block", &["x"])).collect();
        let report = build_report(&rows);
        assert_eq!(report.block, 30);
        // 100 - min(30*5, 50) = 100 - 50 = 50
        assert_eq!(score_report(&report), 50);
    }

    #[test]
    fn warn_penalty_caps_at_20() {
        let rows: Vec<AuditRow> = (0..50).map(|_| row("scan_warn", &["x"])).collect();
        let report = build_report(&rows);
        assert_eq!(report.warn, 50);
        // 100 - min(50, 20) = 80
        assert_eq!(score_report(&report), 80);
    }

    #[test]
    fn block_and_warn_penalties_combine() {
        let mut rows: Vec<AuditRow> = (0..3).map(|_| row("scan_block", &["x"])).collect();
        rows.extend((0..5).map(|_| row("scan_warn", &["y"])));
        let report = build_report(&rows);
        // 100 - (3*5) - 5 = 80
        assert_eq!(score_report(&report), 80);
    }

    #[test]
    fn top_patterns_aggregates_across_rows() {
        let rows = vec![
            row("scan_block", &["phrase:ignore previous instructions"]),
            row("scan_block", &["phrase:ignore previous instructions"]),
            row("scan_warn", &["zero-width:200B"]),
        ];
        let report = build_report(&rows);
        assert_eq!(report.top_patterns.len(), 2);
        assert_eq!(report.top_patterns[0].0, "phrase:ignore previous instructions");
        assert_eq!(report.top_patterns[0].1, 2);
    }

    #[test]
    fn analyze_handles_missing_audit_file() {
        let dir = TempDir::new().unwrap();
        let cmdb = analyze_external_content_safety(dir.path());
        assert_eq!(cmdb["score"], 100);
        assert_eq!(cmdb["total_scans"], 0);
    }

    #[test]
    fn analyze_parses_jsonl_and_emits_cmdb() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let lines = vec![
            r#"{"category":"external_content","kind":"scan_block","payload":{"patterns":["phrase:ignore previous instructions"]}}"#,
            r#"{"category":"external_content","kind":"scan_clean","payload":{"patterns":[]}}"#,
            // unrelated row — must be ignored
            r#"{"category":"agent","kind":"spawn","payload":{}}"#,
        ];
        std::fs::write(claude_dir.join("audit.jsonl"), lines.join("\n")).unwrap();

        let cmdb = analyze_external_content_safety(dir.path());
        assert_eq!(cmdb["score"], 95);
        assert_eq!(cmdb["block"], 1);
        assert_eq!(cmdb["clean"], 1);
        assert_eq!(cmdb["meta"]["updated_by"], "external-content-safety");
    }

    #[test]
    fn malformed_jsonl_lines_are_skipped() {
        let dir = TempDir::new().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let contents = "not valid json\n\
                        {\"category\":\"external_content\",\"kind\":\"scan_warn\",\"payload\":{\"patterns\":[\"x\"]}}\n\
                        \n";
        std::fs::write(claude_dir.join("audit.jsonl"), contents).unwrap();

        let cmdb = analyze_external_content_safety(dir.path());
        assert_eq!(cmdb["warn"], 1);
        assert_eq!(cmdb["block"], 0);
    }
}
