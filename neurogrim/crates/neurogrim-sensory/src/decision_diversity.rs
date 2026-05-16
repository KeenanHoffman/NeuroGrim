//! Decision-diversity analysis (v2-Feature 7, Phase 1 — 2026-05-09).
//!
//! # DISCLAIMER — read first
//!
//! This module surfaces decision-CONCENTRATION patterns. It does NOT
//! detect bias. Concentration may be appropriate (workspace conventions,
//! project constraints, intentional standardization) or may indicate
//! missed alternatives — interpretation is the operator's responsibility.
//!
//! Promotion of a `decision-diversity` Brain domain to weighted (>0.0)
//! is **blocked indefinitely** per LSP-Brains spec §15.5: weighted
//! domains require a validated classifier, and no validated classifier
//! exists for "appropriate vs biased concentration." The operator-flagged
//! concept ("technical bias trajectory tracking") is genuinely difficult
//! without ground truth — this module is the honest reframe: surface
//! distributions, leave interpretation to humans.
//!
//! # What it computes
//!
//! Given a stream of decisions (e.g., from `subagent-outcomes.jsonl`),
//! compute Shannon entropy per capability:
//!
//!     H = -Σ(p_i × log₂(p_i))
//!
//! - High entropy → diverse choices across the population
//! - Low entropy → concentrated choices (one option dominates)
//! - Zero entropy → only one choice was ever made
//!
//! # Phase 1 scope
//!
//! This commit ships the LIBRARY (compute_diversity + parse_outcomes_jsonl
//! + tests). Wiring as a full sensor (rmcp tool router + CMDB output)
//! lands in Phase 2 alongside domain registration in
//! `<project>/.claude/brain-registry.json`.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Single recorded decision. Maps to one row in subagent-outcomes.jsonl
/// (or a similar ledger). Phase 1 cares about three fields; future
/// phases can extend.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Decision {
    /// Capability the agent was exercising — e.g., "llm-invoke",
    /// "lsp-symbol-scan", "supply-chain-decision". The diversity score
    /// is computed PER CAPABILITY because conventions vary.
    pub capability: String,
    /// The choice point — what option the agent picked. Concrete
    /// examples: a model name (`gpt-4o` vs `qwen3.5:0.8b`), a hat
    /// (`critic-hat` vs `architect-hat`), a library / dep / pattern.
    /// Phase 1 reads from `worn_hat` field of subagent-outcomes; future
    /// phases let the caller specify which field is the choice key.
    pub choice: String,
    /// Optional: when the decision was made. Enables time-windowed
    /// analysis ("decision diversity over the last 30 days").
    pub ts: Option<String>,
}

/// Per-capability diversity report. Operator-readable; emit as a
/// finding when concentration crosses a threshold the operator
/// considers worth surfacing.
#[derive(Debug, Clone, Serialize)]
pub struct CapabilityDiversity {
    pub capability: String,
    /// Total decisions recorded for this capability.
    pub sample_size: usize,
    /// Distinct choices observed.
    pub distinct_choices: usize,
    /// Shannon entropy in bits. Range: [0, log₂(distinct_choices)].
    /// 0.0 means single choice; log₂(N) means perfectly uniform across
    /// N distinct choices.
    pub entropy_bits: f64,
    /// Normalized entropy: entropy_bits / log₂(distinct_choices).
    /// 0.0 to 1.0; higher = more diverse. Returns 0.0 when there's
    /// only one distinct choice (no diversity possible).
    pub normalized: f64,
    /// Top 3 most-frequent choices, with their share of the population.
    pub top_choices: Vec<(String, f64)>,
}

/// Aggregate report — every capability's diversity surface.
#[derive(Debug, Clone, Serialize)]
pub struct DiversityReport {
    pub per_capability: Vec<CapabilityDiversity>,
    /// Number of decisions in the input stream.
    pub total_decisions: usize,
    /// Number of distinct capabilities.
    pub capability_count: usize,
}

/// Compute the diversity report for a stream of decisions.
pub fn compute_diversity(decisions: &[Decision]) -> DiversityReport {
    let total = decisions.len();
    let mut by_cap: HashMap<&str, HashMap<&str, usize>> = HashMap::new();
    for d in decisions {
        let entry = by_cap.entry(&d.capability).or_default();
        *entry.entry(&d.choice).or_insert(0) += 1;
    }
    let mut per_capability: Vec<CapabilityDiversity> = Vec::with_capacity(by_cap.len());
    for (cap, choices) in &by_cap {
        let sample_size: usize = choices.values().sum();
        let distinct_choices = choices.len();
        let entropy_bits = shannon_entropy(choices.values().copied(), sample_size);
        let max_entropy = if distinct_choices > 1 {
            (distinct_choices as f64).log2()
        } else {
            0.0
        };
        let normalized = if max_entropy > 0.0 {
            entropy_bits / max_entropy
        } else {
            0.0
        };
        let mut sorted: Vec<(&&str, &usize)> = choices.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        let top_choices: Vec<(String, f64)> = sorted
            .iter()
            .take(3)
            .map(|(name, count)| {
                let share = if sample_size > 0 {
                    **count as f64 / sample_size as f64
                } else {
                    0.0
                };
                ((*name).to_string(), share)
            })
            .collect();
        per_capability.push(CapabilityDiversity {
            capability: cap.to_string(),
            sample_size,
            distinct_choices,
            entropy_bits,
            normalized,
            top_choices,
        });
    }
    per_capability.sort_by(|a, b| {
        // Sort by sample_size desc, then capability name asc, for stable output.
        b.sample_size.cmp(&a.sample_size).then_with(|| a.capability.cmp(&b.capability))
    });
    DiversityReport {
        per_capability,
        total_decisions: total,
        capability_count: by_cap.len(),
    }
}

/// Parse decisions out of a JSONL file. Schema-tolerant: rows without
/// the required fields are silently skipped (with a tracing::warn).
/// Compatible with `<project>/.claude/brain/subagent-outcomes.jsonl`
/// produced by `neurogrim invoke` (Phase 1.4).
pub fn parse_outcomes_jsonl(path: &Path) -> std::io::Result<Vec<Decision>> {
    let text = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(value) => {
                let cap = value.get("capability").and_then(|v| v.as_str());
                let choice = value
                    .get("worn_hat")
                    .and_then(|v| v.as_str())
                    .or_else(|| value.get("model").and_then(|v| v.as_str()));
                let ts = value
                    .get("ts")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let (Some(c), Some(ch)) = (cap, choice) {
                    if !ch.is_empty() {
                        out.push(Decision {
                            capability: c.to_string(),
                            choice: ch.to_string(),
                            ts,
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "decision_diversity: skipping malformed line {} in {}: {e}",
                    lineno + 1,
                    path.display()
                );
            }
        }
    }
    Ok(out)
}

/// Sensor entry point — emits a CMDB envelope summarizing decision-
/// concentration patterns from `<project_root>/.claude/brain/
/// subagent-outcomes.jsonl`.
///
/// **Always scores 100.** Per the module-level disclaimer, this
/// domain surfaces concentration; it never penalizes. Operators
/// interpret the per-capability entropy + top_choices breakdown
/// and decide whether the concentration is appropriate or worth
/// surfacing as a finding. Promotion to weighted (>0.0) is blocked
/// indefinitely per LSP-Brains §15.5 — no validated bias-vs-
/// appropriate-concentration classifier exists.
///
/// Missing ledger file is NOT an error: the CMDB carries
/// `total_decisions: 0` + a `no_ledger_yet` info finding so
/// operators see "no signal yet" rather than a confusing 0 score.
pub async fn analyze_decision_diversity(project_root: &str) -> Value {
    use crate::cmdb::{build_cmdb, Finding};

    let ledger = Path::new(project_root)
        .join(".claude")
        .join("brain")
        .join("subagent-outcomes.jsonl");

    let decisions = match parse_outcomes_jsonl(&ledger) {
        Ok(d) => d,
        Err(_) => Vec::new(), // missing or unreadable → empty stream
    };

    let report = compute_diversity(&decisions);

    let mut findings: Vec<Finding> = Vec::new();
    if report.total_decisions == 0 {
        findings.push(Finding {
            name: "no_ledger_yet".into(),
            status: "info".into(),
            points: 0,
            detail: Some(format!(
                "No decisions in {}. Domain stays informational.",
                ledger.display()
            )),
        });
    } else {
        findings.push(Finding {
            name: "decisions_observed".into(),
            status: "info".into(),
            points: 0,
            detail: Some(format!(
                "{} decisions across {} capabilities",
                report.total_decisions, report.capability_count
            )),
        });
        // Surface a research-only concentration finding for any
        // capability with normalized entropy < 0.5 AND sample size
        // >= 10. Threshold is heuristic; operators read the
        // disclaimer to interpret.
        for cap in &report.per_capability {
            if cap.sample_size >= 10 && cap.normalized < 0.5 {
                findings.push(Finding {
                    name: format!("concentration:{}", cap.capability),
                    status: "info".into(),
                    points: 0,
                    detail: Some(format!(
                        "Capability `{}` shows normalized entropy {:.2} \
                         over {} decisions across {} distinct choices. \
                         Top choices: {:?}. RESEARCH-ONLY: concentration may \
                         be appropriate (workspace conventions) or worth \
                         operator review.",
                        cap.capability,
                        cap.normalized,
                        cap.sample_size,
                        cap.distinct_choices,
                        cap.top_choices,
                    )),
                });
            }
        }
    }

    let extras = vec![
        ("total_decisions", Value::from(report.total_decisions)),
        ("capability_count", Value::from(report.capability_count)),
        (
            "per_capability",
            serde_json::to_value(&report.per_capability).unwrap_or(Value::Null),
        ),
        (
            "disclaimer",
            Value::from(
                "Research-only domain. Surfaces concentration; does NOT detect bias. \
                 Promotion to weighted (>0.0) blocked indefinitely per LSP-Brains §15.5.",
            ),
        ),
    ];

    build_cmdb(
        "decision-diversity",
        100, // always 100 — research-only
        findings,
        Some(extras),
        None,
    )
}

/// Shannon entropy in bits. Inputs are raw counts; the function
/// computes probabilities internally.
fn shannon_entropy(counts: impl Iterator<Item = usize>, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let mut h = 0.0;
    for c in counts {
        if c == 0 {
            continue;
        }
        let p = c as f64 / total as f64;
        h -= p * p.log2();
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(cap: &str, choice: &str) -> Decision {
        Decision {
            capability: cap.to_string(),
            choice: choice.to_string(),
            ts: None,
        }
    }

    #[test]
    fn entropy_zero_for_single_choice() {
        let decisions = vec![d("foo", "a"); 10];
        let report = compute_diversity(&decisions);
        assert_eq!(report.per_capability.len(), 1);
        let cap = &report.per_capability[0];
        assert_eq!(cap.distinct_choices, 1);
        assert_eq!(cap.entropy_bits, 0.0);
        assert_eq!(cap.normalized, 0.0);
        assert_eq!(cap.top_choices, vec![("a".to_string(), 1.0)]);
    }

    #[test]
    fn entropy_one_bit_for_balanced_two_choice() {
        // 50/50 split between two choices => H = 1.0 bit.
        let decisions = vec![d("foo", "a"), d("foo", "b"), d("foo", "a"), d("foo", "b")];
        let report = compute_diversity(&decisions);
        let cap = &report.per_capability[0];
        assert!((cap.entropy_bits - 1.0).abs() < 1e-9);
        assert!((cap.normalized - 1.0).abs() < 1e-9);
    }

    #[test]
    fn entropy_low_for_concentrated_choices() {
        // 9 of 10 are "a", 1 is "b" — heavily concentrated.
        let mut decisions = vec![d("foo", "a"); 9];
        decisions.push(d("foo", "b"));
        let report = compute_diversity(&decisions);
        let cap = &report.per_capability[0];
        // Normalized entropy should be well under 0.5 — concentrated
        // distributions are exactly the case the report flags.
        assert!(cap.normalized < 0.5, "expected concentrated distribution, got normalized = {}", cap.normalized);
        // Top choice's share is 90%.
        assert_eq!(cap.top_choices[0].0, "a");
        assert!((cap.top_choices[0].1 - 0.9).abs() < 1e-9);
    }

    #[test]
    fn per_capability_isolation() {
        let decisions = vec![
            d("cap-a", "x"),
            d("cap-a", "x"),  // cap-a: only one choice
            d("cap-b", "y"),
            d("cap-b", "z"),  // cap-b: balanced two choices
        ];
        let report = compute_diversity(&decisions);
        assert_eq!(report.capability_count, 2);
        let cap_a = report.per_capability.iter().find(|c| c.capability == "cap-a").unwrap();
        let cap_b = report.per_capability.iter().find(|c| c.capability == "cap-b").unwrap();
        assert_eq!(cap_a.entropy_bits, 0.0);
        assert!((cap_b.entropy_bits - 1.0).abs() < 1e-9);
    }

    #[test]
    fn empty_input_returns_empty_report() {
        let report = compute_diversity(&[]);
        assert_eq!(report.total_decisions, 0);
        assert_eq!(report.capability_count, 0);
        assert!(report.per_capability.is_empty());
    }

    #[test]
    fn parse_outcomes_skips_malformed_and_missing_fields() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("outcomes.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        // Valid row
        writeln!(f, r#"{{"capability":"llm-invoke","worn_hat":"advisor","ts":"2026-05-09T00:00:00Z"}}"#).unwrap();
        // Valid row, fallback to model when worn_hat is absent
        writeln!(f, r#"{{"capability":"llm-invoke","model":"qwen3.5:0.8b"}}"#).unwrap();
        // Missing capability — skipped
        writeln!(f, r#"{{"worn_hat":"advisor"}}"#).unwrap();
        // Empty worn_hat + no model — skipped
        writeln!(f, r#"{{"capability":"x","worn_hat":""}}"#).unwrap();
        // Malformed JSON — skipped, doesn't error
        writeln!(f, "not valid json").unwrap();
        // Blank line — skipped
        writeln!(f).unwrap();
        drop(f);

        let decisions = parse_outcomes_jsonl(&path).expect("parse");
        assert_eq!(decisions.len(), 2);
        assert_eq!(decisions[0].choice, "advisor");
        assert_eq!(decisions[1].choice, "qwen3.5:0.8b");
    }

    #[test]
    fn sample_size_correctly_aggregates() {
        let decisions = vec![
            d("foo", "a"),
            d("foo", "a"),
            d("foo", "b"),
            d("bar", "c"),
        ];
        let report = compute_diversity(&decisions);
        assert_eq!(report.total_decisions, 4);
        let foo = report.per_capability.iter().find(|c| c.capability == "foo").unwrap();
        assert_eq!(foo.sample_size, 3);
        let bar = report.per_capability.iter().find(|c| c.capability == "bar").unwrap();
        assert_eq!(bar.sample_size, 1);
    }
}
