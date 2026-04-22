//! Capability hygiene sensory tool — skill description quality scoring.
//!
//! B-12 (2026-04-22, contracted scope: authoring-standard compliance +
//! hygiene scoring, no TOC generator). Claude Code's native skill index
//! loads only the skill's name + lead-paragraph description (1,536-char
//! budget). The description IS the routing contract — this tool scores
//! whether each skill's lead paragraph carries enough routing signal.
//!
//! Per-skill checks:
//!   - Lead paragraph (everything before first `##` header) length
//!     (approximate tokens via char-count / 4).
//!   - Presence of "when to use" or "use this skill when" signal.
//!   - Description doesn't overflow the 1,536-char index budget.
//!
//! Scoring (aggregate across skills):
//!   - Each skill starts worth 10 points.
//!   - Under-described (<40 approx tokens)     → 0/10 (routing-broken).
//!   - Missing "when to use" signal            → 5/10 (partial).
//>   - Description > 1,536 chars (index budget) → 7/10 (fits but truncated).
//!   - Otherwise                                → 10/10 (compliant).
//! Final score = round(100 * total_earned / total_possible), clamped 0-100.
//! No skills present → score 100 (no hygiene problems to report).

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Soft minimum description length (approximate tokens). Below this,
/// the description can't carry reliable routing signal.
const MIN_DESCRIPTION_TOKENS: usize = 40;

/// Soft maximum description length (approximate tokens). Above this,
/// the skill probably has narrative in the lead that belongs in the
/// body.
#[allow(dead_code)]
const MAX_DESCRIPTION_TOKENS: usize = 300;

/// Hard ceiling: Claude Code's native skill index truncates at 1,536
/// characters. Skills whose description exceeds this have content
/// silently dropped from the routing context.
const INDEX_CHAR_BUDGET: usize = 1_536;

/// Character-to-token approximation (English prose ≈ 4 chars / token).
const CHARS_PER_TOKEN: usize = 4;

#[derive(Debug, Clone)]
pub struct CapabilityHygieneServer {
    tool_router: ToolRouter<Self>,
}

impl CapabilityHygieneServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckCapabilityHygieneParams {
    pub project_root: String,
}

#[tool_router]
impl CapabilityHygieneServer {
    #[tool(
        description = "Check capability hygiene: scores each skill in .claude/skills/ \
        for lead-paragraph description quality. The description IS the routing contract \
        in Claude Code's native skill index (1,536-char budget). Returns CMDB-envelope \
        JSON with per-skill compliance findings and aggregate score."
    )]
    async fn check_capability_hygiene(
        &self,
        Parameters(p): Parameters<CheckCapabilityHygieneParams>,
    ) -> String {
        serde_json::to_string_pretty(&analyze_capability_hygiene(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for CapabilityHygieneServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Capability hygiene sensory tool. Scores skill description quality \
                against the authoring standard from write-skill.md."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ── Public analysis entry point ───────────────────────────────────────────────

pub async fn analyze_capability_hygiene(project_root: &str) -> Value {
    let root_raw = PathBuf::from(project_root);
    let root = root_raw.canonicalize().unwrap_or(root_raw);
    let skills_dir = root.join(".claude").join("skills");

    let mut findings: Vec<Finding> = Vec::new();
    let mut skill_details: Vec<Value> = Vec::new();
    let mut total_earned: usize = 0;
    let mut total_possible: usize = 0;
    let mut under_described_count = 0usize;
    let mut missing_when_to_use_count = 0usize;
    let mut over_budget_count = 0usize;
    let mut compliant_count = 0usize;

    let entries = match std::fs::read_dir(&skills_dir) {
        Ok(e) => e,
        Err(_) => {
            // No skills directory → nothing to score; return score 100.
            findings.push(Finding {
                name: "no-skills-directory".to_string(),
                status: "n/a".to_string(),
                points: 0,
                detail: Some(format!(
                    "No `.claude/skills/` directory under `{}`; nothing to score.",
                    root.display()
                )),
            });
            let extras: Vec<(&str, Value)> = vec![
                ("total_skills", json!(0)),
                ("compliant_count", json!(0)),
                ("under_described_count", json!(0)),
                ("missing_when_to_use_count", json!(0)),
                ("over_budget_count", json!(0)),
                ("skill_details", json!([])),
            ];
            return build_cmdb("capability-hygiene", 100, findings, Some(extras));
        }
    };

    let mut skills: Vec<(String, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name.starts_with("README") || name.starts_with('.') {
            continue;
        }
        if let Ok(body) = std::fs::read_to_string(&path) {
            skills.push((name.to_string(), body));
        }
    }

    // Stable output order.
    skills.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, body) in &skills {
        total_possible += 10;
        let description = extract_description_block(body);
        let description_chars = description.chars().count();
        let description_tokens_approx = description_chars / CHARS_PER_TOKEN;
        let has_when_to_use = detect_when_to_use(description);

        let (points, status, reason) = if description_tokens_approx < MIN_DESCRIPTION_TOKENS {
            under_described_count += 1;
            (
                0usize,
                "under-described",
                format!(
                    "Description block is ~{description_tokens_approx} tokens (< {MIN_DESCRIPTION_TOKENS}). \
                     Likely a `## When to Use` section header instead of a lead paragraph. \
                     Rewrite per write-skill.md § 'The Lead Paragraph — Routing-Critical'."
                ),
            )
        } else if description_chars > INDEX_CHAR_BUDGET {
            over_budget_count += 1;
            (
                7,
                "over-budget",
                format!(
                    "Description block is {description_chars} chars (> {INDEX_CHAR_BUDGET} budget). \
                     Claude Code's skill index will truncate. Move narrative into the body."
                ),
            )
        } else if !has_when_to_use {
            missing_when_to_use_count += 1;
            (
                5,
                "missing-when-to-use",
                format!(
                    "Description has ~{description_tokens_approx} tokens but no 'when to use' \
                     or 'use this skill when' phrase. Agents may not route here reliably."
                ),
            )
        } else {
            compliant_count += 1;
            (
                10,
                "compliant",
                format!("~{description_tokens_approx} tokens, when-to-use signal present."),
            )
        };

        total_earned += points;

        if points < 10 {
            findings.push(Finding {
                name: format!("{status}:{name}"),
                status: status.to_string(),
                // Represent lost points as a negative number so existing CMDB
                // display code renders them as penalties.
                points: -((10 - points) as i32),
                detail: Some(reason.clone()),
            });
        }

        skill_details.push(json!({
            "path": name,
            "description_chars": description_chars,
            "description_tokens_approx": description_tokens_approx,
            "has_when_to_use_signal": has_when_to_use,
            "status": status,
            "points": points,
        }));
    }

    let score: u8 = if total_possible == 0 {
        100
    } else {
        ((total_earned as f64 / total_possible as f64) * 100.0).round() as u8
    };

    if compliant_count == skills.len() && !skills.is_empty() {
        findings.push(Finding {
            name: "all-skills-compliant".to_string(),
            status: "ok".to_string(),
            points: 0,
            detail: Some(format!(
                "{compliant_count}/{} skills meet the description authoring standard.",
                skills.len()
            )),
        });
    }

    let extras: Vec<(&str, Value)> = vec![
        ("total_skills", json!(skills.len())),
        ("compliant_count", json!(compliant_count)),
        ("under_described_count", json!(under_described_count)),
        ("missing_when_to_use_count", json!(missing_when_to_use_count)),
        ("over_budget_count", json!(over_budget_count)),
        ("skill_details", json!(skill_details)),
    ];

    build_cmdb("capability-hygiene", score, findings, Some(extras))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract the lead description block — everything from the top of the file
/// up to (but not including) the first `##` section header. Matches the
/// convention codified in `write-skill.md` § "The Lead Paragraph —
/// Routing-Critical".
fn extract_description_block(body: &str) -> &str {
    match body.find("\n## ") {
        Some(idx) => &body[..idx],
        None => body,
    }
}

/// Detect whether the description carries a "when to use" signal. Case-
/// insensitive match against a small set of canonical phrases.
fn detect_when_to_use(description: &str) -> bool {
    let lower = description.to_lowercase();
    lower.contains("when to use this skill")
        || lower.contains("when to read this")
        || lower.contains("use this skill when")
        || lower.contains("use this skill to ")
        || lower.contains("use this skill for ")
        // "Use this skill." catches the plain imperative form some skills use
        // (e.g., plan-critic.md: "Use this skill before implementing any plan.")
        || lower.contains("use this skill before ")
        || lower.contains("use this skill after ")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_skill(brain_root: &Path, basename: &str, body: &str) {
        let dir = brain_root.join(".claude").join("skills");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(basename), body).unwrap();
    }

    #[tokio::test]
    async fn missing_skills_dir_yields_score_100() {
        let tmp = TempDir::new().unwrap();
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["score"], 100);
        assert_eq!(result["total_skills"], 0);
    }

    #[tokio::test]
    async fn compliant_skill_scores_100() {
        let tmp = TempDir::new().unwrap();
        let body = "# Plan Critic\n\n\
                    **When to use this skill:** You are about to implement a non-trivial plan \
                    and want an adversarial review before writing code. The plan critic surfaces \
                    pitfalls, missing rollback paths, and compatibility risks before a single \
                    line is written.\n\n\
                    Role: meta\n\
                    Trigger phrases: \"review my plan\", \"critique this plan\"\n\n\
                    ---\n\n\
                    ## Body\n\nContent.\n";
        write_skill(tmp.path(), "plan-critic.md", body);
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["score"], 100);
        assert_eq!(result["compliant_count"], 1);
        assert_eq!(result["under_described_count"], 0);
    }

    #[tokio::test]
    async fn under_described_skill_is_penalized() {
        let tmp = TempDir::new().unwrap();
        // Only a title + a short line — under-described lead paragraph
        // (the "When to Use" block is a `##` section, not the lead).
        let body = "# coherence\n\n\
                    **Purpose:** cross-domain.\n\n\
                    ## When to Use This Skill\n\n\
                    - item\n- item\n- item\n- item\n\n\
                    ## More content\n\nfoo\n";
        write_skill(tmp.path(), "coherence.md", body);
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["under_described_count"], 1);
        // 0/10 earned out of 10 possible → 0 score for a one-skill corpus.
        assert_eq!(result["score"], 0);
        let findings = result["findings"].as_array().unwrap();
        assert!(findings
            .iter()
            .any(|f| f["name"] == "under-described:coherence.md"));
    }

    #[tokio::test]
    async fn missing_when_to_use_signal_gets_partial_credit() {
        let tmp = TempDir::new().unwrap();
        // Plenty long, but no "when to use" phrasing.
        let body = "# Skill Title\n\n\
                    This skill exists to describe some abstract concept at length. \
                    It explains several things but never says when to reach for it. \
                    The prose goes on and on about what the skill covers rather than \
                    when you would invoke it. Agents reading this description may not \
                    easily decide whether to route to this skill over another one with \
                    a clearer when-to-use signal.\n\n\
                    Role: reference\n\n\
                    ---\n\n\
                    ## Body\n\ncontent.\n";
        write_skill(tmp.path(), "vague.md", body);
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["missing_when_to_use_count"], 1);
        assert_eq!(result["compliant_count"], 0);
        // 5/10 earned → score 50.
        assert_eq!(result["score"], 50);
    }

    #[tokio::test]
    async fn mixed_corpus_aggregates_correctly() {
        let tmp = TempDir::new().unwrap();

        // Compliant skill.
        let good = "# Good\n\n\
                    **When to use this skill:** Operators need a working example of a \
                    compliant lead paragraph that meets the length and signal criteria. \
                    This is that example — long enough to clear the 40-token floor and \
                    carrying an explicit when-to-use signal at the top.\n\n\
                    ## Body\n\ncontent.\n";
        write_skill(tmp.path(), "good.md", good);

        // Under-described skill.
        let bad = "# Bad\n\n\
                   ## Skipped straight to the section header\n\n\
                   body content here.\n";
        write_skill(tmp.path(), "bad.md", bad);

        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["total_skills"], 2);
        assert_eq!(result["compliant_count"], 1);
        assert_eq!(result["under_described_count"], 1);
        // 10 + 0 = 10 out of 20 possible → score 50.
        assert_eq!(result["score"], 50);
    }

    #[test]
    fn extract_description_stops_at_first_double_hash() {
        let body = "# Title\n\nlead paragraph\n\n## First Section\n\nbody";
        let desc = extract_description_block(body);
        assert!(desc.contains("lead paragraph"));
        assert!(!desc.contains("First Section"));
    }

    #[test]
    fn detect_when_to_use_matches_common_phrasings() {
        assert!(detect_when_to_use("**When to use this skill:** trigger description"));
        assert!(detect_when_to_use("Use this skill when you are stuck"));
        assert!(detect_when_to_use("Use this skill before implementing"));
        assert!(detect_when_to_use("**When to read this:** context"));
        assert!(!detect_when_to_use("This skill is a reference document about X"));
    }
}
