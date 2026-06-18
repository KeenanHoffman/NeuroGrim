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
use std::collections::{BTreeMap, BTreeSet};
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
    /// `human` / `agent` / `system` (D-TYPES origin axis); defaults `agent`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub origin: String,
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
    /// Epic/container declared value — the MoSCoW anchor (D-EPIC, D-MOSCOW).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    /// Epic/container priority — the cross-epic ranking key (D-EPIC).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub priority: String,
    /// The backlog's `**Plan when:**` convention — `Some` when the field is
    /// present (even if blank). A non-"always" condition means the item is
    /// deferred until that condition (grooming sweep): the stage is set to
    /// Deferred and the condition becomes the wake.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_when: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BacklogReport {
    pub items: Vec<BacklogItem>,
    pub files_scanned: usize,
    /// `(item_id, missing_dep)` — a dep referenced by an item that doesn't
    /// resolve to any known item id (a dangling dependency, B-LIVE-2).
    pub dangling_deps: Vec<(String, String)>,
    /// Dependency cycles, each the ordered member ids (B-LIVE-3).
    pub cycles: Vec<Vec<String>>,
    /// Item ids with no recognizable stage signal — need triage (B-LIVE-4).
    pub untriaged: Vec<String>,
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

/// Narrative type inference (PM-A2, D-TYPES §6) — best-effort, default
/// `story`. `question`/`request` are created typed (via Discuss), not
/// inferred from markdown, so inference only yields story/bug/discovery.
fn infer_type(it: &BacklogItem) -> &'static str {
    let hay = format!("{} {} {}", it.title, it.status, it.why).to_ascii_lowercase();
    let has = |kw: &str| hay.contains(kw);
    if has("bug") || has("regression") || has("broken") || has("repro") || has("crash")
        || has(" fix ") || hay.starts_with("fix ")
    {
        "bug"
    } else if has("discovery") || has("research") || has("spike") || has("investigate")
        || has("open question") || has("plan when")
    {
        "discovery"
    } else {
        "story"
    }
}

/// Map the legacy status vocabulary to a lifecycle `stage` (D-PIPELINE,
/// discovery-05 §F). `None` = no recognizable signal (→ untriaged). Order
/// matters: a "done" marker (incl. `✅`) wins over an embedded keyword.
fn status_to_stage(status: &str) -> Option<&'static str> {
    let u = status.to_ascii_uppercase();
    let done = u.contains("SHIPPED") || u.contains("COMPLETE") || u.contains("DONE")
        || status.contains('\u{2705}'); // ✅
    if done {
        Some("Human-verified-done")
    } else if u.contains("IN PROGRESS") || u.contains("IN-PROGRESS") || u.contains("WIP") {
        Some("In-progress")
    } else if u.contains("AGENT-DONE") || u.contains("AGENT DONE") {
        Some("Agent-done")
    } else if u.contains("DEFERRED") || u.contains("PARKED") || u.contains("ABSORBED")
        || u.contains("STRETCH") || u.contains("HORIZON") || u.contains("SUCCESSOR")
        || u.contains("NO CURRENT PLAN")
    {
        // A deferral tag (e.g. "CANDIDATE (v5.5 horizon)") wins over the bare
        // CANDIDATE/READY token — the item is deferred, not available.
        Some("Deferred")
    } else if u.contains("BLOCKED") {
        Some("Blocked")
    } else if u.contains("READY") || u.contains("NEXT") {
        Some("Ready")
    } else if u.contains("DISCOVERY") {
        Some("Discovery")
    } else if u.contains("CANDIDATE") || u.contains("PROPOSED") || u.contains("PENDING") {
        Some("Proposed")
    } else {
        None
    }
}

/// Whether a `**Plan when:**` condition is an "always/now" word (so the item
/// is NOT deferred — it's plannable anytime). Empty conditions still defer
/// (the field's presence signals "to be planned").
fn plan_when_is_always(cond: &str) -> bool {
    let c = cond.to_ascii_lowercase();
    c.contains("anytime")
        || c.contains("immediately")
        || c.contains(" now")
        || c.starts_with("now")
        || c.contains("complete")
        || c.contains("opportunistic")
        || c.contains("no blocker")
        || c.contains("no precondition")
}

/// Fill inferred defaults for fields the markdown didn't declare (PM-A2).
/// Only fills when empty — an explicit `**Type:**`/`**Stage:**`/`**Origin:**`
/// always wins.
fn infer_item_fields(it: &mut BacklogItem) {
    let explicit_stage = !it.stage.is_empty();
    if it.item_type.is_empty() {
        it.item_type = infer_type(it).to_string();
    }
    if it.stage.is_empty() {
        it.stage = status_to_stage(&it.status).unwrap_or("Proposed").to_string();
    }
    // Grooming sweep: a `**Plan when:**` field (and the item isn't already
    // done/terminal or explicitly staged) means "deferred until that
    // condition" — UNLESS the condition is an always/now word. The condition
    // becomes the wake. This makes the backlog's own deferral convention
    // machine-readable so the broker stops dispatching deferred items.
    if !explicit_stage && !matches!(it.stage.as_str(), "Human-verified-done" | "Agent-done") {
        if let Some(cond) = it.plan_when.clone() {
            if !plan_when_is_always(&cond) {
                it.stage = "Deferred".to_string();
                if it.wake.is_empty() {
                    let c = cond.trim();
                    it.wake = if c.is_empty() {
                        "manual — see Plan-when".to_string()
                    } else {
                        format!("manual — {c}")
                    };
                }
            }
        }
    }
    if it.origin.is_empty() {
        it.origin = "agent".to_string();
    }
    // PM-A4: bootstrap the epic ref from `ABSORBED into <EPIC>` (D-EPIC §C).
    if it.epic.is_empty() && it.status.to_ascii_lowercase().contains("absorbed") {
        if let Some(first) = extract_ids(&it.status).into_iter().find(|d| d != &it.id) {
            it.epic = first;
        }
    }
}

/// Detect dependency cycles over the known-item dep graph (B-LIVE-3) via
/// colored DFS. Each cycle is the ordered member ids; deduped by member set.
fn find_cycles(items: &[BacklogItem]) -> Vec<Vec<String>> {
    use std::collections::HashMap;
    let index: HashMap<&str, usize> =
        items.iter().enumerate().map(|(i, it)| (it.id.as_str(), i)).collect();
    let adj: Vec<Vec<usize>> = items
        .iter()
        .map(|it| it.deps.iter().filter_map(|d| index.get(d.as_str()).copied()).collect())
        .collect();
    let n = items.len();
    let mut color = vec![0u8; n]; // 0 white, 1 gray (on stack), 2 black
    let mut stack: Vec<usize> = Vec::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    fn dfs(
        u: usize,
        adj: &[Vec<usize>],
        color: &mut [u8],
        stack: &mut Vec<usize>,
        items: &[BacklogItem],
        cycles: &mut Vec<Vec<String>>,
    ) {
        color[u] = 1;
        stack.push(u);
        for &v in &adj[u] {
            if color[v] == 1 {
                if let Some(pos) = stack.iter().position(|&x| x == v) {
                    cycles.push(stack[pos..].iter().map(|&i| items[i].id.clone()).collect());
                }
            } else if color[v] == 0 {
                dfs(v, adj, color, stack, items, cycles);
            }
        }
        stack.pop();
        color[u] = 2;
    }

    for i in 0..n {
        if color[i] == 0 {
            dfs(i, &adj, &mut color, &mut stack, items, &mut cycles);
        }
    }

    // Dedup by sorted member set (the same cycle can be found from several entries).
    let mut seen: BTreeSet<Vec<String>> = BTreeSet::new();
    cycles
        .into_iter()
        .filter(|c| {
            let mut key = c.clone();
            key.sort();
            seen.insert(key)
        })
        .collect()
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
                // Status: the col-2+ cell carrying a recognized status word
                // (ROADMAP tables put Status in a middle column with Effort
                // last, e.g. `| S5-TP-1 | … | **Complete** | XL |`); fall back
                // to the last cell (the `Status / deps` convention).
                let status = cells
                    .iter()
                    .skip(2)
                    .find(|c| status_to_stage(c).is_some())
                    .cloned()
                    .unwrap_or_else(|| cells.last().cloned().unwrap_or_default());
                // Deps: scan every col-2+ cell (status / deps / notes).
                let mut deps: Vec<String> = Vec::new();
                for c in cells.iter().skip(2) {
                    for d in extract_ids(c) {
                        if d != id && !deps.contains(&d) {
                            deps.push(d);
                        }
                    }
                }
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
                // The `**Plan when:**` deferral convention — record its presence
                // even when blank (grooming sweep, handled in infer_item_fields).
                if key == "plan when" {
                    items[idx].plan_when = Some(val.clone());
                }
                if !val.is_empty() {
                    match key.as_str() {
                        "type" => items[idx].item_type = val,
                        "origin" => items[idx].origin = val,
                        "stage" => items[idx].stage = val,
                        "moscow" => items[idx].moscow = val,
                        "epic" => items[idx].epic = val,
                        "source" => items[idx].source_anchor = val,
                        "wake" => items[idx].wake = val,
                        "why" => items[idx].why = val,
                        "evidence" => items[idx].evidence_run_id = val,
                        "value" => items[idx].value = val,
                        "priority" => items[idx].priority = val,
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
    // PM-A2: fill inferred type/stage/origin for fields the markdown omitted.
    for it in &mut items {
        infer_item_fields(it);
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

    let cycles = find_cycles(&items);
    // Untriaged: ended at the default `Proposed` stage because the status
    // gave no recognizable signal (an explicit stage or a CANDIDATE status
    // is NOT untriaged).
    let untriaged: Vec<String> = items
        .iter()
        .filter(|it| status_to_stage(&it.status).is_none() && it.stage == "Proposed")
        .map(|it| it.id.clone())
        .collect();

    BacklogReport {
        items,
        files_scanned: backlog_files.len(),
        dangling_deps: dangling,
        cycles,
        untriaged,
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
    let cycle_count = report.cycles.len();
    let untriaged_count = report.untriaged.len();

    // Health score: penalise broken structure — cycles (worst: deadlock a
    // whole subgraph) > dangling deps > untriaged > a mild unknown-status
    // drag. Advisory; floor 0.
    let unknown_ratio = if total == 0 { 0.0 } else { unknown_status as f64 / total as f64 };
    let unknown_penalty = (unknown_ratio * 20.0).min(20.0);
    let dangling_penalty = ((dangling as f64) * 5.0).min(40.0);
    let cycle_penalty = ((cycle_count as f64) * 10.0).min(40.0);
    let untriaged_penalty = ((untriaged_count as f64) * 2.0).min(15.0);
    let score: i32 = (100.0 - unknown_penalty - dangling_penalty - cycle_penalty - untriaged_penalty)
        .clamp(0.0, 100.0) as i32;

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
    if cycle_count > 0 {
        findings.push(Finding {
            name: "dependency_cycles".into(),
            status: "warn".into(),
            points: -(cycle_penalty as i32),
            detail: Some(format!(
                "{cycle_count} dependency cycle(s) — first: [{}]",
                report.cycles.first().map(|c| c.join(" -> ")).unwrap_or_default()
            )),
        });
    }
    if untriaged_count > 0 {
        findings.push(Finding {
            name: "untriaged_items".into(),
            status: "info".into(),
            points: -(untriaged_penalty as i32),
            detail: Some(format!(
                "{untriaged_count} item(s) have no recognizable stage — first 5: [{}]",
                report.untriaged.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
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

    // PM-A4: derived epic/container rollup. The child `epic:` ref is the source
    // of truth; this roster is computed (D-EPIC §C). value/priority are looked
    // up from the epic's own item if one exists. value-complete ⇔ all Must
    // members done (D-EPIC §D).
    let is_done = |s: &str| s == "Human-verified-done" || s == "Agent-done";
    let item_by_id: BTreeMap<&str, &BacklogItem> =
        report.items.iter().map(|i| (i.id.as_str(), i)).collect();
    let mut epic_members: BTreeMap<&str, Vec<&BacklogItem>> = BTreeMap::new();
    for it in &report.items {
        if !it.epic.is_empty() {
            epic_members.entry(it.epic.as_str()).or_default().push(it);
        }
    }
    let epics: Vec<Value> = epic_members
        .iter()
        .map(|(epic_id, members)| {
            let total = members.len();
            let done = members.iter().filter(|m| is_done(&m.stage)).count();
            let blocked = members.iter().filter(|m| m.stage == "Blocked").count();
            let musts: Vec<_> =
                members.iter().filter(|m| m.moscow.eq_ignore_ascii_case("Must")).collect();
            let must_done = musts.iter().filter(|m| is_done(&m.stage)).count();
            let derived = if total > 0 && done == total {
                "complete"
            } else if blocked > 0 {
                "blocked"
            } else {
                "active"
            };
            let (value, priority) = item_by_id
                .get(epic_id)
                .map(|e| (e.value.clone(), e.priority.clone()))
                .unwrap_or_default();
            serde_json::json!({
                "id": epic_id,
                "members": members.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
                "member_count": total,
                "done_count": done,
                "derived_status": derived,
                "value_complete": !musts.is_empty() && must_done == musts.len(),
                "value": value,
                "priority": priority,
            })
        })
        .collect();

    // PM-A5: the lint gradient (required = f(type, stage), discovery-05 §C) +
    // undeclared epic value. Advisory (enrichment guidance), not a health hit.
    let mut lint: Vec<Value> = Vec::new();
    let ready_or_beyond = |s: &str| {
        matches!(s, "Ready" | "In-progress" | "Agent-done" | "Human-verified-done")
    };
    for it in &report.items {
        let pipeline_type = matches!(it.item_type.as_str(), "story" | "bug" | "discovery");
        if pipeline_type && ready_or_beyond(&it.stage) {
            if it.moscow.is_empty() {
                lint.push(serde_json::json!({"item": it.id, "rule": "missing_moscow", "stage": it.stage}));
            }
            if it.epic.is_empty() {
                lint.push(serde_json::json!({"item": it.id, "rule": "missing_epic", "stage": it.stage}));
            }
        }
        if pipeline_type
            && matches!(it.stage.as_str(), "Agent-done" | "Human-verified-done")
            && it.evidence_run_id.is_empty()
        {
            lint.push(serde_json::json!({"item": it.id, "rule": "missing_evidence", "stage": it.stage}));
        }
    }
    for epic_id in epic_members.keys() {
        let has_value = item_by_id.get(epic_id).map(|e| !e.value.is_empty()).unwrap_or(false);
        if !has_value {
            lint.push(serde_json::json!({"item": epic_id, "rule": "undeclared_epic_value"}));
        }
    }
    let lint_count = lint.len();
    if lint_count > 0 {
        findings.push(Finding {
            name: "lint".into(),
            status: "info".into(),
            points: 0,
            detail: Some(format!("{lint_count} schema-lint enrichment hint(s) (missing moscow/epic/evidence, undeclared epic value)")),
        });
    }

    let items_json = serde_json::to_value(&report.items).unwrap_or(Value::Array(vec![]));
    // The structured defect surface the broker maps to tier-3 groom dispatches
    // and the pane renders as "needs attention" (D-LIVENESS B4).
    let defects = serde_json::json!({
        "dangling": report
            .dangling_deps
            .iter()
            .map(|(item, dep)| serde_json::json!({ "item": item, "dep": dep }))
            .collect::<Vec<_>>(),
        "cycles": report.cycles,
        "untriaged": report.untriaged,
    });
    let extras: Vec<(&str, Value)> = vec![
        ("item_count", Value::Number(total.into())),
        ("heading_items", Value::Number(heading_items.into())),
        ("table_items", Value::Number(table_items.into())),
        ("unknown_status_count", Value::Number(unknown_status.into())),
        ("dangling_dep_count", Value::Number(dangling.into())),
        ("cycle_count", Value::Number(cycle_count.into())),
        ("untriaged_count", Value::Number(untriaged_count.into())),
        ("files_scanned", Value::Number(report.files_scanned.into())),
        ("lint_count", Value::Number(lint_count.into())),
        // The structured defect surface (D-LIVENESS B4).
        ("defects", defects),
        // Derived epic/container rollup (D-EPIC) + the lint gradient (D-SCHEMA).
        ("epics", Value::Array(epics)),
        ("lint", Value::Array(lint)),
        // The live symbol model the IDE caches (the broker + pane read this).
        ("items", items_json),
    ];

    build_cmdb("check-backlog", score.clamp(0, 100) as u8, findings, Some(extras), None)
}

/// The PM broker, CLI face (IDE-BACKLOG-PM): the deterministic tiered
/// next-ready dispatch over the parsed symbol model — the *same* ranking the
/// IDE's `backlog.next_ready` runs, minus the runtime claim/lease + freshness
/// overlay (IDE-local state). Lets any agent session pull work without the IDE;
/// the markdown `stage` field is the coordination source of truth. Tiers:
/// implement → groom → capture → surfaced-idle; never invents filler.
pub fn next_ready(report: &BacklogReport) -> Value {
    use std::collections::{HashMap, HashSet};
    let items = &report.items;
    let ids: HashSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
    let is_done = |it: &BacklogItem| matches!(it.stage.as_str(), "Agent-done" | "Human-verified-done");
    let is_deferred = |it: &BacklogItem| matches!(it.stage.as_str(), "Blocked" | "Deferred");
    let done_ids: HashSet<&str> = items.iter().filter(|i| is_done(i)).map(|i| i.id.as_str()).collect();
    let dangling_items: HashSet<&str> = report.dangling_deps.iter().map(|(i, _)| i.as_str()).collect();

    // epic id -> priority rank (High=0/Medium=1/Low=2), from the epic items.
    let mut epic_prio: HashMap<&str, u8> = HashMap::new();
    for it in items {
        if !it.priority.is_empty() {
            let r = match it.priority.to_ascii_lowercase().trim() {
                "high" => 0,
                "medium" | "med" => 1,
                "low" => 2,
                _ => 3,
            };
            epic_prio.insert(it.id.as_str(), r);
        }
    }
    let moscow_rank = |it: &BacklogItem| -> u8 {
        match it.moscow.to_ascii_lowercase().trim() {
            "must" => 0,
            "should" => 1,
            "could" => 2,
            "won't" | "wont" | "will not" => 4,
            _ => 3,
        }
    };
    let pinned = |it: &BacklogItem| {
        let t = format!("{} {}", it.title, it.status);
        t.contains('\u{1F4CC}') || t.contains('\u{1F4CD}')
            || t.to_uppercase().contains("[PIN]")
            || t.to_uppercase().contains("PINNED")
    };
    let override_lane = |it: &BacklogItem| {
        let t = format!("{} {}", it.title, it.status).to_lowercase();
        t.contains("security") || t.contains("critical") || t.contains("vuln") || t.contains("cve")
    };
    let steering = |it: &BacklogItem| {
        let t = format!("{} {}", it.title, it.status).to_lowercase();
        (it.origin == "human" && it.item_type == "request")
            || t.contains("rework")
            || t.contains("needsredo")
            || t.contains("needs-redo")
    };

    type Key = (u8, u8, u8, u8, usize);
    let mut candidates: Vec<(Key, &BacklogItem)> = Vec::new();
    let (mut blocked, mut deferred, mut done_count, mut in_progress) = (0usize, 0usize, 0usize, 0usize);
    for (idx, it) in items.iter().enumerate() {
        if is_done(it) {
            done_count += 1;
            continue;
        }
        if it.stage == "In-progress" {
            in_progress += 1; // being worked (markdown coordination) — not re-served
            continue;
        }
        if is_deferred(it) {
            deferred += 1;
            continue;
        }
        let mr = moscow_rank(it);
        if mr == 4 {
            deferred += 1; // Won't
            continue;
        }
        if dangling_items.contains(it.id.as_str()) {
            continue; // held back + tier-3 groom target
        }
        let unmet = it.deps.iter().any(|d| ids.contains(d.as_str()) && !done_ids.contains(d.as_str()));
        if unmet {
            blocked += 1;
            continue;
        }
        let lane = if pinned(it) { 0 } else if override_lane(it) { 1 } else if steering(it) { 2 } else { 3 };
        let eprio = epic_prio.get(it.epic.as_str()).copied().unwrap_or(3);
        let dr = if it.stage == "Ready"
            || it.status.to_uppercase().contains("READY")
            || it.status.to_uppercase().contains("NEXT")
        {
            0
        } else {
            1
        };
        candidates.push(((lane, eprio, mr, dr, idx), it));
    }
    candidates.sort_by(|a, b| a.0.cmp(&b.0));

    if let Some((key, it)) = candidates.first() {
        let lane = key.0;
        return serde_json::json!({
            "tier": "implement", "ready": true,
            "item": { "id": it.id, "title": it.title, "type": it.item_type, "stage": it.stage,
                      "moscow": it.moscow, "epic": it.epic, "source_file": it.source_file, "line": it.line },
            "rationale": { "lane": lane, "pinned": lane == 0, "override_lane": lane == 1,
                           "steering": lane == 2, "epic_priority_rank": key.1, "moscow_rank": key.2 },
            "ready_count": candidates.len(),
            "explanation": format!("Next ready: {} — {}. Deps clear; not done/deferred.", it.id, it.title),
        });
    }

    // Tier 3 — groom the top board defect (deterministic groom-id).
    if let Some((item, dep)) = report.dangling_deps.first() {
        return serde_json::json!({ "tier": "groom", "ready": false,
            "groom": { "groom_id": format!("GROOM-dangling-{item}-{dep}"), "kind": "dangling_dep", "item": item, "dep": dep,
                       "action": format!("Fix the phantom dependency `{dep}` on `{item}` — resolve or remove the ref.") },
            "explanation": format!("No implementable item — groom: fix dangling dep {dep} on {item}. The broker never invents filler.") });
    }
    if let Some(cycle) = report.cycles.first() {
        let mut m = cycle.clone();
        m.sort();
        return serde_json::json!({ "tier": "groom", "ready": false,
            "groom": { "groom_id": format!("GROOM-cycle-{}", m.join("-")), "kind": "cycle", "members": m,
                       "action": "Break the dependency cycle — one member must drop its blocking ref." },
            "explanation": "No implementable item — groom: break a dependency cycle." });
    }
    if let Some(uid) = report.untriaged.first() {
        return serde_json::json!({ "tier": "groom", "ready": false,
            "groom": { "groom_id": format!("GROOM-triage-{uid}"), "kind": "untriaged", "item": uid,
                       "action": format!("Triage `{uid}`: assign a type + stage so the broker can rank it.") },
            "explanation": format!("No implementable item — groom: triage {uid}.") });
    }

    // Tier 4 — capture (board fully done; demand-driven refill).
    if blocked + deferred + in_progress == 0 && done_count > 0 {
        return serde_json::json!({ "tier": "capture", "ready": false,
            "capture": { "scope": "recent-changes",
                         "action": "Scan recent changes / code / vision for uncaptured work and propose anchored items (deduped; at Proposed)." },
            "explanation": format!("Board fully done ({done_count} items). Capture: scan for uncaptured work to refill (demand-driven). The broker never invents filler.") });
    }

    // Tier 5 — surfaced idle.
    serde_json::json!({ "tier": "idle", "ready": false,
        "blocked_count": blocked, "deferred_count": deferred, "in_progress": in_progress, "done_count": done_count,
        "explanation": format!("Idle: {blocked} blocked, {deferred} deferred, {in_progress} in-progress, {done_count} done, no groomable defect. The broker never invents filler.") })
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
    fn table_status_in_middle_column_maps_to_done() {
        // ROADMAP stage tables: Status in col 3, Effort last.
        let md = "\
| ID | Subject | Status | Effort |
|---|---|---|---|
| S5-TP-1 | Starter Kit | **Complete** | XL |
| S6-DB-2 | A2A envelope | next — keystone | M |
";
        let items = parse_file(md, "ROADMAP.md");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "S5-TP-1");
        assert_eq!(items[0].stage, "Human-verified-done", "Complete in a middle col → done");
        assert_eq!(items[1].stage, "Ready", "'next' in a middle col → ready");
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
    fn inference_fills_type_stage_origin_omits_optional_fields() {
        // A legacy item gets inferred type/stage/origin (always present); the
        // genuinely-optional fields stay omitted (skip-serialized).
        let items = parse_file("### B-01: legacy thing — CANDIDATE\n", "BACKLOG.md");
        let it = &items[0];
        assert_eq!(it.item_type, "story");
        assert_eq!(it.stage, "Proposed"); // CANDIDATE -> Proposed
        assert_eq!(it.origin, "agent");
        let v = serde_json::to_value(it).unwrap();
        assert_eq!(v["type"], "story");
        assert_eq!(v["stage"], "Proposed");
        assert!(v.get("moscow").is_none(), "absent moscow must be omitted");
        assert!(v.get("wake").is_none());
        assert!(v.get("evidence_run_id").is_none());
    }

    #[test]
    fn infers_type_from_narrative() {
        assert_eq!(parse_file("### X1: login crash on resume — CANDIDATE\n", "b")[0].item_type, "bug");
        assert_eq!(parse_file("### X2: research the broker model — CANDIDATE\n", "b")[0].item_type, "discovery");
        assert_eq!(parse_file("### X3: add a backlog pane — READY\n", "b")[0].item_type, "story");
    }

    #[test]
    fn maps_status_to_stage() {
        assert_eq!(status_to_stage("SHIPPED (2026-04-22)"), Some("Human-verified-done"));
        assert_eq!(status_to_stage("discovery ✅; gates B0"), Some("Human-verified-done")); // ✅ wins
        assert_eq!(status_to_stage("discovery; gates B0"), Some("Discovery"));
        assert_eq!(status_to_stage("**next** — keystone"), Some("Ready"));
        assert_eq!(status_to_stage("CANDIDATE"), Some("Proposed"));
        assert_eq!(status_to_stage("PARKED 2026"), Some("Deferred"));
        assert_eq!(status_to_stage("dep B0 + D2"), None); // a deps cell — no stage signal
    }

    #[test]
    fn detects_dependency_cycle() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "BACKLOG.md",
            "### A1: a — READY\nbody · dep B1\n### B1: b — READY\nbody · dep A1\n");
        let report = parse_backlog(dir.path());
        assert_eq!(report.cycles.len(), 1, "cycles: {:?}", report.cycles);
        let mut members = report.cycles[0].clone();
        members.sort();
        assert_eq!(members, vec!["A1".to_string(), "B1".to_string()]);
        assert!(report.dangling_deps.is_empty(), "both deps resolve — no dangling");
    }

    #[test]
    fn flags_untriaged_items() {
        let dir = TempDir::new().unwrap();
        // C1 has an unrecognizable status (no stage signal); C2 is CANDIDATE (triaged).
        write(dir.path(), "BACKLOG.md", "### C1: mystery — foobar\n### C2: known — CANDIDATE\n");
        let report = parse_backlog(dir.path());
        assert_eq!(report.untriaged, vec!["C1".to_string()]);
    }

    #[tokio::test]
    async fn analyze_backlog_emits_defects() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "BACKLOG.md",
            "### A1: a — READY\n· dep B1\n### B1: b — READY\n· dep A1\n### C1: c — CANDIDATE\n· dep ZZ9\n");
        let cmdb = analyze_backlog(&dir.path().to_string_lossy()).await;
        assert_eq!(cmdb["cycle_count"], 1);
        assert_eq!(cmdb["dangling_dep_count"], 1); // C1 -> ZZ9
        assert_eq!(cmdb["defects"]["cycles"].as_array().unwrap().len(), 1);
        assert_eq!(cmdb["defects"]["dangling"][0]["dep"], "ZZ9");
    }

    #[test]
    fn infers_epic_from_absorbed_into() {
        let items = parse_file("### B-01: thing — ABSORBED into S10-DOMAIN\n", "b");
        assert_eq!(items[0].epic, "S10-DOMAIN");
    }

    #[tokio::test]
    async fn rolls_up_epics_and_value_complete() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "BACKLOG.md", "\
### EP1: the epic — active
**Value:** deliver the thing
**Priority:** High

### S1: must story — SHIPPED
**Epic:** EP1
**MoSCoW:** Must

### S2: could story — CANDIDATE
**Epic:** EP1
**MoSCoW:** Could
");
        let cmdb = analyze_backlog(&dir.path().to_string_lossy()).await;
        let epics = cmdb["epics"].as_array().unwrap();
        assert_eq!(epics.len(), 1);
        let ep = &epics[0];
        assert_eq!(ep["id"], "EP1");
        assert_eq!(ep["member_count"], 2);
        assert_eq!(ep["value"], "deliver the thing");
        assert_eq!(ep["priority"], "High");
        // The single Must (S1) is SHIPPED → value-complete, even though the
        // Could story (S2) isn't done.
        assert_eq!(ep["value_complete"], true);
    }

    #[tokio::test]
    async fn lints_missing_required_fields_at_ready() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "BACKLOG.md", "### S1: ready story — READY\n");
        let cmdb = analyze_backlog(&dir.path().to_string_lossy()).await;
        let lint = cmdb["lint"].as_array().unwrap();
        assert!(lint.iter().any(|l| l["item"] == "S1" && l["rule"] == "missing_moscow"));
        assert!(lint.iter().any(|l| l["item"] == "S1" && l["rule"] == "missing_epic"));
    }

    #[test]
    fn next_ready_implements_top_ranked_by_moscow() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "BACKLOG.md", "\
### R1: do the thing — READY
**MoSCoW:** Could
### R2: build the feature — READY
**MoSCoW:** Must
");
        let report = parse_backlog(dir.path());
        let d = next_ready(&report);
        assert_eq!(d["tier"], "implement");
        assert_eq!(d["item"]["id"], "R2", "Must outranks Could");
    }

    #[test]
    fn next_ready_grooms_when_nothing_implementable() {
        let dir = TempDir::new().unwrap();
        // The only item is held back by a dangling dep → tier-3 groom.
        write(dir.path(), "BACKLOG.md", "### X1: thing — READY\n· dep NOPE9\n");
        let report = parse_backlog(dir.path());
        let d = next_ready(&report);
        assert_eq!(d["tier"], "groom");
        assert_eq!(d["groom"]["kind"], "dangling_dep");
        assert_eq!(d["groom"]["dep"], "NOPE9");
    }

    #[test]
    fn grooming_sweep_defers_plan_when_and_horizon() {
        // "Plan when: <condition>" → Deferred + wake (the condition).
        let d = parse_file("### B1: thing — CANDIDATE\n**Plan when:** we have a gpt-proxy\n", "b");
        assert_eq!(d[0].stage, "Deferred");
        assert!(d[0].wake.contains("gpt-proxy"), "wake: {}", d[0].wake);
        // "Plan when: anytime" → NOT deferred (plannable now).
        let a = parse_file("### B2: x — READY\n**Plan when:** anytime — no preconditions\n", "b");
        assert_eq!(a[0].stage, "Ready");
        // A blank "Plan when:" still defers (presence = to-be-planned).
        let e = parse_file("### B3: x — CANDIDATE\n**Plan when:**\n", "b");
        assert_eq!(e[0].stage, "Deferred");
        // An explicit `**Stage:**` wins over plan-when.
        let x = parse_file("### B4: x — CANDIDATE\n**Stage:** Ready\n**Plan when:** someday\n", "b");
        assert_eq!(x[0].stage, "Ready");
        // Horizon / successor / no-current-plan status tags → Deferred (over CANDIDATE).
        assert_eq!(parse_file("### B5: x — CANDIDATE (v5.5 horizon)\n", "b")[0].stage, "Deferred");
        assert_eq!(parse_file("### B6: x — CANDIDATE (v5.5 successor)\n", "b")[0].stage, "Deferred");
        assert_eq!(parse_file("### B7: x — CANDIDATE (no current plan)\n", "b")[0].stage, "Deferred");
        // A plain CANDIDATE with no deferral signal stays a candidate.
        assert_eq!(parse_file("### B8: x — CANDIDATE\n", "b")[0].stage, "Proposed");
    }

    #[test]
    fn next_ready_idle_when_all_done() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "BACKLOG.md", "### S1: a — SHIPPED\n### S2: b — COMPLETE\n");
        let report = parse_backlog(dir.path());
        let d = next_ready(&report);
        // Everything done + nothing open → capture (demand-driven refill).
        assert_eq!(d["tier"], "capture");
    }
}
