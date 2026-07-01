//! Documentation-graph sensor (v2-Feature 5 Phase 1, 2026-05-09).
//!
//! Walks every `*.md` file under `project_root`, extracts cross-references
//! (markdown links + `§N.M` spec shorthand), builds a directed graph, and
//! emits a CMDB with: total docs, total edges, orphan docs, broken links,
//! strongly-connected components.
//!
//! # Score
//!
//! Weighted blend of three penalties. Each is independently bounded; the
//! score subtracts them from 100 and clamps to [0, 100]:
//!
//! - **Orphan ratio** — `orphan_count / max(1, total_docs)`. Capped at
//!   30 points off (some orphans are intentional — index pages, READMEs).
//! - **Broken link count** — direct subtraction at 5 points each, capped
//!   at 40 points off. Broken links are unambiguously bad.
//! - **Cycle count** — `non_trivial_scc_count` (size > 1) → 2 points each,
//!   capped at 10 points off. Mutual references are sometimes healthy
//!   (paired docs), so the penalty is mild.
//!
//! # Skipped paths (Phase 1 hardcoded)
//!
//! `node_modules/`, `target/`, `.git/`, `.claude/brain/queues/`,
//! `dist/`, `build/`. Future: respect operator-authored
//! `.docignore` (deferred).
//!
//! # Phase 2 (deferred)
//!
//! - Front-matter parsing (YAML — `tags`, `references`, `superseded_by`)
//! - `.docignore` operator config
//! - Anchor link verification (`§N.M` references actually exist in target)
//! - IDE visualization tab (reuse BrainModel's force-directed renderer)

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DocumentationGraphServer {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl DocumentationGraphServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckDocumentationGraphParams {
    pub project_root: String,
}

#[tool_router]
impl DocumentationGraphServer {
    #[tool(
        description = "Build a documentation cross-reference graph for the project. \
        Walks *.md files, parses links, finds orphan docs + broken links + cycles. \
        Returns a CMDB-envelope JSON with score (0-100) + findings."
    )]
    async fn check_documentation_graph(
        &self,
        Parameters(p): Parameters<CheckDocumentationGraphParams>,
    ) -> String {
        serde_json::to_string_pretty(&analyze_documentation_graph(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for DocumentationGraphServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Documentation cross-reference graph sensory tool.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ── Public analyzer (consumable from tests + future integrations) ───

/// Per-document state used by the analyzer. `path` is relative to
/// `project_root` (forward slashes) — used as the canonical node id
/// in the graph.
#[derive(Debug, Clone)]
pub struct DocNode {
    pub path: String,
    /// Outbound edges (link targets, normalized to project-relative
    /// forward-slash paths).
    pub outbound: Vec<String>,
    /// Outbound link targets that didn't resolve to a real file.
    pub broken_outbound: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GraphReport {
    pub docs: BTreeMap<String, DocNode>,
    /// Doc paths with no inbound edges. May include legitimate index
    /// pages (README.md, index.md, mod.rs-style entries); the score
    /// penalizes the ratio mildly to account for false positives.
    pub orphans: Vec<String>,
    pub broken_links: Vec<(String, String)>, // (source_doc, broken_target)
    /// Non-trivial strongly-connected components (cycles): `vec![[a, b, c], ...]`.
    /// Size-1 SCCs (single nodes) are excluded.
    pub cycles: Vec<Vec<String>>,
}

pub async fn analyze_documentation_graph(project_root: &str) -> Value {
    let root = Path::new(project_root);
    // Doc-broker Phase 0: switch to the richer `DocReport` (front-matter +
    // version-drift + reachability + stale-diagram model). Same CMDB id +
    // domain binding ("check-documentation-graph" → `documentation-graph`).
    let report = build_doc_report(root, default_anchor(), &default_doc_excludes());
    let graph = &report.graph;
    let total_docs = graph.docs.len();
    let total_edges: usize = graph.docs.values().map(|n| n.outbound.len()).sum();
    let orphan_count = graph.orphans.len();
    let broken_count = graph.broken_links.len();
    let cycle_count = graph.cycles.len();

    // Layer A enrichment counts.
    let front_door_drift_count = report
        .version_drift
        .iter()
        .filter(|(p, _, _)| report.docs.get(p).map(|d| d.is_front_door).unwrap_or(false))
        .count();
    let drift_count = report.version_drift.len();
    let non_front_drift = drift_count.saturating_sub(front_door_drift_count);
    let stale_count = report
        .docs
        .values()
        .filter(|d| {
            d.staleness
                .iter()
                .any(|s| matches!(s, StalenessSignal::StatusStaleOrSuperseded))
        })
        .count();
    let refs_deleted_count = report.references_to_deleted.len();
    let stale_diagram_count = report.stale_diagrams.len();
    let unreachable_count = report.unreachable_from_front_door.len();

    // Existing blend (see module-level doc).
    let orphan_ratio = if total_docs == 0 {
        0.0
    } else {
        orphan_count as f64 / total_docs as f64
    };
    let orphan_penalty = (orphan_ratio * 30.0).min(30.0);
    let broken_penalty = ((broken_count as f64) * 5.0).min(40.0);
    let cycle_penalty = ((cycle_count as f64) * 2.0).min(10.0);

    // Layer A penalties layered onto the orphan/broken/cycle blend.
    // Front-door drift is heavier (gates every reading path).
    let fd_drift_penalty = ((front_door_drift_count as f64) * 8.0).min(24.0);
    let drift_penalty = ((non_front_drift as f64) * 3.0).min(15.0);
    let stale_penalty = ((stale_count as f64) * 4.0).min(20.0);
    let refs_deleted_penalty = ((refs_deleted_count as f64) * 5.0).min(20.0);
    let stale_diagram_penalty = ((stale_diagram_count as f64) * 3.0).min(15.0);
    let unreachable_penalty = ((unreachable_count as f64) * 2.0).min(15.0);

    let score: i32 = (100.0
        - orphan_penalty
        - broken_penalty
        - cycle_penalty
        - fd_drift_penalty
        - drift_penalty
        - stale_penalty
        - refs_deleted_penalty
        - stale_diagram_penalty
        - unreachable_penalty)
        .clamp(0.0, 100.0) as i32;

    let mut findings: Vec<Finding> = Vec::new();
    if total_docs == 0 {
        findings.push(Finding {
            name: "no_markdown_files".into(),
            status: "info".into(),
            points: 0,
            detail: Some("No *.md files found under project_root".into()),
        });
    } else {
        findings.push(Finding {
            name: "doc_count".into(),
            status: "info".into(),
            points: 0,
            detail: Some(format!("{total_docs} markdown files, {total_edges} outbound links")),
        });
    }
    if orphan_count > 0 {
        findings.push(Finding {
            name: "orphan_docs".into(),
            status: "warn".into(),
            points: -(orphan_penalty as i32),
            detail: Some(format!(
                "{orphan_count} of {total_docs} docs have no inbound references — first 5: [{}]",
                graph.orphans.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
            )),
        });
    }
    if broken_count > 0 {
        findings.push(Finding {
            name: "broken_links".into(),
            status: "fail".into(),
            points: -(broken_penalty as i32),
            detail: Some(format!(
                "{broken_count} broken outbound link(s) — first 5: [{}]",
                graph
                    .broken_links
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
            name: "doc_cycles".into(),
            status: "info".into(),
            points: -(cycle_penalty as i32),
            detail: Some(format!(
                "{cycle_count} non-trivial cycle(s) — pairs with mutual references can be healthy; surfacing for review"
            )),
        });
    }
    // ── Layer A enrichment findings ──────────────────────────────────
    if drift_count > 0 {
        findings.push(Finding {
            name: "version_marker_drift".into(),
            status: "warn".into(),
            points: -((fd_drift_penalty + drift_penalty) as i32),
            detail: Some(format!(
                "{drift_count} doc(s) state a version that diverges from the anchor ({front_door_drift_count} front-door) — first 5: [{}]",
                report
                    .version_drift
                    .iter()
                    .take(5)
                    .map(|(p, s, a)| format!("{p}: {s}≠{a}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        });
    }
    if stale_count > 0 {
        findings.push(Finding {
            name: "stale_or_superseded".into(),
            status: "warn".into(),
            points: -(stale_penalty as i32),
            detail: Some(format!(
                "{stale_count} doc(s) declare a stale/superseded/archived status in front-matter"
            )),
        });
    }
    if refs_deleted_count > 0 {
        findings.push(Finding {
            name: "references_to_deleted".into(),
            status: "fail".into(),
            points: -(refs_deleted_penalty as i32),
            detail: Some(format!(
                "{refs_deleted_count} supersedes/superseded-by reference(s) point at a missing file — first 5: [{}]",
                report
                    .references_to_deleted
                    .iter()
                    .take(5)
                    .map(|(p, t)| format!("{p}->{t}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        });
    }
    if stale_diagram_count > 0 {
        findings.push(Finding {
            name: "stale_diagrams".into(),
            status: "warn".into(),
            points: -(stale_diagram_penalty as i32),
            detail: Some(format!(
                "{stale_diagram_count} diagram signal(s) (forbidden-term drift / pending-spec / missing-mmd) — first 5: [{}]",
                report
                    .stale_diagrams
                    .iter()
                    .take(5)
                    .map(|d| format!("{}: {}", d.diagram, diag_reason_label(&d.reason)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        });
    }
    if unreachable_count > 0 {
        findings.push(Finding {
            name: "unreachable_from_front_door".into(),
            status: "warn".into(),
            points: -(unreachable_penalty as i32),
            detail: Some(format!(
                "{unreachable_count} doc(s) not reachable from any front door — first 5: [{}]",
                report
                    .unreachable_from_front_door
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        });
    }

    let extras: Vec<(&str, Value)> = vec![
        ("total_docs", Value::Number(total_docs.into())),
        ("total_edges", Value::Number(total_edges.into())),
        ("orphan_count", Value::Number(orphan_count.into())),
        ("broken_link_count", Value::Number(broken_count.into())),
        ("cycle_count", Value::Number(cycle_count.into())),
        // Layer A enrichment extras.
        ("front_door_drift_count", Value::Number(front_door_drift_count.into())),
        ("version_drift_count", Value::Number(drift_count.into())),
        ("stale_count", Value::Number(stale_count.into())),
        ("references_to_deleted_count", Value::Number(refs_deleted_count.into())),
        ("stale_diagram_count", Value::Number(stale_diagram_count.into())),
        ("unreachable_count", Value::Number(unreachable_count.into())),
    ];

    build_cmdb(
        "check-documentation-graph",
        score.clamp(0, 100) as u8,
        findings,
        Some(extras),
        None,
    )
}

// ── Pure analyzer (no Tauri / no async) ────────────────────────────

const SKIPPED_DIR_NAMES: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "dist",
    "build",
    "__pycache__",
    ".cargo",
    ".rustup",
    "venv",
    ".venv",
    // Doc-broker Phase 0 (2026-06-30): the NeuroGrim `vendor/` tree
    // (rustsec-advisory-db) is ~1000 vendored advisory .md files — pure
    // noise for the documentation graph. Dir-name skip; `archive`/`audit`/
    // `.claude/skills/archived` are path-prefix excludes (`default_doc_excludes`).
    "vendor",
    // Doc-broker Phase 2 (2026-06-30): CLI `init-templates/` dirs ship
    // adopter-project scaffolding payloads (e.g. README-snippet.md linking to
    // a sibling CLAUDE.md that renders from CLAUDE.md.tmpl) — those links are
    // valid in the RENDERED project, false-broken only in-tree. Skip the
    // payload dirs so they aren't scored as ecosystem docs.
    "init-templates",
    // Doc-broker Phase 4 (2026-06-30): any `archived/` dir at any depth holds
    // retired docs kept for provenance (skill-deprecation convention) — not
    // live reading-path documentation. Skip so superseded docs don't score as
    // orphan/drift/unreachable once moved under `archived/`.
    "archived",
    // Doc-broker Phase 4 (2026-06-30): `data/explain/` holds CLI-served topic
    // payloads (`neurogrim explain`) — include_str!'d, first-line `<!-- topic -->`
    // header asserted by tests, carrying a methodology-version stamp (not an
    // ecosystem-version claim). They can't take YAML front-matter, so skip the
    // dir rather than let their stamp read as ecosystem drift.
    "explain",
];

/// The common documentation noise excluded from the doc-broker walk by
/// default (doc-broker Phase 0). Each is a project-relative forward-slash
/// PATH PREFIX matched by `is_excluded`. `vendor` is handled separately as
/// a dir-name skip in `SKIPPED_DIR_NAMES`.
pub fn default_doc_excludes() -> Vec<String> {
    vec![
        "archive".to_string(),
        "audit".to_string(),
        // `.claude/` is agent-infrastructure — skills (invoked by name, not
        // linked → orphan-by-design), experiments, plans, brain data. It is not
        // reading-path documentation, and skill "version" markers are
        // methodology versions, not ecosystem-version claims. Excluding it keeps
        // orphan/drift/reachability about real narrative docs. Skills have their
        // own governance (capability-hygiene). Subsumes `.claude/skills/archived`.
        ".claude".to_string(),
    ]
}

/// Whether a project-relative forward-slash `rel` falls under any exclude
/// PATH PREFIX. Segment-aware: prefix `archive` matches `archive` and
/// `archive/x.md` but NOT `archived-stuff.md`.
fn is_excluded(rel: &str, excludes: &[String]) -> bool {
    excludes.iter().any(|ex| {
        !ex.is_empty() && (rel == ex.as_str() || rel.starts_with(&format!("{ex}/")))
    })
}

pub fn build_graph(root: &Path, excludes: &[String]) -> GraphReport {
    let mut docs: BTreeMap<String, DocNode> = BTreeMap::new();
    let mut all_paths: BTreeSet<String> = BTreeSet::new();

    // First pass — collect every *.md path (project-relative,
    // forward-slash form) so we can resolve link targets.
    walk_markdown(root, root, excludes, &mut |abs_path| {
        if let Some(rel) = relpath(root, abs_path) {
            all_paths.insert(rel);
        }
    });

    // Second pass — parse each file's outbound links.
    for rel in &all_paths {
        let abs = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let text = match std::fs::read_to_string(&abs) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let raw_targets = extract_link_targets(&text);
        let mut outbound: Vec<String> = Vec::new();
        let mut broken: Vec<String> = Vec::new();
        for raw in raw_targets {
            // Skip external URLs + mail + anchors-only links + javascript:
            if raw.starts_with("http://")
                || raw.starts_with("https://")
                || raw.starts_with("mailto:")
                || raw.starts_with("javascript:")
                || raw.starts_with('#')
            {
                continue;
            }
            // Strip anchor fragment when present — link to the doc itself.
            let cleaned = raw.split('#').next().unwrap_or(&raw).to_string();
            if cleaned.is_empty() {
                continue;
            }
            // Resolve relative to the source doc's directory.
            let source_dir = Path::new(rel).parent();
            let resolved = if cleaned.starts_with('/') {
                cleaned.trim_start_matches('/').to_string()
            } else if let Some(d) = source_dir {
                let combined = d.join(&cleaned);
                normalize_relative(&combined)
            } else {
                cleaned.clone()
            };
            // Phase 1: only consider links whose target exists in the
            // markdown set OR is on disk as a markdown file.
            let resolved_md = if resolved.ends_with(".md") {
                resolved.clone()
            } else if all_paths.contains(&format!("{resolved}.md")) {
                format!("{resolved}.md")
            } else if all_paths.contains(&format!("{resolved}/README.md")) {
                format!("{resolved}/README.md")
            } else {
                resolved.clone()
            };
            if all_paths.contains(&resolved_md) {
                outbound.push(resolved_md);
            } else if resolved.ends_with(".md") {
                // Not in the walked graph. If the target still EXISTS on disk
                // (e.g. it lives under an excluded dir like `.claude/`), it's a
                // valid external link, not broken — only flag genuinely-absent
                // targets. This keeps exclusions from manufacturing false breaks.
                let on_disk = root
                    .join(resolved.replace('/', std::path::MAIN_SEPARATOR_STR))
                    .is_file();
                if !on_disk {
                    broken.push(resolved);
                }
            }
            // Other extensions (.png, .json, etc.) intentionally
            // ignored in Phase 1 — operator may have a mix of doc
            // shapes; we score only doc-to-doc.
        }
        docs.insert(
            rel.clone(),
            DocNode {
                path: rel.clone(),
                outbound,
                broken_outbound: broken,
            },
        );
    }

    let orphans = compute_orphans(&docs);
    let broken_links: Vec<(String, String)> = docs
        .values()
        .flat_map(|d| d.broken_outbound.iter().map(|t| (d.path.clone(), t.clone())))
        .collect();
    let cycles = compute_non_trivial_sccs(&docs);

    GraphReport {
        docs,
        orphans,
        broken_links,
        cycles,
    }
}

fn walk_markdown(root: &Path, dir: &Path, excludes: &[String], visit: &mut impl FnMut(&Path)) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Skip ALL dot-directories at any depth, including `.claude`
            // (agent-infrastructure: skills/experiments/plans/brain — not
            // reading-path documentation; skills are invoked by name, not linked).
            if name.starts_with('.') {
                continue;
            }
            if SKIPPED_DIR_NAMES.contains(&name) {
                continue;
            }
        }
        // Exclude-prefix prune (doc-broker Phase 0): skip any file/dir whose
        // project-relative path falls under an exclude prefix.
        if let Some(rel) = relpath(root, &path) {
            if is_excluded(&rel, excludes) {
                continue;
            }
        }
        if path.is_dir() {
            walk_markdown(root, &path, excludes, visit);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
            .unwrap_or(false)
        {
            visit(&path);
        }
    }
}

fn relpath(root: &Path, abs: &Path) -> Option<String> {
    let rel = abs.strip_prefix(root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

fn normalize_relative(p: &Path) -> String {
    // Resolve `..` segments without touching the filesystem; Path::canonicalize
    // would resolve symlinks + require the path to exist. For graph
    // building we just need `a/b/../c` -> `a/c`.
    let mut parts: Vec<&str> = Vec::new();
    for comp in p.iter() {
        let s = comp.to_string_lossy();
        match s.as_ref() {
            "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(unsafe { &*(other as *const str) }),
        }
    }
    parts.join("/")
}

fn extract_link_targets(markdown: &str) -> Vec<String> {
    // Use pulldown-cmark for proper markdown parsing — handles nested
    // emphasis, code fences (so `[foo](bar)` inside code doesn't fire),
    // reference-style links, etc. v0.10's `Tag::Link` is a struct
    // variant with `dest_url` (renamed from positional `dest`).
    use pulldown_cmark::{Event, Parser, Tag};
    let parser = Parser::new(markdown);
    let mut out: Vec<String> = Vec::new();
    for event in parser {
        if let Event::Start(Tag::Link { dest_url, .. }) = event {
            out.push(dest_url.into_string());
        }
    }
    out
}

fn compute_orphans(docs: &BTreeMap<String, DocNode>) -> Vec<String> {
    // Inbound count per doc.
    let mut inbound: HashMap<&str, usize> = HashMap::new();
    for n in docs.values() {
        for t in &n.outbound {
            *inbound.entry(t.as_str()).or_insert(0) += 1;
        }
    }
    let mut orphans: Vec<String> = Vec::new();
    for (path, _) in docs {
        // Index-style filenames are excluded — README + index are
        // intentional roots, not orphans.
        let lower = path.to_lowercase();
        let basename = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        if basename == "readme.md"
            || basename == "index.md"
            || lower.ends_with("/changelog.md")
            || lower == "changelog.md"
        {
            continue;
        }
        if !inbound.contains_key(path.as_str()) {
            orphans.push(path.clone());
        }
    }
    orphans
}

/// Tarjan's SCC algorithm — non-trivial SCCs only (size > 1).
fn compute_non_trivial_sccs(docs: &BTreeMap<String, DocNode>) -> Vec<Vec<String>> {
    let nodes: Vec<&str> = docs.keys().map(|s| s.as_str()).collect();
    let index_of: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (*n, i))
        .collect();
    let n = nodes.len();
    let mut indices: Vec<i32> = vec![-1; n];
    let mut lowlinks: Vec<i32> = vec![0; n];
    let mut on_stack: Vec<bool> = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut counter: i32 = 0;
    let mut sccs: Vec<Vec<usize>> = Vec::new();

    // Iterative-ish Tarjan via explicit recursion buffer would be
    // prettier; for typical doc-graph sizes (<500 nodes) recursion is
    // fine — Rust's default 8 MB stack handles it.
    fn strongconnect(
        v: usize,
        nodes: &[&str],
        index_of: &HashMap<&str, usize>,
        docs: &BTreeMap<String, DocNode>,
        indices: &mut [i32],
        lowlinks: &mut [i32],
        on_stack: &mut [bool],
        stack: &mut Vec<usize>,
        counter: &mut i32,
        sccs: &mut Vec<Vec<usize>>,
    ) {
        indices[v] = *counter;
        lowlinks[v] = *counter;
        *counter += 1;
        stack.push(v);
        on_stack[v] = true;

        let v_name = nodes[v];
        if let Some(node) = docs.get(v_name) {
            for target in &node.outbound {
                if let Some(&w) = index_of.get(target.as_str()) {
                    if indices[w] == -1 {
                        strongconnect(
                            w, nodes, index_of, docs, indices, lowlinks, on_stack, stack,
                            counter, sccs,
                        );
                        lowlinks[v] = lowlinks[v].min(lowlinks[w]);
                    } else if on_stack[w] {
                        lowlinks[v] = lowlinks[v].min(indices[w]);
                    }
                }
            }
        }

        if lowlinks[v] == indices[v] {
            let mut scc: Vec<usize> = Vec::new();
            loop {
                let w = stack.pop().unwrap();
                on_stack[w] = false;
                scc.push(w);
                if w == v {
                    break;
                }
            }
            if scc.len() > 1 {
                sccs.push(scc);
            }
        }
    }

    for v in 0..n {
        if indices[v] == -1 {
            strongconnect(
                v,
                &nodes,
                &index_of,
                docs,
                &mut indices,
                &mut lowlinks,
                &mut on_stack,
                &mut stack,
                &mut counter,
                &mut sccs,
            );
        }
    }

    sccs.into_iter()
        .map(|ids| ids.into_iter().map(|i| nodes[i].to_string()).collect())
        .collect()
}

// ── Layer A: front-matter + freshness/version model (doc-broker) ────
//
// All net-new and pure. `build_doc_report` wraps `build_graph` (calls,
// does not fork it) and re-reads each file for front-matter + body
// version markers. `next_doc` is the tiered dispatcher mirroring
// `backlog.rs::next_ready`. Nothing here is async; reads only.

/// Declared lifecycle status from a doc's YAML front-matter. serde
/// lowercase (`current`, `draft`, `stale`, `superseded`, `archived`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocStatus {
    Current,
    Draft,
    Stale,
    Superseded,
    Archived,
}

/// Which ecosystem version field governs a doc's stated version. serde
/// lower/kebab (`ecosystem`, `spec`, `neurogrim`, `none`). `none` opts a
/// doc out of version-drift checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    Ecosystem,
    Spec,
    Neurogrim,
    None,
}

/// Parsed YAML front-matter (every field optional — honest-unknown).
/// `raw_present:false` means no parseable front-matter fence was found
/// (itself a mild signal).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub doc_version: Option<String>,
    pub date: Option<String>,
    pub status: Option<DocStatus>,
    pub supersedes: Vec<String>,
    pub superseded_by: Option<String>,
    pub anchored_to: Option<Anchor>,
    pub owner: Option<String>,
    pub front_door: bool,
    pub raw_present: bool,
}

/// The raw deserialize target — all `Option`, container-`default`, so
/// missing fields never error. Kebab-case maps `doc-version`,
/// `superseded-by`, `anchored-to`, `front-door`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
struct RawFrontMatter {
    doc_version: Option<String>,
    date: Option<String>,
    status: Option<DocStatus>,
    supersedes: Option<Vec<String>>,
    superseded_by: Option<String>,
    anchored_to: Option<Anchor>,
    owner: Option<String>,
    front_door: Option<bool>,
}

/// Parse a YAML front-matter fence. Panic-free: ANY parse error or an
/// absent fence yields `FrontMatter{raw_present:false, ..Default}`.
pub fn parse_front_matter(text: &str) -> FrontMatter {
    let body = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"));
    if let Some(rest) = body {
        if let Some(end) = rest.find("\n---") {
            let yaml = &rest[..end];
            match serde_yaml::from_str::<RawFrontMatter>(yaml) {
                Ok(raw) => {
                    return FrontMatter {
                        doc_version: raw.doc_version,
                        date: raw.date,
                        status: raw.status,
                        supersedes: raw.supersedes.unwrap_or_default(),
                        superseded_by: raw.superseded_by,
                        anchored_to: raw.anchored_to,
                        owner: raw.owner,
                        front_door: raw.front_door.unwrap_or(false),
                        raw_present: true,
                    };
                }
                // Malformed / foreign YAML — treat as no front-matter.
                Err(_) => return FrontMatter::default(),
            }
        }
    }
    FrontMatter::default()
}

/// The governing version anchor for the ecosystem at a point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EcosystemAnchor {
    pub ecosystem_version: String,
    pub spec_version: String,
    pub neurogrim_version: String,
    pub anchor_date: String,
}

/// Compile-time default matching today's aligned values (2026-05-09).
pub fn default_anchor() -> EcosystemAnchor {
    EcosystemAnchor {
        ecosystem_version: "5.0".to_string(),
        spec_version: "3.2".to_string(),
        neurogrim_version: "5.0.0".to_string(),
        anchor_date: "2026-05-09".to_string(),
    }
}

/// Resolve the runtime anchor. For now: explicit override, else the
/// compile-time default.
// TODO(doc-broker Phase 1): parse a sentinel file (root `CLAUDE.md` /
// `VERSION.toml`) for anchor values before falling back to the default.
pub fn resolve_anchor(root: &Path, override_: Option<EcosystemAnchor>) -> EcosystemAnchor {
    let _ = root;
    override_.unwrap_or_else(default_anchor)
}

/// A freshness/staleness signal attached to a doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StalenessSignal {
    StatusStaleOrSuperseded,
    VersionMarkerDrift { stated: String, anchor: String },
    ReferencesDeleted(String),
    Orphan,
    BrokenLink(String),
    Unreachable,
    StaleDiagram(String),
    NoFrontMatter,
}

/// Per-doc enrichment over the graph node.
#[derive(Debug, Clone)]
pub struct DocMeta {
    pub path: String,
    pub front_matter: FrontMatter,
    pub staleness: Vec<StalenessSignal>,
    pub is_front_door: bool,
}

/// Why a diagram is flagged stale (deterministic; no rendering).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagReason {
    PendingSpec(String),
    ForbiddenTermDrift(String),
    MissingFromMmdConvention,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleDiagram {
    pub diagram: String,
    pub reason: DiagReason,
}

/// The full documentation report: the graph plus the freshness/version
/// model. Single source of truth for both [Sense]
/// (`analyze_documentation_graph`) and [InnateAbility] (`next_doc`).
#[derive(Debug, Clone)]
pub struct DocReport {
    pub graph: GraphReport,
    pub docs: BTreeMap<String, DocMeta>,
    pub anchor: EcosystemAnchor,
    pub front_doors: Vec<String>,
    pub unreachable_from_front_door: Vec<String>,
    pub references_to_deleted: Vec<(String, String)>,
    /// `(doc_path, stated_version, anchor_version)`.
    pub version_drift: Vec<(String, String, String)>,
    pub stale_diagrams: Vec<StaleDiagram>,
}

const RETIRED_DIAGRAM_TERMS: &[&str] = &["Federation Broker"];

/// Which anchor field governs a doc, from its `anchored-to` front-matter
/// (absent ⇒ ecosystem default; explicit `none` ⇒ opt out → `None`).
enum AnchorKind {
    Ecosystem,
    Spec,
    Neurogrim,
}

fn governing(fm: &FrontMatter) -> Option<AnchorKind> {
    match fm.anchored_to {
        Some(Anchor::None) => None,
        Some(Anchor::Spec) => Some(AnchorKind::Spec),
        Some(Anchor::Neurogrim) => Some(AnchorKind::Neurogrim),
        Some(Anchor::Ecosystem) | None => Some(AnchorKind::Ecosystem),
    }
}

fn anchor_value(kind: &AnchorKind, a: &EcosystemAnchor) -> String {
    match kind {
        AnchorKind::Ecosystem => a.ecosystem_version.clone(),
        AnchorKind::Spec => a.spec_version.clone(),
        AnchorKind::Neurogrim => a.neurogrim_version.clone(),
    }
}

/// Parse a version into `(major, minor, patch)`, ignoring a leading `v`/`V`
/// and any non-numeric suffix (e.g. `-rc.1`); missing components default to 0.
/// `None` if there's no leading numeric component. This is the basis for
/// semver-aware comparison so `5.0.0`, `5.0`, and `v5` all compare equal.
fn parse_semver(v: &str) -> Option<(u32, u32, u32)> {
    let s = v.trim().trim_start_matches(['v', 'V']);
    let core: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut it = core.split('.').filter(|p| !p.is_empty());
    let major = it.next()?.parse::<u32>().ok()?;
    let minor = it.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch = it.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// Semver-normalized equality: `5.0.0` == `5.0` == `v5`. Falls back to
/// case-insensitive trimmed string equality when either side lacks a numeric
/// core (so non-numeric version labels still compare sanely).
fn version_eq_semver(a: &str, b: &str) -> bool {
    match (parse_semver(a), parse_semver(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a.trim().eq_ignore_ascii_case(b.trim()),
    }
}

/// `0` = major-version gap (top priority), `1` = minor/patch gap, `2` =
/// equal. Semver-normalized. Smaller sorts higher in the ascending key.
fn compute_anchor_distance(stated: &str, anchor: &str) -> u8 {
    match (parse_semver(stated), parse_semver(anchor)) {
        (Some(a), Some(b)) if a.0 != b.0 => 0,
        (Some(a), Some(b)) if a == b => 2,
        (Some(_), Some(_)) => 1,
        _ if stated.trim().eq_ignore_ascii_case(anchor.trim()) => 2,
        _ => 1,
    }
}

fn is_front_door(path: &str, fm: &FrontMatter) -> bool {
    if fm.front_door {
        return true;
    }
    let base = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_uppercase();
    matches!(
        base.as_str(),
        "CLAUDE.MD" | "README.MD" | "ROADMAP.MD" | "VISION.MD" | "SCAFFOLDING.MD"
    ) || base.contains("AGENT-PRIMER")
}

fn reference_exists(root: &Path, graph: &GraphReport, target: &str) -> bool {
    if graph.docs.contains_key(target) {
        return true;
    }
    root.join(target.replace('/', std::path::MAIN_SEPARATOR_STR)).exists()
}

/// BFS over `outbound` from the front doors → the set of reachable docs
/// (front doors included as roots).
fn bfs_reachable(graph: &GraphReport, front_doors: &[String]) -> BTreeSet<String> {
    let mut reached: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    for fd in front_doors {
        if reached.insert(fd.clone()) {
            queue.push_back(fd.clone());
        }
    }
    while let Some(cur) = queue.pop_front() {
        if let Some(node) = graph.docs.get(&cur) {
            for t in &node.outbound {
                if graph.docs.contains_key(t) && reached.insert(t.clone()) {
                    queue.push_back(t.clone());
                }
            }
        }
    }
    reached
}

fn status_label(s: &Option<DocStatus>) -> Option<String> {
    s.as_ref().map(|st| {
        match st {
            DocStatus::Current => "current",
            DocStatus::Draft => "draft",
            DocStatus::Stale => "stale",
            DocStatus::Superseded => "superseded",
            DocStatus::Archived => "archived",
        }
        .to_string()
    })
}

fn signal_label(s: &StalenessSignal) -> String {
    match s {
        StalenessSignal::StatusStaleOrSuperseded => "status-stale-or-superseded".to_string(),
        StalenessSignal::VersionMarkerDrift { stated, anchor } => {
            format!("version-marker-drift({stated} vs {anchor})")
        }
        StalenessSignal::ReferencesDeleted(t) => format!("references-deleted({t})"),
        StalenessSignal::Orphan => "orphan".to_string(),
        StalenessSignal::BrokenLink(t) => format!("broken-link({t})"),
        StalenessSignal::Unreachable => "unreachable".to_string(),
        StalenessSignal::StaleDiagram(d) => format!("stale-diagram({d})"),
        StalenessSignal::NoFrontMatter => "no-front-matter".to_string(),
    }
}

fn diag_reason_label(r: &DiagReason) -> String {
    match r {
        DiagReason::PendingSpec(s) => format!("pending-spec({s})"),
        DiagReason::ForbiddenTermDrift(t) => format!("forbidden-term-drift({t})"),
        DiagReason::MissingFromMmdConvention => "missing-from-mmd-convention".to_string(),
    }
}

/// Walk every (non-skipped, non-excluded) file under `root`. Generalizes
/// `walk_markdown` to all extensions for the diagram scan.
fn walk_all_files(root: &Path, dir: &Path, excludes: &[String], visit: &mut impl FnMut(&Path)) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Skip ALL dot-directories at any depth, including `.claude`
            // (agent-infrastructure: skills/experiments/plans/brain — not
            // reading-path documentation; skills are invoked by name, not linked).
            if name.starts_with('.') {
                continue;
            }
            if SKIPPED_DIR_NAMES.contains(&name) {
                continue;
            }
        }
        if let Some(rel) = relpath(root, &path) {
            if is_excluded(&rel, excludes) {
                continue;
            }
        }
        if path.is_dir() {
            walk_all_files(root, &path, excludes, visit);
        } else {
            visit(&path);
        }
    }
}

fn parent_dir(rel: &str) -> String {
    Path::new(rel)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

/// A small sibling walker over `*.svg` / `*.drawio` / `*.mmd` (+ `.md`
/// for the mermaid-convention check). Deterministic, no rendering.
fn build_stale_diagrams(
    root: &Path,
    excludes: &[String],
    md_docs: &BTreeMap<String, DocNode>,
) -> Vec<StaleDiagram> {
    let mut diagrams: Vec<(String, String)> = Vec::new(); // (rel, text)
    let mut mmd_dirs: BTreeSet<String> = BTreeSet::new(); // dirs containing a .mmd
    let mut specs: Vec<(String, String, bool)> = Vec::new(); // (dir, rel, has_pending)

    walk_all_files(root, root, excludes, &mut |abs| {
        let rel = match relpath(root, abs) {
            Some(r) => r,
            None => return,
        };
        let name = Path::new(&rel)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let ext = Path::new(&rel)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        let is_drawio = name.ends_with(".drawio") || name.ends_with(".drawio.svg");
        if ext == "svg" || ext == "mmd" || is_drawio {
            let text = std::fs::read_to_string(abs).unwrap_or_default();
            if ext == "mmd" {
                mmd_dirs.insert(parent_dir(&rel));
            }
            diagrams.push((rel.clone(), text));
        }
        if name.ends_with("-spec.md") && name.contains("-v") {
            let text = std::fs::read_to_string(abs).unwrap_or_default();
            specs.push((parent_dir(&rel), rel.clone(), text.contains("PENDING")));
        }
    });

    diagrams.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out: Vec<StaleDiagram> = Vec::new();
    for (rel, text) in &diagrams {
        for term in RETIRED_DIAGRAM_TERMS {
            if text.contains(term) {
                out.push(StaleDiagram {
                    diagram: rel.clone(),
                    reason: DiagReason::ForbiddenTermDrift((*term).to_string()),
                });
            }
        }
        let dir = parent_dir(rel);
        if let Some((_, spec_rel, _)) = specs.iter().find(|(p, _, pending)| *p == dir && *pending) {
            out.push(StaleDiagram {
                diagram: rel.clone(),
                reason: DiagReason::PendingSpec(spec_rel.clone()),
            });
        }
    }

    // MissingFromMmdConvention: a `.md` embeds a ```mermaid fence but its
    // directory has no sibling `.mmd`.
    for path in md_docs.keys() {
        let abs = root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let text = std::fs::read_to_string(&abs).unwrap_or_default();
        if text.contains("```mermaid") && !mmd_dirs.contains(&parent_dir(path)) {
            out.push(StaleDiagram {
                diagram: path.clone(),
                reason: DiagReason::MissingFromMmdConvention,
            });
        }
    }

    out.sort_by(|a, b| {
        a.diagram
            .cmp(&b.diagram)
            .then(diag_reason_label(&a.reason).cmp(&diag_reason_label(&b.reason)))
    });
    out
}

/// Build the full `DocReport` — calls `build_graph` then re-reads each
/// file for front-matter + body version markers, computes per-doc
/// staleness, BFS reachability, and the stale-diagram model. Pure.
pub fn build_doc_report(root: &Path, anchor: EcosystemAnchor, excludes: &[String]) -> DocReport {
    let graph = build_graph(root, excludes);

    // Broken links grouped by source doc.
    let mut broken_by_src: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (src, tgt) in &graph.broken_links {
        broken_by_src.entry(src.as_str()).or_default().push(tgt.clone());
    }
    let orphan_set: BTreeSet<&str> = graph.orphans.iter().map(|s| s.as_str()).collect();

    let mut docs: BTreeMap<String, DocMeta> = BTreeMap::new();
    let mut front_doors: Vec<String> = Vec::new();
    let mut references_to_deleted: Vec<(String, String)> = Vec::new();
    let mut version_drift: Vec<(String, String, String)> = Vec::new();

    for path in graph.docs.keys() {
        let abs = root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let text = std::fs::read_to_string(&abs).unwrap_or_default();
        let fm = parse_front_matter(&text);
        let is_fd = is_front_door(path, &fm);
        if is_fd {
            front_doors.push(path.clone());
        }

        let mut signals: Vec<StalenessSignal> = Vec::new();

        // Declared status.
        if matches!(
            fm.status,
            Some(DocStatus::Stale) | Some(DocStatus::Superseded) | Some(DocStatus::Archived)
        ) {
            signals.push(StalenessSignal::StatusStaleOrSuperseded);
        }

        // Version drift: declared-version-only + semver-aware. A doc drifts iff
        // its front-matter `doc-version` mismatches its governing anchor axis
        // (semver-normalized: `5.0.0` == `5.0`). `anchored-to: none` opts out; a
        // doc that declares no version does NOT drift (it gets `NoFrontMatter`
        // instead). Replaces the old body-token scan, which false-fired on
        // incidental version-like tokens and the `5.0.0` vs `5.0` format gap.
        if let (Some(kind), Some(stated)) = (governing(&fm), fm.doc_version.clone()) {
            let av = anchor_value(&kind, &anchor);
            if !version_eq_semver(&stated, &av) {
                signals.push(StalenessSignal::VersionMarkerDrift {
                    stated: stated.clone(),
                    anchor: av.clone(),
                });
                version_drift.push((path.clone(), stated, av));
            }
        }

        // Orphan (no inbound).
        if orphan_set.contains(path.as_str()) {
            signals.push(StalenessSignal::Orphan);
        }

        // Broken outbound links.
        if let Some(tgts) = broken_by_src.get(path.as_str()) {
            for t in tgts {
                signals.push(StalenessSignal::BrokenLink(t.clone()));
            }
        }

        // References to deleted (supersedes + superseded-by targets gone).
        let mut refs: Vec<String> = fm.supersedes.clone();
        if let Some(sb) = &fm.superseded_by {
            refs.push(sb.clone());
        }
        for r in refs {
            if !reference_exists(root, &graph, &r) {
                signals.push(StalenessSignal::ReferencesDeleted(r.clone()));
                references_to_deleted.push((path.clone(), r));
            }
        }

        // No front-matter (mild).
        if !fm.raw_present {
            signals.push(StalenessSignal::NoFrontMatter);
        }

        docs.insert(
            path.clone(),
            DocMeta {
                path: path.clone(),
                front_matter: fm,
                staleness: signals,
                is_front_door: is_fd,
            },
        );
    }

    // Reachability from front doors → unreachable set (richer than orphan).
    let reached = bfs_reachable(&graph, &front_doors);
    let mut unreachable_from_front_door: Vec<String> = Vec::new();
    for path in graph.docs.keys() {
        if !reached.contains(path.as_str()) {
            unreachable_from_front_door.push(path.clone());
            if let Some(dm) = docs.get_mut(path) {
                dm.staleness.push(StalenessSignal::Unreachable);
            }
        }
    }

    let stale_diagrams = build_stale_diagrams(root, excludes, &graph.docs);

    DocReport {
        graph,
        docs,
        anchor,
        front_doors,
        unreachable_from_front_door,
        references_to_deleted,
        version_drift,
        stale_diagrams,
    }
}

/// Tiered next-doc dispatcher mirroring `backlog.rs::next_ready`:
/// deterministic, single item, never invents filler. Tiers (rank):
/// 0 reconcile-front-door, 1 refresh-stale, 2 fix-broken-link,
/// 3 update-diagram, 4 cover-orphan, 5 idle. Ranking key
/// `(tier_rank, front_door_first, severity, anchor_distance, idx)`.
pub fn next_doc(report: &DocReport) -> serde_json::Value {
    type Key = (u8, u8, u8, u8, usize);

    struct Cand {
        key: Key,
        tier: &'static str,
        path: String,
        status: Option<String>,
        stated_version: Option<String>,
        anchor_version: Option<String>,
        is_front_door: bool,
        signals: Vec<String>,
        action: String,
    }

    let doc_index: HashMap<&str, usize> = report
        .docs
        .keys()
        .enumerate()
        .map(|(i, k)| (k.as_str(), i))
        .collect();

    let mut cands: Vec<Cand> = Vec::new();

    // Tiers 0/1 (status/drift) + tier 4 (orphan/unreachable) from per-doc signals.
    for (idx, (path, dm)) in report.docs.iter().enumerate() {
        let has_status = dm
            .staleness
            .iter()
            .any(|s| matches!(s, StalenessSignal::StatusStaleOrSuperseded));
        let drift = dm.staleness.iter().find_map(|s| match s {
            StalenessSignal::VersionMarkerDrift { stated, anchor } => {
                Some((stated.clone(), anchor.clone()))
            }
            _ => None,
        });
        let status_str = status_label(&dm.front_matter.status);
        let fdf = if dm.is_front_door { 0 } else { 1 };
        let labels: Vec<String> = dm.staleness.iter().map(signal_label).collect();

        if has_status || drift.is_some() {
            let tier_rank = if dm.is_front_door { 0u8 } else { 1u8 };
            let tier = if dm.is_front_door {
                "reconcile-front-door"
            } else {
                "refresh-stale"
            };
            // severity: superseded 0 < drift 1.
            let severity = if has_status { 0u8 } else { 1u8 };
            let anchor_distance = match &drift {
                Some((s, a)) => compute_anchor_distance(s, a),
                None => 2,
            };
            let action = if dm.is_front_door {
                format!("Reconcile front door `{path}` — align its status/version markers with the anchor.")
            } else {
                format!("Refresh `{path}` — update its status/version markers to the current anchor.")
            };
            cands.push(Cand {
                key: (tier_rank, fdf, severity, anchor_distance, idx),
                tier,
                path: path.clone(),
                status: status_str.clone(),
                stated_version: drift.as_ref().map(|(s, _)| s.clone()),
                anchor_version: drift.as_ref().map(|(_, a)| a.clone()),
                is_front_door: dm.is_front_door,
                signals: labels.clone(),
                action,
            });
        }

        let orphan = dm
            .staleness
            .iter()
            .any(|s| matches!(s, StalenessSignal::Orphan | StalenessSignal::Unreachable));
        if orphan {
            cands.push(Cand {
                key: (4, fdf, 3, 2, idx),
                tier: "cover-orphan",
                path: path.clone(),
                status: status_str,
                stated_version: None,
                anchor_version: None,
                is_front_door: dm.is_front_door,
                signals: labels,
                action: format!(
                    "Cover `{path}` — add an inbound link from a reachable doc so it joins a reading path."
                ),
            });
        }
    }

    // Tier 2 — broken links ∪ references-to-deleted.
    for (src, tgt) in report
        .graph
        .broken_links
        .iter()
        .chain(report.references_to_deleted.iter())
    {
        let idx = doc_index.get(src.as_str()).copied().unwrap_or(usize::MAX);
        let is_fd = report.docs.get(src).map(|d| d.is_front_door).unwrap_or(false);
        cands.push(Cand {
            key: (2, if is_fd { 0 } else { 1 }, 2, 2, idx),
            tier: "fix-broken-link",
            path: src.clone(),
            status: None,
            stated_version: None,
            anchor_version: None,
            is_front_door: is_fd,
            signals: vec![format!("broken-link({tgt})")],
            action: format!("Fix the broken reference `{tgt}` in `{src}` — repoint or remove it."),
        });
    }

    // Tier 3 — stale diagrams.
    for (i, sd) in report.stale_diagrams.iter().enumerate() {
        let label = diag_reason_label(&sd.reason);
        cands.push(Cand {
            key: (3, 1, 2, 2, i),
            tier: "update-diagram",
            path: sd.diagram.clone(),
            status: None,
            stated_version: None,
            anchor_version: None,
            is_front_door: false,
            signals: vec![label.clone()],
            action: format!("Update the diagram `{}` — {}.", sd.diagram, label),
        });
    }

    cands.sort_by(|a, b| a.key.cmp(&b.key));

    if let Some(c) = cands.first() {
        return serde_json::json!({
            "tier": c.tier,
            "ready": true,
            "doc": {
                "path": c.path,
                "status": c.status,
                "stated_version": c.stated_version,
                "anchor_version": c.anchor_version,
                "is_front_door": c.is_front_door,
                "signals": c.signals,
            },
            "rationale": {
                "tier_rank": c.key.0,
                "front_door": c.key.1 == 0,
                "severity": c.key.2,
                "anchor_distance": c.key.3,
            },
            "action": c.action,
            "explanation": format!(
                "Next doc: {} ({}). Deterministic top of the reconcile queue; the broker never invents filler.",
                c.path, c.tier
            ),
        });
    }

    // Tier 5 — idle (counts only; never a fabricated item).
    let broken_count = report.graph.broken_links.len() + report.references_to_deleted.len();
    let orphan_count = report
        .docs
        .values()
        .filter(|d| d.staleness.iter().any(|s| matches!(s, StalenessSignal::Orphan)))
        .count();
    let drift_count = report.version_drift.len();
    let stale_count = report
        .docs
        .values()
        .filter(|d| {
            d.staleness
                .iter()
                .any(|s| matches!(s, StalenessSignal::StatusStaleOrSuperseded))
        })
        .count();
    let unreachable_count = report.unreachable_from_front_door.len();
    let stale_diagram_count = report.stale_diagrams.len();
    serde_json::json!({
        "tier": "idle",
        "ready": false,
        "broken_count": broken_count,
        "orphan_count": orphan_count,
        "drift_count": drift_count,
        "stale_count": stale_count,
        "unreachable_count": unreachable_count,
        "stale_diagram_count": stale_diagram_count,
        "explanation": "Idle: all docs current, reachable, linked, and diagram-clean. The broker never invents filler.",
    })
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
    fn empty_project_returns_empty_report() {
        let dir = TempDir::new().unwrap();
        let report = build_graph(dir.path(), &[]);
        assert!(report.docs.is_empty());
        assert!(report.orphans.is_empty());
        assert!(report.broken_links.is_empty());
        assert!(report.cycles.is_empty());
    }

    #[test]
    fn extracts_outbound_links_via_pulldown_cmark() {
        let links = extract_link_targets("Hi [a](./other.md) and [b](https://example.com).");
        assert_eq!(links.len(), 2);
        assert!(links.contains(&"./other.md".to_string()));
        assert!(links.contains(&"https://example.com".to_string()));
    }

    #[test]
    fn ignores_links_inside_code_fences() {
        // pulldown-cmark handles fenced code blocks correctly; the
        // [link](inside.md) in the fence should not be extracted.
        let md = "```\n[link](inside.md)\n```\n\n[real](real.md)";
        let links = extract_link_targets(md);
        assert_eq!(links, vec!["real.md".to_string()]);
    }

    #[test]
    fn build_graph_resolves_relative_links_and_finds_orphans() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "README.md", "[guide](./guide.md)");
        write(dir.path(), "guide.md", "[index](./README.md)");
        write(dir.path(), "lonely.md", "no links here");
        let report = build_graph(dir.path(), &[]);
        assert_eq!(report.docs.len(), 3);
        // README is index-shaped → not an orphan even with 0 inbound.
        // guide has 1 inbound from README.
        // lonely has 0 inbound + non-index name → orphan.
        assert_eq!(report.orphans, vec!["lonely.md".to_string()]);
        assert!(report.broken_links.is_empty());
    }

    #[test]
    fn build_graph_flags_broken_markdown_links() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "README.md", "[gone](./missing.md)");
        let report = build_graph(dir.path(), &[]);
        assert_eq!(report.broken_links.len(), 1);
        assert_eq!(report.broken_links[0].0, "README.md");
        assert!(report.broken_links[0].1.ends_with("missing.md"));
    }

    #[test]
    fn build_graph_detects_two_node_cycle() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.md", "[b](./b.md)");
        write(dir.path(), "b.md", "[a](./a.md)");
        let report = build_graph(dir.path(), &[]);
        assert_eq!(report.cycles.len(), 1);
        let scc: BTreeSet<&str> = report.cycles[0].iter().map(|s| s.as_str()).collect();
        assert_eq!(scc.len(), 2);
        assert!(scc.contains("a.md"));
        assert!(scc.contains("b.md"));
    }

    #[test]
    fn build_graph_ignores_external_links_in_broken_count() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "README.md",
            "[good](./real.md)\n[external](https://example.com/missing.md)",
        );
        write(dir.path(), "real.md", "back");
        let report = build_graph(dir.path(), &[]);
        assert!(report.broken_links.is_empty());
    }

    #[test]
    fn skipped_dir_names_are_excluded() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "README.md", "root");
        write(dir.path(), "node_modules/some-pkg/README.md", "should not be scanned");
        write(dir.path(), "target/debug/notes.md", "ditto");
        let report = build_graph(dir.path(), &[]);
        assert_eq!(report.docs.len(), 1);
        assert!(report.docs.contains_key("README.md"));
    }

    // ── Layer A (doc-broker Phase 0) ────────────────────────────────

    #[test]
    fn parse_front_matter_valid() {
        let text = "\
---
doc-version: \"5.0\"
date: 2026-05-09
status: current
anchored-to: ecosystem
supersedes:
  - old/a.md
  - old/b.md
superseded-by: new/c.md
owner: keenan
front-door: true
---

# Body
";
        let fm = parse_front_matter(text);
        assert!(fm.raw_present);
        assert_eq!(fm.doc_version.as_deref(), Some("5.0"));
        assert_eq!(fm.status, Some(DocStatus::Current));
        assert_eq!(fm.anchored_to, Some(Anchor::Ecosystem));
        assert_eq!(fm.supersedes, vec!["old/a.md".to_string(), "old/b.md".to_string()]);
        assert_eq!(fm.superseded_by.as_deref(), Some("new/c.md"));
        assert!(fm.front_door);
    }

    #[test]
    fn parse_front_matter_absent_is_not_present() {
        let fm = parse_front_matter("# Just a heading\n\nNo front-matter here.\n");
        assert!(!fm.raw_present);
        assert_eq!(fm, FrontMatter::default());
    }

    #[test]
    fn parse_front_matter_malformed_never_panics() {
        // Bad YAML inside a fence → raw_present:false, no panic.
        let text = "---\n: : : not: valid: yaml: [unclosed\n---\nbody\n";
        let fm = parse_front_matter(text);
        assert!(!fm.raw_present);
        // An unknown enum value also fails gracefully.
        let bad_status = parse_front_matter("---\nstatus: nonsense\n---\nbody\n");
        assert!(!bad_status.raw_present);
    }

    #[test]
    fn exclude_prefix_and_vendor_skip_honored() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "README.md", "[g](./guide.md)");
        write(dir.path(), "guide.md", "body");
        write(dir.path(), "archive/x.md", "archived doc");
        write(dir.path(), "vendor/y.md", "vendored doc");
        let report = build_doc_report(dir.path(), default_anchor(), &default_doc_excludes());
        assert!(report.docs.contains_key("README.md"));
        assert!(report.docs.contains_key("guide.md"));
        // archive/ is an exclude-prefix; vendor/ is a dir-name skip.
        assert!(!report.docs.contains_key("archive/x.md"), "archive excluded");
        assert!(!report.docs.contains_key("vendor/y.md"), "vendor skipped");
    }

    #[test]
    fn version_drift_detected_and_sorts_top() {
        let dir = TempDir::new().unwrap();
        // README is a front door DECLARING doc-version 3.0 against a 5.0
        // ecosystem anchor (declared-version drift, major-version gap).
        write(
            dir.path(),
            "README.md",
            "---\ndoc-version: 3.0\nanchored-to: ecosystem\nfront-door: true\n---\n# Project\n",
        );
        write(dir.path(), "guide.md", "no version here, just prose");
        let report = build_doc_report(dir.path(), default_anchor(), &[]);
        let readme = &report.docs["README.md"];
        let drift = readme.staleness.iter().any(|s| {
            matches!(s, StalenessSignal::VersionMarkerDrift { stated, anchor } if stated == "3.0" && anchor == "5.0")
        });
        assert!(drift, "README doc-version 3.0 vs anchor 5.0 → drift; signals: {:?}", readme.staleness);
        assert!(report.version_drift.iter().any(|(p, s, a)| p == "README.md" && s == "3.0" && a == "5.0"));

        // next_doc dispatches it as the top tier (front-door reconcile, major gap).
        let d = next_doc(&report);
        assert_eq!(d["tier"], "reconcile-front-door");
        assert_eq!(d["doc"]["path"], "README.md");
        assert_eq!(d["rationale"]["anchor_distance"], 0, "major-version gap = closest distance");
        assert_eq!(d["ready"], true);
    }

    #[test]
    fn semver_normalized_no_false_drift() {
        // A front door declaring 5.0.0 must NOT drift against a 5.0 anchor
        // (semver-normalized), and a doc that declares NO version must not
        // drift regardless of incidental body tokens (declared-version-only).
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "README.md",
            "---\ndoc-version: 5.0.0\nanchored-to: ecosystem\nfront-door: true\n---\n# Project v1 legacy note\n",
        );
        write(
            dir.path(),
            "notes.md",
            "---\nanchored-to: ecosystem\n---\n# Notes\n\nmentions v2 and v3 incidentally\n",
        );
        let report = build_doc_report(dir.path(), default_anchor(), &[]);
        assert!(
            report.version_drift.is_empty(),
            "5.0.0==5.0 and undeclared-version docs must not drift; got {:?}",
            report.version_drift
        );
        assert!(version_eq_semver("5.0.0", "5.0"));
        assert!(version_eq_semver("v5", "5.0.0"));
        assert!(!version_eq_semver("3.2", "5.0"));
    }

    #[test]
    fn anchored_to_none_opts_out_of_drift() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "README.md",
            "---\nanchored-to: none\n---\n# Project\n\nThis is v3.0.\n",
        );
        let report = build_doc_report(dir.path(), default_anchor(), &[]);
        assert!(report.version_drift.is_empty(), "anchored-to:none opts out");
    }

    #[test]
    fn references_to_deleted_flagged() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "README.md",
            "---\nsupersedes:\n  - old.md\n---\n# New doc\n",
        );
        // old.md does not exist on disk.
        let report = build_doc_report(dir.path(), default_anchor(), &[]);
        assert!(report
            .references_to_deleted
            .iter()
            .any(|(p, t)| p == "README.md" && t == "old.md"));
        assert!(report.docs["README.md"]
            .staleness
            .iter()
            .any(|s| matches!(s, StalenessSignal::ReferencesDeleted(t) if t == "old.md")));
    }

    #[test]
    fn reachability_distinct_from_orphan() {
        let dir = TempDir::new().unwrap();
        // A front door that links nowhere relevant.
        write(dir.path(), "README.md", "the root");
        // An island pair: island_a links island_b (so island_b has inbound,
        // is NOT an orphan) but neither is reachable from README.
        write(dir.path(), "island_a.md", "[b](./island_b.md)");
        write(dir.path(), "island_b.md", "[a](./island_a.md)");
        let report = build_doc_report(dir.path(), default_anchor(), &[]);

        // island_b is NOT an orphan (island_a links it) ...
        assert!(!report.graph.orphans.contains(&"island_b.md".to_string()), "island_b has inbound");
        // ... but it IS unreachable from the front door.
        assert!(report.unreachable_from_front_door.contains(&"island_b.md".to_string()));
        assert!(report.docs["island_b.md"]
            .staleness
            .iter()
            .any(|s| matches!(s, StalenessSignal::Unreachable)));
    }

    #[test]
    fn stale_diagram_forbidden_term_and_pending_spec() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "README.md", "root");
        // A diagram still carrying the retired "Federation Broker" term.
        write(
            dir.path(),
            "docs/diagrams/broker-pattern.drawio.svg",
            "<svg><text>Federation Broker</text></svg>",
        );
        // A sibling V-spec marked PENDING (catches the stale diagram).
        write(
            dir.path(),
            "docs/diagrams/DIAGRAM-V4-SPEC.md",
            "# Diagram v4\n\nStatus: PENDING — not yet drawn.\n",
        );
        let report = build_doc_report(dir.path(), default_anchor(), &[]);
        let has_forbidden = report.stale_diagrams.iter().any(|d| {
            d.diagram == "docs/diagrams/broker-pattern.drawio.svg"
                && matches!(&d.reason, DiagReason::ForbiddenTermDrift(t) if t == "Federation Broker")
        });
        let has_pending = report.stale_diagrams.iter().any(|d| {
            d.diagram == "docs/diagrams/broker-pattern.drawio.svg"
                && matches!(&d.reason, DiagReason::PendingSpec(_))
        });
        assert!(has_forbidden, "forbidden-term drift: {:?}", report.stale_diagrams);
        assert!(has_pending, "pending-spec: {:?}", report.stale_diagrams);
    }

    #[test]
    fn missing_mmd_convention_flagged() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "README.md", "root");
        // A doc embedding a mermaid fence with no sibling .mmd.
        write(
            dir.path(),
            "docs/arch.md",
            "# Arch\n\n```mermaid\ngraph TD; A-->B;\n```\n",
        );
        let report = build_doc_report(dir.path(), default_anchor(), &[]);
        assert!(report.stale_diagrams.iter().any(|d| {
            d.diagram == "docs/arch.md" && matches!(d.reason, DiagReason::MissingFromMmdConvention)
        }));
    }

    #[test]
    fn next_doc_tiers_fire_in_priority_order() {
        // A front-door drift (tier 0) outranks an orphan (tier 4).
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "README.md",
            "---\ndoc-version: 3.0\nanchored-to: ecosystem\nfront-door: true\n---\n# Root\n\n[g](./guide.md)",
        );
        write(dir.path(), "guide.md", "linked, current");
        write(dir.path(), "stray.md", "orphan, no inbound");
        let report = build_doc_report(dir.path(), default_anchor(), &[]);
        let d = next_doc(&report);
        assert_eq!(d["tier"], "reconcile-front-door", "front-door reconcile beats orphan");

        // Remove the drift signal source → tier should fall to a lower-priority
        // surviving signal. With only the orphan/unreachable stray, expect cover-orphan.
        let dir2 = TempDir::new().unwrap();
        write(dir2.path(), "README.md", "# Root\n[g](./guide.md)");
        write(dir2.path(), "guide.md", "linked, current");
        write(dir2.path(), "stray.md", "orphan");
        let report2 = build_doc_report(dir2.path(), default_anchor(), &[]);
        let d2 = next_doc(&report2);
        assert_eq!(d2["tier"], "cover-orphan");
        assert_eq!(d2["doc"]["path"], "stray.md");
    }

    #[test]
    fn next_doc_fix_broken_link_tier() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "README.md", "[gone](./missing.md)");
        let report = build_doc_report(dir.path(), default_anchor(), &[]);
        let d = next_doc(&report);
        assert_eq!(d["tier"], "fix-broken-link");
        assert_eq!(d["doc"]["path"], "README.md");
    }

    #[test]
    fn next_doc_idle_when_all_current_never_invents_filler() {
        let dir = TempDir::new().unwrap();
        // README front door → guide → back. Both current, reachable, linked.
        write(dir.path(), "README.md", "[guide](./guide.md)");
        write(dir.path(), "guide.md", "[home](./README.md)");
        let report = build_doc_report(dir.path(), default_anchor(), &[]);
        let d = next_doc(&report);
        assert_eq!(d["tier"], "idle");
        assert_eq!(d["ready"], false, "idle is never a fabricated item");
        assert!(d.get("doc").is_none(), "idle returns counts, not a doc");
        assert_eq!(d["broken_count"], 0);
        assert_eq!(d["orphan_count"], 0);
        assert_eq!(d["drift_count"], 0);
        assert!(d["explanation"].as_str().unwrap().contains("never invents filler"));
    }

    #[tokio::test]
    async fn analyze_documentation_graph_enriched_envelope() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "README.md",
            "---\ndoc-version: 3.0\nanchored-to: ecosystem\nfront-door: true\n---\n# Root\n",
        );
        let cmdb = analyze_documentation_graph(&dir.path().to_string_lossy()).await;
        assert_eq!(cmdb["meta"]["updated_by"], "check-documentation-graph");
        assert!(cmdb["score"].is_number());
        // The enriched extras are present.
        assert_eq!(cmdb["front_door_drift_count"], 1);
        assert!(cmdb["stale_diagram_count"].is_number());
        assert!(cmdb["unreachable_count"].is_number());
    }
}
