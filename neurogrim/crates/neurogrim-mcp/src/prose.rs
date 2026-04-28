//! Agent-friendly prose orientation renderer (v3.2 Phase A.1; relocated
//! from neurogrim-cli to neurogrim-mcp in v3.2.1 so the MCP `orient`
//! tool and the `neurogrim agent --prose` CLI command share a single
//! source of truth).
//!
//! Renders a `BrainRegistry` + scored `AgentOutput` + project root as a
//! compact prose summary tuned for AI agents entering an unfamiliar
//! Brain. Same upstream data as the JSON `agent` output; the
//! difference is the rendering target.
//!
//! Sections (in order):
//!   1. Brain identity — name, scope, domain count (weighted vs advisory)
//!   2. Current state — unified score + confidence + trajectory direction
//!   3. Strongest signals — top 3 effective scores
//!   4. Calls to action — top recommendations from the scoring pipeline
//!   5. Available skills — scan .claude/skills/ for SKILL.md frontmatter
//!   6. Available hats — registry.config.hats keys
//!   7. Federation peers — registry.config.children (with read_only flag)
//!   8. Footer — pointers to `neurogrim explain` and `neurogrim doctor`
//!
//! `plain=true` suppresses ANSI colors entirely; when false, the
//! `colored` crate's TTY auto-detection (NO_COLOR / piped-stdout
//! heuristics) handles non-terminal output gracefully.

use colored::*;
use neurogrim_core::agent_output::AgentOutput;
use neurogrim_core::registry::BrainRegistry;
use neurogrim_core::types::{ScoreLabel, TrajectoryClassification};
use std::fmt::Write;
use std::path::Path;

/// Render the entire prose orientation as a `String`. Pass `plain=true`
/// to suppress ANSI color escapes (use this when the output will be
/// piped or embedded in another tool's context — including the MCP
/// `orient` response).
pub fn render_prose(
    registry: &BrainRegistry,
    project_root: &Path,
    agent_output: &AgentOutput,
    plain: bool,
) -> String {
    let mut out = String::new();
    section_identity(&mut out, registry, project_root, plain);
    section_current_state(&mut out, registry, agent_output, plain);
    section_strongest_signals(&mut out, registry, agent_output, plain);
    section_calls_to_action(&mut out, agent_output, plain);
    section_skills(&mut out, project_root);
    section_hats(&mut out, registry, plain);
    section_federation(&mut out, registry, plain);
    section_footer(&mut out, plain);
    out
}

// --- Section 1: Brain identity -----------------------------------------

fn section_identity(out: &mut String, registry: &BrainRegistry, project_root: &Path, plain: bool) {
    let project_label = if !registry.meta.description.is_empty() {
        first_sentence(&registry.meta.description, 80)
    } else {
        project_root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("(unnamed)")
            .to_string()
    };

    let total = registry.config.domain_weights.len();
    let weighted = registry
        .config
        .domain_weights
        .values()
        .filter(|w| **w > 0.0)
        .count();
    let advisory = total - weighted;

    let header = format!("NeuroGrim Brain — {}", project_label);
    let path = format!(" ({})", project_root.display());

    if plain {
        let _ = writeln!(out, "{}{}", header, path);
    } else {
        let _ = writeln!(out, "{}{}", header.bold(), path.dimmed());
    }

    let split = match (weighted, advisory) {
        (0, 0) => "(no domains declared)".to_string(),
        (w, 0) => format!("{} weighted", w),
        (0, a) => format!("{} advisory", a),
        (w, a) => format!("{} weighted, {} advisory", w, a),
    };
    let _ = writeln!(
        out,
        "{} {} — {}",
        total,
        if total == 1 { "domain" } else { "domains" },
        split
    );
    out.push('\n');
}

/// Trim a multi-paragraph description to its first sentence (or the
/// first `max` chars if no sentence break is found in range).
fn first_sentence(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if let Some(idx) = trimmed.find(". ") {
        if idx + 1 <= max {
            return trimmed[..=idx].trim().to_string();
        }
    }
    truncate(trimmed, max)
}

// --- Section 2: Current state ------------------------------------------

fn section_current_state(
    out: &mut String,
    registry: &BrainRegistry,
    agent_output: &AgentOutput,
    plain: bool,
) {
    let weighted = registry
        .config
        .domain_weights
        .values()
        .filter(|w| **w > 0.0)
        .count();
    let is_all_advisory = weighted == 0 && !registry.config.domain_weights.is_empty();

    let _ = writeln!(out, "Current state:");

    if is_all_advisory {
        // All-advisory Brain: unified score is structurally 0 (no
        // weighted contributions). Reporting "Score: 0/100" reads as
        // failure when the actual posture is "observe-only by design."
        let note = "  Score: N/A (all-advisory Brain — observe-only posture)";
        if plain {
            let _ = writeln!(out, "{}", note);
        } else {
            let _ = writeln!(out, "{}", note.dimmed());
        }
    } else {
        let label = ScoreLabel::from_score(agent_output.score, 75, 50);
        let score_str = format!("{}/100", agent_output.score);
        let colored_score = if plain {
            score_str.clone()
        } else {
            match label {
                ScoreLabel::Green => score_str.green().bold().to_string(),
                ScoreLabel::Yellow => score_str.yellow().bold().to_string(),
                ScoreLabel::Red => score_str.red().bold().to_string(),
            }
        };
        let conf_note = if agent_output.unified_confidence < 100 {
            format!(" (confidence: {}%)", agent_output.unified_confidence)
        } else {
            String::new()
        };
        let _ = writeln!(out, "  Score: {}{}", colored_score, conf_note);
    }

    let traj_note = match &agent_output.trajectory {
        Some(t) => format!(
            "  Trajectory: {} (velocity: {:+.1}, samples: {})",
            classification_display(&t.classification),
            t.velocity,
            t.samples
        ),
        None => "  Trajectory: not yet established (no score history)".to_string(),
    };
    let _ = writeln!(out, "{}", traj_note);
    out.push('\n');
}

// --- Section 3: Strongest signals --------------------------------------

fn section_strongest_signals(
    out: &mut String,
    registry: &BrainRegistry,
    agent_output: &AgentOutput,
    plain: bool,
) {
    let mut entries: Vec<(&String, u8, u8, f64)> = agent_output
        .domains
        .iter()
        .map(|(k, d)| (k, d.effective_score, d.confidence, d.weight))
        .collect();
    entries.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.0.cmp(b.0))
    });

    let top: Vec<_> = entries.iter().take(3).collect();
    if top.is_empty() {
        let _ = writeln!(out, "Strongest signals: (no domains scored)");
        out.push('\n');
        return;
    }

    let _ = writeln!(out, "Strongest signals:");
    for (name, eff, conf, _w) in top {
        let display = registry
            .config
            .principle_map
            .get(*name)
            .cloned()
            .unwrap_or_else(|| (*name).to_string());
        let label = ScoreLabel::from_score(*eff, 75, 50);
        let eff_str = format!("{}", eff);
        let colored_eff = if plain {
            eff_str.clone()
        } else {
            match label {
                ScoreLabel::Green => eff_str.green().to_string(),
                ScoreLabel::Yellow => eff_str.yellow().to_string(),
                ScoreLabel::Red => eff_str.red().to_string(),
            }
        };
        let bullet = if plain { "•".to_string() } else { "•".cyan().to_string() };
        let _ = writeln!(
            out,
            "  {} {} — effective {}, confidence {}%",
            bullet, display, colored_eff, conf
        );
    }
    out.push('\n');
}

// --- Section 4: Calls to action ----------------------------------------

fn section_calls_to_action(out: &mut String, agent_output: &AgentOutput, plain: bool) {
    let recs = &agent_output.top_recommendations;
    if recs.is_empty() {
        let _ = writeln!(out, "Calls to action: (none queued)");
        out.push('\n');
        return;
    }
    let _ = writeln!(out, "Calls to action:");
    for (i, r) in recs.iter().take(3).enumerate() {
        let header = if plain {
            format!("  {}. [{}] {}", i + 1, r.domain, r.gate)
        } else {
            format!(
                "  {}. [{}] {}",
                (i + 1).to_string().bold(),
                r.domain.cyan(),
                r.gate
            )
        };
        let _ = writeln!(out, "{}", header);
        if let Some(desc) = &r.description {
            let _ = writeln!(out, "     {}", desc);
        }
        let cmd_line = if plain {
            format!("→ {}", r.command)
        } else {
            format!("→ {}", r.command.dimmed())
        };
        let _ = writeln!(out, "     {}", cmd_line);
    }
    out.push('\n');
}

// --- Section 5: Available skills ---------------------------------------

fn section_skills(out: &mut String, project_root: &Path) {
    let skills_dir = project_root.join(".claude").join("skills");
    let entries = scan_skills(&skills_dir);
    if entries.is_empty() {
        let _ = writeln!(out, "Available skills: (none discovered in .claude/skills/)");
        out.push('\n');
        return;
    }
    let _ = writeln!(out, "Available skills (in .claude/skills/):");
    for (name, desc) in entries.iter().take(20) {
        let line = format!("  {} — {}", name.as_str().cyan(), desc);
        let _ = writeln!(out, "{}", line);
    }
    if entries.len() > 20 {
        let _ = writeln!(out, "  … and {} more", entries.len() - 20);
    }
    out.push('\n');
}

/// Scan `<project>/.claude/skills/<name>/SKILL.md` and return a list of
/// `(skill_name, one_line_description)`. Description is pulled from the
/// frontmatter `description:` field; falls back to the H1 line if absent;
/// falls back to "(no description)" if neither.
fn scan_skills(skills_dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(read) = std::fs::read_dir(skills_dir) else {
        return out;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.starts_with('.') || name == "archived" {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        let desc = std::fs::read_to_string(&skill_md)
            .ok()
            .map(|c| extract_skill_description(&c))
            .unwrap_or_else(|| "(unreadable)".to_string());
        out.push((name.to_string(), desc));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Extract a one-line description from a SKILL.md content blob. Looks
/// for `description:` in YAML frontmatter first, then the first `# `
/// heading. Handles plain scalars, folded scalars (`>-`), and literal
/// block scalars (`|-`). Truncates to 100 chars.
fn extract_skill_description(content: &str) -> String {
    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let frontmatter = &rest[..end];
            let lines: Vec<&str> = frontmatter.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let Some(val) = line.strip_prefix("description:") else {
                    continue;
                };
                let val = val.trim();
                if val == ">-" || val == ">" || val == "|-" || val == "|" || val.is_empty() {
                    let mut acc = String::new();
                    for cont in &lines[i + 1..] {
                        if cont.starts_with(' ') || cont.starts_with('\t') {
                            if !acc.is_empty() {
                                acc.push(' ');
                            }
                            acc.push_str(cont.trim());
                        } else {
                            break;
                        }
                    }
                    return truncate(acc.trim(), 100);
                }
                return truncate(val.trim_matches('"').trim_matches('\''), 100);
            }
        }
    }
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            return truncate(rest.trim(), 100);
        }
    }
    "(no description)".to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

// --- Section 6: Available hats -----------------------------------------

fn section_hats(out: &mut String, registry: &BrainRegistry, plain: bool) {
    let hats = &registry.config.hats;
    if hats.is_empty() {
        let _ = writeln!(
            out,
            "Available hats: registry declares none (spec defaults available; \
             see `neurogrim explain hat`)"
        );
        out.push('\n');
        return;
    }
    let mut names: Vec<&String> = hats.keys().collect();
    names.sort();
    let _ = writeln!(out, "Available hats:");
    let line = names
        .iter()
        .map(|n| if plain { (*n).clone() } else { n.cyan().to_string() })
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(out, "  {}", line);
    out.push('\n');
}

// --- Section 7: Federation peers ---------------------------------------

fn section_federation(out: &mut String, registry: &BrainRegistry, plain: bool) {
    let children = registry
        .config
        .extra
        .get("children")
        .and_then(|v| v.as_object());
    let Some(children) = children else {
        let _ = writeln!(out, "Federation peers: none declared");
        out.push('\n');
        return;
    };
    if children.is_empty() {
        let _ = writeln!(out, "Federation peers: none declared");
        out.push('\n');
        return;
    }
    let _ = writeln!(out, "Federation peers ({}):", children.len());
    let mut entries: Vec<_> = children.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (id, val) in entries {
        let display_name = val
            .get("display_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let endpoint = val
            .get("a2a_endpoint")
            .and_then(|v| v.as_str())
            .unwrap_or("(no endpoint)");
        let read_only = val
            .get("read_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let enabled = val
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let arrow = if plain { "→".to_string() } else { "→".cyan().to_string() };
        let id_part = if plain { id.clone() } else { id.bold().to_string() };
        let endpoint_part = if plain {
            endpoint.to_string()
        } else {
            endpoint.dimmed().to_string()
        };
        let suffix_raw = match (read_only, enabled) {
            (true, true) => " [read-only]".to_string(),
            (false, false) => " [disabled]".to_string(),
            (true, false) => " [read-only, disabled]".to_string(),
            (false, true) => String::new(),
        };
        let suffix = if plain || suffix_raw.is_empty() {
            suffix_raw
        } else {
            suffix_raw.dimmed().to_string()
        };
        let _ = writeln!(out, "  {} {} @ {}{}", arrow, id_part, endpoint_part, suffix);
        if !display_name.is_empty() {
            let _ = writeln!(out, "     {}", display_name);
        }
    }
    out.push('\n');
}

// --- Section 8: Footer -------------------------------------------------

fn section_footer(out: &mut String, plain: bool) {
    let l1 = "Run `neurogrim explain methodology` to learn the model.";
    let l2 = "Run `neurogrim doctor` to validate this Brain's config.";
    if plain {
        let _ = writeln!(out, "{}", l1);
        let _ = writeln!(out, "{}", l2);
    } else {
        let _ = writeln!(out, "{}", l1.dimmed());
        let _ = writeln!(out, "{}", l2.dimmed());
    }
}

// --- Helpers -----------------------------------------------------------

fn classification_display(c: &TrajectoryClassification) -> &'static str {
    match c {
        TrajectoryClassification::Improving => "improving",
        TrajectoryClassification::Degrading => "degrading",
        TrajectoryClassification::Stable => "stable",
        TrajectoryClassification::Volatile => "volatile",
        TrajectoryClassification::NoData => "insufficient data",
    }
}

// --- Tests ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_unchanged() {
        assert_eq!(truncate("hello", 100), "hello");
    }

    #[test]
    fn truncate_long_shortened_with_ellipsis() {
        let long = "x".repeat(200);
        let t = truncate(&long, 50);
        assert!(t.chars().count() <= 50);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn extract_description_from_frontmatter() {
        let md = "---\nname: foo\ndescription: A short summary.\n---\n\n# Skill: Foo\n\nbody";
        assert_eq!(extract_skill_description(md), "A short summary.");
    }

    #[test]
    fn extract_description_falls_back_to_h1() {
        let md = "# Skill: Bar\n\nbody";
        assert_eq!(extract_skill_description(md), "Skill: Bar");
    }

    #[test]
    fn extract_description_handles_no_match() {
        assert_eq!(
            extract_skill_description("just some text"),
            "(no description)"
        );
    }

    #[test]
    fn extract_description_handles_folded_scalar() {
        let md = "---\nname: foo\ndescription: >-\n  This is a folded\n  scalar description.\n---\n\nbody";
        let out = extract_skill_description(md);
        assert!(out.starts_with("This is a folded"), "got: {out}");
        assert!(out.contains("scalar description"));
    }

    #[test]
    fn extract_description_handles_literal_block() {
        let md = "---\nname: foo\ndescription: |-\n  Literal block\n  preserves newlines\n---\n";
        let out = extract_skill_description(md);
        assert!(out.contains("Literal block"));
        assert!(out.contains("preserves newlines"));
    }

    #[test]
    fn first_sentence_takes_first_period() {
        assert_eq!(first_sentence("Hello. World.", 100), "Hello.");
    }

    #[test]
    fn first_sentence_falls_back_to_truncate() {
        let s = "no sentence break here";
        assert_eq!(first_sentence(s, 100), s);
    }

    #[test]
    fn classification_strings_stable() {
        assert_eq!(
            classification_display(&TrajectoryClassification::Improving),
            "improving"
        );
        assert_eq!(
            classification_display(&TrajectoryClassification::Stable),
            "stable"
        );
    }
}
