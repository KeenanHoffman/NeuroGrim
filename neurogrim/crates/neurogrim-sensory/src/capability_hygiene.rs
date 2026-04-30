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
//!   - **hat_contracts** (`.claude/skills/hats/*.md`, E-B2-3 C5/C6):
//!     persona-hat contract validation against `hat-contract-v1.schema.json`
//!     (Hat-model B per spec §5.4.1). Advisory-only — findings carry
//!     `points: 0` (Q3); contributes to `total_capabilities` +
//!     `overall_compliant_count` rollups (C6) but NOT to the hygiene
//!     score numerator/denominator (`earned`/`possible` remain 0 for the
//!     type).
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

// ── E-B2-3 C5 (2026-04-27): persona-hat contract vocabulary ─────────────────
//
// Closed-set tool-name vocabulary for `forbidden_tools` / `allowed_tools` in
// persona-hat contracts (spec §5.4.1, Q1 = closed-set v1). MUST mirror the
// schema's `definitions.ToolName.enum` in
// `LSP-Brains/schemas/hat-contract-v1.schema.json`. Drift between this const
// and the schema is caught by the `hat_contract_schema_conformance` test
// (which pins the schema's vocabulary independently). Adding new vocabulary
// requires both a schema bump and a const bump — same additivity discipline
// as `culture-manifest-v1.values`.
//
// Used by `score_hat_contracts` to recognize unknown vocabulary terms cleanly
// when walking jsonschema validation errors, so a vocabulary violation
// surfaces as `hat_contract:vocabulary:<hat>:<term>` rather than collapsing
// into the generic `hat_contract:declaration:<hat>` malformed bucket.
const HAT_CONTRACT_TOOL_VOCABULARY: &[&str] = &[
    "Bash",
    "Write",
    "Edit",
    "WebFetch",
    "WebSearch",
    "network_egress",
    "mcp:*",
    "package_install",
];

/// Embedded persona-hat contract schema. v3.2.2: schema vendored into
/// `data/schemas/` so it resolves in `cargo publish` tarballs (the
/// LSP-Brains sibling repo isn't included in published crates).
/// Canonical source remains `LSP-Brains/schemas/hat-contract-v1.schema.json`;
/// drift between the two copies is caught by the schema-conformance tests.
const HAT_CONTRACT_SCHEMA_JSON: &str = include_str!(
    "../data/schemas/hat-contract-v1.schema.json"
);

/// Correlation: minimum description length (chars).
const CORRELATION_MIN_DESC_CHARS: usize = 60;

/// Correlation: minimum insight length (chars).
const CORRELATION_MIN_INSIGHT_CHARS: usize = 20;

// ── Axis 4 v1 (2026-04-22): invocation ledger + dead-skill detection ─────────

/// Days without invocation before a skill is flagged dead (default).
/// Skills can opt out of this default by declaring
/// `usage-rarity: rare` in their frontmatter, which bumps to
/// `DEAD_WINDOW_DAYS_RARE`.
const DEAD_WINDOW_DAYS_DEFAULT: i64 = 90;

/// Extended window for skills marked `usage-rarity: rare`. Examples:
/// incident-response, rollback-deployment — invoked once a quarter/year.
const DEAD_WINDOW_DAYS_RARE: i64 = 365;

/// Grace period after a skill file is created. Freshly-authored skills
/// get this window to accrue invocations before being flagged dead.
/// File mtime is the proxy for skill age (imperfect after a fresh clone
/// but acceptable for v1 — see plan-critic notes).
const GRACE_PERIOD_DAYS: i64 = 30;

/// Below this total invocation count across the ledger, dead findings
/// are marked `low_confidence`. Small sample sizes produce noisy signals.
const LOW_CONFIDENCE_TOTAL_INVOCATIONS: usize = 20;

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
    // E-B2-3 C5: persona-hat contracts (Hat-model B per spec §5.4.1) —
    // distinct axis from registry-hats (`score_hats` above). Findings flow
    // through to CMDB extras + breakdown JSON.
    //
    // E-B2-3 C6 (2026-04-27): hat_contracts now contributes to
    // `total_capabilities` + `overall_compliant_count`. `earned`/`possible`
    // remain 0 for every per-hat result (hat_contracts is advisory per Q3:
    // findings carry `points: 0` and the type does NOT influence the score
    // numerator/denominator — only the "how many capabilities exist /
    // validate cleanly" denominators).
    let hat_contracts = score_hat_contracts(&root);

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
        + personas.total
        + hat_contracts.total;
    let overall_compliant: usize = skills.compliant
        + subagents.compliant
        + tools.compliant
        + hats.compliant
        + correlations.compliant
        + personas.compliant
        + hat_contracts.compliant;

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
    // E-B2-3 C5: hat-contract findings (advisory, points: 0 each — they
    // surface in CMDB extras but never deduct hygiene score, per Q3).
    all_findings.extend(hat_contracts.findings.iter().cloned());

    // If aggregate compliant == total and we have at least one capability,
    // add a green "all-capabilities-compliant" finding.
    if total_capabilities > 0 && overall_compliant == total_capabilities {
        all_findings.push(Finding {
            name: "all-capabilities-compliant".to_string(),
            status: "ok".to_string(),
            points: 0,
            detail: Some(format!(
                "All {total_capabilities} capabilities across 7 types meet their authoring standards."
            )),
        });
    }

    // Backward-compat: existing brain-registry.json exported_variables
    // reference these top-level skill-specific fields.
    let skills_status = |s: &str| skills.status_counts.get(s).copied().unwrap_or(0);

    let breakdown = json!({
        "skills":         skills.to_breakdown(),
        "subagents":      subagents.to_breakdown(),
        "tools":          tools.to_breakdown(),
        "hats":           hats.to_breakdown(),
        "correlations":   correlations.to_breakdown(),
        "personas":       personas.to_breakdown(),
        // E-B2-3 C5/C6: persona-hat contracts. C6 (2026-04-27) folded
        // hat_contracts.total + hat_contracts.compliant into the top-level
        // `total_capabilities` / `overall_compliant_count` rollups so the
        // hat-contract axis influences the "how many capabilities exist /
        // validate cleanly" denominators. The `earned`/`possible` axis
        // remains untouched — hat_contracts is advisory per Q3 (findings
        // carry `points: 0`; the type does NOT influence the hygiene
        // score numerator/denominator).
        "hat_contracts":  hat_contracts.to_breakdown(),
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

    build_cmdb("capability-hygiene", score, all_findings, Some(extras), None)
}

// ── Scorer: skills ────────────────────────────────────────────────────────────

fn score_skills(root: &Path) -> TypeResult {
    let mut result = TypeResult::new("skills");
    let dir = root.join(".claude").join("skills");

    // Tier A partial (2026-04-22): scan BOTH legacy `.claude/skills/
    // <name>.md` AND plugin `.claude/skills/<name>/SKILL.md` formats.
    let entries = collect_skill_entries(&dir);
    if entries.is_empty() {
        return result;
    }

    // Axis 4 v1: read the invocation ledger once and classify each
    // skill as alive/dead/new. Dead findings are ADVISORY — they don't
    // subtract hygiene points (hygiene = description quality; usage =
    // separate axis). Findings surface the observation; operator reads
    // + decides.
    let now = chrono::Utc::now();
    let ledger = read_invocation_ledger(root, now);
    let low_confidence = ledger.total_invocations < LOW_CONFIDENCE_TOTAL_INVOCATIONS;

    // Per-type usage-status tally for the CMDB breakdown extras.
    let mut alive_count = 0usize;
    let mut dead_count = 0usize;
    let mut new_count = 0usize;
    let mut legacy_count = 0usize;
    let mut plugin_count = 0usize;

    for entry in entries {
        let SkillFileEntry {
            skill_id,
            display_path,
            body,
            format,
        } = entry;
        match format {
            SkillFormat::Legacy => legacy_count += 1,
            SkillFormat::Plugin => plugin_count += 1,
        }

        // Select routing text based on format:
        //  - Plugin: description + when_to_use from YAML frontmatter (if
        //    parseable and non-empty).
        //  - Legacy (or malformed plugin): lead paragraph fallback.
        //
        // `plugin_has_when_field` is true iff the plugin frontmatter
        // explicitly declared a non-empty `when_to_use:` key. That's an
        // unambiguous routing signal, so it satisfies the when-to-use
        // requirement even when the free-text doesn't match the regex.
        let (description_source, plugin_has_when_field): (String, bool) = match format {
            SkillFormat::Plugin => match extract_skill_md_routing_text(&body) {
                Some((text, has_field)) => (text, has_field),
                None => (extract_description_block(&body).to_string(), false),
            },
            SkillFormat::Legacy => (extract_description_block(&body).to_string(), false),
        };
        let description: &str = &description_source;
        let chars = description.chars().count();
        let tokens = chars / CHARS_PER_TOKEN;
        let has_signal = plugin_has_when_field || detect_when_to_use(description);

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

        // Axis 4 v1: usage classification.
        //
        // Rarity lookup is format-specific: legacy uses `usage-rarity:`
        // frontmatter line in the lead paragraph; plugin skills declare
        // it as a proper YAML frontmatter key. `parse_usage_rarity`
        // handles both (it operates on the raw body string — the plugin
        // YAML frontmatter is a superset that still includes
        // `usage-rarity` if present as a field).
        let rarity = parse_usage_rarity(&body);
        let window_days = if rarity == "rare" {
            DEAD_WINDOW_DAYS_RARE
        } else {
            DEAD_WINDOW_DAYS_DEFAULT
        };

        // Ledger lookup uses `skill_id` (no extension, no path prefix)
        // which matches both (a) `tool_input.skill` for plugin skills,
        // and (b) the hook-extracted name for legacy once the migration
        // corrects the capture path. Fall back to display_path for
        // historical legacy ledger entries.
        let usage = ledger
            .per_skill
            .get(&skill_id)
            .or_else(|| ledger.per_skill.get(&display_path))
            .cloned()
            .unwrap_or_default();
        let invocations_in_window = if rarity == "rare" {
            usage.count_rare_window
        } else {
            usage.count_default_window
        };
        let age_days = skill_age_days_for(root, &skill_id, format, now);
        let usage_status: &'static str = if age_days < GRACE_PERIOD_DAYS {
            new_count += 1;
            "new"
        } else if invocations_in_window == 0 {
            dead_count += 1;
            "dead"
        } else {
            alive_count += 1;
            "alive"
        };

        let format_str = match format {
            SkillFormat::Legacy => "legacy",
            SkillFormat::Plugin => "plugin",
        };

        let detail = json!({
            "path": display_path,
            "skill_id": skill_id,
            "format": format_str,
            "description_chars": chars,
            "description_tokens_approx": tokens,
            "has_when_to_use_signal": has_signal,
            "status": status,
            "points": points,
            "usage_rarity": rarity,
            "usage_status": usage_status,
            "window_days": window_days,
            "invocations_in_window": invocations_in_window,
            "last_invoked": usage.last_invoked.map(|t| t.to_rfc3339()),
            "skill_age_days": if age_days == i64::MAX { Value::Null } else { json!(age_days) },
        });

        // Hygiene finding — subtracts points as before.
        let finding = if points < 10 {
            Some(Finding {
                name: format!("{status}:skill:{display_path}"),
                status: status.to_string(),
                points: -((10 - points) as i32),
                detail: Some(reason),
            })
        } else {
            None
        };

        result.add(detail, finding, points, status);

        // Dead finding — ADVISORY, zero points deducted. Always surfaced
        // unless low-confidence (total ledger below threshold), in which
        // case the finding name is prefixed `low-confidence:`.
        if usage_status == "dead" {
            let confidence_prefix = if low_confidence { "low-confidence-" } else { "" };
            let window_hint = if rarity == "rare" { "365-day rare" } else { "90-day" };
            result.findings.push(Finding {
                name: format!("{confidence_prefix}dead-skill:{display_path}"),
                status: "dead".to_string(),
                points: 0,
                detail: Some(format!(
                    "`{display_path}` has 0 invocations in the {window_hint} window. \
                     skill_age_days={age_days}, total_ledger_invocations={}. \
                     {}",
                    ledger.total_invocations,
                    if low_confidence {
                        "Ledger sample size is below the confidence threshold \
                         — treat this as a weak signal."
                    } else {
                        "Combine with hygiene score before acting: dead + \
                         low hygiene = probably misdescribed; dead + high \
                         hygiene = possibly genuine niche (consider \
                         `usage-rarity: rare` or archive)."
                    }
                )),
            });
        }
    }

    // Stash usage aggregation into the TypeResult as a special extras
    // channel. The main orchestration reads this and injects it into
    // the capability_breakdown.skills block.
    result.status_counts.insert("usage:alive", alive_count);
    result.status_counts.insert("usage:dead", dead_count);
    result.status_counts.insert("usage:new", new_count);
    result.status_counts.insert("ledger:total_invocations", ledger.total_invocations);
    result.status_counts.insert(
        "ledger:low_confidence",
        if low_confidence { 1 } else { 0 },
    );
    // Tier A partial: surface per-format counts so operators can watch
    // the migration progress over time.
    result.status_counts.insert("format:legacy", legacy_count);
    result.status_counts.insert("format:plugin", plugin_count);

    result
}

/// Format-aware file-age resolver. Legacy skills live at
/// `.claude/skills/<skill_id>.md`; plugin skills live at
/// `.claude/skills/<skill_id>/SKILL.md`.
fn skill_age_days_for(
    root: &Path,
    skill_id: &str,
    format: SkillFormat,
    now: chrono::DateTime<chrono::Utc>,
) -> i64 {
    let path = match format {
        SkillFormat::Legacy => root
            .join(".claude")
            .join("skills")
            .join(format!("{skill_id}.md")),
        SkillFormat::Plugin => root
            .join(".claude")
            .join("skills")
            .join(skill_id)
            .join("SKILL.md"),
    };
    let meta = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(_) => return i64::MAX,
    };
    let modified = match meta.modified() {
        Ok(t) => t,
        Err(_) => return i64::MAX,
    };
    let modified_dt: chrono::DateTime<chrono::Utc> = modified.into();
    (now - modified_dt).num_days()
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

// ── Scorer: hat contracts (persona-hat .md frontmatter) ─────────────────────
//
// E-B2-3 C5 (2026-04-27). Static-only validator (per Q9) that scans
// `<root>/.claude/skills/hats/*.md` (skipping `SKILL.md` — that's the
// catalog index, not a per-hat contract) and validates each file's YAML
// frontmatter against `hat-contract-v1.schema.json`. Persona-hat = Hat-
// model B in the §5.4.1 glossary; distinct from registry-hats (Hat-model
// A) scored above by `score_hats`.
//
// All findings emit `points: 0` (advisory per Q3). v1 ships two finding
// kinds (per Q3): `hat_contract:declaration:<hat>` (no frontmatter / other
// schema-level malformedness) and `hat_contract:vocabulary:<hat>:<term>`
// (frontmatter declares an unknown closed-set term). The third Q3 finding
// kind, `hat_contract:violation:<hat>:<observed_tool>`, is deferred to v2
// per BACKLOG B-23 (runtime enforcement requires invocation-ledger
// extension that v1 doesn't ship).
//
// Recursion-guard (Q6 hard rule): this function MUST be pure file-read +
// JSON-Schema-validate. It does NOT shell out, does NOT invoke any tool,
// does NOT use `std::process::Command`. The validator is plain code, not
// a hat. A unit test in `tests/hat_contract_sensor_behavior.rs` pins this
// invariant by reading the source file and asserting the absence of
// shell-execution references inside this function's span.

/// Validate persona-hat contracts under `<root>/.claude/skills/hats/`.
fn score_hat_contracts(root: &Path) -> TypeResult {
    let mut result = TypeResult::new("hat_contracts");
    let dir = root.join(".claude").join("skills").join("hats");
    if !dir.is_dir() {
        // No hats catalog on this Brain — clean zero state, no findings.
        return result;
    }

    // Compile the embedded schema once per call. The schema text is
    // embedded at compile time via `include_str!`; if the file isn't
    // reachable at compile time, the build fails. This is intentional:
    // v1 NeuroGrim ships with the bundled LSP-Brains submodule and we
    // don't want a silent fallback for a missing schema.
    let compiled_schema: Option<jsonschema::JSONSchema> =
        compile_hat_contract_schema_inline();

    for hat_md in collect_hat_contract_files(&dir) {
        let HatContractFile { hat_name, body } = hat_md;

        // Step 1: extract frontmatter. Missing fences ⇒ neutral
        // declaration finding (per Q4: permissive default + advisory).
        let yaml_text = match extract_hat_frontmatter(&body) {
            HatFrontmatter::Found(yaml) => yaml,
            HatFrontmatter::Missing | HatFrontmatter::Unterminated => {
                let detail_msg = format!(
                    "`{hat_name}.md` has no parseable YAML frontmatter. \
                     Per Q4, the hat is treated as `forbidden_tools: []` + \
                     `allowed_tools: [\"*\"]` (permissive default) but is \
                     surfaced as an advisory to encourage authoring."
                );
                let detail = json!({
                    "hat": hat_name,
                    "kind": "declaration",
                    "reason": "no_frontmatter",
                    "points": 0,
                });
                let finding = Finding {
                    name: format!("hat_contract:declaration:{hat_name}"),
                    status: "neutral".to_string(),
                    points: 0,
                    detail: Some(detail_msg),
                };
                // `total` increments; `compliant` does NOT (no schema
                // validation passed); `earned`/`possible` both 0 — the
                // type does not contribute to the aggregate score
                // (advisory-only per Q3).
                result.total += 1;
                *result.status_counts.entry("declaration").or_insert(0) += 1;
                result.details.push(detail);
                result.findings.push(finding);
                continue;
            }
        };

        // Step 2: parse YAML into a JSON value (jsonschema validates
        // serde_json::Value — YAML is a strict superset for our schema).
        let instance: Value = match parse_yaml_as_json(&yaml_text) {
            Some(v) => v,
            None => {
                let detail_msg = format!(
                    "`{hat_name}.md` frontmatter fences present but body \
                     failed YAML parse. Treating as malformed declaration."
                );
                let detail = json!({
                    "hat": hat_name,
                    "kind": "declaration",
                    "reason": "yaml_parse_error",
                    "points": 0,
                });
                let finding = Finding {
                    name: format!("hat_contract:declaration:{hat_name}"),
                    status: "error".to_string(),
                    points: 0,
                    detail: Some(detail_msg),
                };
                result.total += 1;
                *result.status_counts.entry("declaration").or_insert(0) += 1;
                result.details.push(detail);
                result.findings.push(finding);
                continue;
            }
        };

        // Step 3: validate against the embedded schema. If the schema
        // itself failed to compile (cosmic-ray-grade build issue), skip
        // validation and surface a neutral declaration finding so the
        // operator notices.
        let Some(schema) = compiled_schema.as_ref() else {
            let detail = json!({
                "hat": hat_name,
                "kind": "declaration",
                "reason": "schema_unavailable",
                "points": 0,
            });
            let finding = Finding {
                name: format!("hat_contract:declaration:{hat_name}"),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(
                    "hat-contract schema failed to compile at runtime; \
                     skipping validation for this hat."
                        .to_string(),
                ),
            };
            result.total += 1;
            *result.status_counts.entry("declaration").or_insert(0) += 1;
            result.details.push(detail);
            result.findings.push(finding);
            continue;
        };

        let validation = schema.validate(&instance);
        match validation {
            Ok(()) => {
                // Well-formed contract. Compliant; emit no finding.
                let detail = json!({
                    "hat": hat_name,
                    "kind": "compliant",
                    "points": 0,
                });
                result.total += 1;
                result.compliant += 1;
                *result.status_counts.entry("compliant").or_insert(0) += 1;
                result.details.push(detail);
            }
            Err(errors) => {
                // Walk validation errors. Vocabulary errors get specific
                // findings; everything else collapses into one generic
                // declaration-malformed finding (per Q3).
                let mut emitted_vocabulary = false;
                let mut emitted_declaration = false;
                let mut declaration_reason: Option<String> = None;

                for err in errors {
                    let path_str = err.instance_path.to_string();
                    let is_vocab_path = path_str.contains("/forbidden_tools/")
                        || path_str.contains("/allowed_tools/");
                    let is_enum_kind = matches!(
                        err.kind,
                        jsonschema::error::ValidationErrorKind::Enum { .. }
                    );

                    if is_vocab_path && is_enum_kind {
                        // Pull the offending term out of the instance
                        // value at the failed path. For `Enum { ... }`
                        // on an array item, `err.instance` is the
                        // string value that failed the enum check.
                        let offending_term = err
                            .instance
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| err.instance.to_string());
                        let finding = Finding {
                            name: format!(
                                "hat_contract:vocabulary:{hat_name}:{offending_term}"
                            ),
                            status: "error".to_string(),
                            points: 0,
                            detail: Some(format!(
                                "`{hat_name}.md` declares `{offending_term}` \
                                 in {path_str} — not in the closed-set \
                                 vocabulary (Q1). Allowed terms: {:?}.",
                                HAT_CONTRACT_TOOL_VOCABULARY
                            )),
                        };
                        let detail = json!({
                            "hat": hat_name,
                            "kind": "vocabulary",
                            "term": offending_term,
                            "path": path_str,
                            "points": 0,
                        });
                        *result.status_counts.entry("vocabulary").or_insert(0) += 1;
                        result.details.push(detail);
                        result.findings.push(finding);
                        emitted_vocabulary = true;
                    } else if !emitted_declaration {
                        // First non-vocabulary error becomes the single
                        // declaration finding for this hat. Subsequent
                        // errors are surfaced via the detail text but
                        // not as additional findings (per Q3 — only
                        // declaration + vocabulary kinds in v1).
                        declaration_reason = Some(format!(
                            "schema validation failed at {path_str}: {err}"
                        ));
                        emitted_declaration = true;
                    }
                }

                // Emit the declaration finding if we recorded a non-
                // vocabulary failure.
                if emitted_declaration {
                    let reason = declaration_reason.unwrap_or_else(|| {
                        "schema validation failed (no diagnostic captured)"
                            .to_string()
                    });
                    let detail = json!({
                        "hat": hat_name,
                        "kind": "declaration",
                        "reason": "malformed",
                        "points": 0,
                    });
                    let finding = Finding {
                        name: format!("hat_contract:declaration:{hat_name}"),
                        status: "error".to_string(),
                        points: 0,
                        detail: Some(reason),
                    };
                    *result.status_counts.entry("declaration").or_insert(0) += 1;
                    result.details.push(detail);
                    result.findings.push(finding);
                }

                // total counts the hat regardless of finding kind.
                result.total += 1;
                // Not compliant — schema failed; suppress error if a
                // vocabulary finding was the only kind emitted (still
                // not compliant in the strict sense).
                let _ = emitted_vocabulary;
            }
        }
    }

    result
}

// E-B2-3 C5 helper types + functions for `score_hat_contracts`.

#[derive(Debug)]
struct HatContractFile {
    /// Hat identifier — filename stem (e.g., `supply-chain-auditor` for
    /// `supply-chain-auditor.md`). Used in finding names.
    hat_name: String,
    /// Raw file body (frontmatter + prose).
    body: String,
}

#[derive(Debug)]
enum HatFrontmatter {
    /// Frontmatter delimiters present + body returned. The caller YAML-
    /// parses + schema-validates.
    Found(String),
    /// No leading `---` fence. Treated as a neutral "no contract" state
    /// per Q4 (NOT a schema-validation failure).
    Missing,
    /// Leading `---` fence present but no closing `---` before EOF.
    /// Surfaced as a malformed declaration finding (similar to Missing
    /// at the finding-name level — they're both "no validatable contract
    /// present" — but the detail text differs).
    Unterminated,
}

/// Walk `<dir>/*.md`, returning per-hat contract files. `SKILL.md` is
/// excluded — that's the 287-line catalog index (Hat-model B prose),
/// NOT a per-hat contract. The 287-line catalog has prose like "MUST NOT
/// execute code" embedded in markdown that would otherwise be picked up
/// as malformed frontmatter; the SKILL.md exclusion is structurally
/// load-bearing for E-B2-3 v1.
///
/// Also skips `README*` and dotfiles (mirrors `collect_md_files` for
/// consistency).
fn collect_hat_contract_files(dir: &Path) -> Vec<HatContractFile> {
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
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // SKILL.md is the catalog index, not a per-hat contract — skip
        // it so the validator doesn't churn on its prose.
        if name == "SKILL.md" {
            continue;
        }
        if name.starts_with("README") || name.starts_with('.') {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Ok(body) = std::fs::read_to_string(&path) {
            out.push(HatContractFile {
                hat_name: stem.to_string(),
                body,
            });
        }
    }
    out.sort_by(|a, b| a.hat_name.cmp(&b.hat_name));
    out
}

/// Extract YAML frontmatter delimited by `---` lines at the start of a
/// markdown file. Mirrors the test-side `extract_frontmatter` shape
/// (Found / Missing / Unterminated) used by
/// `tests/hat_contract_schema_conformance.rs`. Production-side mirror
/// rather than a shared helper because: (a) test-helpers live under
/// `tests/test_support/mod.rs` and are not visible to production code
/// without a deliberate refactor (would expose internals beyond v1's
/// scope); (b) the production extractor is small enough that the
/// duplication cost is lower than the refactor cost. If a third
/// consumer arrives, factor into a shared internal module then.
///
/// Tolerates both `\n` and `\r\n` line endings — mirrors the test
/// helper's CRLF-on-Windows accommodation.
fn extract_hat_frontmatter(markdown: &str) -> HatFrontmatter {
    let mut lines = markdown.split_inclusive('\n');
    let first = match lines.next() {
        Some(l) => l.trim_end(),
        None => return HatFrontmatter::Missing,
    };
    if first != "---" {
        return HatFrontmatter::Missing;
    }
    let mut yaml = String::new();
    for line in lines {
        if line.trim_end() == "---" {
            return HatFrontmatter::Found(yaml);
        }
        yaml.push_str(line);
    }
    HatFrontmatter::Unterminated
}

/// Parse a YAML string into a `serde_json::Value` so it can feed the
/// jsonschema validator (which is JSON-typed). Returns `None` on YAML
/// parse error; the caller emits a `declaration` finding in that case.
fn parse_yaml_as_json(yaml: &str) -> Option<Value> {
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml).ok()?;
    serde_json::to_value(yaml_value).ok()
}

/// Parse + compile the embedded `hat-contract-v1.schema.json` into a
/// `JSONSchema`. Returns `None` only on the (cosmic-ray-grade) case where
/// the embedded schema text is malformed JSON or fails to compile —
/// neither of which can happen if the LSP-Brains schema file is well-
/// formed at NeuroGrim build time. Returning `Option` rather than
/// panicking keeps the validator a no-op in degraded build environments
/// rather than crashing the entire `analyze_capability_hygiene` call.
fn compile_hat_contract_schema_inline() -> Option<jsonschema::JSONSchema> {
    let parsed: Value = serde_json::from_str(HAT_CONTRACT_SCHEMA_JSON).ok()?;
    jsonschema::JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&parsed)
        .ok()
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

// ── Tier A partial (2026-04-22): SKILL.md plugin-format support ──────────────

/// A skill entry discovered under `.claude/skills/`. The scanner returns
/// one of two shapes depending on how the skill is laid out on disk.
#[derive(Debug, Clone)]
struct SkillFileEntry {
    /// Skill identifier — the name without any extension or path prefix.
    /// Used both for ledger lookup (matches `tool_input.skill` for
    /// plugin skills) and as the cross-Brain comparison key.
    skill_id: String,
    /// Human-visible path shown in CMDB findings. `<name>.md` for
    /// legacy; `<name>/SKILL.md` for plugin.
    display_path: String,
    /// Raw file body.
    body: String,
    /// Format of this skill entry.
    format: SkillFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillFormat {
    /// `.claude/skills/<name>.md` direct-descendant markdown. The lead
    /// paragraph (everything before first `##`) is the routing signal.
    Legacy,
    /// `.claude/skills/<name>/SKILL.md` plugin format. The YAML
    /// frontmatter's `description` (+ optional `when_to_use`) is the
    /// routing signal.
    Plugin,
}

/// Scan `<root>/.claude/skills/` for both legacy `.md` files and
/// modern `<name>/SKILL.md` plugin skills. Skips `archived/`,
/// `README*`, dotfiles.
fn collect_skill_entries(skills_dir: &Path) -> Vec<SkillFileEntry> {
    let mut out: Vec<SkillFileEntry> = Vec::new();
    let entries = match std::fs::read_dir(skills_dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if name.starts_with("README") || name.starts_with('.') || name == "archived" {
            continue;
        }

        if path.is_file() {
            // Legacy: `.claude/skills/<name>.md`.
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let skill_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&name)
                .to_string();
            if let Ok(body) = std::fs::read_to_string(&path) {
                out.push(SkillFileEntry {
                    skill_id,
                    display_path: name,
                    body,
                    format: SkillFormat::Legacy,
                });
            }
        } else if path.is_dir() {
            // Plugin: `.claude/skills/<name>/SKILL.md`.
            let skill_md = path.join("SKILL.md");
            if !skill_md.is_file() {
                continue;
            }
            if let Ok(body) = std::fs::read_to_string(&skill_md) {
                out.push(SkillFileEntry {
                    skill_id: name.clone(),
                    display_path: format!("{name}/SKILL.md"),
                    body,
                    format: SkillFormat::Plugin,
                });
            }
        }
    }
    out.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));
    out
}

/// Extract the SKILL.md routing description.
///
/// Returns `Some((combined_text, has_when_to_use_field))` if valid
/// frontmatter is found with at least one of `description` or
/// `when_to_use` populated. `combined_text` concatenates both fields
/// (when present) for length/budget scoring. `has_when_to_use_field`
/// is the definitive signal — if the YAML has a `when_to_use` field,
/// the skill has declared its activation context explicitly and
/// passes the when-to-use signal check regardless of phrase matching.
///
/// Claude Code truncates the combined text at 1,536 chars in its
/// skill listing (see docs/invocation-ledger.md). We score the raw
/// combined value; over-budget gets flagged, not truncated.
///
/// Returns `None` when frontmatter is absent or both fields are
/// empty — callers fall back to scoring the body as legacy.
fn extract_skill_md_routing_text(body: &str) -> Option<(String, bool)> {
    // Frontmatter is bounded by leading `---` and terminating `---`
    // at the start of a line. Be forgiving with trailing whitespace.
    let trimmed = body.trim_start();
    let rest = trimmed.strip_prefix("---")?;
    // Find the closing `---` on its own line.
    let close_idx = rest.find("\n---")?;
    let frontmatter = &rest[..close_idx];

    let parsed: serde_yaml::Value = serde_yaml::from_str(frontmatter).ok()?;
    let description = parsed
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let when_to_use = parsed
        .get("when_to_use")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let has_when_field = when_to_use.is_some();
    let combined = match (description, when_to_use) {
        (Some(d), Some(w)) => format!("{d}\n\n{w}"),
        (Some(d), None) => d,
        (None, Some(w)) => w,
        (None, None) => return None,
    };
    Some((combined, has_when_field))
}

// ── Axis 4 v1: invocation ledger + skill age + usage-rarity ──────────────────

/// Summary of a skill's invocations from the ledger. Used by
/// `score_skills` to classify alive/dead/new.
#[derive(Debug, Default, Clone)]
struct SkillUsage {
    /// Invocations in the last DEAD_WINDOW_DAYS_DEFAULT.
    count_default_window: usize,
    /// Invocations in the last DEAD_WINDOW_DAYS_RARE.
    count_rare_window: usize,
    /// Most recent invocation timestamp (any age, not windowed).
    last_invoked: Option<chrono::DateTime<chrono::Utc>>,
}

/// Index of per-skill invocation counts + timestamps, plus the total
/// ledger size (used to gate low-confidence findings).
#[derive(Debug, Default)]
struct InvocationIndex {
    per_skill: std::collections::HashMap<String, SkillUsage>,
    total_invocations: usize,
}

/// Read the `_neurogrim/skill-invocations` SQLite bus topic into an
/// index of per-skill usage. The topic is lazily caught up from
/// `{root}/.claude/brain/invocation-ledger.jsonl` (the canonical
/// shell-hook target) on every call — see
/// `neurogrim_core::skill_invocations`. Tolerates missing files,
/// missing fields, and malformed payloads (each silently skipped).
fn read_invocation_ledger(
    root: &Path,
    now: chrono::DateTime<chrono::Utc>,
) -> InvocationIndex {
    use chrono::Duration;
    use neurogrim_core::queue_backend::QueueBackend;
    use neurogrim_core::skill_invocations;

    let mut index = InvocationIndex::default();

    let backend = match skill_invocations::ingest_and_open(root) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("capability-hygiene: skill-invocations ingest failed: {e}");
            return index;
        }
    };
    let total = backend.len().unwrap_or(0);
    if total == 0 {
        return index;
    }

    let default_cutoff = now - Duration::days(DEAD_WINDOW_DAYS_DEFAULT);
    let rare_cutoff = now - Duration::days(DEAD_WINDOW_DAYS_RARE);

    // Read all messages — the rare-window scan is unbounded by design.
    // SQLite full-table read is materially faster than the JSONL parse
    // it replaces (no string-line splitting, no per-line JSON parse).
    let msgs = match backend.read_from(1, total as usize) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("capability-hygiene: skill-invocations read_from failed: {e}");
            return index;
        }
    };

    for stored in msgs {
        let v = &stored.message.payload;
        let name = match v.get("name").and_then(|x| x.as_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let ts_str = match v.get("ts").and_then(|x| x.as_str()) {
            Some(t) => t,
            None => continue,
        };
        let ts = match chrono::DateTime::parse_from_rfc3339(ts_str) {
            Ok(t) => t.with_timezone(&chrono::Utc),
            Err(_) => continue,
        };

        index.total_invocations += 1;

        // Don't window-track invocations older than the longest window.
        if ts < rare_cutoff {
            continue;
        }

        let entry = index.per_skill.entry(name).or_default();
        if ts >= default_cutoff {
            entry.count_default_window += 1;
        }
        entry.count_rare_window += 1;
        entry.last_invoked = Some(match entry.last_invoked {
            Some(prev) if prev > ts => prev,
            _ => ts,
        });
    }

    index
}

/// Parse the optional `usage-rarity:` frontmatter field from a skill's
/// description block. Returns `"rare"` or `"common"` (default).
/// Case-insensitive. Only considers lines within the description block.
fn parse_usage_rarity(description: &str) -> &'static str {
    for line in description.lines() {
        let trimmed = line.trim().to_lowercase();
        if let Some(rest) = trimmed.strip_prefix("usage-rarity:") {
            let val = rest.trim();
            if val == "rare" {
                return "rare";
            }
            return "common";
        }
    }
    "common"
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

    // ── Axis 4 v1: ledger + rarity + dead-skill tests ──

    fn write_ledger(brain_root: &Path, lines: &[&str]) {
        let dir = brain_root.join(".claude").join("brain");
        std::fs::create_dir_all(&dir).unwrap();
        let body = lines.join("\n") + "\n";
        std::fs::write(dir.join("invocation-ledger.jsonl"), body).unwrap();
    }

    #[test]
    fn parse_usage_rarity_defaults_common() {
        assert_eq!(parse_usage_rarity("# Foo\n\ndescription only"), "common");
    }

    #[test]
    fn parse_usage_rarity_detects_rare() {
        let desc = "# Title\n\n**When to use:** x.\n\nusage-rarity: rare\n";
        assert_eq!(parse_usage_rarity(desc), "rare");
    }

    #[test]
    fn parse_usage_rarity_case_insensitive() {
        let desc = "# Title\n\nUsage-Rarity: RARE\n";
        assert_eq!(parse_usage_rarity(desc), "rare");
    }

    #[test]
    fn read_invocation_ledger_missing_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let ix = read_invocation_ledger(tmp.path(), chrono::Utc::now());
        assert_eq!(ix.total_invocations, 0);
        assert!(ix.per_skill.is_empty());
    }

    #[test]
    fn read_invocation_ledger_basic_counts() {
        let tmp = TempDir::new().unwrap();
        let now = chrono::Utc::now();
        let recent = (now - chrono::Duration::days(1)).to_rfc3339();
        let old_in_rare = (now - chrono::Duration::days(200)).to_rfc3339();
        let too_old = (now - chrono::Duration::days(500)).to_rfc3339();
        write_ledger(
            tmp.path(),
            &[
                &format!(r#"{{"schema_version":"1","ts":"{recent}","type":"skill","name":"rubber-duck","session_id":"s","invocation_id":"i"}}"#),
                &format!(r#"{{"schema_version":"1","ts":"{old_in_rare}","type":"skill","name":"rubber-duck","session_id":"s","invocation_id":"i2"}}"#),
                &format!(r#"{{"schema_version":"1","ts":"{too_old}","type":"skill","name":"rubber-duck","session_id":"s","invocation_id":"i3"}}"#),
            ],
        );
        let ix = read_invocation_ledger(tmp.path(), now);
        assert_eq!(ix.total_invocations, 3);
        let usage = &ix.per_skill["rubber-duck"];
        assert_eq!(usage.count_default_window, 1); // only the recent one
        assert_eq!(usage.count_rare_window, 2);    // recent + 200d ago
        assert!(usage.last_invoked.is_some());
    }

    #[test]
    fn read_invocation_ledger_tolerates_malformed() {
        let tmp = TempDir::new().unwrap();
        let now = chrono::Utc::now();
        let recent = (now - chrono::Duration::days(1)).to_rfc3339();
        write_ledger(
            tmp.path(),
            &[
                "not-json-at-all",
                &format!(r#"{{"ts":"{recent}","name":"ok"}}"#),
                r#"{"ts":"not-a-date","name":"bad-ts"}"#,
                r#"{"name":"missing-ts"}"#,
                "",
            ],
        );
        let ix = read_invocation_ledger(tmp.path(), now);
        // Only the one well-formed line should be counted.
        assert_eq!(ix.total_invocations, 1);
        assert_eq!(ix.per_skill.len(), 1);
        assert_eq!(ix.per_skill["ok"].count_default_window, 1);
    }

    #[tokio::test]
    async fn dead_skill_flagged_after_grace_period_with_no_invocations() {
        let tmp = TempDir::new().unwrap();
        let body = "# Skill\n\n\
                    **When to use this skill:** This skill has a compliant lead \
                    paragraph with plenty of description text to clear the 40-token \
                    floor and the required when-to-use signal. It just happens to \
                    never get invoked by any agent anywhere.\n\n\
                    ## Body\n\ncontent.\n";
        write_skill(tmp.path(), "ghost.md", body);
        // Force file mtime to be old (past the grace period).
        let path = tmp.path().join(".claude").join("skills").join("ghost.md");
        let old_time = std::time::SystemTime::now()
            - std::time::Duration::from_secs(60 * 60 * 24 * 100);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(old_time))
            .ok();

        // Populate the ledger with enough unrelated invocations so we
        // cross the low-confidence threshold.
        let now = chrono::Utc::now();
        let recent = (now - chrono::Duration::days(1)).to_rfc3339();
        let mut lines: Vec<String> = Vec::new();
        for i in 0..25 {
            lines.push(format!(
                r#"{{"schema_version":"1","ts":"{recent}","type":"skill","name":"other","session_id":"s","invocation_id":"{i}"}}"#
            ));
        }
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        write_ledger(tmp.path(), &line_refs);

        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        let findings = result["findings"].as_array().unwrap();
        assert!(findings.iter().any(|f| {
            f["name"].as_str() == Some("dead-skill:ghost.md")
        }), "expected a dead-skill finding, got: {:?}", findings);
    }

    #[tokio::test]
    async fn new_skill_inside_grace_period_not_flagged_dead() {
        let tmp = TempDir::new().unwrap();
        let body = "# New Skill\n\n\
                    **When to use this skill:** Recently authored skill that hasn't \
                    had time to accrue invocations yet. The description is long \
                    enough to pass hygiene checks and carries the when-to-use signal.\n\n\
                    ## Body\n\ncontent.\n";
        write_skill(tmp.path(), "fresh.md", body);
        // Do NOT backdate mtime — leave it as now, well inside grace period.
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        let findings = result["findings"].as_array().unwrap();
        assert!(!findings.iter().any(|f| {
            f["name"].as_str() == Some("dead-skill:fresh.md")
                || f["name"].as_str() == Some("low-confidence-dead-skill:fresh.md")
        }), "new skill should not be flagged dead, findings: {:?}", findings);
    }

    #[tokio::test]
    async fn low_confidence_prefix_when_ledger_is_sparse() {
        let tmp = TempDir::new().unwrap();
        let body = "# Skill\n\n\
                    **When to use this skill:** A skill whose description passes \
                    hygiene but the ledger is too sparse to draw firm conclusions \
                    about liveness. We still flag it, but with the low-confidence \
                    prefix so the operator knows to weight the signal accordingly.\n\n\
                    ## Body\n\ncontent.\n";
        write_skill(tmp.path(), "sparse.md", body);
        let path = tmp.path().join(".claude").join("skills").join("sparse.md");
        let old_time = std::time::SystemTime::now()
            - std::time::Duration::from_secs(60 * 60 * 24 * 100);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(old_time))
            .ok();
        // No ledger written — total_invocations = 0 < LOW_CONFIDENCE_TOTAL_INVOCATIONS.
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        let findings = result["findings"].as_array().unwrap();
        assert!(findings.iter().any(|f| {
            f["name"].as_str() == Some("low-confidence-dead-skill:sparse.md")
        }), "expected low-confidence-prefixed dead finding, got: {:?}", findings);
    }

    #[tokio::test]
    async fn rare_skill_uses_extended_window() {
        let tmp = TempDir::new().unwrap();
        // Skill marked rare, last invoked 200 days ago — would be dead
        // under the 90d window but alive under the 365d rare window.
        let body = "# Rare Skill\n\n\
                    **When to use this skill:** Skill that's deliberately niche. \
                    Invoked rarely, but when invoked it's critical. The extended \
                    365-day window accommodates this usage pattern without \
                    triggering false dead findings.\n\n\
                    usage-rarity: rare\n\n\
                    ## Body\n\ncontent.\n";
        write_skill(tmp.path(), "rare-one.md", body);
        let path = tmp.path().join(".claude").join("skills").join("rare-one.md");
        let old_time = std::time::SystemTime::now()
            - std::time::Duration::from_secs(60 * 60 * 24 * 100);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(old_time))
            .ok();

        let now = chrono::Utc::now();
        let two_hundred_days_ago = (now - chrono::Duration::days(200)).to_rfc3339();
        let mut lines: Vec<String> = vec![
            format!(
                r#"{{"schema_version":"1","ts":"{two_hundred_days_ago}","type":"skill","name":"rare-one","session_id":"s","invocation_id":"r1"}}"#
            ),
        ];
        for i in 0..25 {
            let recent = (now - chrono::Duration::days(1)).to_rfc3339();
            lines.push(format!(
                r#"{{"schema_version":"1","ts":"{recent}","type":"skill","name":"other","session_id":"s","invocation_id":"{i}"}}"#
            ));
        }
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        write_ledger(tmp.path(), &line_refs);

        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        let findings = result["findings"].as_array().unwrap();
        assert!(!findings.iter().any(|f| {
            let n = f["name"].as_str().unwrap_or("");
            n == "dead-skill:rare-one.md" || n == "low-confidence-dead-skill:rare-one.md"
        }), "rare skill with invocation 200d ago should stay alive under 365d window, findings: {:?}", findings);
    }

    // ── Tier A partial (2026-04-22): SKILL.md plugin-format tests ──

    /// Helper: write a `.claude/skills/<name>/SKILL.md`.
    fn write_skill_md(brain_root: &Path, dir_name: &str, body: &str) {
        let dir = brain_root
            .join(".claude")
            .join("skills")
            .join(dir_name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    #[test]
    fn extract_skill_md_routing_text_parses_description_only() {
        let body = "---\nname: foo\ndescription: what this skill does\n---\nbody here";
        let (text, has_when_field) = extract_skill_md_routing_text(body).unwrap();
        assert_eq!(text, "what this skill does");
        assert!(!has_when_field);
    }

    #[test]
    fn extract_skill_md_routing_text_combines_description_and_when_to_use() {
        let body = "---\nname: foo\ndescription: A\nwhen_to_use: B\n---\nbody";
        let (text, has_when_field) = extract_skill_md_routing_text(body).unwrap();
        assert!(text.contains("A"));
        assert!(text.contains("B"));
        assert!(has_when_field);
    }

    #[test]
    fn extract_skill_md_routing_text_returns_none_for_no_frontmatter() {
        let body = "# Plain Markdown\n\nNo frontmatter here.";
        assert!(extract_skill_md_routing_text(body).is_none());
    }

    #[test]
    fn extract_skill_md_routing_text_returns_none_for_empty_fields() {
        let body = "---\nname: foo\n---\nbody";
        assert!(extract_skill_md_routing_text(body).is_none());
    }

    #[tokio::test]
    async fn compliant_skill_md_plugin_scores_100() {
        let tmp = TempDir::new().unwrap();
        let body = "---\n\
                    name: plan-critic\n\
                    description: Adversarial review of a plan file before \
                    implementation. Surfaces pitfalls, missing rollback paths, \
                    gate gaps, and compatibility risks before code is written.\n\
                    when_to_use: When you see a plan file in .claude/plans/ \
                    and the user has said 'review my plan' or 'sanity check this'.\n\
                    ---\n\
                    # Plan Critic\n\n\
                    (body content)\n";
        write_skill_md(tmp.path(), "plan-critic", body);
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["score"], 100);
        assert_eq!(result["compliant_count"], 1);
        assert_eq!(result["total_skills"], 1);
    }

    #[tokio::test]
    async fn skill_md_without_description_falls_back_to_lead_paragraph() {
        let tmp = TempDir::new().unwrap();
        // Malformed-ish: YAML frontmatter is present but description field
        // is absent. Falls back to scoring the body's lead paragraph — so
        // the skill should still score well if the body has a decent lead.
        let body = "---\n\
                    name: foo\n\
                    ---\n\
                    # Foo\n\n\
                    **When to use this skill:** This skill exists as a \
                    fallback example. When the YAML frontmatter doesn't \
                    carry a description, we score the lead paragraph the \
                    legacy way. The skill still authors cleanly if the \
                    body follows the authoring standard.\n\n\
                    ## Section\n\nbody.";
        write_skill_md(tmp.path(), "foo", body);
        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["compliant_count"], 1);
        assert_eq!(result["score"], 100);
    }

    #[tokio::test]
    async fn skill_md_plugin_is_scannable_alongside_legacy() {
        // Mixed-format Brain: one legacy skill + one plugin skill.
        // Both should be discovered; aggregate score considers both.
        let tmp = TempDir::new().unwrap();

        // Legacy skill, compliant.
        let legacy = "# Legacy\n\n\
                      **When to use this skill:** Demonstrates the legacy \
                      single-file authoring format. Includes a long enough \
                      lead paragraph to meet the length floor and the \
                      when-to-use signal requirement.\n\n\
                      ## Body\n\ncontent.\n";
        write_skill(tmp.path(), "legacy-one.md", legacy);

        // Plugin skill, compliant.
        let plugin = "---\n\
                      name: plugin-one\n\
                      description: Demonstrates the plugin SKILL.md format \
                      with a frontmatter-based routing description.\n\
                      when_to_use: When scenario testing requires both a \
                      legacy and a plugin skill present simultaneously.\n\
                      ---\n\
                      # Plugin One\n\nbody.\n";
        write_skill_md(tmp.path(), "plugin-one", plugin);

        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["total_skills"], 2);
        assert_eq!(result["compliant_count"], 2);
        assert_eq!(result["score"], 100);

        // Per-format counts land in the breakdown via status_counts.
        let breakdown = &result["capability_breakdown"]["skills"];
        assert_eq!(breakdown["format:legacy"], 1);
        assert_eq!(breakdown["format:plugin"], 1);
    }

    #[tokio::test]
    async fn skill_md_plugin_ledger_lookup_uses_directory_name() {
        // Plugin skill's ledger key is the DIRECTORY name (matches
        // Claude Code's `tool_input.skill`), not `<name>/SKILL.md`.
        let tmp = TempDir::new().unwrap();
        let body = "---\n\
                    name: rubber-duck\n\
                    description: Socratic rubber-duck skill for stuck agents.\n\
                    when_to_use: When the user says 'duck it' or the agent \
                    is circling without testing an approach in conversation.\n\
                    ---\n\
                    # Rubber Duck\n\nbody.\n";
        write_skill_md(tmp.path(), "rubber-duck", body);

        // Backdate mtime so the skill is past the grace period.
        let path = tmp
            .path()
            .join(".claude")
            .join("skills")
            .join("rubber-duck")
            .join("SKILL.md");
        let old_time = std::time::SystemTime::now()
            - std::time::Duration::from_secs(60 * 60 * 24 * 100);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(old_time))
            .ok();

        // Ledger records an invocation under the directory name.
        let now = chrono::Utc::now();
        let recent = (now - chrono::Duration::days(1)).to_rfc3339();
        let mut lines: Vec<String> = vec![format!(
            r#"{{"schema_version":"1","ts":"{recent}","type":"skill","name":"rubber-duck","session_id":"s","invocation_id":"r1"}}"#
        )];
        for i in 0..25 {
            lines.push(format!(
                r#"{{"schema_version":"1","ts":"{recent}","type":"skill","name":"other","session_id":"s","invocation_id":"{i}"}}"#
            ));
        }
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        write_ledger(tmp.path(), &line_refs);

        let result = analyze_capability_hygiene(tmp.path().to_str().unwrap()).await;
        let findings = result["findings"].as_array().unwrap();

        // The skill should be ALIVE (not dead), since the ledger has
        // an invocation under the directory name.
        assert!(!findings.iter().any(|f| {
            let n = f["name"].as_str().unwrap_or("");
            n.contains("dead-skill:rubber-duck")
        }), "plugin skill with recent invocation should be alive, findings: {:?}", findings);
    }
}
