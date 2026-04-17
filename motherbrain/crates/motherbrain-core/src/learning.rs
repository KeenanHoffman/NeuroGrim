//! Learning protocol (spec Section 12).
//!
//! Manages the proposal ledger, computes effectiveness per action type,
//! and feeds into autonomy resolution.

use crate::governance::ProposalConfidence;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single entry in the proposal ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalLedgerEntry {
    pub timestamp: String,
    #[serde(default)]
    pub proposals: Vec<Proposal>,
    #[serde(default)]
    pub pre_score: Option<i64>,
    #[serde(default)]
    pub post_score: Option<i64>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub hat: Option<String>,
}

/// A single proposal within a ledger entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub action_type: Option<String>,
}

/// Effectiveness stats for a single action type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionEffectiveness {
    pub avg_delta: f64,
    pub sample_count: u32,
    pub success_count: u32,
    pub effectiveness_rate: f64,
    pub sufficient: bool,
}

/// Compute proposal confidence (effectiveness rate) for a specific action type.
///
/// Used by autonomy resolution (Step 2) to determine confidence level.
pub fn compute_proposal_confidence(
    ledger: &[ProposalLedgerEntry],
    action_type: &str,
) -> ProposalConfidence {
    let mut total = 0u32;
    let mut successes = 0u32;

    for entry in ledger {
        let post = match entry.post_score {
            Some(p) => p,
            None => continue,
        };
        let pre = entry.pre_score.unwrap_or(0);
        let delta = post - pre;

        for proposal in &entry.proposals {
            let at = proposal.action_type.as_deref().unwrap_or("other");
            if at == action_type {
                total += 1;
                if delta > 0 {
                    successes += 1;
                }
            }
        }
    }

    if total == 0 {
        return ProposalConfidence {
            effectiveness_rate: 0.0,
            sample_count: 0,
            success_count: 0,
        };
    }

    ProposalConfidence {
        effectiveness_rate: (successes as f64 / total as f64 * 1000.0).round() / 1000.0,
        sample_count: total,
        success_count: successes,
    }
}

/// Compute effectiveness stats for ALL action types in the ledger.
///
/// Returns a map sorted by avg_delta (descending).
pub fn compute_all_effectiveness(
    ledger: &[ProposalLedgerEntry],
    min_samples: u32,
) -> HashMap<String, ActionEffectiveness> {
    let mut by_type: HashMap<String, (f64, u32, u32)> = HashMap::new(); // total_delta, count, successes

    for entry in ledger {
        let post = match entry.post_score {
            Some(p) => p,
            None => continue,
        };
        let pre = entry.pre_score.unwrap_or(0);
        let delta = post - pre;

        for proposal in &entry.proposals {
            let action_type = proposal
                .action_type
                .as_deref()
                .unwrap_or("other")
                .to_string();

            let stats = by_type.entry(action_type).or_insert((0.0, 0, 0));
            stats.0 += delta as f64;
            stats.1 += 1;
            if delta > 0 {
                stats.2 += 1;
            }
        }
    }

    by_type
        .into_iter()
        .map(|(action_type, (total_delta, count, successes))| {
            let avg = if count > 0 {
                (total_delta / count as f64 * 10.0).round() / 10.0
            } else {
                0.0
            };
            let rate = if count > 0 {
                (successes as f64 / count as f64 * 1000.0).round() / 1000.0
            } else {
                0.0
            };
            (
                action_type,
                ActionEffectiveness {
                    avg_delta: avg,
                    sample_count: count,
                    success_count: successes,
                    effectiveness_rate: rate,
                    sufficient: count >= min_samples,
                },
            )
        })
        .collect()
}

/// Prune old entries from the proposal ledger.
pub fn prune_ledger(
    ledger: &[ProposalLedgerEntry],
    retention_days: u32,
) -> Vec<ProposalLedgerEntry> {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    ledger
        .iter()
        .filter(|e| {
            e.timestamp
                .parse::<chrono::DateTime<chrono::Utc>>()
                .map(|ts| ts >= cutoff)
                .unwrap_or(true) // Keep entries with unparseable timestamps
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(action_type: &str, pre: i64, post: Option<i64>) -> ProposalLedgerEntry {
        ProposalLedgerEntry {
            timestamp: "2026-04-11T00:00:00Z".to_string(),
            proposals: vec![Proposal {
                id: None,
                command: None,
                domain: None,
                action_type: Some(action_type.to_string()),
            }],
            pre_score: Some(pre),
            post_score: post,
            commit: None,
            hat: None,
        }
    }

    #[test]
    fn empty_ledger_returns_zero() {
        let result = compute_proposal_confidence(&[], "clear-gate");
        assert_eq!(result.sample_count, 0);
        assert_eq!(result.effectiveness_rate, 0.0);
    }

    #[test]
    fn effectiveness_rate_correct() {
        let ledger = vec![
            make_entry("clear-gate", 50, Some(70)), // delta +20, success
            make_entry("clear-gate", 60, Some(80)), // delta +20, success
            make_entry("clear-gate", 70, Some(65)), // delta -5, failure
        ];
        let result = compute_proposal_confidence(&ledger, "clear-gate");
        assert_eq!(result.sample_count, 3);
        assert_eq!(result.success_count, 2);
        assert!((result.effectiveness_rate - 0.667).abs() < 0.001);
    }

    #[test]
    fn unresolved_entries_skipped() {
        let ledger = vec![
            make_entry("clear-gate", 50, Some(70)),
            make_entry("clear-gate", 60, None), // No post_score
        ];
        let result = compute_proposal_confidence(&ledger, "clear-gate");
        assert_eq!(result.sample_count, 1);
    }

    #[test]
    fn different_action_types_independent() {
        let ledger = vec![
            make_entry("clear-gate", 50, Some(70)),
            make_entry("deploy", 60, Some(65)),
        ];
        let gate_result = compute_proposal_confidence(&ledger, "clear-gate");
        let deploy_result = compute_proposal_confidence(&ledger, "deploy");
        assert_eq!(gate_result.sample_count, 1);
        assert_eq!(deploy_result.sample_count, 1);
        assert_eq!(gate_result.success_count, 1);
        assert_eq!(deploy_result.success_count, 1);
    }

    #[test]
    fn all_effectiveness_groups_by_type() {
        let ledger = vec![
            make_entry("clear-gate", 50, Some(70)),
            make_entry("clear-gate", 60, Some(50)),
            make_entry("deploy", 40, Some(80)),
        ];
        let result = compute_all_effectiveness(&ledger, 2);
        assert!(result.contains_key("clear-gate"));
        assert!(result.contains_key("deploy"));
        assert_eq!(result["clear-gate"].sample_count, 2);
        assert!(result["clear-gate"].sufficient);
        assert_eq!(result["deploy"].sample_count, 1);
        assert!(!result["deploy"].sufficient);
    }
}
