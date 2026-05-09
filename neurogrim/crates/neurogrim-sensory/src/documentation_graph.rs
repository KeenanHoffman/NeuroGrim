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
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
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
    let report = build_graph(root);
    let total_docs = report.docs.len();
    let total_edges: usize = report.docs.values().map(|n| n.outbound.len()).sum();
    let orphan_count = report.orphans.len();
    let broken_count = report.broken_links.len();
    let cycle_count = report.cycles.len();

    // Score blend (see module-level doc).
    let orphan_ratio = if total_docs == 0 {
        0.0
    } else {
        orphan_count as f64 / total_docs as f64
    };
    let orphan_penalty = (orphan_ratio * 30.0).min(30.0);
    let broken_penalty = ((broken_count as f64) * 5.0).min(40.0);
    let cycle_penalty = ((cycle_count as f64) * 2.0).min(10.0);
    let score: i32 = (100.0 - orphan_penalty - broken_penalty - cycle_penalty)
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
                report.orphans.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
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
                report
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

    let extras: Vec<(&str, Value)> = vec![
        ("total_docs", Value::Number(total_docs.into())),
        ("total_edges", Value::Number(total_edges.into())),
        ("orphan_count", Value::Number(orphan_count.into())),
        ("broken_link_count", Value::Number(broken_count.into())),
        ("cycle_count", Value::Number(cycle_count.into())),
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
];

pub fn build_graph(root: &Path) -> GraphReport {
    let mut docs: BTreeMap<String, DocNode> = BTreeMap::new();
    let mut all_paths: BTreeSet<String> = BTreeSet::new();

    // First pass — collect every *.md path (project-relative,
    // forward-slash form) so we can resolve link targets.
    walk_markdown(root, root, &mut |abs_path| {
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
                // Looks like a markdown link but target is missing.
                broken.push(resolved);
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

fn walk_markdown(root: &Path, dir: &Path, visit: &mut impl FnMut(&Path)) {
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
            walk_markdown(root, &path, visit);
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
        let report = build_graph(dir.path());
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
        let report = build_graph(dir.path());
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
        let report = build_graph(dir.path());
        assert_eq!(report.broken_links.len(), 1);
        assert_eq!(report.broken_links[0].0, "README.md");
        assert!(report.broken_links[0].1.ends_with("missing.md"));
    }

    #[test]
    fn build_graph_detects_two_node_cycle() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.md", "[b](./b.md)");
        write(dir.path(), "b.md", "[a](./a.md)");
        let report = build_graph(dir.path());
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
        let report = build_graph(dir.path());
        assert!(report.broken_links.is_empty());
    }

    #[test]
    fn skipped_dir_names_are_excluded() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "README.md", "root");
        write(dir.path(), "node_modules/some-pkg/README.md", "should not be scanned");
        write(dir.path(), "target/debug/notes.md", "ditto");
        let report = build_graph(dir.path());
        assert_eq!(report.docs.len(), 1);
        assert!(report.docs.contains_key("README.md"));
    }
}
