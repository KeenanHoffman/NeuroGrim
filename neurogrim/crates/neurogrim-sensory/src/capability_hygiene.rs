//! Capability hygiene sensory tool — description quality scoring across the
//! full capability surface.
//!
//! B-12 (2026-04-22, initial scope: skills only) extended 2026-04-22 with
//! Tier 2 broader adoption (Axis 3): now scores description quality for
//! skills, subagents, MCP tools, hats, correlations, and personas.
//!
//! The unifying insight: every capability type has a description that is
//! the routing contract for agents to decide when to invoke it. Bad
//! description → wrong routing → capability never fires. This tool
//! enforces a minimum authoring standard across all six types and
//! aggregates into one hygiene score per Brain.
//!
//! Per-type scoring (each capability is worth 10 points):
//!   - **skills** (`.claude/skills/*.md`): lead paragraph (everything before
//!     first `## `). Under-described (<40 approx tokens)=0; missing
//!     when-to-use signal=5; over 1,536 chars (index budget)=7;
//!     compliant=10.
//!   - **subagents** (`.claude/subagents/*.md`): same checks as skills.
//!     If the directory doesn't exist, scored as 0 capabilities.
//!   - **tools** (MCP `#[tool(description = "...")]` in local Rust source):
//!     too short (<40 chars)=0; too long (>500 chars)=7; compliant=10.
//!     No "when to use" signal required — MCP tools have a different
//!     routing context.
//!   - **hats** (`config.hats.*.description` in `brain-registry.json`):
//!     too short (<20 chars)=0; too long (>300)=7; compliant=10.
//!   - **correlations** (`config.correlations.*` with `description` +
//!     `insight` fields): both required; description <60 chars or insight
//!     <20 chars=5; both present+length OK=10; both missing=0.
//!   - **personas** (`config.human_personas.*.description`): same
//!     thresholds as hats.
//!
//! Aggregate score = round(100 * sum_earned / sum_possible), clamped 0-100.
//! No capabilities present → score 100.
//!
//! CMDB output shape (post-Tier-2):
//!   - `score`: aggregate across all types.
//!   - `capability_breakdown`: per-type `{total, compliant, earned,
//!     possible, details: [...], ...}`.
//!   - Top-level backward-compat: `total_skills`, `compliant_count`,
//!     `under_described_count`, `missing_when_to_use_count`,
//!     `over_budget_count`, `skill_details` — all refer to SKILLS ONLY,
//!     for consumers (like existing `exported_variables` in
//!     brain-registry.json) that predate Tier 2.
//!   - Top-level aggregate: `total_capabilities`, `overall_compliant_count`.

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

// ── Thresholds ────────────────────────────────────────────────────────────────

/// Skill/subagent: soft minimum description length (approximate tokens).
const SKILL_MIN_DESCRIPTION_TOKENS: usize = 40;

/// Skill/subagent: hard ceiling — Claude Code's native skill index
/// truncates at 1,536 characters.
const SKILL_INDEX_CHAR_BUDGET: usize = 1_536;

/// Character-to-token approximation (English prose ≈ 4 chars / token).
const CHARS_PER_TOKEN: usize = 4;

/// MCP tool: soft minimum description length (chars). Below this, the tool
/// is under-described in the agent's routing context.
const TOOL_MIN_CHARS: usize = 40;

/// MCP tool: soft maximum description length (chars). Above this, the
/// description is verbose for an always-loaded schema.
const TOOL_MAX_CHARS: usize = 500;

/// Hat/persona: minimum description length (chars).
const HAT_MIN_CHARS: usize = 20;

/// Hat/persona: maximum description length (chars).
const HAT_MAX_CHARS: usize = 300;

/// Correlation: minimum description length (chars).
const CORRELATION_MIN_DESC_CHARS: usize = 60;

/// Correlation: minimum insight length (chars).
const CORRELATION_MIN_INSIGHT_CHARS: usize = 20;

// ── MCP Server glue ───────────────────────────────────────────────────────────

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
        description = "Check capability hygiene: scores description quality across the \
        full capability surface (skills, subagents, MCP tools, hats, correlations, \
        personas). Returns CMDB-envelope JSON with per-type compliance breakdown + \
        aggregate score. Tier 2 (Axis 3) extension — 2026-04-22."
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
                "Capability hygiene sensory tool. Scores description quality across \
                skills, subagents, MCP tools, hats, correlations, personas. See \
                write-skill.md § 'The Lead Paragraph — Routing-Critical' for the \
                skill authoring standard."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ── Shared per-type result shape ──────────────────────────────────────────────

#[derive(Debug, Default)]
struct TypeResult {
    type_name: &'static str,
    total: usize,
    compliant: usize,
    earned: usize,
    possible: usize,
    details: Vec<Value>,
    findings: Vec<Finding>,
    /// Per-type status counters (e.g., `{"under_described": 1, ...}`).
    status_counts: std::collections::BTreeMap<&'static str, usize>,
}

impl TypeResult {
    fn new(type_name: &'static str) -> Self {
        Self {
            type_name,
            ..Default::default()
        }
    }

    fn add(&mut self, detail: Value, finding: Option<Finding>, points: usize, status: &'static str) {
        self.total += 1;
        self.earned += points;
        self.possible += 10;
        if points == 10 {
            self.compliant += 1;
        }
        *self.status_counts.entry(status).or_insert(0) += 1;
        self.details.push(detail);
        if let Some(f) = finding {
            self.findings.push(f);
        }
    }

    fn to_breakdown(&self) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert("total".into(), json!(self.total));
        obj.insert("compliant".into(), json!(self.compliant));
        obj.insert("earned".into(), json!(self.earned));
        obj.insert("possible".into(), json!(self.possible));
        for (status, count) in &self.status_counts {
            obj.insert((*status).to_string(), json!(count));
        }
        obj.insert("details".into(), json!(self.details));
        Value::Object(obj)
    }
}

// ── Public analysis entry point ───────────────────────────────────────────────

pub async fn analyze_capability_hygiene(project_root: &str) -> Value {
    let root_raw = PathBuf::from(project_root);
    let root = root_raw.canonicalize().unwrap_or(root_raw);

    let skills = score_skills(&root);
    let subagents = score_subagents(&root);
    let tools = score_tools(&root);
    let hats = score_hats(&root);
    let correlations = score_correlations(&root);
    let personas = score_personas(&root);

    let total_earned: usize = skills.earned
        + subagents.earned
        + tools.earned
        + hats.earned
        + correlations.earned
        + personas.earned;
    let total_possible: usize = skills.possible
        + subagents.possible
        + tools.possible
        + hats.possible
        + correlations.possible
        + personas.possible;
    let total_capabilities: usize = skills.total
        + subagents.total
        + tools.total
        + hats.total
        + correlations.total
        + personas.total;
    let overall_compliant: usize = skills.compliant
        + subagents.compliant
        + tools.compliant
        + hats.compliant
        + correlations.compliant
        + personas.compliant;

    let score: u8 = if total_possible == 0 {
        100
    } else {
        ((total_earned as f64 / total_possible as f64) * 100.0).round() as u8
    };

    // Merge findings from all types into a flat list for the CMDB.
    let mut all_findings: Vec<Finding> = Vec::new();
    all_findings.extend(skills.findings.iter().cloned());
    all_findings.extend(subagents.findings.iter().cloned());
    all_findings.extend(tools.findings.iter().cloned());
    all_findings.extend(hats.findings.iter().cloned());
    all_findings.extend(correlations.findings.iter().cloned());
    all_findings.extend(personas.findings.iter().cloned());

    // If aggregate compliant == total and we have at least one capability,
    // add a green "all-capabilities-compliant" finding.
    if total_capabilities > 0 && overall_compliant == total_capabilities {
        all_findings.push(Finding {
            name: "all-capabilities-compliant".to_string(),
            status: "ok".to_string(),
            points: 0,
            detail: Some(format!(
                "All {total_capabilities} capabilities across 6 types meet their authoring standards."
            )),
        });
    }

    // Backward-compat: existing brain-registry.json exported_variables
    // reference these top-level skill-specific fields.
    let skills_status = |s: &str| skills.status_counts.get(s).copied().unwrap_or(0);

    let breakdown = json!({
        "skills":       skills.to_breakdown(),
        "subagents":    subagents.to_breakdown(),
        "tools":        tools.to_breakdown(),
        "hats":         hats.to_breakdown(),
        "correlations": correlations.to_breakdown(),
        "personas":     personas.to_breakdown(),
    });

    let extras: Vec<(&str, Value)> = vec![
        // Aggregate rollups (Tier 2+)
        ("total_capabilities", json!(total_capabilities)),
        ("overall_compliant_count", json!(overall_compliant)),
        ("capability_breakdown", breakdown),
        // Backward-compat (skills-only, for consumers that predate Tier 2)
        ("total_skills", json!(skills.total)),
        ("compliant_count", json!(skills.compliant)),
        ("under_described_count", json!(skills_status("under-described"))),
        ("missing_when_to_use_count", json!(skills_status("missing-when-to-use"))),
        ("over_budget_count", json!(skills_status("over-budget"))),
        ("skill_details", json!(skills.details)),
    ];

    build_cmdb("capability-hygiene", score, all_findings, Some(extras))
}

// ── Scorer: skills ────────────────────────────────────────────────────────────

fn score_skills(root: &Path) -> TypeResult {
    let mut result = TypeResult::new("skills");
    let dir = root.join(".claude").join("skills");

    let files = collect_md_files(&dir);
    if files.is_empty() {
        return result;
    }

    for (name, body) in files {
        let description = extract_description_block(&body);
        let chars = description.chars().count();
        let tokens = chars / CHARS_PER_TOKEN;
        let has_signal = detect_when_to_use(description);

        let (points, status, reason) = if tokens < SKILL_MIN_DESCRIPTION_TOKENS {
            (
                0usize,
                "under-described",
                format!(
                    "Description block is ~{tokens} tokens (< {SKILL_MIN_DESCRIPTION_TOKENS}). \
                     Likely a `## When to Use` section header instead of a lead paragraph. \
                     Rewrite per write-skill.md § 'The Lead Paragraph — Routing-Critical'."
                ),
            )
        } else if chars > SKILL_INDEX_CHAR_BUDGET {
            (
                7,
                "over-budget",
                format!(
                    "Description block is {chars} chars (> {SKILL_INDEX_CHAR_BUDGET} budget). \
                     Claude Code's skill index will truncate. Move narrative into the body."
                ),
            )
        } else if !has_signal {
            (
                5,
                "missing-when-to-use",
                format!(
                    "Description has ~{tokens} tokens but no 'when to use' \
                     or 'use this skill when' phrase. Agents may not route here reliably."
                ),
            )
        } else {
            (
                10,
                "compliant",
                format!("~{tokens} tokens, when-to-use signal present."),
            )
        };

        let detail = json!({
            "path": name,
            "description_chars": chars,
            "description_tokens_approx": tokens,
            "has_when_to_use_signal": has_signal,
            "status": status,
            "points": points,
        });

        let finding = if points < 10 {
            Some(Finding {
                name: format!("{status}:skill:{name}"),
                status: status.to_string(),
                points: -((10 - points) as i32),
                detail: Some(reason),
            })
        } else {
            None
        };

        result.add(detail, finding, points, status);
    }

    result
}

// ── Scorer: subagents ─────────────────────────────────────────────────────────

fn score_subagents(root: &Path) -> TypeResult {
    let mut result = TypeResult::new("subagents");
    let dir = root.join(".claude").join("subagents");
    if !dir.is_dir() {
        // Convention not in use here — no capabilities to score.
        return result;
    }

    let files = collect_md_files(&dir);
    for (name, body) in files {
        let description = extract_description_block(&body);
        let chars = description.chars().count();
        let tokens = chars / CHARS_PER_TOKEN;
        let has_signal = detect_when_to_use(description);

        let (points, status, reason) = if tokens < SKILL_MIN_DESCRIPTION_TOKENS {
            (
                0usize,
                "under-described",
                format!("Subagent description is ~{tokens} tokens (< {SKILL_MIN_DESCRIPTION_TOKENS})."),
            )
        } else if chars > SKILL_INDEX_CHAR_BUDGET {
            (
                7,
                "over-budget",
                format!("Subagent description is {chars} chars (> {SKILL_INDEX_CHAR_BUDGET})."),
            )
        } else if !has_signal {
            (
                5,
                "missing-when-to-use",
                format!("Subagent description has ~{tokens} tokens but no when-to-use phrase."),
            )
        } else {
            (10, "compliant", format!("~{tokens} tokens, compliant."))
        };

        let detail = json!({
            "path": name,
            "description_chars": chars,
            "description_tokens_approx": tokens,
            "has_when_to_use_signal": has_signal,
            "status": status,
            "points": points,
        });

        let finding = if points < 10 {
            Some(Finding {
                name: format!("{status}:subagent:{name}"),
                status: status.to_string(),
                points: -((10 - points) as i32),
                detail: Some(reason),
            })
        } else {
            None
        };

        result.add(detail, finding, points, status);
    }

    result
}

// ── Scorer: MCP tools (Rust source) ───────────────────────────────────────────

fn score_tools(root: &Path) -> TypeResult {
    let mut result = TypeResult::new("tools");

    // Scan any Rust source under the project that might contain `#[tool(` attrs.
    // Heuristic: walk `<root>/**/src/**/*.rs`. If no Rust source, we score 0
    // capabilities (e.g., LSP-Brains, python-starter).
    let tool_descriptions = extract_tool_descriptions(root);
    for td in tool_descriptions {
        let chars = td.description.chars().count();

        let (points, status, reason) = if chars < TOOL_MIN_CHARS {
            (
                0usize,
                "under-described",
                format!(
                    "Tool description is {chars} chars (< {TOOL_MIN_CHARS}). Agent has too \
                     little routing signal when this tool is injected into the system prompt."
                ),
            )
        } else if chars > TOOL_MAX_CHARS {
            (
                7,
                "too-long",
                format!(
                    "Tool description is {chars} chars (> {TOOL_MAX_CHARS}). Verbose for an \
                     always-loaded schema; trim or extract narrative to a doc."
                ),
            )
        } else {
            (10, "compliant", format!("{chars} chars, compliant."))
        };

        let detail = json!({
            "tool": td.tool_name,
            "source": td.source_path,
            "description_chars": chars,
            "status": status,
            "points": points,
        });

        let finding = if points < 10 {
            Some(Finding {
                name: format!("{status}:tool:{}", td.tool_name),
                status: status.to_string(),
                points: -((10 - points) as i32),
                detail: Some(reason),
            })
        } else {
            None
        };

        result.add(detail, finding, points, status);
    }

    result
}

// ── Scorer: hats (brain-registry.json) ────────────────────────────────────────

fn score_hats(root: &Path) -> TypeResult {
    let mut result = TypeResult::new("hats");
    let registry = match load_registry(root) {
        Some(v) => v,
        None => return result,
    };
    let hats = registry
        .pointer("/config/hats")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    for (name, hat) in hats {
        let desc = hat
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let chars = desc.chars().count();

        let (points, status, reason) = if chars < HAT_MIN_CHARS {
            (
                0usize,
                "under-described",
                format!(
                    "Hat `{name}` description is {chars} chars (< {HAT_MIN_CHARS}). \
                     Agents can't tell when to wear it."
                ),
            )
        } else if chars > HAT_MAX_CHARS {
            (
                7,
                "too-long",
                format!(
                    "Hat `{name}` description is {chars} chars (> {HAT_MAX_CHARS}). \
                     Trim — hats should be one-line operating lenses."
                ),
            )
        } else {
            (10, "compliant", format!("{chars} chars, compliant."))
        };

        let detail = json!({
            "hat": name,
            "description_chars": chars,
            "status": status,
            "points": points,
        });

        let finding = if points < 10 {
            Some(Finding {
                name: format!("{status}:hat:{name}"),
                status: status.to_string(),
                points: -((10 - points) as i32),
                detail: Some(reason),
            })
        } else {
            None
        };

        result.add(detail, finding, points, status);
    }

    result
}

// ── Scorer: correlations ──────────────────────────────────────────────────────

fn score_correlations(root: &Path) -> TypeResult {
    let mut result = TypeResult::new("correlations");
    let registry = match load_registry(root) {
        Some(v) => v,
        None => return result,
    };
    let correlations = registry
        .pointer("/config/correlations")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for corr in correlations {
        let id = corr
            .get("id")
            .or_else(|| corr.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("<unnamed>");
        let desc = corr
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let insight = corr
            .get("insight")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let desc_chars = desc.chars().count();
        let insight_chars = insight.chars().count();

        let (points, status, reason) = if desc.is_empty() && insight.is_empty() {
            (
                0usize,
                "under-described",
                format!(
                    "Correlation `{id}` has neither description nor insight. Agents can't explain \
                     why it fired."
                ),
            )
        } else if desc_chars < CORRELATION_MIN_DESC_CHARS
            || insight_chars < CORRELATION_MIN_INSIGHT_CHARS
        {
            (
                5,
                "partial",
                format!(
                    "Correlation `{id}` has description={desc_chars} chars \
                     (min {CORRELATION_MIN_DESC_CHARS}) and insight={insight_chars} chars \
                     (min {CORRELATION_MIN_INSIGHT_CHARS}). Both are required for agents to \
                     interpret a firing."
                ),
            )
        } else {
            (
                10,
                "compliant",
                format!("description={desc_chars} chars, insight={insight_chars} chars."),
            )
        };

        let detail = json!({
            "id": id,
            "description_chars": desc_chars,
            "insight_chars": insight_chars,
            "status": status,
            "points": points,
        });

        let finding = if points < 10 {
            Some(Finding {
                name: format!("{status}:correlation:{id}"),
                status: status.to_string(),
                points: -((10 - points) as i32),
                detail: Some(reason),
            })
        } else {
            None
        };

        result.add(detail, finding, points, status);
    }

    result
}

// ── Scorer: personas ──────────────────────────────────────────────────────────

fn score_personas(root: &Path) -> TypeResult {
    let mut result = TypeResult::new("personas");
    let registry = match load_registry(root) {
        Some(v) => v,
        None => return result,
    };
    let personas = registry
        .pointer("/config/human_personas")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    for (name, persona) in personas {
        let desc = persona
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let chars = desc.chars().count();

        let (points, status, reason) = if chars < HAT_MIN_CHARS {
            (
                0usize,
                "under-described",
                format!(
                    "Persona `{name}` description is {chars} chars (< {HAT_MIN_CHARS}). \
                     Agents can't tailor output shape without a description."
                ),
            )
        } else if chars > HAT_MAX_CHARS {
            (
                7,
                "too-long",
                format!(
                    "Persona `{name}` description is {chars} chars (> {HAT_MAX_CHARS}). \
                     Trim — personas are short role descriptors."
                ),
            )
        } else {
            (10, "compliant", format!("{chars} chars, compliant."))
        };

        let detail = json!({
            "persona": name,
            "description_chars": chars,
            "status": status,
            "points": points,
        });

        let finding = if points < 10 {
            Some(Finding {
                name: format!("{status}:persona:{name}"),
                status: status.to_string(),
                points: -((10 - points) as i32),
                detail: Some(reason),
            })
        } else {
            None
        };

        result.add(detail, finding, points, status);
    }

    result
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract the lead description block — everything from the top of the file
/// up to (but not including) the first `##` section header.
fn extract_description_block(body: &str) -> &str {
    match body.find("\n## ") {
        Some(idx) => &body[..idx],
        None => body,
    }
}

/// Detect whether the description carries a "when to use" signal. Case-
/// insensitive match against canonical phrasings.
fn detect_when_to_use(description: &str) -> bool {
    let lower = description.to_lowercase();
    lower.contains("when to use this skill")
        || lower.contains("when to read this")
        || lower.contains("use this skill when")
        || lower.contains("use this skill to ")
        || lower.contains("use this skill for ")
        || lower.contains("use this skill before ")
        || lower.contains("use this skill after ")
}

/// Read all non-archived `.md` files under a directory. Returns (basename,
/// full body) pairs. Skips README and dotfiles. Non-recursive.
fn collect_md_files(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if name.starts_with("README") || name.starts_with('.') {
                continue;
            }
            if let Ok(body) = std::fs::read_to_string(&path) {
                out.push((name.to_string(), body));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Load `{root}/.claude/brain-registry.json` as a serde_json::Value. Returns
/// None if the file is missing or invalid.
fn load_registry(root: &Path) -> Option<Value> {
    let path = root.join(".claude").join("brain-registry.json");
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

// ── MCP tool description extraction ───────────────────────────────────────────

#[derive(Debug, Clone)]
struct ToolDescription {
    tool_name: String,
    description: String,
    source_path: String,
}

/// Scan all `.rs` source files under the project root for
/// `#[tool(description = "...")]` attributes + the `async fn <name>(...)`
/// that immediately follows. Returns one entry per declared MCP tool.
/// If no Rust source is found, returns an empty Vec (expected for
/// non-NeuroGrim Brains).
///
/// Only scans files that declare an `impl ServerHandler` (the rmcp
/// convention for MCP tool servers). This avoids false positives from
/// test-fixture string literals that contain `#[tool(...)]` syntax —
/// e.g., this very file's test module has Rust source embedded in
/// `r##"..."##` blocks that would otherwise be picked up.
fn extract_tool_descriptions(root: &Path) -> Vec<ToolDescription> {
    let mut out = Vec::new();
    let rust_files = find_rust_sources(root);
    for path in rust_files {
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !source.contains("impl ServerHandler") {
            continue;
        }
        // Exclude content after `#[cfg(test)]` to avoid picking up
        // `#[tool(...)]` patterns embedded in test string literals.
        let live_source = match source.find("#[cfg(test)]") {
            Some(idx) => &source[..idx],
            None => source.as_str(),
        };
        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        for (tool_name, description) in parse_tool_attrs(live_source) {
            out.push(ToolDescription {
                tool_name,
                description,
                source_path: rel_path.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.tool_name.cmp(&b.tool_name));
    out
}

/// Walk `<root>` looking for `.rs` files under any `src/` directory. Skips
/// `target/`, `.git/`, and any `tests/` directory. Non-recursive walker
/// that bails after traversing a reasonable depth — adequate for Cargo
/// workspace layouts.
fn find_rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if matches!(name, "target" | ".git" | "node_modules" | "tests") {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
    out
}

/// Parse `#[tool(description = "...")]` attributes from a Rust source
/// string. Returns (tool_name, description) pairs. The tool name is
/// extracted from the `async fn <name>` that follows the attribute.
///
/// Handles multi-line descriptions (string continuations + escaped
/// newlines). Handles both `#[tool(description = "...")]` and the multi-
/// line form used in rmcp:
///     #[tool(
///         description = "..."
///     )]
fn parse_tool_attrs(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = 0;

    while let Some(attr_idx) = source[cursor..].find("#[tool(") {
        let attr_start = cursor + attr_idx;
        cursor = attr_start + "#[tool(".len();

        // Find the matching `)]` for this attribute. Simple brace counter
        // scoped to parens.
        let mut paren_depth = 1usize;
        let mut attr_end: Option<usize> = None;
        let mut in_string = false;
        let mut escape_next = false;
        let bytes = source.as_bytes();
        let mut i = cursor;
        while i < bytes.len() {
            let b = bytes[i];
            if escape_next {
                escape_next = false;
            } else if b == b'\\' && in_string {
                escape_next = true;
            } else if b == b'"' {
                in_string = !in_string;
            } else if !in_string {
                if b == b'(' {
                    paren_depth += 1;
                } else if b == b')' {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        attr_end = Some(i);
                        break;
                    }
                }
            }
            i += 1;
        }

        let attr_end = match attr_end {
            Some(e) => e,
            None => break,
        };
        let attr_body = &source[cursor..attr_end];
        cursor = attr_end + 1;
        // Skip the `]` that closes the `#[...]` attribute after the `)`.
        if source.as_bytes().get(cursor) == Some(&b']') {
            cursor += 1;
        }

        // Extract the description = "..." string from the attr body.
        let description = match extract_description_attr(attr_body) {
            Some(d) => d,
            None => continue,
        };

        // Find the tool name — the `async fn <name>` that follows the
        // closing `)]`.
        let after_attr = &source[cursor..];
        let tool_name = match find_following_fn_name(after_attr) {
            Some(n) => n,
            None => continue,
        };

        out.push((tool_name, description));
    }
    out
}

/// Extract the `description = "..."` string from an attribute body.
/// Handles adjacent string literals concatenated by whitespace (Rust
/// parser joins `"a" "b"` into `"ab"`).
fn extract_description_attr(attr_body: &str) -> Option<String> {
    let desc_marker = "description";
    let marker_idx = attr_body.find(desc_marker)?;
    let after = &attr_body[marker_idx + desc_marker.len()..];
    // Skip whitespace + `=` + whitespace.
    let after = after.trim_start();
    let after = after.strip_prefix('=')?;
    let after = after.trim_start();

    // Parse one-or-more adjacent string literals (Rust concatenates them).
    let mut out = String::new();
    let mut rest = after;
    loop {
        if !rest.starts_with('"') {
            break;
        }
        let (piece, remainder) = read_quoted_string(rest)?;
        out.push_str(&piece);
        rest = remainder.trim_start();
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Read a quoted Rust string literal starting at the beginning of `input`.
/// Returns (unescaped content, remainder after the closing quote).
fn read_quoted_string(input: &str) -> Option<(String, &str)> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut out = String::new();
    let mut i = 1;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' {
            // Simple escape: just push the next char without interpretation.
            // Adequate for our purposes — we're scoring char count, not
            // executing the string.
            if i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            return None;
        }
        if b == b'"' {
            return Some((out, &input[i + 1..]));
        }
        out.push(b as char);
        i += 1;
    }
    None
}

/// Find the tool-name from the `async fn <name>` declaration that follows
/// the attribute. Scans through intervening whitespace, pub visibility
/// modifiers, and `async` keyword.
fn find_following_fn_name(after_attr: &str) -> Option<String> {
    let after = after_attr.trim_start();
    // Skip `pub` or `pub(crate)` if present.
    let after = after.strip_prefix("pub").map(|s| s.trim_start()).unwrap_or(after);
    let after = if after.starts_with("(crate)") {
        after.strip_prefix("(crate)")?.trim_start()
    } else {
        after
    };
    // Skip `async` if present.
    let after = after.strip_prefix("async").map(|s| s.trim_start()).unwrap_or(after);
    // Now we should be at `fn <name>`.
    let after = after.strip_prefix("fn")?.trim_start();
    // Read identifier.
    let name_end = after
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(after.len());
    if name_end == 0 {
        return None;
    }
    Some(after[..name_end].to_string())
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

    fn write_subagent(brain_root: &Path, basename: &str, body: &str) {
        let dir = brain_root.join(".claude").join("subagents");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(basename), body).unwrap();
    }

    fn write_registry(brain_root: &Path, registry: Value) {
        let dir = brain_root.join(".claude");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("brain-registry.json"),
            serde_json::to_string_pretty(&registry).unwrap(),
        )
        .unwrap();
    }

    fn write_rust_source(brain_root: &Path, rel: &str, body: &str) {
        let path = brain_root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
    }

    #[tokio::test]
    async fn missing_skills_dir_yields_score_100() {
        let tmp = TempDir::new().unwrap();
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["score"], 100);
        assert_eq!(result["total_skills"], 0);
        assert_eq!(result["total_capabilities"], 0);
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
        assert_eq!(result["total_capabilities"], 1);
    }

    #[tokio::test]
    async fn under_described_skill_is_penalized() {
        let tmp = TempDir::new().unwrap();
        let body = "# coherence\n\n\
                    **Purpose:** cross-domain.\n\n\
                    ## When to Use This Skill\n\n\
                    - item\n- item\n- item\n- item\n\n\
                    ## More content\n\nfoo\n";
        write_skill(tmp.path(), "coherence.md", body);
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["under_described_count"], 1);
        assert_eq!(result["score"], 0);
        let findings = result["findings"].as_array().unwrap();
        assert!(findings
            .iter()
            .any(|f| f["name"] == "under-described:skill:coherence.md"));
    }

    #[tokio::test]
    async fn missing_when_to_use_signal_gets_partial_credit() {
        let tmp = TempDir::new().unwrap();
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
        assert_eq!(result["score"], 50);
    }

    #[tokio::test]
    async fn subagents_are_scored_when_dir_exists() {
        let tmp = TempDir::new().unwrap();
        let body = "# Explore\n\n\
                    **When to use this skill:** You need to explore a codebase for specific \
                    patterns or file types. The Explore subagent is fast at keyword search \
                    and glob-based discovery across many files.\n\n\
                    ## Invocation\n\ncontent.\n";
        write_subagent(tmp.path(), "explore.md", body);
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        let breakdown = &result["capability_breakdown"];
        assert_eq!(breakdown["subagents"]["total"], 1);
        assert_eq!(breakdown["subagents"]["compliant"], 1);
        assert_eq!(result["total_capabilities"], 1);
    }

    #[tokio::test]
    async fn hats_scored_from_registry() {
        let tmp = TempDir::new().unwrap();
        write_registry(
            tmp.path(),
            json!({
                "config": {
                    "hats": {
                        "engineer": {
                            "description": "Active development — emphasize test health and build quality"
                        },
                        "bad": {
                            "description": "too terse"
                        }
                    }
                }
            }),
        );
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        let breakdown = &result["capability_breakdown"];
        assert_eq!(breakdown["hats"]["total"], 2);
        assert_eq!(breakdown["hats"]["compliant"], 1);
        assert_eq!(breakdown["hats"]["under-described"], 1);
    }

    #[tokio::test]
    async fn correlations_scored_from_registry() {
        let tmp = TempDir::new().unwrap();
        write_registry(
            tmp.path(),
            json!({
                "config": {
                    "correlations": [
                        {
                            "id": "good",
                            "description": "Deploy pipeline is healthy while test baseline is degraded — can ship, cannot verify.",
                            "insight": "Raise test-health above 40 before treating deploy-readiness as reliable."
                        },
                        {
                            "id": "missing-insight",
                            "description": "Some correlation description that is plenty long enough to pass.",
                            "insight": ""
                        },
                        {
                            "id": "empty"
                        }
                    ]
                }
            }),
        );
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        let breakdown = &result["capability_breakdown"];
        assert_eq!(breakdown["correlations"]["total"], 3);
        assert_eq!(breakdown["correlations"]["compliant"], 1);
        assert_eq!(breakdown["correlations"]["partial"], 1);
        assert_eq!(breakdown["correlations"]["under-described"], 1);
    }

    #[tokio::test]
    async fn personas_scored_from_registry() {
        let tmp = TempDir::new().unwrap();
        write_registry(
            tmp.path(),
            json!({
                "config": {
                    "human_personas": {
                        "exec": {
                            "description": "C-suite and stakeholders — score + top risk only"
                        },
                        "tiny": {
                            "description": "too short"
                        }
                    }
                }
            }),
        );
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        let breakdown = &result["capability_breakdown"];
        assert_eq!(breakdown["personas"]["total"], 2);
        assert_eq!(breakdown["personas"]["compliant"], 1);
        assert_eq!(breakdown["personas"]["under-described"], 1);
    }

    #[tokio::test]
    async fn tools_scored_from_rust_source() {
        let tmp = TempDir::new().unwrap();
        // Must contain `impl ServerHandler for` so our scanner recognizes
        // this as a real MCP tool server file (vs a test fixture).
        let src = r##"
pub struct Fake;
impl ServerHandler for Fake {}

#[tool(description = "A well-described tool that tells the agent exactly when to invoke it and for what purpose.")]
async fn good_tool(params: Params<()>) -> String { "ok".into() }

#[tool(description = "too short")]
async fn bad_tool(params: Params<()>) -> String { "ok".into() }
"##;
        write_rust_source(tmp.path(), "src/lib.rs", src);
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        let breakdown = &result["capability_breakdown"];
        assert_eq!(breakdown["tools"]["total"], 2);
        assert_eq!(breakdown["tools"]["compliant"], 1);
        assert_eq!(breakdown["tools"]["under-described"], 1);
    }

    #[test]
    fn parse_tool_attrs_handles_multi_line_form() {
        let src = r##"
#[tool(
    description = "This is a multi-line \
                   description that spans \
                   several lines in source."
)]
async fn multi_line(params: Params<()>) -> String { "ok".into() }
"##;
        let pairs = parse_tool_attrs(src);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "multi_line");
        assert!(pairs[0].1.contains("multi-line"));
    }

    #[test]
    fn parse_tool_attrs_handles_inline_form() {
        let src = r##"
#[tool(description = "Inline description here.")]
async fn inline_form(params: Params<()>) -> String { "ok".into() }
"##;
        let pairs = parse_tool_attrs(src);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "inline_form");
        assert_eq!(pairs[0].1, "Inline description here.");
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

    #[tokio::test]
    async fn mixed_corpus_aggregates_correctly() {
        let tmp = TempDir::new().unwrap();

        // 1 compliant skill + 1 under-described skill = 10/20 → 50 on the
        // skills axis alone.
        let good = "# Good\n\n\
                    **When to use this skill:** Operators need a working example of a \
                    compliant lead paragraph that meets the length and signal criteria. \
                    This is that example — long enough to clear the 40-token floor and \
                    carrying an explicit when-to-use signal at the top.\n\n\
                    ## Body\n\ncontent.\n";
        write_skill(tmp.path(), "good.md", good);

        let bad = "# Bad\n\n\
                   ## Skipped straight to the section header\n\n\
                   body content here.\n";
        write_skill(tmp.path(), "bad.md", bad);

        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["total_skills"], 2);
        assert_eq!(result["compliant_count"], 1);
        assert_eq!(result["under_described_count"], 1);
        assert_eq!(result["score"], 50);
        assert_eq!(result["total_capabilities"], 2);
    }
}
