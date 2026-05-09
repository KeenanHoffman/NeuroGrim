//! `neurogrim explain <topic>` — bundled methodology primer (v3.2 Phase B).
//!
//! Thin CLI wrapper around `neurogrim_mcp::explain` (v3.2.1 — content
//! relocated to the mcp crate so the MCP `explain` tool can use the
//! same source of truth without cross-crate `include_str!` hacks).
//!
//! Output is plain markdown — no rendering. Agents read raw markdown
//! natively and the behavior is identical across TTY and pipe.

use anyhow::Result;
use clap::Args as ClapArgs;
use neurogrim_mcp::explain as mcp_explain;
use neurogrim_mcp::version_summary;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Topic to print. Omit to list available topics. Special token
    /// `version` requires a second positional with the version string
    /// (e.g., `neurogrim explain version 5.0.0`).
    pub topic: Option<String>,

    /// Second positional — only meaningful when `topic == "version"`.
    /// The version to summarize (e.g., "5.0.0", "Unreleased").
    pub version_arg: Option<String>,

    /// Print the bundled spec version + canonical-source path.
    #[arg(long)]
    pub version: bool,

    /// When summarizing a CHANGELOG version, render as human prose
    /// instead of structured JSON. Default is JSON for agent consumption.
    #[arg(long)]
    pub prose: bool,
}

pub async fn run(args: Args) -> Result<()> {
    if args.version {
        println!(
            "Bundled methodology version: {}",
            mcp_explain::BUNDLED_VERSION
        );
        println!("Canonical source: {}", mcp_explain::CANONICAL_SOURCE);
        return Ok(());
    }

    match args.topic.as_deref() {
        None => print_topic_list(),
        Some("version") => print_version_summary(args.version_arg.as_deref(), args.prose)?,
        Some(name) => print_topic(name)?,
    }
    Ok(())
}

fn print_version_summary(version: Option<&str>, prose: bool) -> Result<()> {
    let version = match version {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => {
            anyhow::bail!(
                "`neurogrim explain version` requires a version argument. \
                 Available: {}",
                version_summary::bundled_versions().join(", ")
            )
        }
    };
    let entry = version_summary::bundled_entry(version).ok_or_else(|| {
        anyhow::anyhow!(
            "no entry for version {version:?} in bundled CHANGELOG. Available: {}",
            version_summary::bundled_versions().join(", ")
        )
    })?;
    if prose {
        // Human-readable prose: heading + raw body (preserves the
        // operator's original markdown formatting).
        println!("# Changelog entry: {}", entry.version);
        if let Some(date) = entry.date.as_deref() {
            println!("Released: {date}");
        }
        println!();
        println!("{}", entry.raw_body);
    } else {
        // Structured JSON for agent consumption.
        let sections: Vec<serde_json::Value> = entry
            .sections
            .iter()
            .map(|s| {
                serde_json::json!({
                    "heading": s.heading,
                    "items":   s.items,
                    "raw":     s.raw,
                })
            })
            .collect();
        let json = serde_json::json!({
            "version":  entry.version,
            "date":     entry.date,
            "sections": sections,
            "raw_body": entry.raw_body,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    }
    Ok(())
}

fn print_topic_list() {
    println!(
        "neurogrim explain — bundled methodology primer ({})",
        mcp_explain::BUNDLED_VERSION
    );
    println!();
    println!("Available topics:");
    for (name, summary, _) in mcp_explain::topics() {
        println!("  {:<13} {}", name, summary);
    }
    println!();
    println!("Run `neurogrim explain <topic>` for any topic.");
    println!("Run `neurogrim explain --version` for bundle metadata.");
}

fn print_topic(name: &str) -> Result<()> {
    if let Some(body) = mcp_explain::lookup(name) {
        print!("{}", body);
        return Ok(());
    }
    let names = mcp_explain::topic_names().join(", ");
    anyhow::bail!(
        "unknown topic '{name}'. Available: {names}. Run `neurogrim explain` to list with summaries."
    );
}
