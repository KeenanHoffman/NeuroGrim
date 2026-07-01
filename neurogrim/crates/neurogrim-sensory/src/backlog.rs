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

    /// INTERNAL-ONLY composite-key scope: the nearest ancestor `##`
    /// (top-level / epic) heading the item lives under, or `""` when the item
    /// is above any `##`. Not serialized — humans read the bare `id`; this is
    /// used purely for scope-first dep resolution + graph identity so that the
    /// same bare id appearing in different files/sections becomes DISTINCT
    /// nodes (kills the phantom cycle + merge-inflated dangling).
    #[serde(skip)]
    pub section: String,
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
    // Reject range notation (`S10-DP-1..3`) — the ONE safe parser tightening.
    // No real id contains `..`; ranges are enumerations, not dependencies.
    if t.contains("..") {
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

/// All id-looking tokens in `text` (direction-agnostic — used where the caller
/// already knows every id is a reference, e.g. `ABSORBED into <EPIC>`).
fn extract_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')) {
        if looks_like_id(raw) && !out.iter().any(|e| e == raw) {
            out.push(raw.to_string());
        }
    }
    out
}

/// Whether a clause names a REVERSE-direction relationship — `X gates Y` /
/// `X blocks Y` mean X is a PREREQUISITE of Y (the true edge is `Y → X`), so
/// ids in such a clause are NOT forward deps of the current item. Word-matched
/// (so `delegates`/`blocked`/`blocker` don't false-trigger).
fn clause_is_reverse(clause: &str) -> bool {
    clause.split(|c: char| !c.is_ascii_alphabetic()).any(|w| {
        let l = w.to_ascii_lowercase();
        l == "gates" || l == "gate" || l == "blocks"
    })
}

/// FORWARD dep ids in `text` (a status/deps cell, heading tail, or prose line
/// like `dep B0 + D2; gates B6`). Splits into clauses on `;`/newlines and drops
/// any clause that expresses a reverse relationship (`gates`/`blocks`) — those
/// ids belong to the OTHER item. Everything else (`dep`/`depends`/`requires`/
/// `after`/`blocked by`) is a forward dep of this item.
fn extract_dep_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for clause in text.split(|c: char| c == ';' || c == '\n') {
        if clause_is_reverse(clause) {
            continue;
        }
        for d in extract_ids(clause) {
            if !out.iter().any(|e| e == &d) {
                out.push(d);
            }
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

/// Resolve each item's bare dep tokens against the COMPOSITE-key node set,
/// scope-first, returning `(adjacency, dangling)` over composite nodes.
///
/// A dep token `T` on an item at `(file, section)` resolves to:
/// 1. the node at `(file, section, T)` (same file + same `##` section), else
/// 2. a node at `(file, T)` (same file, any section — first by order), else
/// 3. the GLOBALLY-UNIQUE node with id `T`, else
/// 4. it is dangling.
///
/// This is the core fix: the same bare id appearing in multiple files/sections
/// stays DISTINCT (no bare-string merge), so a dep never fabricates a
/// cross-file edge onto an arbitrary same-named node.
fn resolve_graph(items: &[BacklogItem]) -> (Vec<Vec<usize>>, Vec<(String, String)>) {
    use std::collections::HashMap;
    let mut by_fsi: HashMap<(&str, &str, &str), usize> = HashMap::new();
    let mut by_fi: HashMap<(&str, &str), Vec<usize>> = HashMap::new();
    let mut by_i: HashMap<&str, Vec<usize>> = HashMap::new();
    for (idx, it) in items.iter().enumerate() {
        by_fsi
            .entry((it.source_file.as_str(), it.section.as_str(), it.id.as_str()))
            .or_insert(idx);
        by_fi.entry((it.source_file.as_str(), it.id.as_str())).or_default().push(idx);
        by_i.entry(it.id.as_str()).or_default().push(idx);
    }
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); items.len()];
    let mut dangling: Vec<(String, String)> = Vec::new();
    for (idx, it) in items.iter().enumerate() {
        for d in &it.deps {
            let resolved = by_fsi
                .get(&(it.source_file.as_str(), it.section.as_str(), d.as_str()))
                .copied()
                .or_else(|| {
                    by_fi.get(&(it.source_file.as_str(), d.as_str())).and_then(|v| v.first().copied())
                })
                .or_else(|| match by_i.get(d.as_str()) {
                    Some(v) if v.len() == 1 => Some(v[0]),
                    _ => None,
                });
            match resolved {
                Some(j) if j != idx => {
                    if !adjacency[idx].contains(&j) {
                        adjacency[idx].push(j);
                    }
                }
                Some(_) => {} // self-reference — ignore
                None => dangling.push((it.id.clone(), d.clone())),
            }
        }
    }
    (adjacency, dangling)
}

/// Detect dependency cycles over the COMPOSITE-key dep graph (B-LIVE-3) via
/// colored DFS on the prebuilt `adj` (from `resolve_graph`). Each cycle is the
/// ordered member ids; deduped by member set.
fn find_cycles(items: &[BacklogItem], adj: &[Vec<usize>]) -> Vec<Vec<String>> {
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
            dfs(i, adj, &mut color, &mut stack, items, &mut cycles);
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
    let lower = rel.to_ascii_lowercase();
    let base = Path::new(&lower).file_name().and_then(|n| n.to_str()).unwrap_or("");
    if base == "backlog.md" || base == "roadmap.md" || base == "execution.md" {
        return true;
    }
    // Any `*-backlog.md` (e.g. `broker-framework-backlog.md`).
    if base.ends_with("-backlog.md") {
        return true;
    }
    // Any markdown under a `roadmap/epics/` path (the epic docs the sensor was
    // previously blind to — ~118 real work items).
    if base.ends_with(".md") && lower.contains("roadmap/epics/") {
        return true;
    }
    false
}

/// Locate the FORWARD-dependency column of a table by header text. Deps are
/// read ONLY from a `Depends on` / `Deps` / `Blocked by` / `Requires`-headed
/// column — never from `Stage` / `Effort` / `Layer` columns (so the broker
/// master table's `S0-T`-class Stage cells don't become phantom deps), and
/// never from a reverse `gates` / `blocks` column (those express the inverse
/// edge). `None` = no forward-dep column.
fn find_deps_col(header: &[String]) -> Option<usize> {
    header.iter().position(|h| {
        let l = h.to_ascii_lowercase();
        l.contains("dep") || l.contains("blocked") || l.contains("requir") || l.contains("after")
    })
}

/// Upsert a parsed item into `items`, deduping FIRST-WINS on the composite
/// `(section, id)` key within the current file. On a duplicate the later
/// occurrence is NOT pushed (counted once); its deps are merged into the first
/// occurrence (union). Returns the index of the surviving node (so a heading's
/// body lines attach to it).
fn upsert_item(
    items: &mut Vec<BacklogItem>,
    seen: &mut std::collections::HashMap<(String, String), usize>,
    new_item: BacklogItem,
) -> usize {
    let key = (new_item.section.clone(), new_item.id.clone());
    if let Some(&existing) = seen.get(&key) {
        for d in &new_item.deps {
            if d != &items[existing].id && !items[existing].deps.contains(d) {
                items[existing].deps.push(d.clone());
            }
        }
        existing
    } else {
        items.push(new_item);
        let idx = items.len() - 1;
        seen.insert(key, idx);
        idx
    }
}

/// Parse one markdown file into items. Line-based with a fenced-code
/// guard (so `### B0` inside a ``` fence isn't an item).
fn parse_file(text: &str, rel: &str) -> Vec<BacklogItem> {
    use std::collections::HashMap;
    let mut items: Vec<BacklogItem> = Vec::new();
    let mut in_fence = false;
    // Index of the heading item whose body we're currently inside, so
    // `**Dependencies:**`/`dep`/`gates` body lines attach to it (NeuroGrim
    // `BACKLOG.md` declares deps in prose under the heading).
    let mut cur: Option<usize> = None;
    // The nearest ancestor `##` (top-level / epic) heading — the composite-key
    // scope. Reset on `#` (H1), set on `##`, untouched by `###`/`####`.
    let mut current_section = String::new();
    // Per-`(section, id)` dedup map (FIRST-WINS) within this file.
    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    // Header + dep-column of the table block we're currently inside; cleared
    // whenever we leave the table (any non-`|` line).
    let mut table_header: Option<Vec<String>> = None;
    let mut deps_col: Option<usize> = None;
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
        // Any non-table line ends the current table block (markdown tables are
        // contiguous), so header + dep-column scope stays per-table.
        if !trimmed.starts_with('|') {
            table_header = None;
            deps_col = None;
        }

        // Heading items: ## / ### / #### with a leading id.
        if trimmed.starts_with('#') {
            cur = None;
            let hashes = trimmed.chars().take_while(|c| *c == '#').count();
            let head = trimmed[hashes..].trim();
            // Track the `##` section scope for composite keys.
            if hashes == 1 {
                current_section = String::new();
            } else if hashes == 2 {
                current_section = head.to_string();
            }
            if (2..=4).contains(&hashes) {
                if let Some((id, rest)) = leading_id(head) {
                    let (title, status) = split_title_status(&rest);
                    let mut deps = extract_dep_ids(&status);
                    // Also scan the heading tail (execution batch headings
                    // carry `· dep D1` after the title). Direction-aware so a
                    // `gates`/`blocks` tail is not read as a forward dep.
                    for d in extract_dep_ids(&rest) {
                        if d != id && !deps.contains(&d) {
                            deps.push(d);
                        }
                    }
                    deps.retain(|d| d != &id);
                    let item = BacklogItem {
                        id,
                        title: title.trim_matches('`').trim().to_string(),
                        status,
                        deps,
                        source_file: rel.to_string(),
                        line: i + 1,
                        format: "heading",
                        section: current_section.clone(),
                        ..Default::default()
                    };
                    cur = Some(upsert_item(&mut items, &mut seen, item));
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
            // First row of this table block = header. Record it + locate the
            // dep column. (Header cells like `ID` aren't ids, so this row won't
            // also parse as an item.)
            if table_header.is_none() {
                deps_col = find_deps_col(&cells);
                table_header = Some(cells.clone());
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
                // Deps: ONLY the `Depends on`/`Deps`-headed column (never the
                // Stage/Effort/Layer columns). No dep column → no table deps
                // (deps come from `**Dependencies:**` body lines on headings).
                let mut deps: Vec<String> = Vec::new();
                if let Some(dc) = deps_col {
                    if let Some(cell) = cells.get(dc) {
                        // Direction-aware: `gates`/`blocks` ids in the cell are
                        // REVERSE (they belong to the other item), not forward
                        // deps — otherwise `D2 … gates B2` + `B2 … dep D2`
                        // fabricates a D2↔B2 cycle.
                        for d in extract_dep_ids(cell) {
                            if d != id && !deps.contains(&d) {
                                deps.push(d);
                            }
                        }
                    }
                }
                let item = BacklogItem {
                    id,
                    title: title.trim_matches('`').trim().to_string(),
                    status,
                    deps,
                    source_file: rel.to_string(),
                    line: i + 1,
                    format: "table",
                    section: current_section.clone(),
                    ..Default::default()
                };
                upsert_item(&mut items, &mut seen, item);
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
            // Declared FORWARD deps (`**Dependencies:**` / `dep` / `blocked
            // by`). `gates`/`blocks` are REVERSE and deliberately NOT triggers;
            // even when a line also carries a forward dep, `extract_dep_ids`
            // drops the reverse clause's ids.
            let low = line.to_lowercase();
            if low.contains("depend") || low.contains(" dep ") || low.contains("· dep")
                || low.contains("blocked")
            {
                let own = items[idx].id.clone();
                for d in extract_dep_ids(line) {
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

    // Composite-key graph: resolve every dep token scope-first over the
    // (file, section, id) node set, so duplicate bare ids don't merge.
    let (adjacency, dangling) = resolve_graph(&items);
    let cycles = find_cycles(&items, &adjacency);
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

/// One enrichment-lint hint (PM-A5): a Ready+ pipeline item missing a required
/// field, or an epic with no declared value. `stage` is empty for the
/// `undeclared_epic_value` rule (it's epic-level, not item-lifecycle).
struct LintHint {
    item: String,
    rule: String,
    stage: String,
}

/// The lint gradient (required = f(type, stage)) + undeclared epic value —
/// the single computation shared by `analyze_backlog` (rendered into the CMDB
/// `lint` extra) and `groom_queue` (rendered into GroomItems). Advisory.
fn compute_lint(report: &BacklogReport) -> Vec<LintHint> {
    let mut lint: Vec<LintHint> = Vec::new();
    let ready_or_beyond =
        |s: &str| matches!(s, "Ready" | "In-progress" | "Agent-done" | "Human-verified-done");
    let item_by_id: BTreeMap<&str, &BacklogItem> =
        report.items.iter().map(|i| (i.id.as_str(), i)).collect();
    let mut epic_members: BTreeMap<&str, Vec<&BacklogItem>> = BTreeMap::new();
    for it in &report.items {
        if !it.epic.is_empty() {
            epic_members.entry(it.epic.as_str()).or_default().push(it);
        }
    }
    for it in &report.items {
        let pipeline_type = matches!(it.item_type.as_str(), "story" | "bug" | "discovery");
        if pipeline_type && ready_or_beyond(&it.stage) {
            if it.moscow.is_empty() {
                lint.push(LintHint { item: it.id.clone(), rule: "missing_moscow".into(), stage: it.stage.clone() });
            }
            if it.epic.is_empty() {
                lint.push(LintHint { item: it.id.clone(), rule: "missing_epic".into(), stage: it.stage.clone() });
            }
        }
        if pipeline_type
            && matches!(it.stage.as_str(), "Agent-done" | "Human-verified-done")
            && it.evidence_run_id.is_empty()
        {
            lint.push(LintHint { item: it.id.clone(), rule: "missing_evidence".into(), stage: it.stage.clone() });
        }
    }
    for epic_id in epic_members.keys() {
        let has_value = item_by_id.get(epic_id).map(|e| !e.value.is_empty()).unwrap_or(false);
        if !has_value {
            lint.push(LintHint { item: epic_id.to_string(), rule: "undeclared_epic_value".into(), stage: String::new() });
        }
    }
    lint
}

/// A single groomable defect — one entry in the prioritized grooming queue the
/// PM dashboard renders + the broker drains. `groom_id` is deterministic and
/// reuses the exact `next_ready` groom formats so the queue and the broker's
/// single-groom dispatch share one identity space.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GroomItem {
    pub groom_id: String,
    /// `cycle` | `dangling` | `untriaged` | `missing_moscow` | `missing_epic`
    /// | `missing_evidence` | `undeclared_epic_value`.
    pub kind: String,
    pub target_id: String,
    pub source_file: String,
    pub line: usize,
    pub action: String,
    pub severity: u8,
    pub points: i32,
}

/// The FULL prioritized grooming queue — the single ranking authority the
/// PM dashboard's Grooming view consumes and `next_ready`'s groom tier draws
/// its top entry from. Aggregates the existing `defects` (cycles / dangling /
/// untriaged) + `lint` surfaces. Order: cycle → dangling → untriaged → lint
/// (severity desc). Deterministic.
pub fn groom_queue(report: &BacklogReport) -> Vec<GroomItem> {
    use std::collections::HashMap;
    // id -> (source_file, line) of its FIRST occurrence (for one-click-to-item).
    let mut loc: HashMap<&str, (String, usize)> = HashMap::new();
    for it in &report.items {
        loc.entry(it.id.as_str()).or_insert((it.source_file.clone(), it.line));
    }
    let at = |id: &str| loc.get(id).cloned().unwrap_or_default();

    let mut q: Vec<GroomItem> = Vec::new();
    // Tier 1 — cycles (severity 4): deadlock a whole subgraph.
    for cycle in &report.cycles {
        let mut m = cycle.clone();
        m.sort();
        let target = m.first().cloned().unwrap_or_default();
        let (source_file, line) = at(&target);
        q.push(GroomItem {
            groom_id: format!("GROOM-cycle-{}", m.join("-")),
            kind: "cycle".into(),
            target_id: target,
            source_file,
            line,
            action: "Break the dependency cycle — one member must drop its blocking ref.".into(),
            severity: 4,
            points: 10,
        });
    }
    // Tier 2 — dangling deps (severity 3).
    for (item, dep) in &report.dangling_deps {
        let (source_file, line) = at(item);
        q.push(GroomItem {
            groom_id: format!("GROOM-dangling-{item}-{dep}"),
            kind: "dangling".into(),
            target_id: item.clone(),
            source_file,
            line,
            action: format!("Fix the phantom dependency `{dep}` on `{item}` — resolve or remove the ref."),
            severity: 3,
            points: 5,
        });
    }
    // Tier 3 — untriaged (severity 2).
    for uid in &report.untriaged {
        let (source_file, line) = at(uid);
        q.push(GroomItem {
            groom_id: format!("GROOM-triage-{uid}"),
            kind: "untriaged".into(),
            target_id: uid.clone(),
            source_file,
            line,
            action: format!("Triage `{uid}`: assign a type + stage so the broker can rank it."),
            severity: 2,
            points: 2,
        });
    }
    // Tier 4 — enrichment lint (severity 1, advisory: 0 pts).
    for h in compute_lint(report) {
        let (source_file, line) = at(&h.item);
        let action = match h.rule.as_str() {
            "missing_moscow" => format!("Add a `**MoSCoW:**` field to `{}` (Ready+ items need a MoSCoW).", h.item),
            "missing_epic" => format!("Add an `**Epic:**` ref to `{}` (Ready+ items need an epic).", h.item),
            "missing_evidence" => format!("Add an `**Evidence:**` run-id to `{}` (done items need evidence).", h.item),
            "undeclared_epic_value" => format!("Declare a `**Value:**` on epic `{}` (the MoSCoW anchor).", h.item),
            _ => format!("Enrich `{}`.", h.item),
        };
        q.push(GroomItem {
            groom_id: format!("GROOM-lint-{}-{}", h.rule, h.item),
            kind: h.rule,
            target_id: h.item,
            source_file,
            line,
            action,
            severity: 1,
            points: 0,
        });
    }
    q
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
    // Computed once by `compute_lint` (shared with `groom_queue` — one source).
    let lint: Vec<Value> = compute_lint(&report)
        .into_iter()
        .map(|h| {
            if h.rule == "undeclared_epic_value" {
                serde_json::json!({"item": h.item, "rule": h.rule})
            } else {
                serde_json::json!({"item": h.item, "rule": h.rule, "stage": h.stage})
            }
        })
        .collect();
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
        // The full prioritized grooming queue (cycle → dangling → untriaged →
        // lint). Same ranking authority `next_ready`'s groom tier draws from.
        ("groom_queue", serde_json::to_value(groom_queue(&report)).unwrap_or(Value::Array(vec![]))),
        // The live symbol model the IDE caches (the broker + pane read this).
        ("items", items_json),
    ];

    build_cmdb("check-backlog", score.clamp(0, 100) as u8, findings, Some(extras), None)
}

/// Runtime state the IDE overlays on top of the markdown symbol model when it
/// consumes the canonical ranker. The CLI passes `LiveState::default()` (empty),
/// which is why the default `next_ready` envelope is unchanged.
///
/// - `claimed`: item ids a live agent session currently holds a claim/lease on.
///   Treated as in-progress and excluded from candidates — a SUPERSET of the
///   markdown `stage == "In-progress"` exclusion, not a replacement for it.
/// - `done_overlay`: item ids the IDE knows are freshly done ahead of the
///   markdown being re-scanned. Treated as done (freshness overlay on top of the
///   stage-based `is_done`).
#[derive(Debug, Clone, Default)]
pub struct LiveState {
    pub claimed: std::collections::HashSet<String>,
    pub done_overlay: std::collections::HashSet<String>,
}

/// A single rankable candidate with its decomposed ranking key components.
/// `lane`/`epic_priority_rank`/`moscow_rank`/`declared_ready` mirror the 5-tuple
/// sort key (minus the tie-breaking source index). Emitted in sorted order.
pub struct RankedItem<'a> {
    pub item: &'a BacklogItem,
    pub lane: u8,
    pub epic_priority_rank: u8,
    pub moscow_rank: u8,
    pub declared_ready: bool,
}

/// The full ranking outcome: the sorted candidate list plus the reconciliation
/// counts the envelope (CLI or IDE) reports.
pub struct RankOutcome<'a> {
    pub candidates: Vec<RankedItem<'a>>,
    pub blocked: usize,
    pub deferred: usize,
    pub done_count: usize,
    pub in_progress: usize,
    pub defect_blocked: usize,
}

/// The canonical backlog ranking authority (IDE-BACKLOG-PM). Runs the tiered
/// candidate reconciliation over the parsed symbol model + a `LiveState` overlay,
/// returning candidates SORTED ascending by the 5-tuple key
/// `(lane, epic_priority_rank, moscow_rank, declared_ready, source_idx)` plus the
/// bucket counts. Both the CLI face (`next_ready`, empty `LiveState`) and the IDE
/// (populated `LiveState`) consume this — one ranking, two envelopes.
///
/// Reconciliation (in loop order):
/// - **done** = stage-based `is_done(it)` OR `live.done_overlay` membership →
///   `done_count`, skip.
/// - **in-progress** = `live.claimed` membership OR markdown `stage == "In-progress"`
///   (deliberate correctness-superset) → `in_progress`, skip.
/// - **deferred** = stage `Blocked`/`Deferred`, and Won't-MoSCoW → `deferred`, skip.
/// - **dangling** held-back items → `defect_blocked`, skip (tier-3 groom targets).
/// - **blocked** = has an unmet in-board dep → `blocked`, skip.
pub fn rank_backlog<'a>(report: &'a BacklogReport, live: &LiveState) -> RankOutcome<'a> {
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
    let mut ranked: Vec<(Key, RankedItem<'a>)> = Vec::new();
    let (mut blocked, mut deferred, mut done_count, mut in_progress, mut defect_blocked) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for (idx, it) in items.iter().enumerate() {
        // done: stage-based is_done, plus the IDE's freshness overlay.
        if is_done(it) || live.done_overlay.contains(&it.id) {
            done_count += 1;
            continue;
        }
        // in-progress: live claim OR markdown coordination stage (superset — both
        // exclude, neither replaces the other).
        if live.claimed.contains(&it.id) || it.stage == "In-progress" {
            in_progress += 1;
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
            defect_blocked += 1; // held back + tier-3 groom target
            continue;
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
        ranked.push((
            (lane, eprio, mr, dr, idx),
            RankedItem { item: it, lane, epic_priority_rank: eprio, moscow_rank: mr, declared_ready: dr == 0 },
        ));
    }
    ranked.sort_by(|a, b| a.0.cmp(&b.0));
    let candidates = ranked.into_iter().map(|(_, ri)| ri).collect();
    RankOutcome { candidates, blocked, deferred, done_count, in_progress, defect_blocked }
}

/// The PM broker, CLI face (IDE-BACKLOG-PM): the deterministic tiered
/// next-ready dispatch over the parsed symbol model — the *same* ranking the
/// IDE's `backlog.next_ready` runs, minus the runtime claim/lease + freshness
/// overlay (IDE-local state). Lets any agent session pull work without the IDE;
/// the markdown `stage` field is the coordination source of truth. Tiers:
/// implement → groom → capture → surfaced-idle; never invents filler.
pub fn next_ready(report: &BacklogReport) -> Value {
    // Default (CLI) path: no live runtime state. The IDE injects a populated
    // `LiveState` via `rank_backlog` directly; here the empty default keeps the
    // markdown `stage` field as the sole coordination source (byte-identical
    // envelope to the pre-extraction implementation).
    let o = rank_backlog(report, &LiveState::default());
    let (blocked, deferred, done_count, in_progress) =
        (o.blocked, o.deferred, o.done_count, o.in_progress);

    if let Some(ri) = o.candidates.first() {
        let it = ri.item;
        let lane = ri.lane;
        return serde_json::json!({
            "tier": "implement", "ready": true,
            "item": { "id": it.id, "title": it.title, "type": it.item_type, "stage": it.stage,
                      "moscow": it.moscow, "epic": it.epic, "source_file": it.source_file, "line": it.line },
            "rationale": { "lane": lane, "pinned": lane == 0, "override_lane": lane == 1,
                           "steering": lane == 2, "epic_priority_rank": ri.epic_priority_rank, "moscow_rank": ri.moscow_rank },
            "ready_count": o.candidates.len(),
            "explanation": format!("Next ready: {} — {}. Deps clear; not done/deferred.", it.id, it.title),
        });
    }

    // Tier 3 — groom the top board defect. ONE authority: the ranked
    // `groom_queue`. The broker's groom tier dispatches only the STRUCTURAL
    // defects (cycle → dangling → untriaged) before capture — advisory lint
    // enrichment lives in the full queue (PM view) but never preempts capture.
    // Because structural defects sort ahead of lint, this is `first()` whenever
    // a defect exists. Mapped back into the historical per-kind envelope so
    // existing consumers keep their field shape (`dangling_dep`/`members`/`item`).
    let queue = groom_queue(report);
    if let Some(g) = queue.iter().find(|g| matches!(g.kind.as_str(), "cycle" | "dangling" | "untriaged")) {
        let groom = match g.kind.as_str() {
            "cycle" => {
                // Recover the ordered members from the matching report cycle.
                let members: Vec<String> = report
                    .cycles
                    .iter()
                    .find(|c| {
                        let mut m = (*c).clone();
                        m.sort();
                        format!("GROOM-cycle-{}", m.join("-")) == g.groom_id
                    })
                    .map(|c| {
                        let mut m = c.clone();
                        m.sort();
                        m
                    })
                    .unwrap_or_default();
                serde_json::json!({ "groom_id": g.groom_id, "kind": "cycle", "members": members, "action": g.action })
            }
            "dangling" => {
                // Recover the dep from the matching dangling pair.
                let dep = report
                    .dangling_deps
                    .iter()
                    .find(|(item, dep)| format!("GROOM-dangling-{item}-{dep}") == g.groom_id)
                    .map(|(_, dep)| dep.clone())
                    .unwrap_or_default();
                serde_json::json!({ "groom_id": g.groom_id, "kind": "dangling_dep", "item": g.target_id, "dep": dep, "action": g.action })
            }
            "untriaged" => {
                serde_json::json!({ "groom_id": g.groom_id, "kind": "untriaged", "item": g.target_id, "action": g.action })
            }
            _ => {
                // Enrichment-lint groom (missing_moscow/epic/evidence, …).
                serde_json::json!({ "groom_id": g.groom_id, "kind": g.kind, "item": g.target_id, "action": g.action })
            }
        };
        return serde_json::json!({ "tier": "groom", "ready": false, "groom": groom,
            "explanation": format!("No implementable item — groom: {} ({}). The broker never invents filler.", g.groom_id, g.kind) });
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
        // `gates B0` is a REVERSE relationship (D1 is a prerequisite of B0), so
        // it is NOT a forward dep of D1. Only `dep`-direction words count.
        assert!(items[0].deps.is_empty(), "gates is reverse: {:?}", items[0].deps);
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

    // The R1(Could)/R2(Must) fixture reused across the rank_backlog tests.
    fn two_ready_report(dir: &TempDir) -> BacklogReport {
        write(dir.path(), "BACKLOG.md", "\
### R1: do the thing — READY
**MoSCoW:** Could
### R2: build the feature — READY
**MoSCoW:** Must
");
        parse_backlog(dir.path())
    }

    #[test]
    fn rank_backlog_empty_livestate_matches_next_ready() {
        let dir = TempDir::new().unwrap();
        let report = two_ready_report(&dir);
        let o = rank_backlog(&report, &LiveState::default());
        // Same top candidate + ready_count as the CLI envelope.
        assert_eq!(o.candidates.first().unwrap().item.id, "R2", "Must outranks Could");
        assert_eq!(o.candidates.len(), 2);
        let d = next_ready(&report);
        assert_eq!(d["item"]["id"], "R2");
        assert_eq!(d["ready_count"], o.candidates.len());
    }

    #[test]
    fn rank_backlog_claimed_id_excluded_as_in_progress() {
        let dir = TempDir::new().unwrap();
        let report = two_ready_report(&dir);
        let live = LiveState {
            claimed: std::collections::HashSet::from(["R2".to_string()]),
            ..Default::default()
        };
        let o = rank_backlog(&report, &live);
        // R2 is claimed → drops out; R1 becomes the top candidate.
        assert_eq!(o.candidates.len(), 1);
        assert_eq!(o.candidates.first().unwrap().item.id, "R1");
        assert_eq!(o.in_progress, 1);
    }

    #[test]
    fn rank_backlog_done_overlay_treats_id_as_done() {
        let dir = TempDir::new().unwrap();
        let report = two_ready_report(&dir);
        let live = LiveState {
            done_overlay: std::collections::HashSet::from(["R2".to_string()]),
            ..Default::default()
        };
        let o = rank_backlog(&report, &live);
        // R2 is done via the freshness overlay → drops out, done_count increments.
        assert_eq!(o.candidates.len(), 1);
        assert_eq!(o.candidates.first().unwrap().item.id, "R1");
        assert_eq!(o.done_count, 1);
    }

    #[test]
    fn rank_backlog_counts_defect_blocked_for_dangling_dep() {
        let dir = TempDir::new().unwrap();
        // Single item held back by a dangling dep → defect_blocked, no candidate.
        write(dir.path(), "BACKLOG.md", "### X1: thing — READY\n· dep NOPE9\n");
        let report = parse_backlog(dir.path());
        let o = rank_backlog(&report, &LiveState::default());
        assert!(o.candidates.is_empty());
        assert_eq!(o.defect_blocked, 1);
    }

    // ── Composite-key graph identity (Phase 1 core fix) ──────────────────────

    #[test]
    fn rejects_range_notation_token() {
        // The ONE safe `extract_ids`/`looks_like_id` tightening: `..` ranges.
        assert!(!looks_like_id("S10-DP-1..3"), "range notation is not an id");
        assert!(looks_like_id("S10-DP-3"), "a real id still parses");
        // A range token yields no dep; the real neighbour still does.
        let deps = extract_ids("dep S10-DP-1..3 then S10-DP-3");
        assert_eq!(deps, vec!["S10-DP-3".to_string()]);
    }

    #[test]
    fn composite_key_splits_duplicate_ids_across_files() {
        // Fixture (a): three files each defining `B0` with a distinct LOCAL dep.
        // Composite keys keep them DISTINCT — no merge, no phantom cycle, and no
        // cross-file dangling (each B0's dep resolves inside its own file).
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a/BACKLOG.md", "## Epic\n### B0: a — READY\nbody · dep AH1\n### AH1: helper — READY\n");
        write(dir.path(), "b/BACKLOG.md", "## Epic\n### B0: b — READY\nbody · dep BH1\n### BH1: helper — READY\n");
        write(dir.path(), "c/BACKLOG.md", "## Epic\n### B0: c — READY\nbody · dep CH1\n### CH1: helper — READY\n");
        let report = parse_backlog(dir.path());
        assert_eq!(report.items.iter().filter(|i| i.id == "B0").count(), 3, "3 distinct B0 nodes");
        assert!(report.cycles.is_empty(), "no phantom cycle: {:?}", report.cycles);
        assert!(report.dangling_deps.is_empty(), "no cross-file dangling: {:?}", report.dangling_deps);
    }

    #[test]
    fn composite_key_prevents_phantom_cross_file_cycle() {
        // Each file is internally acyclic (B0 leaf; C1→B0 in a; B0→C1 in b), but
        // a bare-id string index would collapse the duplicate `B0`/`C1` and
        // fabricate a C1↔B0 cycle. Composite keys resolve scope-first → none.
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a/BACKLOG.md", "## E\n### B0: a — READY\n### C1: a — READY\nbody · dep B0\n");
        write(dir.path(), "b/BACKLOG.md", "## E\n### B0: b — READY\nbody · dep C1\n");
        let report = parse_backlog(dir.path());
        assert!(report.cycles.is_empty(), "no phantom cycle: {:?}", report.cycles);
        assert!(report.dangling_deps.is_empty(), "deps resolve: {:?}", report.dangling_deps);
    }

    #[test]
    fn composite_key_splits_by_section() {
        // Fixture (b): two `##` epic sections in one file, each with a `B0` →
        // two distinct nodes carrying their own section scope.
        let md = "\
## Epic One
### B0: one — READY
body · dep A1
### A1: helper — READY
## Epic Two
### B0: two — READY
body · dep A2
### A2: helper — READY
";
        let items = parse_file(md, "roadmap/BACKLOG.md");
        let b0s: Vec<&BacklogItem> = items.iter().filter(|i| i.id == "B0").collect();
        assert_eq!(b0s.len(), 2, "two distinct B0 nodes (one per section)");
        assert_eq!(b0s[0].section, "Epic One");
        assert_eq!(b0s[1].section, "Epic Two");
    }

    #[test]
    fn dedup_table_row_and_heading_in_same_section() {
        // Fixture (c): the broker-framework case — a BRK-style id appears as a
        // table row AND a `####` heading in the SAME `##` section → one node.
        let md = "\
## Epic
| ID | Subject | Depends on |
|---|---|---|
| B0 | table form | (none) |

### B0: heading form — READY
body
";
        let items = parse_file(md, "roadmap/broker-framework-backlog.md");
        assert_eq!(items.iter().filter(|i| i.id == "B0").count(), 1, "table + heading dedup to one node");
        // First-wins: the table row survives.
        assert_eq!(items.iter().find(|i| i.id == "B0").unwrap().format, "table");
    }

    // ── Coverage widening + dep-scan guards (atomic) ─────────────────────────

    #[test]
    fn broker_master_table_drops_stage_cells_and_dedups() {
        // broker-framework-backlog.md-shaped: a Stage column (`S0-T`-class) that
        // must NOT be scanned for deps, only the `Depends on` column, and a
        // `####` heading that dedups against the table row (same section).
        let md = "\
## Master table
| ID | BB | Layer | Stage | Effort | Depends on |
|---|---|---|---|---|---|
| BRK-01-TRAIT | #1 cap | A | S0-T | M | #4, #6 |
| BRK-02-OVERLAY | #2 ov | A | S1-T | M | (none) |

#### BRK-01-TRAIT (BB #1)
**Description:** the trait every broker implements.
**Acceptance:** compiles.
";
        let items = parse_file(md, "roadmap/broker-framework-backlog.md");
        assert_eq!(items.iter().filter(|i| i.id == "BRK-01-TRAIT").count(), 1, "table + heading dedup");
        assert_eq!(items.len(), 2, "BRK-01 (deduped) + BRK-02");
        // No `S0-T`/`S1-T` Stage cell leaked in as a dependency.
        assert!(
            items.iter().all(|it| !it.deps.iter().any(|d| d.starts_with("S0-T") || d.starts_with("S1-T"))),
            "Stage cells must not become deps: {:?}",
            items.iter().map(|i| (&i.id, &i.deps)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn is_backlog_file_covers_backlog_suffix_and_epics_dir() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "roadmap/epics/S1-thing.md", "### E1: epic item — READY\n");
        write(dir.path(), "sub/my-framework-backlog.md", "### M1: thing — READY\n");
        write(dir.path(), "docs/notes.md", "### N1: not a backlog file — READY\n");
        let report = parse_backlog(dir.path());
        assert_eq!(report.files_scanned, 2, "epics-dir + *-backlog.md scanned; plain .md skipped");
        assert!(report.items.iter().any(|i| i.id == "E1"), "epics-dir item parsed");
        assert!(report.items.iter().any(|i| i.id == "M1"), "*-backlog.md item parsed");
        assert!(!report.items.iter().any(|i| i.id == "N1"), "plain notes.md not scanned");
    }

    #[test]
    fn range_token_not_dangling_but_real_id_parses() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "BACKLOG.md",
            "### T1: thing — READY\nbody · dep S10-DP-1..3\n· dep S10-DP-3\n### S10-DP-3: real — READY\n");
        let report = parse_backlog(dir.path());
        let t1 = report.items.iter().find(|i| i.id == "T1").unwrap();
        assert_eq!(t1.deps, vec!["S10-DP-3".to_string()], "range token dropped; real dep kept");
        assert!(report.dangling_deps.is_empty(), "the real dep resolves: {:?}", report.dangling_deps);
    }

    #[test]
    fn gates_is_reverse_direction_no_fabricated_cycle() {
        // The IDE ROADMAP shape: `D2 … gates B2` means D2 is a PREREQUISITE of
        // B2 (true edge B2→D2), while B2's row carries the real `dep D2`. The
        // sensor must read `gates B2` as REVERSE (no D2→B2 edge) — otherwise
        // D2→B2 + B2→D2 fabricates a phantom D2↔B2 cycle.
        let md = "\
| ID | Subject | Status / deps |
|---|---|---|
| D2 | broker | discovery; gates B2 |
| B2 | broker impl | dep D2 |
| D4 | pane | discovery; gates B4/B5 |
| B4 | pane impl | dep D4 |
| B5 | governance | dep B4 + D4 |
";
        let items = parse_file(md, "docs/plans/ROADMAP.md");
        let by = |id: &str| items.iter().find(|i| i.id == id).unwrap();
        // Reverse-direction rows carry NO forward dep.
        assert!(by("D2").deps.is_empty(), "D2 gates B2 → no dep: {:?}", by("D2").deps);
        assert!(by("D4").deps.is_empty(), "D4 gates B4/B5 → no dep: {:?}", by("D4").deps);
        // Forward `dep` rows still resolve.
        assert!(by("B2").deps.contains(&"D2".to_string()));
        assert!(by("B4").deps.contains(&"D4".to_string()));

        // End-to-end: no fabricated cycle over these rows.
        let dir = TempDir::new().unwrap();
        write(dir.path(), "docs/plans/ROADMAP.md", md);
        let report = parse_backlog(dir.path());
        assert!(report.cycles.is_empty(), "no fabricated cycle: {:?}", report.cycles);
    }

    // ── groom_queue unified with next_ready (one authority) ──────────────────

    #[test]
    fn groom_queue_ranks_cycle_then_dangling_then_untriaged() {
        let dir = TempDir::new().unwrap();
        // A1↔B1 cycle; C1→ZZ9 dangling; D1 untriaged (unrecognizable status).
        write(dir.path(), "BACKLOG.md", "\
### A1: a — READY
· dep B1
### B1: b — READY
· dep A1
### C1: c — READY
· dep ZZ9
### D1: d — foobar
");
        let report = parse_backlog(dir.path());
        let q = groom_queue(&report);
        assert_eq!(q[0].kind, "cycle");
        assert_eq!(q[0].groom_id, "GROOM-cycle-A1-B1");
        assert_eq!(q[0].points, 10);
        assert_eq!(q[1].kind, "dangling");
        assert_eq!(q[1].groom_id, "GROOM-dangling-C1-ZZ9");
        assert!(q.iter().any(|g| g.kind == "untriaged" && g.groom_id == "GROOM-triage-D1"));
        // Ordering invariant: severity is non-increasing down the queue.
        assert!(q.windows(2).all(|w| w[0].severity >= w[1].severity), "queue not severity-ordered");
    }

    #[test]
    fn groom_queue_emits_lint_grooms() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "BACKLOG.md", "### S1: ready story — READY\n");
        let report = parse_backlog(dir.path());
        let q = groom_queue(&report);
        assert!(q.iter().any(|g| g.kind == "missing_moscow" && g.groom_id == "GROOM-lint-missing_moscow-S1"));
        assert!(q.iter().any(|g| g.kind == "missing_epic" && g.groom_id == "GROOM-lint-missing_epic-S1"));
    }

    #[test]
    fn next_ready_groom_matches_groom_queue_first() {
        let dir = TempDir::new().unwrap();
        // Cycle + dangling both present → cycle-first (was dangling-first before
        // the groom_queue unification; the queue is now the sole authority).
        write(dir.path(), "BACKLOG.md", "\
### A1: a — READY
· dep B1
### B1: b — READY
· dep A1
### C1: c — READY
· dep ZZ9
");
        let report = parse_backlog(dir.path());
        let q = groom_queue(&report);
        let d = next_ready(&report);
        assert_eq!(d["tier"], "groom");
        assert_eq!(d["groom"]["kind"], "cycle");
        assert_eq!(d["groom"]["groom_id"], q[0].groom_id);
        assert_eq!(d["groom"]["members"], serde_json::json!(["A1", "B1"]));
    }

    #[tokio::test]
    async fn analyze_backlog_exposes_groom_queue_extra() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "BACKLOG.md", "### X1: thing — READY\n· dep NOPE9\n");
        let cmdb = analyze_backlog(&dir.path().to_string_lossy()).await;
        let gq = cmdb["groom_queue"].as_array().expect("groom_queue extra present");
        assert!(gq.iter().any(|g| g["kind"] == "dangling" && g["groom_id"] == "GROOM-dangling-X1-NOPE9"));
    }
}
