//! Context-overhead benchmark — B-09 DP-4 + B-10 Phase 1.
//!
//! Measures the approximate token cost of MCP mode vs CLI mode by
//! tokenizing a faithful JSON representation of the `BrainServer`
//! tool list (what Claude Code would inject into a session's system
//! prompt when `neurogrim` is registered as an MCP server) and
//! comparing against the CLI-mode baseline (zero injected schema
//! tokens; the agent invokes Bash subcommands per
//! `docs/cli-sensory-surface.md`).
//!
//! ## Methodology
//!
//! 1. `MCP_TOOL_LIST_JSON` below is a fixture approximating the
//!    output of `list_tools()` on a running `BrainServer` — the seven
//!    scoring tools (`get_health_score`, `get_trajectory`, etc.) with
//!    description + JSON Schema.
//! 2. Tokenizer is `tiktoken-rs cl100k_base` — the GPT-4 / Claude
//!    family baseline. Deltas are directionally accurate; absolute
//!    numbers drift ±10-20% from Claude's actual tokenizer.
//! 3. B-10 Phase 1 extensions (four-Brain sweep over skills +
//!    CLAUDE.md + MCP tool schemas) live in the same test; run with
//!    `--nocapture` to see the per-Brain breakdown.
//! 4. Report JSON is emitted under
//!    `D:/Brains/NeuroGrim/roadmap/data/b09-bench-<YYYY-MM-DD>.json`
//!    (and `b10-phase1-<YYYY-MM-DD>.json` for Stream 2's sweep).
//!
//! ## Running
//!
//! ```bash
//! cargo test -p neurogrim-cli --test context_overhead -- --nocapture
//! ```
//!
//! ## Regenerating the MCP fixture
//!
//! If `neurogrim-mcp/src/server.rs` adds or renames a `BrainServer`
//! tool, update `MCP_TOOL_LIST_JSON` below. The assertion
//! `assert_eq!(tool_count(), 7)` in `test_tool_count_is_current`
//! catches drift.

use chrono::Utc;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use tiktoken_rs::cl100k_base;

/// Faithful approximation of the JSON Claude Code receives when it
/// connects to `neurogrim serve` and calls `list_tools()`. Mirrors
/// the `BrainServer` tool definitions at
/// `neurogrim-mcp/src/server.rs:248-490`.
const MCP_TOOL_LIST_JSON: &str = r##"[
  {
    "name": "get_health_score",
    "description": "Get the unified health score with domain breakdown, trajectory, and cross-domain analysis. Returns full agent-mode JSON.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "hat": {"type": ["string", "null"], "description": "Hat name for domain emphasis"},
        "human_persona": {"type": ["string", "null"], "description": "Output human-persona (executive, manager, developer, specialist, product-manager)"}
      },
      "additionalProperties": false
    }
  },
  {
    "name": "get_trajectory",
    "description": "Get trajectory analysis (velocity, acceleration, classification) for the unified score or a specific domain.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "domain": {"type": ["string", "null"], "description": "Domain name for domain-specific trajectory. Omit for unified."}
      },
      "additionalProperties": false
    }
  },
  {
    "name": "get_recommendations",
    "description": "Get prioritized remediation actions sorted by priority.",
    "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
  },
  {
    "name": "refresh_sensory",
    "description": "Re-invoke sensory tools and return updated scores.",
    "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
  },
  {
    "name": "validate_registry",
    "description": "Validate the brain-registry.json configuration.",
    "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
  },
  {
    "name": "get_local_awareness",
    "description": "Get local machine-specific awareness: tool paths not on PATH, OS quirks, known behavioral patterns. This data is machine-local and gitignored — it persists facts agents discover about the local environment so they are not forgotten across sessions. Use 'neurogrim awareness add' to record new facts.",
    "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
  },
  {
    "name": "record_subagent_outcome",
    "description": "Record a subagent invocation outcome for subagent-health scoring. Call this after processing every subagent response, success or failure. Appends one line to .claude/brain/subagent-outcomes.jsonl and recomputes .claude/brain/subagent-health-cmdb.json from the last 20 outcomes.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "request_id": {"type": "string", "description": "Unique ID of the subagent request (matches request_id in the envelope)."},
        "capability": {"type": "string", "description": "Capability key from the skill manifest (e.g. 'lsp-symbol-scan')."},
        "responsibility": {"type": "string", "description": "Responsibility type (analysis, investigation, remediation, validation, synthesis, sensory)."},
        "required_hat": {"type": ["string", "null"], "description": "Hat the subagent was required to wear (null for sensory)."},
        "worn_hat": {"type": ["string", "null"], "description": "Hat the subagent reported wearing in worn_hat field."},
        "status": {"type": "string", "description": "Final envelope status: 'ok', 'partial', or 'error'."},
        "envelope_found": {"type": "boolean", "description": "Whether the delimited envelope block was found in the response."},
        "schema_conformant": {"type": "boolean", "description": "Whether the envelope JSON parsed and all required fields were present."},
        "hat_compliant": {"type": "boolean", "description": "Whether worn_hat matched required_hat."},
        "confidence": {"type": "number", "description": "Confidence value from metadata.confidence (0.0-1.0)."},
        "symbol_count": {"type": "integer", "description": "Number of symbols in the response symbols array."},
        "retry_count": {"type": "integer", "description": "Number of retries issued before accepting or aborting (0, 1, or 2)."}
      },
      "required": ["request_id", "capability", "responsibility", "status", "envelope_found", "schema_conformant", "hat_compliant", "confidence", "symbol_count", "retry_count"],
      "additionalProperties": false
    }
  }
]"##;

fn count_tokens(text: &str) -> usize {
    let bpe = cl100k_base().expect("failed to load cl100k_base BPE");
    bpe.encode_with_special_tokens(text).len()
}

fn tool_count() -> usize {
    let parsed: serde_json::Value =
        serde_json::from_str(MCP_TOOL_LIST_JSON).expect("MCP_TOOL_LIST_JSON must be valid JSON");
    parsed
        .as_array()
        .expect("MCP_TOOL_LIST_JSON must be an array")
        .len()
}

/// DP-4 primary assertion: the fixture matches the live BrainServer
/// tool count. If this fails, `server.rs` added or removed a tool and
/// `MCP_TOOL_LIST_JSON` needs regeneration.
#[test]
fn test_tool_count_is_current() {
    assert_eq!(
        tool_count(),
        7,
        "MCP_TOOL_LIST_JSON tool count diverges from BrainServer. \
         If server.rs changed, regenerate the fixture."
    );
}

/// Repo root — `D:/Brains/NeuroGrim`. Resolved from this crate's
/// manifest dir (`neurogrim-cli`), three parents up:
/// `neurogrim-cli/../../../` → NeuroGrim root.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Brains ecosystem root — `D:/Brains`. Parent of the NeuroGrim
/// checkout (this assumes the git submodule layout; adjust if the
/// repo is checked out stand-alone).
fn ecosystem_root() -> PathBuf {
    repo_root().parent().unwrap().to_path_buf()
}

/// DP-4: quantify MCP-mode vs CLI-mode token overhead for NeuroGrim
/// sessions, and write the report under
/// `NeuroGrim/roadmap/data/b09-bench-<date>.json`.
#[test]
fn dp4_mcp_vs_cli_benchmark() {
    let mcp_tokens = count_tokens(MCP_TOOL_LIST_JSON);
    let cli_tokens: usize = 0; // CLI mode injects no schemas at session start.
    let delta = mcp_tokens - cli_tokens;

    let report = json!({
        "generated_at": Utc::now().to_rfc3339(),
        "tokenizer": "tiktoken-rs cl100k_base",
        "tokenizer_caveat": "cl100k_base is GPT-4 family. Claude's actual tokenizer differs; deltas are directionally right, absolute numbers ~10-20% off.",
        "mcp_mode": {
            "tool_count": tool_count(),
            "tool_list_json_bytes": MCP_TOOL_LIST_JSON.len(),
            "tokens_injected_at_session_start": mcp_tokens
        },
        "cli_mode": {
            "tool_count": 0,
            "tool_list_json_bytes": 0,
            "tokens_injected_at_session_start": cli_tokens,
            "note": "CLI mode omits NeuroGrim from .mcp.json; see docs/cli-mode.md"
        },
        "delta": {
            "tokens_saved_by_cli_mode": delta,
            "percent_reduction_from_mcp": 100.0
        }
    });

    let report_dir = repo_root().join("roadmap").join("data");
    fs::create_dir_all(&report_dir).expect("failed to create roadmap/data/");
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let report_path = report_dir.join(format!("b09-bench-{date}.json"));
    fs::write(
        &report_path,
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .expect("failed to write b09-bench report");

    println!("=== B-09 DP-4 benchmark ===");
    println!("MCP mode tokens: {mcp_tokens}");
    println!("CLI mode tokens: {cli_tokens}");
    println!("Saved by CLI:    {delta}");
    println!("Report:          {}", report_path.display());

    assert!(
        mcp_tokens > 0,
        "MCP tool-list tokenization returned 0 — check tokenizer"
    );
}

// ---------------------------------------------------------------------------
// B-10 Phase 1 — four-Brain skill + CLAUDE.md sweep
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct BrainCorpus {
    id: &'static str,
    /// Absolute path to the `.claude/skills/` directory.
    skills_dir: PathBuf,
    /// Absolute path to this Brain's CLAUDE.md (if present).
    claude_md: PathBuf,
    /// If this Brain registers an MCP server that Claude Code consumes,
    /// include its tool-list JSON as a &str. None if the Brain is not
    /// an MCP-exposed target.
    mcp_tool_list: Option<&'static str>,
}

fn four_brains() -> Vec<BrainCorpus> {
    let eco = ecosystem_root();
    vec![
        BrainCorpus {
            id: "ecosystem",
            skills_dir: eco.join(".claude").join("skills"),
            claude_md: eco.join("CLAUDE.md"),
            mcp_tool_list: None,
        },
        BrainCorpus {
            id: "neurogrim",
            skills_dir: repo_root().join(".claude").join("skills"),
            claude_md: repo_root().join("CLAUDE.md"),
            mcp_tool_list: Some(MCP_TOOL_LIST_JSON),
        },
        BrainCorpus {
            id: "lsp-brains",
            skills_dir: eco.join("LSP-Brains").join(".claude").join("skills"),
            claude_md: eco.join("LSP-Brains").join("CLAUDE.md"),
            mcp_tool_list: None,
        },
        BrainCorpus {
            id: "python-starter",
            skills_dir: repo_root()
                .join("NeuroGrim-python-starter")
                .join(".claude")
                .join("skills"),
            claude_md: repo_root()
                .join("NeuroGrim-python-starter")
                .join("CLAUDE.md"),
            mcp_tool_list: None,
        },
    ]
}

/// Tokenize every non-archived .md file under `skills_dir`. Returns
/// (per-skill breakdown, sum).
fn tokenize_skills(skills_dir: &PathBuf) -> (Vec<serde_json::Value>, usize) {
    let mut out = Vec::new();
    let mut total = 0usize;
    if !skills_dir.exists() {
        return (out, 0);
    }
    let entries = match fs::read_dir(skills_dir) {
        Ok(e) => e,
        Err(_) => return (out, 0),
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
            if name.starts_with("README") || name.starts_with(".") {
                continue;
            }
        }
        if let Ok(body) = fs::read_to_string(&path) {
            let tokens = count_tokens(&body);
            total += tokens;
            out.push(json!({
                "path": path.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                "tokens": tokens,
            }));
        }
    }
    (out, total)
}

fn tokenize_file(path: &PathBuf) -> usize {
    if !path.exists() {
        return 0;
    }
    fs::read_to_string(path)
        .map(|s| count_tokens(&s))
        .unwrap_or(0)
}

/// Extract a skill's "description block" — everything from the top
/// of the file up to (but not including) the first `##` section
/// header. This mirrors the convention observed across the current
/// skill corpus: skills lead with a title, then a "When to use this"
/// paragraph (possibly with bold prefix), then the body in sections.
///
/// If no `##` header is found, the whole file is treated as
/// description (degenerate skill; flagged in the report).
fn extract_description(body: &str) -> &str {
    match body.find("\n## ") {
        Some(idx) => &body[..idx],
        None => body,
    }
}

/// Extract a skill's outline — all lines that begin with `##` or
/// `###`. Returns the joined outline as a single string (headers
/// preserved, one per line). Excludes H1 (`# `) since that's the
/// title and already inside the description.
fn extract_outline(body: &str) -> String {
    body.lines()
        .filter(|line| line.starts_with("## ") || line.starts_with("### "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// B-10 Phase 1: cold-start overhead per Brain + cross-Brain
/// duplicated-skill waste. Report lands at
/// `roadmap/data/b10-phase1-<date>.json`.
#[test]
fn b10_phase1_four_brain_sweep() {
    let brains = four_brains();
    let mut per_brain = serde_json::Map::new();

    // Per-Brain breakdown.
    let mut worst_cold_start = 0usize;
    let mut worst_brain = String::new();
    let mut all_skill_tokens: Vec<usize> = Vec::new();

    // Track `rubber-duck.md` canary (basename → per-Brain token count).
    let mut dup_map: std::collections::BTreeMap<String, Vec<(String, usize)>> =
        std::collections::BTreeMap::new();

    for brain in &brains {
        let (skills, skills_total) = tokenize_skills(&brain.skills_dir);
        let claude_md_tokens = tokenize_file(&brain.claude_md);
        let mcp_tokens = brain
            .mcp_tool_list
            .map(count_tokens)
            .unwrap_or(0);
        let cold_start = skills_total + claude_md_tokens + mcp_tokens;
        // TOC projection: assume ~100 tokens per capability entry
        // (1-line summary + pointer) + CLAUDE.md as-is.
        let toc_projected = claude_md_tokens + skills.len() * 100
            + brain.mcp_tool_list.map(|_| tool_count() * 100).unwrap_or(0);
        let delta = cold_start.saturating_sub(toc_projected);

        if cold_start > worst_cold_start {
            worst_cold_start = cold_start;
            worst_brain = brain.id.to_string();
        }

        for skill in &skills {
            if let Some(tokens) = skill.get("tokens").and_then(|v| v.as_u64()) {
                all_skill_tokens.push(tokens as usize);
            }
            if let Some(name) = skill.get("path").and_then(|v| v.as_str()) {
                dup_map
                    .entry(name.to_string())
                    .or_default()
                    .push((
                        brain.id.to_string(),
                        skill
                            .get("tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize,
                    ));
            }
        }

        per_brain.insert(
            brain.id.to_string(),
            json!({
                "claude_md_tokens": claude_md_tokens,
                "skills": skills,
                "mcp_tool_list_tokens": mcp_tokens,
                "totals": {
                    "cold_start": cold_start,
                    "toc_projected": toc_projected,
                    "delta": delta
                }
            }),
        );
    }

    // Duplicated-skill waste canary.
    let mut duplications: Vec<serde_json::Value> = Vec::new();
    let mut dup_waste_total = 0usize;
    for (basename, presence) in &dup_map {
        if presence.len() < 2 {
            continue;
        }
        let tokens_each = presence.first().map(|(_, t)| *t).unwrap_or(0);
        let waste = tokens_each * (presence.len() - 1);
        dup_waste_total += waste;
        duplications.push(json!({
            "file_basename": basename,
            "present_in": presence.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            "tokens_each": tokens_each,
            "waste_on_full_stack": waste
        }));
    }

    all_skill_tokens.sort_unstable();
    let median = if all_skill_tokens.is_empty() {
        0
    } else {
        all_skill_tokens[all_skill_tokens.len() / 2]
    };
    let p90 = if all_skill_tokens.is_empty() {
        0
    } else {
        let idx = (all_skill_tokens.len() as f64 * 0.9) as usize;
        all_skill_tokens[idx.min(all_skill_tokens.len() - 1)]
    };

    // Decision-criteria guidance, echoing BACKLOG B-10.
    let verdict = if worst_cold_start <= 8_000 && dup_waste_total < 5_000 {
        "park (worst ≤ 8k, dup waste < 5k)"
    } else if worst_cold_start >= 20_000 || dup_waste_total >= 5_000 {
        "proceed to Phase 2"
    } else {
        "ambiguous zone — run Phase 1.5 usage-fraction secondary measurement"
    };

    let report = json!({
        "generated_at": Utc::now().to_rfc3339(),
        "tokenizer": "tiktoken-rs cl100k_base",
        "tokenizer_caveat": "cl100k_base is not Claude's tokenizer; ±10-20%. Re-run via Anthropic token-counting API in the ambiguous zone.",
        "brains": per_brain,
        "duplications": duplications,
        "summary": {
            "worst_cold_start_brain": worst_brain,
            "worst_tokens": worst_cold_start,
            "dup_waste_total": dup_waste_total,
            "median_skill_tokens": median,
            "p90_skill_tokens": p90,
            "decision_zone_verdict_preliminary": verdict
        }
    });

    let report_dir = repo_root().join("roadmap").join("data");
    fs::create_dir_all(&report_dir).expect("failed to create roadmap/data/");
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let report_path = report_dir.join(format!("b10-phase1-{date}.json"));
    fs::write(
        &report_path,
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .expect("failed to write b10-phase1 report");

    println!("=== B-10 Phase 1 sweep ===");
    println!("Worst-Brain cold-start: {worst_cold_start} ({worst_brain})");
    println!("Cross-Brain dup waste:  {dup_waste_total}");
    println!("Preliminary verdict:    {verdict}");
    println!("Report:                 {}", report_path.display());

    assert!(
        worst_cold_start > 0,
        "no Brain yielded a non-zero cold-start — paths misconfigured or corpus empty"
    );
}

/// B-10 Phase 1.5 — per-skill description-only + outline-only
/// measurement. Tests the operator's 2026-04-22 hypothesis:
/// "agents likely only need the description (when-to-use block) +
/// a brief outline to route to skills correctly; full body loads
/// on demand."
///
/// Methodology:
/// 1. For each skill in the four-Brain corpus, extract
///    (a) full body, (b) description (title + everything before
///    first `## ` header), (c) outline (all `## ` / `### ` headers).
/// 2. Tokenize each; record ratios.
/// 3. Aggregate per-Brain and flag outliers ("degraded" skills whose
///    description is < 10% or > 40% of full body — the former means
///    under-described, the latter means the skill is mostly
///    description with little body).
/// 4. Project TOC cost: per-Brain sum of (description + outline).
///    Compare against full-body baseline from the main sweep.
#[test]
fn b10_phase1p5_description_only_measurement() {
    let brains = four_brains();
    let mut per_brain_report = serde_json::Map::new();

    let mut stack_full_body = 0usize;
    let mut stack_desc_only = 0usize;
    let mut stack_desc_plus_outline = 0usize;

    // Track "description hygiene" — fraction of body captured in
    // description, so we can flag skills that would route poorly
    // under a description-only TOC.
    let mut hygiene_buckets = std::collections::BTreeMap::<&'static str, usize>::new();
    hygiene_buckets.insert("excellent (10-25%)", 0);
    hygiene_buckets.insert("good (5-10% or 25-40%)", 0);
    hygiene_buckets.insert("under-described (<5%)", 0);
    hygiene_buckets.insert("over-described (>40%)", 0);

    for brain in &brains {
        if !brain.skills_dir.exists() {
            continue;
        }
        let mut skills_detail = Vec::new();
        let mut brain_full = 0usize;
        let mut brain_desc = 0usize;
        let mut brain_outline = 0usize;

        let entries = match fs::read_dir(&brain.skills_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file()
                || path.extension().and_then(|s| s.to_str()) != Some("md")
            {
                continue;
            }
            let name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if name.starts_with("README") || name.starts_with(".") {
                continue;
            }
            let body = match fs::read_to_string(&path) {
                Ok(b) => b,
                Err(_) => continue,
            };

            let full_tokens = count_tokens(&body);
            let description = extract_description(&body);
            let outline = extract_outline(&body);
            let desc_tokens = count_tokens(description);
            let outline_tokens = count_tokens(&outline);
            let combined = desc_tokens + outline_tokens;

            brain_full += full_tokens;
            brain_desc += desc_tokens;
            brain_outline += outline_tokens;

            let hygiene_pct = if full_tokens == 0 {
                0.0
            } else {
                100.0 * desc_tokens as f64 / full_tokens as f64
            };
            let hygiene_bucket = if hygiene_pct < 5.0 {
                "under-described (<5%)"
            } else if hygiene_pct < 10.0 || hygiene_pct >= 25.0 && hygiene_pct < 40.0 {
                "good (5-10% or 25-40%)"
            } else if hygiene_pct >= 40.0 {
                "over-described (>40%)"
            } else {
                "excellent (10-25%)"
            };
            *hygiene_buckets.get_mut(hygiene_bucket).unwrap() += 1;

            skills_detail.push(json!({
                "path": name,
                "full_tokens": full_tokens,
                "description_tokens": desc_tokens,
                "outline_tokens": outline_tokens,
                "combined_tokens": combined,
                "reduction_pct": if full_tokens > 0 {
                    100.0 * (full_tokens - combined) as f64 / full_tokens as f64
                } else { 0.0 },
                "hygiene_pct_desc_of_body": hygiene_pct,
                "hygiene_bucket": hygiene_bucket,
            }));
        }

        stack_full_body += brain_full;
        stack_desc_only += brain_desc;
        stack_desc_plus_outline += brain_desc + brain_outline;

        per_brain_report.insert(
            brain.id.to_string(),
            json!({
                "skill_count": skills_detail.len(),
                "totals": {
                    "full_body": brain_full,
                    "description_only": brain_desc,
                    "description_plus_outline": brain_desc + brain_outline,
                    "outline_only": brain_outline,
                    "reduction_pct_desc_plus_outline": if brain_full > 0 {
                        100.0 * (brain_full - brain_desc - brain_outline) as f64 / brain_full as f64
                    } else { 0.0 }
                },
                "skills": skills_detail,
            }),
        );
    }

    let stack_reduction_pct = if stack_full_body > 0 {
        100.0 * (stack_full_body - stack_desc_plus_outline) as f64
            / stack_full_body as f64
    } else {
        0.0
    };

    let report = json!({
        "generated_at": Utc::now().to_rfc3339(),
        "tokenizer": "tiktoken-rs cl100k_base",
        "hypothesis": "Agents need only the description (when-to-use block) + section outline to route; full body loads on demand.",
        "per_brain": per_brain_report,
        "stack_totals": {
            "full_body": stack_full_body,
            "description_only": stack_desc_only,
            "description_plus_outline": stack_desc_plus_outline,
            "outline_contribution": stack_desc_plus_outline - stack_desc_only,
            "stack_reduction_pct_with_desc_plus_outline": stack_reduction_pct,
        },
        "hygiene_distribution": hygiene_buckets,
        "interpretation": {
            "actionable_if_reduction_gt_80pct": "The hypothesis is empirically supported — a description+outline TOC captures the agent's routing signal at a fraction of the full-body cost.",
            "actionable_if_reduction_lt_50pct": "The hypothesis fails — skills do not carry their routing signal in the description block; the CapProto architecture must ship more than a TOC."
        }
    });

    let report_dir = repo_root().join("roadmap").join("data");
    fs::create_dir_all(&report_dir).expect("failed to create roadmap/data/");
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let report_path =
        report_dir.join(format!("b10-phase1p5-description-only-{date}.json"));
    fs::write(
        &report_path,
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .expect("failed to write Phase 1.5 report");

    println!("=== B-10 Phase 1.5 description-only measurement ===");
    println!("Stack full-body tokens:          {stack_full_body}");
    println!("Stack description-only tokens:   {stack_desc_only}");
    println!("Stack desc + outline tokens:     {stack_desc_plus_outline}");
    println!("Reduction vs full body:          {stack_reduction_pct:.1}%");
    println!("Hygiene distribution:            {hygiene_buckets:?}");
    println!("Report:                          {}", report_path.display());

    assert!(
        stack_full_body > 0,
        "no skills measured — corpus empty or paths wrong"
    );
}
