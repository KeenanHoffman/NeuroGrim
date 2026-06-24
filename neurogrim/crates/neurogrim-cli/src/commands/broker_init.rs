//! `neurogrim broker-init` — Wave 5.5 operator-setup helper.
//!
//! One-time setup that scaffolds the directory structure + sample manifests
//! + .mcp.json registration + CLAUDE.md auto-load wiring for a broker-harness
//! deployment in the operator's project. Reduces the operator-side ceremony
//! that Wave 5 left as a manual checklist (V0-RETROSPECTIVE.md §D1 +
//! ultra-pass §U2 / §U11).
//!
//! ## What it creates
//!
//! Under `<project_root>/.claude/brain/broker/`:
//! - `cluster.toml` — sample cluster manifest
//! - `work-broker.toml` — sample per-broker manifest
//! - `segments/` — empty dir (Materializer Composer will populate)
//!
//! Under `<project_root>/.claude/`:
//! - `.mcp.json` — registers `neurogrim broker-serve` if not present
//!
//! Under `<project_root>/CLAUDE.md`:
//! - Appends an auto-load reference to `current-projection.md` if not present
//!
//! All operations are idempotent: re-running broker-init won't overwrite
//! existing files (just notes which are already in place).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub async fn run(project_root: &str, yes: bool) -> Result<()> {
    let root = Path::new(project_root);
    eprintln!("✦ Initializing broker harness in: {}", root.display());

    let broker_dir = root.join(".claude/brain/broker");
    let segments_dir = broker_dir.join("segments");
    std::fs::create_dir_all(&segments_dir)
        .with_context(|| format!("creating {}", segments_dir.display()))?;
    eprintln!("  • created {}", segments_dir.display());

    // Cluster manifest
    let cluster_path = broker_dir.join("cluster.toml");
    create_if_missing(&cluster_path, sample_cluster_manifest(), yes)?;

    // Per-broker manifest
    let broker_path = broker_dir.join("work-broker.toml");
    create_if_missing(&broker_path, sample_work_broker_manifest(), yes)?;

    // .mcp.json registration
    let mcp_json_path = root.join(".claude/.mcp.json");
    register_mcp_server(&mcp_json_path, yes)?;

    // CLAUDE.md auto-load wiring
    let claude_md_path = root.join("CLAUDE.md");
    add_claude_md_reference(&claude_md_path, yes)?;

    eprintln!("\n✦ Broker harness initialized.");
    eprintln!("\nNext steps:");
    eprintln!("  1. Edit {} to declare your broker(s)", cluster_path.display());
    eprintln!("  2. Edit {} to configure the work broker", broker_path.display());
    eprintln!(
        "  3. Restart Claude Code in this directory; the broker-serve MCP server \
         will start automatically + write current-projection.md"
    );
    eprintln!(
        "  4. Verify {} contains broker overlay segments after a tick",
        broker_dir.join("current-projection.md").display()
    );

    Ok(())
}

fn create_if_missing(path: &Path, contents: &str, _yes: bool) -> Result<()> {
    if path.exists() {
        eprintln!("  • {} already exists; preserving", path.display());
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
        .with_context(|| format!("writing {}", path.display()))?;
    eprintln!("  • created {}", path.display());
    Ok(())
}

fn register_mcp_server(mcp_json_path: &Path, _yes: bool) -> Result<()> {
    let existing: serde_json::Value = if mcp_json_path.exists() {
        let txt = std::fs::read_to_string(mcp_json_path)?;
        serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!({"mcpServers": {}}))
    } else {
        serde_json::json!({"mcpServers": {}})
    };

    let mut root = existing.clone();
    let servers = root
        .as_object_mut()
        .and_then(|o| o.entry("mcpServers").or_insert(serde_json::json!({})).as_object_mut());

    if let Some(servers) = servers {
        if servers.contains_key("neurogrim-broker") {
            eprintln!(
                "  • {} already declares `neurogrim-broker`; preserving",
                mcp_json_path.display()
            );
            return Ok(());
        }
        servers.insert(
            "neurogrim-broker".to_string(),
            serde_json::json!({
                "command": "neurogrim",
                "args": ["broker-serve", "--cluster", ".claude/brain/broker/cluster.toml"]
            }),
        );
    }

    if let Some(parent) = mcp_json_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(mcp_json_path, serde_json::to_string_pretty(&root)?)?;
    eprintln!("  • registered `neurogrim-broker` MCP server in {}", mcp_json_path.display());
    Ok(())
}

fn add_claude_md_reference(claude_md_path: &Path, _yes: bool) -> Result<()> {
    let reference_line = "@.claude/brain/broker/current-projection.md";
    let existing = if claude_md_path.exists() {
        std::fs::read_to_string(claude_md_path)?
    } else {
        String::new()
    };

    if existing.contains("current-projection.md") {
        eprintln!(
            "  • {} already references current-projection.md; preserving",
            claude_md_path.display()
        );
        return Ok(());
    }

    let appended = if existing.is_empty() {
        format!(
            "# Project CLAUDE.md\n\n\
             ## Broker harness auto-load\n\n\
             Generated by `neurogrim broker-init`. The line below auto-loads the broker substrate's \
             current projection into agent context on every turn — this is the agent's primary \
             discovery surface for broker-defined pipelines.\n\n\
             {}\n",
            reference_line
        )
    } else {
        format!(
            "{}\n\n## Broker harness auto-load (appended by `neurogrim broker-init`)\n\n{}\n",
            existing.trim_end(),
            reference_line
        )
    };

    std::fs::write(claude_md_path, appended)?;
    eprintln!(
        "  • appended broker auto-load reference to {}",
        claude_md_path.display()
    );
    Ok(())
}

fn sample_cluster_manifest() -> &'static str {
    r#"# NeuroGrim broker-harness cluster manifest
# Generated by `neurogrim broker-init`. Edit to suit your deployment.

[cluster]
id = "dev-cluster"
name = "Development Broker Cluster"
brokers_dir = "./"

# Declare each broker that should run in this cluster. The broker_id MUST
# match the [broker] id field in the referenced manifest.
[cluster.brokers.work-broker]
manifest_path = "work-broker.toml"

[cluster.materializer]
# Operator-declared segment composition order. Governance segment is
# placed FIRST regardless (Untunable per R-O-3 closure). This list
# controls the relative order of non-governance segments.
composition_order = ["overlay-work-broker", "awareness-routing-work-broker"]

# Output path for current-projection.md (CLAUDE.md auto-load target)
output_path = "current-projection.md"

# Segment files directory
segments_dir = "segments"

# Context-window budget. If exceeded, composer falls back to governance-only
# projection (per R-O-3 closure truncation alarm).
context_budget_chars = 16384
"#
}

fn sample_work_broker_manifest() -> &'static str {
    r#"# NeuroGrim Work Broker manifest
# Generated by `neurogrim broker-init`. Edit to suit your deployment.

[broker]
id = "work-broker"
name = "Work Broker"

# Role-set per BROKER-CONTRACT §"Broker roles". Work Broker carries
# InnateAbility (narrow judgment about which work unit to dispatch).
roles = ["innate-ability"]

# Per-broker cold-store path (R-O-4 closure: per-broker file isolation
# enforced at construction time).
cold_store_path = "work-broker-cold/"

# Catalog source. MVP: the Work Broker exposes its catalog via Rust code
# (broker.catalog() method) rather than reading YAML, per V0-RETROSPECTIVE
# §B2. This field reserved for future per-broker YAML-overlay-tuning
# (operator overlays atop broker default catalog within tunability bounds).
catalog_path = "work-broker-catalog.yaml"
"#
}
