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

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Topic to print. Omit to list available topics.
    pub topic: Option<String>,

    /// Print the bundled spec version + canonical-source path.
    #[arg(long)]
    pub version: bool,
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
        Some(name) => print_topic(name)?,
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
