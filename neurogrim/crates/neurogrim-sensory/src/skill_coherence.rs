//! Skill coherence sensory tool — cross-Brain skill byte-equality detector.
//!
//! B-11 (2026-04-22, contracted scope: drift-detection only, no central defn).
//!
//! Scans `.claude/skills/*.md` in the current Brain and in automatically-
//! discovered sibling/child Brains, and reports whether skill files that
//! share a basename across Brains are byte-identical or have drifted.
//!
//! Scoring:
//!   - Start at 100.
//!   - For each duplicated basename that has drifted (bytes differ across
//!     Brains), subtract 10 points. Cap at 0.
//!   - If no duplications exist at all, score is 100 (no-op).
//!
//! Discovery:
//!   - `{project_root}/.claude/skills/` is the self corpus.
//!   - `{project_root}/..` is probed for a `.claude/skills/` directory; if
//!     present, treated as a sibling.
//!   - Other directories under the parent (e.g., `D:/Brains/NeuroGrim`,
//!     `D:/Brains/LSP-Brains`) are scanned; any containing `.claude/skills/`
//!     is added.
//!   - Directories directly under `project_root` (e.g.,
//!     `NeuroGrim-python-starter/`) are scanned the same way.
//!
//! Findings (one per drifted basename) record which Brains disagreed.

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SkillCoherenceServer {
    // rmcp #[tool_router] macro accesses this through generated dispatch — rustc can't see the uses
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl SkillCoherenceServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckSkillCoherenceParams {
    pub project_root: String,
}

#[tool_router]
impl SkillCoherenceServer {
    #[tool(
        description = "Check skill coherence: scans .claude/skills/*.md across the \
        current Brain and auto-discovered sibling/child Brains; reports whether skill \
        files with the same basename are byte-identical or have drifted. Returns \
        CMDB-envelope JSON with per-drift findings."
    )]
    async fn check_skill_coherence(
        &self,
        Parameters(p): Parameters<CheckSkillCoherenceParams>,
    ) -> String {
        serde_json::to_string_pretty(&analyze_skill_coherence(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for SkillCoherenceServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Skill coherence sensory tool. Detects byte-level drift between \
                duplicated skill files across Brains in a fractal-composition \
                topology."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ── Public analysis entry point ───────────────────────────────────────────────

pub async fn analyze_skill_coherence(project_root: &str) -> Value {
    // Canonicalize so `.` and other relative paths resolve to an absolute
    // path before we start walking `.parent()`. On Windows especially,
    // `PathBuf::from(".").parent()` returns `Some("")`, which breaks the
    // sibling scan. Fall back to the raw path if canonicalize fails (e.g.,
    // the dir genuinely doesn't exist — the tool should still produce a
    // CMDB with 0 brains).
    let root_raw = PathBuf::from(project_root);
    let root = root_raw.canonicalize().unwrap_or(root_raw);
    let brain_roots = discover_brain_roots(&root);

    // brain_id → basename → bytes
    let mut by_brain: BTreeMap<String, BTreeMap<String, Vec<u8>>> = BTreeMap::new();
    for brain in &brain_roots {
        let id = brain_id_from_path(&brain.path, &root);
        let skills = load_skills(&brain.path);
        if !skills.is_empty() {
            by_brain.insert(id, skills);
        }
    }

    // basename → list of (brain_id, bytes)
    let mut by_basename: BTreeMap<String, Vec<(String, Vec<u8>)>> = BTreeMap::new();
    for (brain_id, skills) in &by_brain {
        for (basename, bytes) in skills {
            by_basename
                .entry(basename.clone())
                .or_default()
                .push((brain_id.clone(), bytes.clone()));
        }
    }

    let mut findings: Vec<Finding> = Vec::new();
    let mut total_duplicated = 0usize;
    let mut in_sync = 0usize;
    let mut drift_count = 0usize;
    let mut drift_details: Vec<Value> = Vec::new();

    for (basename, copies) in &by_basename {
        if copies.len() < 2 {
            continue;
        }
        total_duplicated += 1;
        let first_bytes = &copies[0].1;
        let all_match = copies.iter().all(|(_, b)| b == first_bytes);
        if all_match {
            in_sync += 1;
        } else {
            drift_count += 1;
            let brain_ids: Vec<&str> = copies.iter().map(|(id, _)| id.as_str()).collect();
            findings.push(Finding {
                name: format!("drift:{basename}"),
                status: "drift".to_string(),
                points: -10,
                detail: Some(format!(
                    "`{basename}` differs across {} brain(s): {}",
                    copies.len(),
                    brain_ids.join(", ")
                )),
            });
            drift_details.push(json!({
                "basename": basename,
                "present_in": brain_ids,
                "bytes_match": false,
            }));
        }
    }

    // Score: 100 baseline, -10 per drift incident; floor at 0.
    let deduction: i32 = (drift_count as i32) * 10;
    let score: u8 = (100i32).saturating_sub(deduction).max(0) as u8;

    // Add a "no-drift" finding when everything is clean (or nothing to sync).
    if drift_count == 0 && total_duplicated > 0 {
        findings.push(Finding {
            name: "all-duplicates-in-sync".to_string(),
            status: "ok".to_string(),
            points: 0,
            detail: Some(format!(
                "{total_duplicated} duplicated basename(s) across {} brain(s); all byte-identical.",
                by_brain.len()
            )),
        });
    } else if total_duplicated == 0 {
        findings.push(Finding {
            name: "no-duplicated-skills".to_string(),
            status: "n/a".to_string(),
            points: 0,
            detail: Some(format!(
                "{} brain(s) discovered; no skill basenames appear in multiple brains.",
                by_brain.len()
            )),
        });
    }

    let brain_paths: Vec<Value> = brain_roots
        .iter()
        .map(|b| {
            json!({
                "id": brain_id_from_path(&b.path, &root),
                "path": b.path.to_string_lossy(),
                "skill_count": by_brain
                    .get(&brain_id_from_path(&b.path, &root))
                    .map(|m| m.len())
                    .unwrap_or(0),
            })
        })
        .collect();

    let extras: Vec<(&str, Value)> = vec![
        ("brains_discovered", json!(by_brain.len())),
        ("brain_paths", json!(brain_paths)),
        ("total_duplicated_basenames", json!(total_duplicated)),
        ("in_sync_count", json!(in_sync)),
        ("drift_count", json!(drift_count)),
        ("drift_details", json!(drift_details)),
    ];

    build_cmdb("skill-coherence", score, findings, Some(extras), None)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct BrainRootCandidate {
    path: PathBuf,
}

/// Discover Brain roots relative to `project_root`. A "Brain root" is any
/// directory containing a `.claude/skills/` directory. The returned list
/// always starts with `project_root` itself (if it's a Brain) and then
/// includes sibling directories under the parent and child directories
/// directly under `project_root`.
fn discover_brain_roots(project_root: &Path) -> Vec<BrainRootCandidate> {
    let mut seen: BTreeMap<PathBuf, ()> = BTreeMap::new();
    let mut out: Vec<BrainRootCandidate> = Vec::new();

    let self_skills = project_root.join(".claude").join("skills");
    if self_skills.is_dir() {
        let canon = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        seen.insert(canon.clone(), ());
        out.push(BrainRootCandidate {
            path: project_root.to_path_buf(),
        });
    }

    // Parent + siblings under the parent.
    if let Some(parent) = project_root.parent() {
        add_if_brain_root(parent, &mut seen, &mut out);
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    add_if_brain_root(&p, &mut seen, &mut out);
                }
            }
        }
    }

    // Children directly under project_root (e.g. submodule Brains).
    if let Ok(entries) = std::fs::read_dir(project_root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                add_if_brain_root(&p, &mut seen, &mut out);
            }
        }
    }

    out
}

fn add_if_brain_root(
    candidate: &Path,
    seen: &mut BTreeMap<PathBuf, ()>,
    out: &mut Vec<BrainRootCandidate>,
) {
    let skills = candidate.join(".claude").join("skills");
    if !skills.is_dir() {
        return;
    }
    let canon = candidate
        .canonicalize()
        .unwrap_or_else(|_| candidate.to_path_buf());
    if seen.contains_key(&canon) {
        return;
    }
    seen.insert(canon, ());
    out.push(BrainRootCandidate {
        path: candidate.to_path_buf(),
    });
}

/// Human-friendly brain id derived from the final path segment; falls back to
/// the canonicalized path string.
fn brain_id_from_path(brain_root: &Path, invocation_root: &Path) -> String {
    let canon = brain_root
        .canonicalize()
        .unwrap_or_else(|_| brain_root.to_path_buf());
    let canon_invocation = invocation_root
        .canonicalize()
        .unwrap_or_else(|_| invocation_root.to_path_buf());
    if canon == canon_invocation {
        return "self".to_string();
    }
    brain_root
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| brain_root.to_string_lossy().to_string())
}

/// Load all skill entries under `{brain_root}/.claude/skills/`.
///
/// Finds TWO patterns (2026-04-22, Tier A partial — supports migration from
/// legacy format to SKILL.md without a big-bang switch):
///
/// 1. **Legacy:** `.claude/skills/<name>.md` direct descendants.
///    Key in the returned map: `<name>.md` (unchanged from original
///    behavior, so existing ledgers + comparisons keep working).
///
/// 2. **Plugin:** `.claude/skills/<name>/SKILL.md`.
///    Key in the returned map: `<name>/SKILL.md` (directory-prefixed so
///    drift detection compares SKILL.md to SKILL.md across Brains, not
///    against legacy single-file entries).
///
/// Cross-format drift (one Brain with `rubber-duck.md`, another with
/// `rubber-duck/SKILL.md`) is NOT detected by byte comparison — that's
/// expected migration churn, not a coherence problem. When migration
/// completes across all Brains to the same format, comparisons work
/// cleanly again.
///
/// Skips README, dotfiles, and `archived/` subdirectory entirely.
fn load_skills(brain_root: &Path) -> BTreeMap<String, Vec<u8>> {
    let dir = brain_root.join(".claude").join("skills");
    let mut out = BTreeMap::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Skip README, dotfiles, and the archived/ subdirectory.
        if name.starts_with("README") || name.starts_with('.') || name == "archived" {
            continue;
        }

        if path.is_file() {
            // Legacy pattern: direct-descendant `.md` file.
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path) {
                out.insert(name, bytes);
            }
        } else if path.is_dir() {
            // Plugin pattern: `<name>/SKILL.md` (the Claude Code modern
            // discovery pattern — see docs/invocation-ledger.md).
            let skill_md = path.join("SKILL.md");
            if skill_md.is_file() {
                if let Ok(bytes) = std::fs::read(&skill_md) {
                    let key = format!("{name}/SKILL.md");
                    out.insert(key, bytes);
                }
            }
        }
    }
    out
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

    /// Wrap the test in a dedicated parent tempdir so the sibling-scan
    /// logic (`discover_brain_roots` reads the parent of project_root)
    /// doesn't pick up unrelated brain directories created by other
    /// tests running in the shared OS temp space.
    fn isolated_brain(tag: &str) -> (TempDir, PathBuf) {
        let parent = TempDir::new().unwrap();
        let brain = parent.path().join(tag);
        std::fs::create_dir_all(&brain).unwrap();
        (parent, brain)
    }

    #[tokio::test]
    async fn empty_project_yields_score_100() {
        let (_parent, brain) = isolated_brain("empty");
        let result = analyze_skill_coherence(brain.to_str().unwrap()).await;
        assert_eq!(result["score"], 100);
        assert_eq!(result["brains_discovered"], 0);
    }

    #[tokio::test]
    async fn single_brain_no_duplicates_yields_score_100() {
        let (_parent, brain) = isolated_brain("single");
        write_skill(&brain, "rubber-duck.md", "# Rubber Duck\n\nbody");
        let result = analyze_skill_coherence(brain.to_str().unwrap()).await;
        assert_eq!(result["score"], 100);
        assert_eq!(result["total_duplicated_basenames"], 0);
        assert_eq!(result["drift_count"], 0);
    }

    #[tokio::test]
    async fn identical_duplicates_across_siblings_score_100() {
        // Layout:
        //   <parent>/A/.claude/skills/rubber-duck.md
        //   <parent>/B/.claude/skills/rubber-duck.md  (same bytes)
        let parent = TempDir::new().unwrap();
        let a = parent.path().join("A");
        let b = parent.path().join("B");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let body = "# Rubber Duck\n\nshared body";
        write_skill(&a, "rubber-duck.md", body);
        write_skill(&b, "rubber-duck.md", body);

        let result = analyze_skill_coherence(a.to_str().unwrap()).await;
        assert_eq!(result["score"], 100);
        assert_eq!(result["total_duplicated_basenames"], 1);
        assert_eq!(result["in_sync_count"], 1);
        assert_eq!(result["drift_count"], 0);
    }

    #[tokio::test]
    async fn drifted_duplicate_deducts_10_points() {
        let parent = TempDir::new().unwrap();
        let a = parent.path().join("A");
        let b = parent.path().join("B");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        write_skill(&a, "rubber-duck.md", "# Rubber Duck\n\nversion-A");
        write_skill(&b, "rubber-duck.md", "# Rubber Duck\n\nversion-B-DRIFT");

        let result = analyze_skill_coherence(a.to_str().unwrap()).await;
        assert_eq!(result["score"], 90);
        assert_eq!(result["drift_count"], 1);
        let findings = result["findings"].as_array().unwrap();
        assert!(findings.iter().any(|f| f["name"] == "drift:rubber-duck.md"));
    }

    #[tokio::test]
    async fn multiple_drifts_compound_deduction() {
        let parent = TempDir::new().unwrap();
        let a = parent.path().join("A");
        let b = parent.path().join("B");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        write_skill(&a, "one.md", "# One\n\nversion-A");
        write_skill(&b, "one.md", "# One\n\nversion-B");
        write_skill(&a, "two.md", "# Two\n\nversion-A");
        write_skill(&b, "two.md", "# Two\n\nversion-B");

        let result = analyze_skill_coherence(a.to_str().unwrap()).await;
        assert_eq!(result["score"], 80);
        assert_eq!(result["drift_count"], 2);
    }

    #[tokio::test]
    async fn child_brains_are_discovered() {
        // Isolated layout:
        //   <parent>/root/.claude/skills/a.md
        //   <parent>/root/child/.claude/skills/a.md
        // (No siblings under <parent> other than `root`, so discovery
        // finds exactly 2 brains: self + child.)
        let (_parent, root) = isolated_brain("root");
        let child = root.join("child");
        std::fs::create_dir_all(&child).unwrap();
        write_skill(&root, "a.md", "# A");
        write_skill(&child, "a.md", "# A");

        let result = analyze_skill_coherence(root.to_str().unwrap()).await;
        assert_eq!(result["brains_discovered"], 2);
        assert_eq!(result["drift_count"], 0);
        assert_eq!(result["score"], 100);
    }

    /// Helper: create a SKILL.md inside `<brain_root>/.claude/skills/<name>/`.
    fn write_skill_md(brain_root: &Path, skill_name: &str, body: &str) {
        let dir = brain_root
            .join(".claude")
            .join("skills")
            .join(skill_name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    #[tokio::test]
    async fn skill_md_plugin_format_is_discovered() {
        // Tier A partial (2026-04-22): the scanner must find
        // `.claude/skills/<name>/SKILL.md` in addition to legacy
        // `.claude/skills/<name>.md`.
        let parent = TempDir::new().unwrap();
        let a = parent.path().join("A");
        let b = parent.path().join("B");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let body = "---\nname: rubber-duck\ndescription: shared\n---\nbody";
        write_skill_md(&a, "rubber-duck", body);
        write_skill_md(&b, "rubber-duck", body);

        let result = analyze_skill_coherence(a.to_str().unwrap()).await;
        assert_eq!(result["score"], 100);
        assert_eq!(result["total_duplicated_basenames"], 1);
        assert_eq!(result["in_sync_count"], 1);
        assert_eq!(result["drift_count"], 0);
    }

    #[tokio::test]
    async fn skill_md_drift_between_brains_is_detected() {
        let parent = TempDir::new().unwrap();
        let a = parent.path().join("A");
        let b = parent.path().join("B");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        write_skill_md(&a, "rubber-duck", "---\nname: rubber-duck\n---\nA-body");
        write_skill_md(&b, "rubber-duck", "---\nname: rubber-duck\n---\nB-body-DRIFT");

        let result = analyze_skill_coherence(a.to_str().unwrap()).await;
        assert_eq!(result["drift_count"], 1);
        assert_eq!(result["score"], 90);
        let findings = result["findings"].as_array().unwrap();
        assert!(findings.iter().any(|f| {
            f["name"].as_str() == Some("drift:rubber-duck/SKILL.md")
        }), "expected SKILL.md drift finding, got: {:?}", findings);
    }

    #[tokio::test]
    async fn mixed_legacy_and_plugin_formats_coexist() {
        // One Brain has skills in both formats simultaneously; both
        // should be discovered and counted. No drift (each file is
        // unique across Brains).
        let (_parent, root) = isolated_brain("mixed");
        write_skill(&root, "legacy-one.md", "# Legacy\n\nbody");
        write_skill_md(&root, "plugin-one", "---\nname: plugin-one\n---\nbody");

        let result = analyze_skill_coherence(root.to_str().unwrap()).await;
        // Single-brain scan with no siblings → no duplications, but both
        // skills are counted as "discovered" via the self entry.
        assert_eq!(result["brains_discovered"], 1);
        assert_eq!(result["total_duplicated_basenames"], 0);
        assert_eq!(result["score"], 100);

        // Inspect the self Brain's skill count via the brain_paths field.
        let paths = result["brain_paths"].as_array().unwrap();
        let self_brain = paths
            .iter()
            .find(|b| b["id"] == "self")
            .expect("self brain entry");
        assert_eq!(self_brain["skill_count"], 2);
    }
}
