//! `neurogrim federation register` — add a child Brain to the local
//! registry's federation (v3.1.1 init automation Phase 4).
//!
//! Targets an ecosystem coordinator's `brain-registry.json`. Adds an
//! entry to `config.children`, auto-allocates the next available port
//! starting at 8421 if `--port` is unspecified, sets `read_only: true`
//! when `--read-only` is passed, and bumps `meta.schema_version` from
//! "2" to "2.1" if not already there (the schema additive landed in
//! LSP-Brains commit 9cb83cf).
//!
//! Idempotent: re-registering the same name updates the existing entry.

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args as ClapArgs, Subcommand};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub subcommand: FederationCmd,
}

#[derive(Subcommand, Debug)]
pub enum FederationCmd {
    /// Add (or update) a child Brain entry in the local registry's
    /// `config.children` map.
    ///
    /// Auto-allocates the next available port from 8421 if `--port`
    /// is unspecified. Sets `read_only: true` when `--read-only` is
    /// passed (codifies the non-influencing posture for sibling-project
    /// peers; LSP-Brains spec v2.1+).
    Register {
        /// Child Brain identifier (key in config.children). Convention:
        /// kebab-case matching the child's project name.
        #[arg(long)]
        name: String,

        /// Path to the child Brain (used as `brain_path` in the
        /// registry entry). Relative or absolute.
        #[arg(long)]
        path: String,

        /// Optional A2A port. Defaults to next available starting at 8421.
        #[arg(long)]
        port: Option<u16>,

        /// Mark this child as observed-only (read_only: true). Codifies
        /// the parent's non-influencing posture per LSP-Brains v2.1+.
        #[arg(long)]
        read_only: bool,

        /// Path to the registry to modify. Defaults to
        /// `.claude/brain-registry.json` relative to CWD.
        #[arg(long, default_value = ".claude/brain-registry.json")]
        registry: String,

        /// Optional human-readable display name. Defaults to the `name`
        /// value. Surfaced in operator-facing output.
        #[arg(long)]
        display_name: Option<String>,
    },

    /// Rewrite a child entry's `a2a_endpoint` + `agent_card_url` to
    /// match the child's persisted port (v3.5.0+). Operator-explicit
    /// migration tool for the case where a child's `ports.json` was
    /// re-allocated and the parent's hardcoded endpoint became stale.
    /// No silent registry mutations; the operator runs this once
    /// per affected child after a v3.5 upgrade.
    ///
    /// With `--probe-only`, prints the diff and exits 0 without
    /// modifying anything.
    Rewire {
        /// Child Brain identifier — must already exist in the
        /// registry's `config.children` map.
        #[arg(long)]
        child: String,

        /// Path to the registry to modify. Defaults to
        /// `.claude/brain-registry.json` relative to CWD.
        #[arg(long, default_value = ".claude/brain-registry.json")]
        registry: String,

        /// Print the planned diff (current vs new endpoint + agent
        /// card URL) and exit 0. Don't modify the registry.
        #[arg(long)]
        probe_only: bool,
    },
}

pub async fn run(args: Args) -> Result<()> {
    match args.subcommand {
        FederationCmd::Register {
            name,
            path,
            port,
            read_only,
            registry,
            display_name,
        } => {
            cmd_register(
                &name,
                &path,
                port,
                read_only,
                &registry,
                display_name.as_deref(),
            )
            .await
        }
        FederationCmd::Rewire {
            child,
            registry,
            probe_only,
        } => cmd_rewire(&child, &registry, probe_only).await,
    }
}

async fn cmd_register(
    name: &str,
    path: &str,
    port_arg: Option<u16>,
    read_only: bool,
    registry_path: &str,
    display_name: Option<&str>,
) -> Result<()> {
    let registry_pb = PathBuf::from(registry_path);
    let registry_text = fs::read_to_string(&registry_pb)
        .await
        .with_context(|| format!("failed to read registry at {}", registry_pb.display()))?;
    let mut registry: Value = serde_json::from_str(&registry_text)
        .with_context(|| format!("failed to parse {} as JSON", registry_pb.display()))?;

    // Determine the port: explicit --port wins; otherwise allocate the
    // next free port starting at 8421 (the canonical first-child slot).
    // v3.3 F7: the allocator now walks transitively into each child's
    // own registry (when reachable on disk), so we won't pick a port
    // already in use by a grandchild.
    let allocated_port = match port_arg {
        Some(p) => p,
        None => allocate_next_port(&registry, &registry_pb).unwrap_or(8421),
    };

    // Build the child entry.
    let display = display_name.unwrap_or(name);
    let mut child_entry = json!({
        "display_name": display,
        "a2a_endpoint": format!("http://localhost:{allocated_port}/a2a/v1/"),
        "agent_card_url": format!("http://localhost:{allocated_port}/.well-known/agent-card.json"),
        "brain_path": path,
        "interface_version": "1",
        "depends_on": [],
        "weight": if read_only { 0.0 } else { 1.0 },
        "enabled": true,
    });
    if read_only {
        child_entry["read_only"] = json!(true);
    }

    // Insert or update under config.children.
    let config = registry
        .get_mut("config")
        .ok_or_else(|| anyhow!("registry has no `config` block"))?;
    if !config.is_object() {
        bail!("registry's `config` is not an object");
    }
    let config_obj = config.as_object_mut().unwrap();
    let children = config_obj
        .entry("children")
        .or_insert_with(|| json!({}));
    if !children.is_object() {
        bail!("registry's `config.children` is not an object");
    }
    let children_obj = children.as_object_mut().unwrap();
    let was_existing = children_obj.contains_key(name);
    children_obj.insert(name.to_string(), child_entry);

    // Bump schema_version 2 → 2.1 if read_only is set and version is "2".
    if read_only {
        if let Some(meta) = registry.get_mut("meta").and_then(|m| m.as_object_mut()) {
            if let Some(sv) = meta.get_mut("schema_version") {
                if sv == "2" {
                    *sv = json!("2.1");
                    eprintln!("Bumped meta.schema_version: 2 → 2.1 (read_only awareness)");
                }
            }
        }
    }

    let updated = serde_json::to_string_pretty(&registry)? + "\n";
    fs::write(&registry_pb, updated)
        .await
        .with_context(|| format!("failed to write {}", registry_pb.display()))?;

    let action = if was_existing { "updated" } else { "registered" };
    let ro_note = if read_only { " (read-only)" } else { "" };
    eprintln!("{action}: {name}{ro_note} → port {allocated_port}, brain_path={path}");
    Ok(())
}

/// `neurogrim federation rewire --child <name>` — rewrite a child
/// entry's `a2a_endpoint` + `agent_card_url` to match the child's
/// persisted port (`<brain_path>/.claude/brain/ports.json::a2a_port`).
///
/// Operator-explicit migration tool. v3.5.0 introduced per-project
/// port allocation; existing parent registries still hardcode the
/// pre-v3.5 child ports (8421, 8422, …). When a child's ports.json
/// has drifted from what the parent advertises, this command
/// reconciles them. No silent registry mutations on parent's start.
async fn cmd_rewire(child_name: &str, registry_path: &str, probe_only: bool) -> Result<()> {
    let registry_pb = PathBuf::from(registry_path);
    let registry_text = fs::read_to_string(&registry_pb)
        .await
        .with_context(|| format!("failed to read registry at {}", registry_pb.display()))?;
    let mut registry: Value = serde_json::from_str(&registry_text)
        .with_context(|| format!("failed to parse {} as JSON", registry_pb.display()))?;

    // Locate the child entry.
    let child_entry = registry
        .get("config")
        .and_then(|c| c.get("children"))
        .and_then(|c| c.get(child_name))
        .ok_or_else(|| {
            anyhow!(
                "child {child_name:?} not found in {}'s config.children — \
                 run `neurogrim federation register` first",
                registry_pb.display()
            )
        })?
        .clone();

    let brain_path_str = child_entry
        .get("brain_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("child {child_name} entry has no `brain_path` field"))?;

    // Resolve brain_path. The child's brain_path is recorded relative
    // to the parent's project root (parent of the registry file's
    // parent — `.claude/`). Mirror what `lib.rs::serve` does for the
    // dashboard to keep the path-resolution semantics consistent.
    let parent_project_root = registry_pb
        .parent()
        .and_then(Path::parent)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let child_root_relative = parent_project_root.join(brain_path_str);
    let child_root = std::fs::canonicalize(&child_root_relative).unwrap_or(child_root_relative);

    // Read child's ports.json. We don't allocate from here — that
    // would mutate the child's state on a parent-side command, which
    // violates the "operator-explicit" stance.
    let child_ports = neurogrim_core::ports::read_ports(&child_root).ok_or_else(|| {
        anyhow!(
            "child {child_name} has no ports.json at {} — \
             run `neurogrim a2a-serve --project-root {}` once to allocate it, \
             then re-run rewire",
            neurogrim_core::ports::ports_file_path(&child_root).display(),
            child_root.display()
        )
    })?;

    let new_endpoint = format!("http://localhost:{}/a2a/v1/", child_ports.a2a_port);
    let new_card_url = format!(
        "http://localhost:{}/.well-known/agent-card.json",
        child_ports.a2a_port
    );

    let old_endpoint = child_entry
        .get("a2a_endpoint")
        .and_then(|v| v.as_str())
        .unwrap_or("(none)");
    let old_card_url = child_entry
        .get("agent_card_url")
        .and_then(|v| v.as_str())
        .unwrap_or("(none)");

    // Compute the diff for the operator to inspect.
    let needs_update = old_endpoint != new_endpoint || old_card_url != new_card_url;

    eprintln!("✦ federation rewire: child={child_name}");
    eprintln!("  brain_path:        {}", child_root.display());
    eprintln!("  child a2a_port:    {} (from ports.json)", child_ports.a2a_port);
    eprintln!("  current endpoint:  {old_endpoint}");
    eprintln!("  proposed endpoint: {new_endpoint}");
    eprintln!("  current card URL:  {old_card_url}");
    eprintln!("  proposed card URL: {new_card_url}");

    if !needs_update {
        eprintln!("  status:            already in sync (no rewrite needed)");
        return Ok(());
    }

    if probe_only {
        eprintln!("  status:            --probe-only set; not rewriting");
        return Ok(());
    }

    // Apply the rewrite.
    let config = registry
        .get_mut("config")
        .ok_or_else(|| anyhow!("registry has no `config` block"))?;
    let children = config
        .get_mut("children")
        .ok_or_else(|| anyhow!("registry has no `config.children` block"))?;
    let child = children
        .get_mut(child_name)
        .ok_or_else(|| anyhow!("child {child_name} disappeared during rewire"))?;
    if !child.is_object() {
        bail!("registry's config.children.{child_name} is not an object");
    }
    let child_obj = child.as_object_mut().unwrap();
    child_obj.insert("a2a_endpoint".to_string(), json!(new_endpoint));
    child_obj.insert("agent_card_url".to_string(), json!(new_card_url));

    let updated = serde_json::to_string_pretty(&registry)? + "\n";
    fs::write(&registry_pb, updated)
        .await
        .with_context(|| format!("failed to write {}", registry_pb.display()))?;

    eprintln!("  status:            rewritten ✓");
    Ok(())
}

/// Walk `config.children` transitively and find the next port not in
/// use anywhere in the federation tree. Starts at 8421 (NeuroGrim's
/// first-child convention) and increments.
///
/// v3.3 F7: the walk reads each child's `brain_path/.claude/brain-registry.json`
/// when reachable, so allocator decisions consider the FULL transitive
/// federation, not just direct children. This prevents the v3.2.2-era
/// failure mode where job-hunt got allocated to port 8423 (which is
/// python-starter's port — NeuroGrim's child, the ecosystem's grandchild).
///
/// Returns None when the registry has no `config.children` block yet
/// (caller defaults to 8421).
fn allocate_next_port(registry: &Value, registry_path: &PathBuf) -> Option<u16> {
    use neurogrim_core::registry::BrainRegistry;
    use neurogrim_mcp::doctor::collect_transitive_ports;

    // Need a BrainRegistry to call collect_transitive_ports. Fall back to
    // the local-only walk if parsing fails.
    let registry_str = serde_json::to_string(registry).ok()?;
    let parsed = BrainRegistry::from_json(&registry_str).ok()?;
    let project_root = registry_path
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let by_port = collect_transitive_ports(&parsed, &project_root);
    let used: std::collections::HashSet<u16> = by_port.keys().copied().collect();

    if used.is_empty() {
        // Mirror the v3.2.x behavior: returning None lets the caller
        // default to 8421 (the first-child slot) when no children
        // block exists yet.
        let children_empty = registry
            .get("config")
            .and_then(|c| c.get("children"))
            .and_then(|c| c.as_object())
            .map(|o| o.is_empty())
            .unwrap_or(true);
        if children_empty {
            return None;
        }
    }

    let mut candidate = 8421u16;
    while used.contains(&candidate) {
        candidate = candidate.checked_add(1)?;
    }
    Some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn minimal_registry() -> Value {
        json!({
            "meta": {
                "schema_version": "2",
                "description": "test",
                "updated_by": "test",
            },
            "tools": {},
            "data_sources": {},
            "config": {
                "domain_weights": {},
                "domain_definitions": {}
            }
        })
    }

    fn registry_with_children() -> Value {
        json!({
            "meta": {
                "schema_version": "2.1",
                "description": "test",
                "updated_by": "test",
            },
            "tools": {},
            "data_sources": {},
            "config": {
                "domain_weights": {},
                "domain_definitions": {},
                "children": {
                    "existing-child": {
                        "a2a_endpoint": "http://localhost:8421/a2a/v1/",
                        "brain_path": "../existing"
                    }
                }
            }
        })
    }

    #[test]
    fn allocate_next_port_starts_at_8421_when_no_children() {
        let r = minimal_registry();
        let tmp = TempDir::new().unwrap();
        let reg_path = tmp.path().join("brain-registry.json");
        assert_eq!(allocate_next_port(&r, &reg_path), None);
    }

    #[test]
    fn allocate_next_port_skips_used() {
        let r = registry_with_children();
        let tmp = TempDir::new().unwrap();
        let reg_path = tmp.path().join("brain-registry.json");
        // 8421 is in use → 8422 is next.
        assert_eq!(allocate_next_port(&r, &reg_path), Some(8422));
    }

    #[tokio::test]
    async fn register_creates_child_entry_in_minimal_registry() {
        let tmp = TempDir::new().unwrap();
        let reg_path = tmp.path().join("brain-registry.json");
        std::fs::write(&reg_path, minimal_registry().to_string()).unwrap();

        cmd_register(
            "new-child",
            "../new",
            None,
            false,
            reg_path.to_str().unwrap(),
            None,
        )
        .await
        .unwrap();

        let updated: Value =
            serde_json::from_str(&std::fs::read_to_string(&reg_path).unwrap()).unwrap();
        let child = &updated["config"]["children"]["new-child"];
        assert_eq!(child["brain_path"], "../new");
        assert_eq!(child["a2a_endpoint"], "http://localhost:8421/a2a/v1/");
        assert_eq!(child["weight"], 1.0);
        assert!(child.get("read_only").is_none());
    }

    #[tokio::test]
    async fn register_with_read_only_sets_flag_and_weight_zero() {
        let tmp = TempDir::new().unwrap();
        let reg_path = tmp.path().join("brain-registry.json");
        std::fs::write(&reg_path, minimal_registry().to_string()).unwrap();

        cmd_register(
            "sibling",
            "../sibling",
            None,
            true,
            reg_path.to_str().unwrap(),
            Some("Sibling Project"),
        )
        .await
        .unwrap();

        let updated: Value =
            serde_json::from_str(&std::fs::read_to_string(&reg_path).unwrap()).unwrap();
        let child = &updated["config"]["children"]["sibling"];
        assert_eq!(child["read_only"], true);
        assert_eq!(child["weight"], 0.0);
        assert_eq!(child["display_name"], "Sibling Project");
        // schema_version bumped from 2 → 2.1 because read_only was set.
        assert_eq!(updated["meta"]["schema_version"], "2.1");
    }

    #[tokio::test]
    async fn register_auto_allocates_next_port_when_not_specified() {
        let tmp = TempDir::new().unwrap();
        let reg_path = tmp.path().join("brain-registry.json");
        std::fs::write(&reg_path, registry_with_children().to_string()).unwrap();

        cmd_register(
            "second-child",
            "../second",
            None,
            false,
            reg_path.to_str().unwrap(),
            None,
        )
        .await
        .unwrap();

        let updated: Value =
            serde_json::from_str(&std::fs::read_to_string(&reg_path).unwrap()).unwrap();
        let child = &updated["config"]["children"]["second-child"];
        // 8421 was used by existing-child; auto-allocate gives 8422.
        assert_eq!(child["a2a_endpoint"], "http://localhost:8422/a2a/v1/");
    }

    #[tokio::test]
    async fn register_explicit_port_wins_over_auto() {
        let tmp = TempDir::new().unwrap();
        let reg_path = tmp.path().join("brain-registry.json");
        std::fs::write(&reg_path, registry_with_children().to_string()).unwrap();

        cmd_register(
            "third",
            "../third",
            Some(9999),
            false,
            reg_path.to_str().unwrap(),
            None,
        )
        .await
        .unwrap();

        let updated: Value =
            serde_json::from_str(&std::fs::read_to_string(&reg_path).unwrap()).unwrap();
        let child = &updated["config"]["children"]["third"];
        assert_eq!(child["a2a_endpoint"], "http://localhost:9999/a2a/v1/");
    }

    #[tokio::test]
    async fn register_idempotent_updates_existing_entry() {
        let tmp = TempDir::new().unwrap();
        let reg_path = tmp.path().join("brain-registry.json");
        std::fs::write(&reg_path, registry_with_children().to_string()).unwrap();

        // existing-child is in the fixture; re-register with new path.
        cmd_register(
            "existing-child",
            "../updated",
            Some(8421),
            true,
            reg_path.to_str().unwrap(),
            None,
        )
        .await
        .unwrap();

        let updated: Value =
            serde_json::from_str(&std::fs::read_to_string(&reg_path).unwrap()).unwrap();
        let child = &updated["config"]["children"]["existing-child"];
        assert_eq!(child["brain_path"], "../updated");
        assert_eq!(child["read_only"], true);
    }

    /// Build a parent registry + a child with a populated ports.json on
    /// disk. Returns `(parent_registry_path, child_root)` for the test
    /// to use. Parent: `<tmp>/parent/.claude/brain-registry.json` with
    /// `config.children.<child_name>.{brain_path, a2a_endpoint, agent_card_url}`.
    /// Child: `<tmp>/<child_dir>/.claude/brain/ports.json` with the
    /// caller-supplied a2a_port.
    fn setup_rewire_fixture(
        tmp: &TempDir,
        child_name: &str,
        child_dir: &str,
        registry_a2a_port: u16,
        ports_a2a_port: u16,
    ) -> (PathBuf, PathBuf) {
        // Parent layout.
        let parent_root = tmp.path().join("parent");
        let parent_claude = parent_root.join(".claude");
        std::fs::create_dir_all(&parent_claude).unwrap();
        let registry_path = parent_claude.join("brain-registry.json");

        // Use absolute brain_path so canonicalize succeeds without
        // relying on cwd.
        let child_root = tmp.path().join(child_dir);
        std::fs::create_dir_all(child_root.join(".claude").join("brain")).unwrap();

        // Write child ports.json.
        let ports_cfg = neurogrim_core::ports::PortConfig {
            schema_version: "1".into(),
            dashboard_port: ports_a2a_port + 100,
            a2a_port: ports_a2a_port,
            created_at: chrono::Utc::now(),
            generated_by: "test".into(),
        };
        neurogrim_core::ports::save_ports(&child_root, &ports_cfg).unwrap();

        // Write parent registry pointing at the child.
        let registry_json = json!({
            "meta": {
                "schema_version": "2.1",
                "description": "test",
                "updated_by": "test"
            },
            "tools": {},
            "data_sources": {},
            "config": {
                "domain_weights": {},
                "domain_definitions": {},
                "children": {
                    child_name: {
                        "a2a_endpoint": format!("http://localhost:{registry_a2a_port}/a2a/v1/"),
                        "agent_card_url": format!(
                            "http://localhost:{registry_a2a_port}/.well-known/agent-card.json"
                        ),
                        "brain_path": child_root.to_string_lossy().to_string(),
                        "interface_version": "1",
                        "weight": 1.0,
                        "enabled": true,
                    }
                }
            }
        });
        std::fs::write(
            &registry_path,
            serde_json::to_string_pretty(&registry_json).unwrap(),
        )
        .unwrap();

        (registry_path, child_root)
    }

    #[tokio::test]
    async fn rewire_updates_endpoint_when_child_ports_drift() {
        let tmp = TempDir::new().unwrap();
        // Parent thinks child is on 8421; child's ports.json says 51234.
        let (registry_path, _child_root) =
            setup_rewire_fixture(&tmp, "neurogrim", "neurogrim-child", 8421, 51234);

        cmd_rewire("neurogrim", registry_path.to_str().unwrap(), false)
            .await
            .unwrap();

        let updated: Value =
            serde_json::from_str(&std::fs::read_to_string(&registry_path).unwrap()).unwrap();
        let child = &updated["config"]["children"]["neurogrim"];
        assert_eq!(child["a2a_endpoint"], "http://localhost:51234/a2a/v1/");
        assert_eq!(
            child["agent_card_url"],
            "http://localhost:51234/.well-known/agent-card.json"
        );
    }

    #[tokio::test]
    async fn rewire_probe_only_does_not_modify_registry() {
        let tmp = TempDir::new().unwrap();
        let (registry_path, _child_root) =
            setup_rewire_fixture(&tmp, "neurogrim", "neurogrim-child", 8421, 51234);

        let before = std::fs::read_to_string(&registry_path).unwrap();
        cmd_rewire("neurogrim", registry_path.to_str().unwrap(), true)
            .await
            .unwrap();
        let after = std::fs::read_to_string(&registry_path).unwrap();
        assert_eq!(before, after, "--probe-only must not modify the registry");
    }

    #[tokio::test]
    async fn rewire_fails_when_child_has_no_ports_json() {
        let tmp = TempDir::new().unwrap();
        // Set up parent + child dir but DO NOT call save_ports — the
        // child has no ports.json yet.
        let parent_root = tmp.path().join("parent");
        let parent_claude = parent_root.join(".claude");
        std::fs::create_dir_all(&parent_claude).unwrap();
        let registry_path = parent_claude.join("brain-registry.json");
        let child_root = tmp.path().join("child-no-ports");
        std::fs::create_dir_all(&child_root).unwrap();
        let registry_json = json!({
            "meta": {"schema_version": "2.1", "description": "t", "updated_by": "t"},
            "tools": {}, "data_sources": {},
            "config": {
                "domain_weights": {}, "domain_definitions": {},
                "children": {
                    "child": {
                        "brain_path": child_root.to_string_lossy().to_string(),
                        "a2a_endpoint": "http://localhost:8421/a2a/v1/"
                    }
                }
            }
        });
        std::fs::write(
            &registry_path,
            serde_json::to_string_pretty(&registry_json).unwrap(),
        )
        .unwrap();

        let result = cmd_rewire("child", registry_path.to_str().unwrap(), false).await;
        assert!(result.is_err(), "rewire must fail when child has no ports.json");
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("ports.json"),
            "error message must mention ports.json; got: {err}"
        );
    }

    #[tokio::test]
    async fn rewire_idempotent_when_already_in_sync() {
        let tmp = TempDir::new().unwrap();
        // Parent and child both think the port is 51234 — no diff.
        let (registry_path, _child_root) =
            setup_rewire_fixture(&tmp, "neurogrim", "neurogrim-child", 51234, 51234);

        let before = std::fs::read_to_string(&registry_path).unwrap();
        cmd_rewire("neurogrim", registry_path.to_str().unwrap(), false)
            .await
            .unwrap();
        let after = std::fs::read_to_string(&registry_path).unwrap();
        // The "already in sync" path skips the rewrite, so the file
        // is byte-identical.
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn rewire_fails_when_child_not_in_registry() {
        let tmp = TempDir::new().unwrap();
        let reg_path = tmp.path().join("brain-registry.json");
        std::fs::write(&reg_path, minimal_registry().to_string()).unwrap();

        let result = cmd_rewire("nonexistent", reg_path.to_str().unwrap(), false).await;
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("not found") || err.contains("nonexistent"),
            "error must name the missing child; got: {err}"
        );
    }
}
