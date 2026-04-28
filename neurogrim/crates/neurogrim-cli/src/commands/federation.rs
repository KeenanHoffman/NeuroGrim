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
use std::path::PathBuf;
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
}
