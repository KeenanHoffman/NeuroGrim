//! Backlog-symbol sensor (IDE-BACKLOG B0, 2026-06-17).
//!
//! Overlays STRUCTURE onto the backlog/roadmap markdown — the "symbol"
//! view the IDE-BACKLOG pane + deterministic broker + agents consume.
//! Walks `project_root` for backlog-shaped files (`BACKLOG.md`,
//! `ROADMAP.md`, `execution.md`) and parses work-items in TWO conventions
//! (the multi-format finding):
//!
//! - **heading-sections** — `### <ID>: title — STATUS` (NeuroGrim
//!   `roadmap/BACKLOG.md`) and `### <ID> — name` (epic `execution.md`
//!   batch headings),
//! - **table-rows** — `| <ID> | Subject | dep B0 / gates B2 |` (the IDE
//!   `docs/plans/ROADMAP.md` epic tables — more machine-readable).
//!
//! An item id is a token that starts uppercase + contains a digit
//! (`B0`, `D1`, `B-54`, `WU-0.1`, `IDE-BACKLOG-B0`, `R-LSP62`) — which
//! cleanly excludes prose headings (`Why`, `Status`) and table header
//! rows (`ID`, `Subject`).
//!
//! Emits a cmdb-envelope JSON (the `documentation-graph` pattern):
//! `score` + `findings` + an `items` extra (the live symbol model the
//! IDE caches) + counts. The `backlog-health` Brain domain reads the
//! `score`; the IDE reads `items`.
//!
//! Convention-discovery (under `project_root`) is the v1 source model —
//! each repo's `neurogrim sensory backlog` parses its own backlog files;
//! an explicit `--source` path list is a future refinement (the Sensor
//! trait passes only `project_root`).

use crate::cmdb::{build_cmdb, Finding};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

const SKIPPED_DIR_NAMES: &[&str] = &[
    "node_modules", "target", ".git", "dist", "build", "__pycache__", ".cargo", ".rustup", "venv",
    ".venv",
];

/// One parsed work-item (a backlog "symbol").
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct BacklogItem {
    pub id: String,
    pub title: String,
    /// Best-effort status string (`CANDIDATE`, `SHIPPED`, `dep B0`, …) or empty.
    pub status: String,
    /// Referenced item ids (dep/gates), best-effort.
    pub deps: Vec<String>,
    pub source_file: String,
    pub line: usize,
    /// `"heading"` or `"table"`.
    pub format: &'static str,

    // IDE-BACKLOG-PM schema fields (PM-A1). Parsed from the `**Field:**`
    // body-line convention (mirrors the existing `**Dependencies:**` scan);
    // empty when absent. Inference + defaults land in PM-A2. Empty fields
    // are skip-serialized so back-compat consumers see the unchanged shape.
    /// `story` / `bug` / `discovery` / `request` / `question` (D-TYPES).
    #[serde(rename = "type", skip_serializing_if = "String::is_empty")]
    pub item_type: String,
    /// Lifecycle stage (D-PIPELINE): Proposed/Discovery/Ready/In-progress/…
    #[serde(skip_serializing_if = "String::is_empty")]
    pub stage: String,
    /// Must / Should / Could / Won't (epic-relative, D-MOSCOW).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub moscow: String,
    /// Container ref — the epic/stage this item belongs to (D-EPIC).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub epic: String,
    /// Code/vision anchor (`file:line` / `vision:#N` / `change:seq`). The
    /// capture anchor (D-WRITEBACK); distinct from `source_file` (the backlog file).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source_anchor: String,
    /// Deferred wake condition (`dep:X` / `date:…` / `flag:…` / `manual`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub wake: String,
    /// One-line rationale (`**Why:**`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub why: String,
    /// Pointer to the `ConfirmationRow` evidencing DoD (D-DONE); `run:<uuid>`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub evidence_run_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct BacklogReport {
    pub items: Vec<BacklogItem>,
    pub files_scanned: usize,
    /// `(item_id, missing_dep)` — a dep referenced by an item that doesn't
    /// resolve to any known item id (a dangling dependency).
    pub dangling_deps: Vec<(String, String)>,
}

/// An item id: starts with an ASCII-uppercase letter, contains only
/// `[A-Za-z0-9._-]`, has at least one digit, and is short. The
/// digit requirement excludes prose headings (`Why`, `Status`,
/// `Cross-references`) and table header cells (`ID`, `Subject`).
fn looks_like_id(token: &str) -> bool {
    let t = token.trim_matches(|c| c == ':' || c == '.' || c == ',' || c == '*' || c == '`');
    if t.len() < 2 || t.len() > 40 {
        return false;
    }
    let mut chars = t.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    let mut has_digit = false;
    for c in t.chars() {
        if c.is_ascii_digit() {
            has_digit = true;
        }
        if !(c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_') {
            return false;
        }
    }
    has_digit
}

/// Extract the leading id token from heading/cell text, stripping
/// markdown emphasis. Returns `(id, rest)` where `rest` is the text
/// after the id.
fn leading_id(text: &str) -> Option<(String, String)> {
    let s = text.trim().trim_start_matches('*').trim_start_matches('`').trim_start();
    let mut split = s.splitn(2, char::is_whitespace);
    let first = split.next().unwrap_or("");
    let rest = split.next().unwrap_or("").to_string();
    let cleaned = first.trim_matches(|c| c == ':' || c == '*' || c == '`');
    if looks_like_id(cleaned) {
        Some((cleaned.to_string(), rest))
    } else {
        None
    }
}

/// All id-looking tokens in `text` (used for dep extraction from a
/// status/deps cell or heading tail like `· dep B0 + D2; gates B6`).
fn extract_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')) {
        if looks_like_id(raw) && !out.iter().any(|e| e == raw) {
            out.push(raw.to_string());
        }
    }
    out
}

/// Split the heading/title text into `(title, status)` on the last
/// ` — ` (em-dash) or ` -- ` separator, when the tail looks like a
/// status (a keyword or a parenthesised date). Otherwise `(text, "")`.
fn split_title_status(text: &str) -> (String, String) {
    let t = text.trim().trim_end_matches('*').trim();
    // Prefer the em-dash separator used in `### B-09: title — STATUS`.
    for sep in [" — ", " -- "] {
        if let Some(idx) = t.rfind(sep) {
            let (title, status) = t.split_at(idx);
            let status = status[sep.len()..].trim().to_string();
            return (title.trim().to_string(), status);
        }
    }
    (t.to_string(), String::new())
}

/// Parse a `**Key:** value` schema body line → `(lowercase-key, value)`.
/// Mirrors the existing `**Dependencies:**` convention for the
/// IDE-BACKLOG-PM fields (`**Type:**`, `**MoSCoW:**`, `**Epic:**`,
/// `**Stage:**`, `**Source:**`, `**Wake:**`, `**Why:**`, `**Evidence:**`).
/// A trailing `<!-- … -->` template comment is stripped from the value.
/// Returns `None` for non-field lines.
fn parse_field_line(line: &str) -> Option<(String, String)> {
    let body = line.trim().strip_prefix("**")?;
    let close = body.find("**")?;
    let key = body[..close].trim().trim_end_matches(':').trim().to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }
    let mut val = body[close + 2..].trim();
    if let Some(c) = val.find("<!--") {
        val = val[..c].trim();
    }
    Some((key, val.to_string()))
}

fn is_backlog_file(rel: &str) -> bool {
    let base = Path::new(rel)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    base == "backlog.md" || base == "roadmap.md" || base == "execution.md"
}

/// Parse one markdown file into items. Line-based with a fenced-code
/// guard (so `### B0` inside a ``` fence isn't an item).
fn parse_file(text: &str, rel: &str) -> Vec<BacklogItem> {
    let mut items: Vec<BacklogItem> = Vec::new();
    let mut in_fence = false;
    // Index of the heading item whose body we're currently inside, so
    // `**Dependencies:**`/`dep`/`gates` body lines attach to it (NeuroGrim
    // `BACKLOG.md` declares deps in prose under the heading).
    let mut cur: Option<usize> = None;
    for (i, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim_end();
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }

        // Heading items: ## / ### / #### with a leading id.
        if trimmed.starts_with('#') {
            cur = None;
            let hashes = trimmed.chars().take_while(|c| *c == '#').count();
            if (2..=4).contains(&hashes) {
                let head = trimmed[hashes..].trim();
                if let Some((id, rest)) = leading_id(head) {
                    let (title, status) = split_title_status(&rest);
                    let mut deps = extract_ids(&status);
                    // Also scan the heading tail (execution batch headings
                    // carry `· dep D1` after the title).
                    for d in extract_ids(&rest) {
                        if d != id && !deps.contains(&d) {
                            deps.push(d);
                        }
                    }
                    deps.retain(|d| d != &id);
                    items.push(BacklogItem {
                        id,
                        title: title.trim_matches('`').trim().to_string(),
                        status,
                        deps,
                        source_file: rel.to_string(),
                        line: i + 1,
                        format: "heading",
                        ..Default::default()
                    });
                    cur = Some(items.len() - 1);
                }
            }
            continue;
        }

        // Table-row items: `| <id> | subject | … | deps |`.
        if trimmed.starts_with('|') {
            let cells: Vec<String> = trimmed
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .collect();
            if cells.len() < 2 {
                continue;
            }
            // Skip separator rows (`| --- | --- |`).
            if cells[0].chars().all(|c| c == '-' || c == ':' || c.is_whitespace()) {
                continue;
            }
            if let Some((id, _rest)) = leading_id(&cells[0]) {
                let title = cells.get(1).cloned().unwrap_or_default();
                let deps_cell = cells.last().cloned().unwrap_or_default();
                let status = deps_cell.clone();
                let mut deps = extract_ids(&deps_cell);
                deps.retain(|d| d != &id);
                items.push(BacklogItem {
                    id,
                    title: title.trim_matches('`').trim().to_string(),
                    status,
                    deps,
                    source_file: rel.to_string(),
                    line: i + 1,
                    format: "table",
                    ..Default::default()
                });
            }
            continue;
        }

        // Body line of the current heading item.
        if let Some(idx) = cur {
            // `**Field:** value` schema lines (PM-A1) — set the matching field.
            if let Some((key, val)) = parse_field_line(line) {
                if !val.is_empty() {
                    match key.as_str() {
                        "type" => items[idx].item_type = val,
                        "stage" => items[idx].stage = val,
                        "moscow" => items[idx].moscow = val,
                        "epic" => items[idx].epic = val,
                        "source" => items[idx].source_anchor = val,
                        "wake" => items[idx].wake = val,
                        "why" => items[idx].why = val,
                        "evidence" => items[idx].evidence_run_id = val,
                        _ => {}
                    }
                }
            }
            // Declared deps (`**Dependencies:**` / `dep` / `gates` / `blocked`).
            let low = line.to_lowercase();
            if low.contains("depend") || low.contains(" dep ") || low.contains("· dep")
                || low.contains("gates ") || low.contains("blocked")
            {
                let own = items[idx].id.clone();
                for d in extract_ids(line) {
                    if d != own && !items[idx].deps.contains(&d) {
                        items[idx].deps.push(d);
                    }
                }
            }
        }
    }
    items
}

fn relpath(root: &Path, abs: &Path) -> Option<String> {
    Some(abs.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/"))
}

fn walk(root: &Path, dir: &Path, visit: &mut impl FnMut(&Path)) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') && name != ".claude" {
                continue;
            }
            if SKIPPED_DIR_NAMES.contains(&name) {
                continue;
            }
        }
        if path.is_dir() {
            walk(root, &path, visit);
        } else {
            visit(&path);
        }
    }
}

/// Pure parser — walk `root` for backlog-shaped files + parse items +
/// compute dangling deps. No async, no I/O beyond reads.
pub fn parse_backlog(root: &Path) -> BacklogReport {
    let mut backlog_files: Vec<String> = Vec::new();
    walk(root, root, &mut |abs| {
        if let Some(rel) = relpath(root, abs) {
            if is_backlog_file(&rel) {
                backlog_files.push(rel);
            }
        }
    });
    backlog_files.sort();

    let mut items: Vec<BacklogItem> = Vec::new();
    for rel in &backlog_files {
        let abs = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Ok(text) = std::fs::read_to_string(&abs) {
            items.extend(parse_file(&text, rel));
        }
    }

    let known: BTreeSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
    let mut dangling: Vec<(String, String)> = Vec::new();
    for it in &items {
        for d in &it.deps {
            if !known.contains(d.as_str()) {
                dangling.push((it.id.clone(), d.clone()));
            }
        }
    }

    BacklogReport {
        items,
        files_scanned: backlog_files.len(),
        dangling_deps: dangling,
    }
}

/// Sensor entry point (the `documentation-graph` shape). Returns a
/// cmdb-envelope JSON: `score` + `findings` + an `items` extra (the live
/// symbol model) + counts.
pub async fn analyze_backlog(project_root: &str) -> Value {
    let report = parse_backlog(Path::new(project_root));
    let total = report.items.len();
    let heading_items = report.items.iter().filter(|i| i.format == "heading").count();
    let table_items = report.items.iter().filter(|i| i.format == "table").count();
    let unknown_status = report.items.iter().filter(|i| i.status.trim().is_empty()).count();
    let dangling = report.dangling_deps.len();

    // Health score: penalise dangling deps (broken structure) + a mild
    // unknown-status drag (an item with no status marker is harder to
    // dispatch). Advisory; floor 0.
    let unknown_ratio = if total == 0 { 0.0 } else { unknown_status as f64 / total as f64 };
    let unknown_penalty = (unknown_ratio * 20.0).min(20.0);
    let dangling_penalty = ((dangling as f64) * 5.0).min(40.0);
    let score: i32 = (100.0 - unknown_penalty - dangling_penalty).clamp(0.0, 100.0) as i32;

    let mut findings: Vec<Finding> = Vec::new();
    if total == 0 {
        findings.push(Finding {
            name: "no_backlog_items".into(),
            status: "info".into(),
            points: 0,
            detail: Some("No backlog items found (looked for BACKLOG.md / ROADMAP.md / execution.md)".into()),
        });
    } else {
        findings.push(Finding {
            name: "item_count".into(),
            status: "info".into(),
            points: 0,
            detail: Some(format!(
                "{total} items across {} file(s) ({heading_items} heading, {table_items} table)",
                report.files_scanned
            )),
        });
    }
    if dangling > 0 {
        findings.push(Finding {
            name: "dangling_deps".into(),
            status: "warn".into(),
            points: -(dangling_penalty as i32),
            detail: Some(format!(
                "{dangling} dep reference(s) point at unknown items — first 5: [{}]",
                report
                    .dangling_deps
                    .iter()
                    .take(5)
                    .map(|(s, t)| format!("{s}->{t}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        });
    }
    if unknown_status > 0 {
        findings.push(Finding {
            name: "items_without_status".into(),
            status: "info".into(),
            points: -(unknown_penalty as i32),
            detail: Some(format!("{unknown_status} of {total} items have no status marker")),
        });
    }

    let items_json = serde_json::to_value(&report.items).unwrap_or(Value::Array(vec![]));
    let extras: Vec<(&str, Value)> = vec![
        ("item_count", Value::Number(total.into())),
        ("heading_items", Value::Number(heading_items.into())),
        ("table_items", Value::Number(table_items.into())),
        ("unknown_status_count", Value::Number(unknown_status.into())),
        ("dangling_dep_count", Value::Number(dangling.into())),
        ("files_scanned", Value::Number(report.files_scanned.into())),
        // The live symbol model the IDE caches (the broker + pane read this).
        ("items", items_json),
    ];

    build_cmdb("check-backlog", score.clamp(0, 100) as u8, findings, Some(extras), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn id_heuristic_accepts_real_ids_rejects_prose() {
        for ok in ["B0", "D1", "B-54", "WU-0.1", "IDE-BACKLOG-B0", "R-LSP62", "X1", "O1.2"] {
            assert!(looks_like_id(ok), "{ok} should be an id");
        }
        for no in ["Why", "Status", "Cross-references", "Subject", "ID", "the"] {
            assert!(!looks_like_id(no), "{no} should NOT be an id");
        }
    }

    #[test]
    fn parses_heading_section_items() {
        let md = "\
# Backlog

### B-09: CLI-mode sensory access — COMPLETE (2026-04-22)
**Dependencies:** none.

### B-54: BACKLOG-SYMBOLS — CANDIDATE (2026-06-17)
prose body.

### Why it's here
not an item (no id).
";
        let items = parse_file(md, "roadmap/BACKLOG.md");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "B-09");
        assert_eq!(items[0].format, "heading");
        assert!(items[0].status.starts_with("COMPLETE"));
        assert_eq!(items[1].id, "B-54");
    }

    #[test]
    fn parses_table_row_items_skips_header_and_separator() {
        let md = "\
| ID | Subject | Status / deps |
|---|---|---|
| D1 | Symbol substrate | discovery; gates B0 |
| B0 — symbol sensor | Heading parser | dep D1; standalone |
";
        let items = parse_file(md, "docs/plans/ROADMAP.md");
        // The header row (`ID`) + separator are skipped; D1 + B0 are items.
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "D1");
        assert_eq!(items[0].format, "table");
        assert!(items[0].deps.contains(&"B0".to_string()));
        assert_eq!(items[1].id, "B0");
        assert!(items[1].deps.contains(&"D1".to_string()));
    }

    #[test]
    fn ignores_headings_inside_code_fences() {
        let md = "```\n### B0 — fake item in a fence\n```\n\n### B1 — real item\n";
        let items = parse_file(md, "execution.md");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "B1");
    }

    #[test]
    fn parse_backlog_walks_and_flags_dangling_deps() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "roadmap/BACKLOG.md", "### B-01: thing — CANDIDATE\nbody · dep B-99\n");
        write(dir.path(), "src/main.rs", "not markdown");
        let report = parse_backlog(dir.path());
        assert_eq!(report.files_scanned, 1);
        assert_eq!(report.items.len(), 1);
        // B-99 is referenced but not a known item → dangling.
        assert_eq!(report.dangling_deps, vec![("B-01".to_string(), "B-99".to_string())]);
    }

    #[tokio::test]
    async fn analyze_backlog_emits_cmdb_envelope_with_items() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "roadmap/BACKLOG.md", "### B-01: alpha — CANDIDATE\n### B-02: beta — SHIPPED\n");
        let cmdb = analyze_backlog(&dir.path().to_string_lossy()).await;
        assert_eq!(cmdb["meta"]["updated_by"], "check-backlog");
        assert!(cmdb["score"].is_number());
        assert_eq!(cmdb["item_count"], 2);
        assert_eq!(cmdb["items"].as_array().unwrap().len(), 2);
        assert_eq!(cmdb["items"][0]["id"], "B-01");
    }

    #[test]
    fn parses_schema_field_body_lines() {
        let md = "\
### PM-A1: schema field parse — Ready
**Type:** story            <!-- story | bug | discovery | request | question -->
**MoSCoW:** Must
**Epic:** IDE-BACKLOG-PM
**Source:** crates/neurogrim-sensory/src/backlog.rs:43
**Wake:** manual
**Why:** the foundation everything reads
**Evidence:** run:abc-123
**Dependencies:** none
";
        let items = parse_file(md, "execution.md");
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.id, "PM-A1");
        assert_eq!(it.item_type, "story", "**Type:** comment must be stripped");
        assert_eq!(it.moscow, "Must");
        assert_eq!(it.epic, "IDE-BACKLOG-PM");
        assert_eq!(it.source_anchor, "crates/neurogrim-sensory/src/backlog.rs:43");
        assert_eq!(it.wake, "manual");
        assert_eq!(it.why, "the foundation everything reads");
        assert_eq!(it.evidence_run_id, "run:abc-123");
        // The `**Epic:**` ref must NOT be mis-parsed as a dependency.
        assert!(it.deps.is_empty(), "deps: {:?}", it.deps);
    }

    #[test]
    fn schema_fields_skip_serialize_when_absent() {
        // A legacy item with no `**Field:**` lines serializes unchanged
        // (back-compat: the new keys are omitted, not emitted as empty).
        let items = parse_file("### B-01: legacy — CANDIDATE\n", "BACKLOG.md");
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["id"], "B-01");
        assert!(v.get("type").is_none(), "absent type must be omitted");
        assert!(v.get("moscow").is_none());
        assert!(v.get("stage").is_none());
    }
}
