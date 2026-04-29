//! Skill inventory + invocation-ledger reading for the dashboard's
//! `/api/skills` endpoint.
//!
//! Mirrors a small subset of the canonical
//! `neurogrim-sensory::capability_hygiene` logic — just enough to list
//! every `.claude/skills/<name>/SKILL.md` (or legacy `.md`) and pair
//! it with the most-recent invocation from
//! `.claude/brain/invocation-ledger.jsonl`.
//!
//! ## Why a separate module
//!
//! The capability_hygiene helpers (`collect_skill_entries`,
//! `read_invocation_ledger`) are intentionally private — the production
//! sensor's API surface is `evaluate(...)` returning findings, and
//! exposing internals would lock in field shapes that the sensor needs
//! to evolve. The dashboard reads the same files but emits a
//! human-tuned shape (sortable rows, classification banner, "no ledger
//! yet" CTA) so it has its own lightweight reader. The path conventions
//! are aligned with the sensor; if those drift, both will need to
//! update.

use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::path::Path;

use crate::types::SkillDto;

/// Window used to classify alive vs dead. 30 days matches the
/// canonical `capability_hygiene` `DEAD_WINDOW_DAYS_DEFAULT` so the
/// dashboard's "alive" badge agrees with the sensor's "alive" finding.
pub const ALIVE_WINDOW_DAYS: i64 = 30;

/// Result of scanning the project for skills + ledger entries.
pub struct SkillScan {
    pub skills: Vec<SkillDto>,
    pub ledger_present: bool,
    pub total_invocations: u32,
}

/// Walk the project's `.claude/skills/` directory and pair each skill
/// with its ledger stats. `now` is the reference time for windowing
/// (alive vs dead) — passed in rather than read from `Utc::now` so
/// tests can pin it.
pub fn scan(project_root: &Path, now: DateTime<Utc>) -> SkillScan {
    let skills_dir = project_root.join(".claude").join("skills");
    let raw_skills = collect_skill_entries(&skills_dir);

    let ledger_path = project_root
        .join(".claude")
        .join("brain")
        .join("invocation-ledger.jsonl");
    let ledger_present = ledger_path.exists();
    let (per_skill, total_invocations) = read_invocation_ledger(&ledger_path, now);

    let alive_cutoff = now - Duration::days(ALIVE_WINDOW_DAYS);

    let mut skills: Vec<SkillDto> = raw_skills
        .into_iter()
        .map(|raw| {
            let stats = per_skill.get(&raw.skill_id);
            let invocation_count = stats.map(|s| s.total_count).unwrap_or(0);
            let hard_count = stats.map(|s| s.hard_count).unwrap_or(0);
            let soft_count = stats.map(|s| s.soft_count).unwrap_or(0);
            let last_invoked_at = stats.and_then(|s| s.last_invoked).map(|t| t.to_rfc3339());
            let recent_invocation_count = stats
                .map(|s| {
                    s.invocations
                        .iter()
                        .filter(|(t, _)| *t >= alive_cutoff)
                        .count() as u32
                })
                .unwrap_or(0);
            let recent_hard_count = stats
                .map(|s| {
                    s.invocations
                        .iter()
                        .filter(|(t, k)| *t >= alive_cutoff && *k == InvocationSubtype::Hard)
                        .count() as u32
                })
                .unwrap_or(0);
            let recent_soft_count = stats
                .map(|s| {
                    s.invocations
                        .iter()
                        .filter(|(t, k)| *t >= alive_cutoff && *k == InvocationSubtype::Soft)
                        .count() as u32
                })
                .unwrap_or(0);
            let hygiene_status = classify(
                ledger_present,
                invocation_count,
                recent_invocation_count,
            );
            SkillDto {
                name: raw.skill_id,
                path: raw.path,
                format: match raw.format {
                    SkillFormat::Plugin => "plugin".to_string(),
                    SkillFormat::Legacy => "legacy".to_string(),
                },
                description: raw.description,
                last_invoked_at,
                invocation_count,
                recent_invocation_count,
                hard_invocations: hard_count,
                soft_invocations: soft_count,
                recent_hard_invocations: recent_hard_count,
                recent_soft_invocations: recent_soft_count,
                hygiene_status,
            }
        })
        .collect();

    skills.sort_by(|a, b| a.name.cmp(&b.name));

    SkillScan {
        skills,
        ledger_present,
        total_invocations,
    }
}

fn classify(
    ledger_present: bool,
    invocation_count: u32,
    recent_invocation_count: u32,
) -> String {
    if !ledger_present {
        "no-ledger".to_string()
    } else if recent_invocation_count > 0 {
        "alive".to_string()
    } else if invocation_count > 0 {
        "dead".to_string()
    } else {
        "new".to_string()
    }
}

#[derive(Debug)]
struct RawSkill {
    skill_id: String,
    path: String,
    description: String,
    format: SkillFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillFormat {
    Plugin, // .claude/skills/<name>/SKILL.md
    Legacy, // .claude/skills/<name>.md
}

fn collect_skill_entries(skills_dir: &Path) -> Vec<RawSkill> {
    let mut out: Vec<RawSkill> = Vec::new();
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
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let skill_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&name)
                .to_string();
            if let Ok(body) = std::fs::read_to_string(&path) {
                out.push(RawSkill {
                    skill_id,
                    path: format!(".claude/skills/{name}"),
                    description: extract_legacy_description(&body),
                    format: SkillFormat::Legacy,
                });
            }
        } else if path.is_dir() {
            let skill_md = path.join("SKILL.md");
            if !skill_md.is_file() {
                continue;
            }
            if let Ok(body) = std::fs::read_to_string(&skill_md) {
                out.push(RawSkill {
                    skill_id: name.clone(),
                    path: format!(".claude/skills/{name}/SKILL.md"),
                    description: extract_plugin_description(&body),
                    format: SkillFormat::Plugin,
                });
            }
        }
    }
    out
}

/// Pull the `description:` field out of a SKILL.md YAML frontmatter.
/// Frontmatter is bounded by leading `---` and a closing `---` on its
/// own line. We do a minimal hand-roll instead of pulling in serde_yaml
/// — only one field, and the format is well-controlled.
fn extract_plugin_description(body: &str) -> String {
    let trimmed = body.trim_start();
    let rest = match trimmed.strip_prefix("---") {
        Some(r) => r,
        None => return String::new(),
    };
    let close_idx = match rest.find("\n---") {
        Some(i) => i,
        None => return String::new(),
    };
    let frontmatter = &rest[..close_idx];

    // Walk lines, accumulate the value of `description:` until the
    // next top-level key (a line where the first non-space char is a
    // letter and the line contains `:`). YAML block scalars (`|` or
    // `>`) are preserved verbatim.
    let mut iter = frontmatter.lines().peekable();
    while let Some(line) = iter.next() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("description:") {
            let mut value = rest.trim().to_string();

            // YAML block-scalar markers — collect indented continuation lines.
            // Accept all four forms: `|`, `|-` (literal), `>`, `>-` (folded).
            // We treat them all the same for routing-text extraction —
            // chomping (`-`) and keep-newlines (default) only matter for
            // exact reproduction, which the dashboard doesn't need.
            if matches!(value.as_str(), "|" | "|-" | ">" | ">-") {
                value.clear();
                while let Some(next) = iter.peek() {
                    let leading_ws = next
                        .chars()
                        .take_while(|c| c.is_whitespace() && *c != '\n')
                        .count();
                    if leading_ws == 0 || next.trim().is_empty() {
                        if next.trim().is_empty() {
                            iter.next();
                            continue;
                        }
                        break;
                    }
                    let nxt = iter.next().unwrap();
                    if !value.is_empty() {
                        value.push(' ');
                    }
                    value.push_str(nxt.trim());
                }
            } else {
                // Plain scalar may continue on indented next lines.
                while let Some(next) = iter.peek() {
                    let leading_ws = next
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .count();
                    let next_trimmed = next.trim_start();
                    let looks_like_key =
                        next_trimmed.contains(':') && !next_trimmed.starts_with('-');
                    if leading_ws == 0 || looks_like_key {
                        break;
                    }
                    let nxt = iter.next().unwrap();
                    if !value.is_empty() {
                        value.push(' ');
                    }
                    value.push_str(nxt.trim());
                }
            }
            return strip_quotes(&value).to_string();
        }
    }
    String::new()
}

fn strip_quotes(s: &str) -> &str {
    let trimmed = s.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}

/// Pull the lead paragraph from a legacy `.claude/skills/<name>.md`
/// (everything before the first `##` heading, with leading H1 + blank
/// lines stripped).
fn extract_legacy_description(body: &str) -> String {
    let mut paragraph: Vec<&str> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("# ") {
            // Drop H1 — it's the title, not the description.
            continue;
        }
        if trimmed.starts_with("## ") {
            break;
        }
        if trimmed.is_empty() {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }
        paragraph.push(trimmed);
    }
    paragraph.join(" ")
}

/// Invocation subtype as recorded in the ledger.
///
/// - `Hard` — explicit `Skill` tool calls. The original signal.
/// - `Soft` — agent reads of a SKILL.md file via the Read tool.
///   Captures the more common pattern where an agent follows
///   skill guidance directly from the file rather than going
///   through the Skill tool. Without this signal the ledger
///   under-counts skill usage by an order of magnitude — agents
///   typically read 5-50 SKILL.md files per session for every
///   1 explicit Skill invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InvocationSubtype {
    Hard,
    Soft,
}

#[derive(Debug, Default)]
struct SkillStats {
    /// Total invocations across all time, both subtypes.
    total_count: u32,
    /// Hard invocations (explicit Skill tool calls), all-time.
    hard_count: u32,
    /// Soft invocations (Read on SKILL.md), all-time.
    soft_count: u32,
    /// Most-recent invocation timestamp (any subtype).
    last_invoked: Option<DateTime<Utc>>,
    /// Per-invocation timestamps + subtypes. Retained so we can
    /// window-count by subtype (recent_hard / recent_soft) without
    /// re-reading the ledger.
    invocations: Vec<(DateTime<Utc>, InvocationSubtype)>,
}

/// Read the JSONL ledger and return per-skill stats + the total count
/// across all rows. Tolerates malformed lines (silently skipped) and
/// missing files (returns empty + `0`).
fn read_invocation_ledger(
    ledger_path: &Path,
    _now: DateTime<Utc>,
) -> (HashMap<String, SkillStats>, u32) {
    let mut per_skill: HashMap<String, SkillStats> = HashMap::new();
    let mut total: u32 = 0;
    let text = match std::fs::read_to_string(ledger_path) {
        Ok(t) => t,
        Err(_) => return (per_skill, total),
    };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // Only count `type == "skill"` rows; the schema is open to
        // future hook events (mcp_call, command, etc.).
        let event_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("skill");
        if event_type != "skill" {
            continue;
        }
        let name = match parsed.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let ts_str = match parsed.get("ts").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => continue,
        };
        let ts = match DateTime::parse_from_rfc3339(ts_str) {
            Ok(t) => t.with_timezone(&Utc),
            Err(_) => continue,
        };

        // Subtype: schema_version 2+ ledgers carry `subtype: "hard"`
        // or `subtype: "soft"`. Schema 1 entries (and entries with
        // an unknown subtype) default to `hard` — that matches the
        // semantics of the original Skill-tool-only hook.
        let subtype = match parsed.get("subtype").and_then(|v| v.as_str()) {
            Some("soft") => InvocationSubtype::Soft,
            _ => InvocationSubtype::Hard,
        };

        total += 1;
        let entry = per_skill.entry(name).or_default();
        entry.total_count += 1;
        match subtype {
            InvocationSubtype::Hard => entry.hard_count += 1,
            InvocationSubtype::Soft => entry.soft_count += 1,
        }
        entry.invocations.push((ts, subtype));
        entry.last_invoked = Some(match entry.last_invoked {
            Some(prev) if prev > ts => prev,
            _ => ts,
        });
    }

    (per_skill, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_now() -> DateTime<Utc> {
        // 2026-04-28T12:00:00Z — matches the in-flight session date so
        // the ALIVE_WINDOW_DAYS math stays interpretable.
        DateTime::parse_from_rfc3339("2026-04-28T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn extracts_plain_description() {
        let body = "---\nname: foo\ndescription: A tiny rubber duck\nwhen_to_use: Stuck\n---\n\n# Body";
        assert_eq!(extract_plugin_description(body), "A tiny rubber duck");
    }

    #[test]
    fn extracts_quoted_description() {
        let body = "---\ndescription: \"hello, world\"\n---\n\n# Body";
        assert_eq!(extract_plugin_description(body), "hello, world");
    }

    #[test]
    fn extracts_block_scalar_description() {
        let body = "---\ndescription: |\n  First line\n  second line\n---\n";
        let got = extract_plugin_description(body);
        assert!(got.contains("First line"));
        assert!(got.contains("second line"));
    }

    #[test]
    fn extracts_folded_strip_block_scalar() {
        // `>-` is YAML folded with strip-newline — common in real
        // SKILL.md files in the ecosystem (e.g., coherence).
        let body =
            "---\ndescription: >-\n  Your unified score seems off; two\n  domains diverge.\n---\n";
        let got = extract_plugin_description(body);
        assert!(got.contains("Your unified score"));
        assert!(got.contains("domains diverge"));
        assert!(!got.starts_with(">-"));
    }

    #[test]
    fn extracts_literal_strip_block_scalar() {
        // `|-` is YAML literal with strip-newline.
        let body = "---\ndescription: |-\n  Line one\n  line two\n---\n";
        let got = extract_plugin_description(body);
        assert!(got.contains("Line one"));
        assert!(got.contains("line two"));
        assert!(!got.starts_with("|-"));
    }

    #[test]
    fn extracts_legacy_description_from_lead_paragraph() {
        let body = "# Skill Name\n\nThis is the description.\nMore prose here.\n\n## Section\n\nrest";
        let got = extract_legacy_description(body);
        assert!(got.contains("This is the description"));
        assert!(got.contains("More prose here"));
        assert!(!got.contains("rest"));
    }

    #[test]
    fn classify_no_ledger() {
        assert_eq!(classify(false, 0, 0), "no-ledger");
        assert_eq!(classify(false, 5, 5), "no-ledger");
    }

    #[test]
    fn classify_with_ledger() {
        assert_eq!(classify(true, 0, 0), "new");
        assert_eq!(classify(true, 5, 0), "dead");
        assert_eq!(classify(true, 5, 2), "alive");
    }

    #[test]
    fn scan_handles_missing_skills_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let scan = scan(tmp.path(), fixed_now());
        assert_eq!(scan.skills.len(), 0);
        assert!(!scan.ledger_present);
        assert_eq!(scan.total_invocations, 0);
    }

    #[test]
    fn scan_pairs_plugin_skill_with_ledger_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = tmp.path().join(".claude").join("skills");
        let brain = tmp.path().join(".claude").join("brain");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::create_dir_all(&brain).unwrap();

        let duck = skills.join("rubber-duck");
        std::fs::create_dir_all(&duck).unwrap();
        std::fs::write(
            duck.join("SKILL.md"),
            "---\nname: rubber-duck\ndescription: Socratic listener.\n---\n\n# body",
        )
        .unwrap();

        // Ledger: one alive (2 days ago), one dead (60 days ago).
        let now = fixed_now();
        let alive_ts = (now - Duration::days(2)).to_rfc3339();
        let dead_ts = (now - Duration::days(60)).to_rfc3339();
        let ledger = format!(
            "{{\"type\":\"skill\",\"name\":\"rubber-duck\",\"ts\":\"{alive_ts}\"}}\n\
             {{\"type\":\"skill\",\"name\":\"rubber-duck\",\"ts\":\"{dead_ts}\"}}\n"
        );
        std::fs::write(brain.join("invocation-ledger.jsonl"), ledger).unwrap();

        let scan = scan(tmp.path(), now);
        assert_eq!(scan.skills.len(), 1);
        let s = &scan.skills[0];
        assert_eq!(s.name, "rubber-duck");
        assert_eq!(s.format, "plugin");
        assert_eq!(s.invocation_count, 2);
        assert_eq!(s.recent_invocation_count, 1); // only the 2-days-ago one
        assert_eq!(s.hygiene_status, "alive");
        assert!(s.last_invoked_at.is_some());
        assert!(scan.ledger_present);
        assert_eq!(scan.total_invocations, 2);
    }

    #[test]
    fn scan_classifies_dead_skill_when_only_old_invocations() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = tmp.path().join(".claude").join("skills");
        let brain = tmp.path().join(".claude").join("brain");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::create_dir_all(&brain).unwrap();

        let dir = skills.join("forgotten");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\ndescription: Used once, never again.\n---\n",
        )
        .unwrap();

        let now = fixed_now();
        let stale = (now - Duration::days(120)).to_rfc3339();
        std::fs::write(
            brain.join("invocation-ledger.jsonl"),
            format!("{{\"type\":\"skill\",\"name\":\"forgotten\",\"ts\":\"{stale}\"}}\n"),
        )
        .unwrap();

        let scan = scan(tmp.path(), now);
        let s = &scan.skills[0];
        assert_eq!(s.hygiene_status, "dead");
        assert_eq!(s.invocation_count, 1);
        assert_eq!(s.recent_invocation_count, 0);
    }

    #[test]
    fn scan_classifies_new_when_skill_exists_but_no_invocation() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = tmp.path().join(".claude").join("skills");
        let brain = tmp.path().join(".claude").join("brain");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::create_dir_all(&brain).unwrap();

        let dir = skills.join("brand-new");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\ndescription: Just authored.\n---\n",
        )
        .unwrap();
        // Empty ledger file present.
        std::fs::write(brain.join("invocation-ledger.jsonl"), "").unwrap();

        let scan = scan(tmp.path(), fixed_now());
        let s = &scan.skills[0];
        assert_eq!(s.hygiene_status, "new");
        assert_eq!(s.invocation_count, 0);
    }

    #[test]
    fn scan_marks_no_ledger_when_file_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = tmp.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        let dir = skills.join("alpha");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "---\ndescription: hi.\n---\n").unwrap();

        let scan = scan(tmp.path(), fixed_now());
        assert!(!scan.ledger_present);
        assert_eq!(scan.skills[0].hygiene_status, "no-ledger");
    }

    #[test]
    fn scan_splits_hard_and_soft_invocations() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = tmp.path().join(".claude").join("skills");
        let brain = tmp.path().join(".claude").join("brain");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::create_dir_all(&brain).unwrap();
        let dir = skills.join("rubber-duck");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\ndescription: Socratic listener.\n---\n",
        )
        .unwrap();

        let now = fixed_now();
        let recent = (now - Duration::days(2)).to_rfc3339();
        // Mix of subtypes + a schema-1 entry (no subtype field — must
        // default to hard for backward compat).
        let ledger = format!(
            "{{\"schema_version\":\"2\",\"type\":\"skill\",\"subtype\":\"hard\",\"name\":\"rubber-duck\",\"ts\":\"{recent}\"}}\n\
             {{\"schema_version\":\"2\",\"type\":\"skill\",\"subtype\":\"soft\",\"name\":\"rubber-duck\",\"ts\":\"{recent}\"}}\n\
             {{\"schema_version\":\"2\",\"type\":\"skill\",\"subtype\":\"soft\",\"name\":\"rubber-duck\",\"ts\":\"{recent}\"}}\n\
             {{\"schema_version\":\"1\",\"type\":\"skill\",\"name\":\"rubber-duck\",\"ts\":\"{recent}\"}}\n"
        );
        std::fs::write(brain.join("invocation-ledger.jsonl"), ledger).unwrap();

        let scan = scan(tmp.path(), now);
        assert_eq!(scan.skills.len(), 1);
        let s = &scan.skills[0];
        assert_eq!(s.invocation_count, 4, "total = hard + soft");
        assert_eq!(
            s.hard_invocations, 2,
            "1 explicit hard + 1 schema-1 (defaults to hard)"
        );
        assert_eq!(s.soft_invocations, 2);
        assert_eq!(s.recent_hard_invocations, 2);
        assert_eq!(s.recent_soft_invocations, 2);
        assert_eq!(s.recent_invocation_count, 4);
    }

    #[test]
    fn scan_excludes_archived_and_readme() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = tmp.path().join(".claude").join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::write(skills.join("README.md"), "# README").unwrap();
        std::fs::create_dir_all(skills.join("archived")).unwrap();
        std::fs::write(
            skills.join("archived").join("SKILL.md"),
            "---\ndescription: stale.\n---\n",
        )
        .unwrap();
        let dir = skills.join("real");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "---\ndescription: live.\n---\n").unwrap();

        let scan = scan(tmp.path(), fixed_now());
        assert_eq!(scan.skills.len(), 1);
        assert_eq!(scan.skills[0].name, "real");
    }
}
