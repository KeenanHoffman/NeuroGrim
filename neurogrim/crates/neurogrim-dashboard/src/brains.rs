//! Multi-Brain discovery — walks `config.children` transitively from
//! the host registry to build a map of every Brain reachable from
//! this dashboard.
//!
//! ## Why
//!
//! Path 2 of the v3.4 dashboard architecture: the operator runs ONE
//! `neurogrim ui` server, but can navigate from the host Brain into
//! any of its children (and grandchildren) without spinning up
//! additional servers. The federation page becomes a navigation
//! map; the per-Brain Overview shows the *opinionated* score for
//! that Brain even when the host is all-advisory.
//!
//! ## Brain IDs
//!
//! Each Brain has a stable kebab-case id derived from
//! `meta.project` (preferred) or, failing that, the project_root
//! basename. IDs are guaranteed unique across the discovered tree
//! by suffixing with parent context on collision.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One Brain reachable from the host. The ID is the URL-routing
/// handle (`/brains/<id>/...`); the registry_path is what
/// BrainContext::load reads.
#[derive(Debug, Clone)]
pub struct BrainEntry {
    pub id: String,
    pub display_name: String,
    pub registry_path: PathBuf,
    pub project_root: PathBuf,
    /// `None` for the host Brain; otherwise the id of the Brain
    /// whose `config.children` declared this entry.
    pub parent_id: Option<String>,
    /// 0 for the host, 1 for direct children, 2 for grandchildren, etc.
    pub depth: usize,
}

/// Discovery result — host + all transitively-reachable Brains.
#[derive(Debug, Clone)]
pub struct BrainTree {
    pub self_id: String,
    pub entries: HashMap<String, BrainEntry>,
}

impl BrainTree {
    /// Walk from the host registry, reading each child's registry on
    /// disk to recurse. Cycle-guarded via canonical-path visited set
    /// (the same registry reached via two different paths counts as
    /// one Brain).
    ///
    /// Failures along the walk are non-fatal — a missing or unreadable
    /// child registry is logged and the walk continues with the rest.
    /// We always return at least the host Brain.
    pub fn discover(self_registry_path: &Path) -> Self {
        let mut entries: HashMap<String, BrainEntry> = HashMap::new();
        let mut visited: HashSet<PathBuf> = HashSet::new();

        let host_canonical = std::fs::canonicalize(self_registry_path)
            .unwrap_or_else(|_| self_registry_path.to_path_buf());
        visited.insert(host_canonical.clone());

        let host_entry = match read_brain(&host_canonical, None, 0) {
            Some(e) => e,
            None => {
                // Couldn't read the host registry. Return a minimal
                // BrainTree with a synthetic self entry so the rest
                // of the dashboard still works.
                let id = "self".to_string();
                let entry = BrainEntry {
                    id: id.clone(),
                    display_name: "Self".to_string(),
                    registry_path: self_registry_path.to_path_buf(),
                    project_root: self_registry_path
                        .parent()
                        .and_then(Path::parent)
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| PathBuf::from(".")),
                    parent_id: None,
                    depth: 0,
                };
                entries.insert(id.clone(), entry);
                return BrainTree {
                    self_id: id,
                    entries,
                };
            }
        };

        let self_id = host_entry.id.clone();
        let project_root = host_entry.project_root.clone();
        entries.insert(self_id.clone(), host_entry);

        // Walk children breadth-first so depth assignments are
        // correct even when the same Brain is reachable via
        // multiple paths.
        let mut queue: Vec<(String, PathBuf, usize)> = vec![(
            self_id.clone(),
            project_root,
            0,
        )];

        while let Some((parent_id, parent_root, parent_depth)) = queue.pop() {
            let parent_registry = parent_root.join(".claude").join("brain-registry.json");
            let raw = match std::fs::read_to_string(&parent_registry) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let parsed: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let children = match parsed
                .get("config")
                .and_then(|c| c.get("children"))
                .and_then(|c| c.as_object())
            {
                Some(c) => c,
                None => continue,
            };

            for (child_key, child_body) in children {
                let brain_path = match child_body
                    .get("brain_path")
                    .and_then(|v| v.as_str())
                {
                    Some(p) => p,
                    None => continue,
                };
                // Resolve child path: relative to parent's project_root,
                // unless it's already absolute.
                let candidate = PathBuf::from(brain_path);
                let child_root = if candidate.is_absolute() {
                    candidate
                } else {
                    parent_root.join(&candidate)
                };
                let child_registry = child_root.join(".claude").join("brain-registry.json");
                let canonical = match std::fs::canonicalize(&child_registry) {
                    Ok(c) => c,
                    Err(_) => {
                        // Child registry doesn't exist on disk — record
                        // the declared entry but skip walking it.
                        let id = unique_id(child_key, &entries);
                        let display = child_body
                            .get("display_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(child_key)
                            .to_string();
                        let entry = BrainEntry {
                            id: id.clone(),
                            display_name: display,
                            registry_path: child_registry.clone(),
                            project_root: child_root.clone(),
                            parent_id: Some(parent_id.clone()),
                            depth: parent_depth + 1,
                        };
                        entries.insert(id, entry);
                        continue;
                    }
                };
                if visited.contains(&canonical) {
                    continue;
                }
                visited.insert(canonical.clone());

                let entry = match read_brain(
                    &canonical,
                    Some(parent_id.clone()),
                    parent_depth + 1,
                ) {
                    Some(mut e) => {
                        // Override id if it collides with an existing
                        // entry: prefer the child_key from the parent's
                        // registry, suffixed with parent if needed.
                        let preferred = unique_id(&e.id, &entries);
                        if preferred != e.id {
                            e.id = preferred;
                        }
                        e
                    }
                    None => continue,
                };
                let next_root = entry.project_root.clone();
                let next_id = entry.id.clone();
                entries.insert(entry.id.clone(), entry);
                queue.push((next_id, next_root, parent_depth + 1));
            }
        }

        BrainTree { self_id, entries }
    }

    pub fn get(&self, id: &str) -> Option<&BrainEntry> {
        self.entries.get(id)
    }

    /// Returns entries sorted: self first, then by depth + id for
    /// deterministic display order.
    pub fn list(&self) -> Vec<&BrainEntry> {
        let mut v: Vec<&BrainEntry> = self.entries.values().collect();
        v.sort_by(|a, b| {
            // self first (depth 0); then by depth, then by id
            (a.depth, &a.id).cmp(&(b.depth, &b.id))
        });
        v
    }

    /// Convenience for the SSE filesystem watcher: every Brain's
    /// project_root, deduplicated and absolute.
    pub fn watch_roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = self
            .entries
            .values()
            .map(|e| e.project_root.clone())
            .collect();
        roots.sort();
        roots.dedup();
        roots
    }
}

/// Read a single Brain registry and produce a BrainEntry. Returns
/// None on read/parse failure (caller decides whether to skip or
/// substitute).
fn read_brain(
    registry_path: &Path,
    parent_id: Option<String>,
    depth: usize,
) -> Option<BrainEntry> {
    let raw = std::fs::read_to_string(registry_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let project_root = registry_path
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let id = derive_id(&parsed, &project_root);
    let display_name = parsed
        .get("meta")
        .and_then(|m| m.get("project"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| id.clone());
    Some(BrainEntry {
        id,
        display_name,
        registry_path: registry_path.to_path_buf(),
        project_root,
        parent_id,
        depth,
    })
}

/// Derive a kebab-case id from `meta.project` (preferred) or the
/// project_root basename.
fn derive_id(registry: &serde_json::Value, project_root: &Path) -> String {
    let raw = registry
        .get("meta")
        .and_then(|m| m.get("project"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| {
            project_root
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("brain")
                .to_string()
        });
    slugify(&raw)
}

/// Lower-case ASCII slugify: alphanumeric + dash, runs of non-allowed
/// characters collapsed to a single dash, leading/trailing dashes
/// stripped.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "brain".to_string()
    } else {
        out
    }
}

/// If `id` is already in `entries`, suffix with `-N` until unique.
fn unique_id(id: &str, entries: &HashMap<String, BrainEntry>) -> String {
    if !entries.contains_key(id) {
        return id.to_string();
    }
    let mut i = 2;
    loop {
        let candidate = format!("{id}-{i}");
        if !entries.contains_key(&candidate) {
            return candidate;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_handles_common_cases() {
        assert_eq!(slugify("LSP-Brains-Ecosystem"), "lsp-brains-ecosystem");
        assert_eq!(slugify("NeuroGrim"), "neurogrim");
        assert_eq!(slugify("Hello World 2026"), "hello-world-2026");
        assert_eq!(slugify("  --  "), "brain");
        assert_eq!(slugify(""), "brain");
    }

    fn write_registry(dir: &Path, project: &str, children: &[(&str, &str)]) {
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        let children_obj = if children.is_empty() {
            String::new()
        } else {
            let entries: Vec<String> = children
                .iter()
                .map(|(name, path)| {
                    format!(
                        r#""{name}":{{"brain_path":"{path}","interface_version":"1","weight":1.0,"enabled":true}}"#
                    )
                })
                .collect();
            format!(r#","children":{{{}}}"#, entries.join(","))
        };
        let json = format!(
            r#"{{
              "meta": {{"schema_version":"2","description":"t","updated_by":"t","project":"{project}"}},
              "tools":{{}},"data_sources":{{}},
              "config":{{
                "domain_weights":{{"d":0.0}},
                "domain_definitions":{{"d":{{"principle":"x","scoring_source":null,"exported_variables":{{}}}}}}
                {children_obj}
              }}
            }}"#
        );
        std::fs::write(dir.join(".claude/brain-registry.json"), json).unwrap();
    }

    #[test]
    fn discovers_host_only_when_no_children() {
        let tmp = tempfile::tempdir().unwrap();
        write_registry(tmp.path(), "Solo", &[]);
        let tree = BrainTree::discover(&tmp.path().join(".claude/brain-registry.json"));
        assert_eq!(tree.entries.len(), 1);
        assert_eq!(tree.self_id, "solo");
        assert_eq!(tree.entries[&tree.self_id].depth, 0);
        assert_eq!(tree.entries[&tree.self_id].parent_id, None);
    }

    #[test]
    fn discovers_direct_children() {
        let tmp = tempfile::tempdir().unwrap();
        let host = tmp.path();
        let child_a = host.join("a");
        let child_b = host.join("b");
        std::fs::create_dir_all(&child_a).unwrap();
        std::fs::create_dir_all(&child_b).unwrap();
        write_registry(&child_a, "Alpha", &[]);
        write_registry(&child_b, "Bravo", &[]);
        write_registry(host, "Host", &[("alpha", "a"), ("bravo", "b")]);

        let tree = BrainTree::discover(&host.join(".claude/brain-registry.json"));
        assert_eq!(tree.entries.len(), 3);
        assert!(tree.entries.contains_key("host"));
        assert!(tree.entries.contains_key("alpha"));
        assert!(tree.entries.contains_key("bravo"));
        assert_eq!(tree.entries["alpha"].depth, 1);
        assert_eq!(tree.entries["alpha"].parent_id.as_deref(), Some("host"));
    }

    #[test]
    fn discovers_grandchildren_through_recursion() {
        let tmp = tempfile::tempdir().unwrap();
        let host = tmp.path();
        let child = host.join("child");
        let grand = child.join("grand");
        std::fs::create_dir_all(&grand).unwrap();
        write_registry(&grand, "Grand", &[]);
        write_registry(&child, "Child", &[("grand", "grand")]);
        write_registry(host, "Host", &[("child", "child")]);

        let tree = BrainTree::discover(&host.join(".claude/brain-registry.json"));
        assert_eq!(tree.entries.len(), 3);
        let g = &tree.entries["grand"];
        assert_eq!(g.depth, 2);
        assert_eq!(g.parent_id.as_deref(), Some("child"));
    }

    #[test]
    fn list_orders_self_first_then_by_depth() {
        let tmp = tempfile::tempdir().unwrap();
        let host = tmp.path();
        let child = host.join("child");
        let grand = child.join("grand");
        std::fs::create_dir_all(&grand).unwrap();
        write_registry(&grand, "Grand", &[]);
        write_registry(&child, "Child", &[("grand", "grand")]);
        write_registry(host, "Host", &[("child", "child")]);

        let tree = BrainTree::discover(&host.join(".claude/brain-registry.json"));
        let ordered = tree.list();
        assert_eq!(ordered[0].id, "host");
        assert_eq!(ordered[0].depth, 0);
        assert_eq!(ordered[1].id, "child");
        assert_eq!(ordered[1].depth, 1);
        assert_eq!(ordered[2].id, "grand");
        assert_eq!(ordered[2].depth, 2);
    }

    #[test]
    fn missing_child_registry_logged_but_walk_continues() {
        // Declare a child whose brain_path doesn't exist on disk.
        // The host should still load + record the missing child.
        let tmp = tempfile::tempdir().unwrap();
        write_registry(tmp.path(), "Host", &[("ghost", "nowhere")]);
        let tree = BrainTree::discover(&tmp.path().join(".claude/brain-registry.json"));
        assert_eq!(tree.self_id, "host");
        // ghost recorded with declared display_name even though
        // its registry can't be read.
        assert!(tree.entries.contains_key("ghost"));
        let g = &tree.entries["ghost"];
        assert_eq!(g.depth, 1);
        assert_eq!(g.parent_id.as_deref(), Some("host"));
    }

    #[test]
    fn unique_id_resolves_collisions() {
        let tmp = tempfile::tempdir().unwrap();
        let host = tmp.path();
        let alpha1 = host.join("a1");
        let alpha2 = host.join("a2");
        std::fs::create_dir_all(&alpha1).unwrap();
        std::fs::create_dir_all(&alpha2).unwrap();
        // Both children declare `meta.project: "Alpha"` → same slug.
        write_registry(&alpha1, "Alpha", &[]);
        write_registry(&alpha2, "Alpha", &[]);
        write_registry(host, "Host", &[("first", "a1"), ("second", "a2")]);

        let tree = BrainTree::discover(&host.join(".claude/brain-registry.json"));
        assert_eq!(tree.entries.len(), 3);
        assert!(tree.entries.contains_key("alpha"));
        assert!(tree.entries.contains_key("alpha-2"));
    }
}
