//! `neurogrim sca-review` — Layer 3 supply-chain review CLI.
//!
//! Three sub-commands:
//!
//! - **`create`** — open a new review ticket for a flagged package.
//!   Appends a `review-pending` entry to the decision ledger and
//!   writes a JSON ticket file.
//! - **`list`** — print all tickets (or just open ones).
//! - **`resolve`** — close an open ticket with an operator decision.
//!   Appends a `review-triaged` entry that supersedes the
//!   `review-pending` predecessor.
//!
//! See `docs/supply-chain-review.md` for full operator guide.

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use std::path::Path;

#[derive(Subcommand, Debug)]
pub enum ScaReviewCmd {
    /// Open a new review ticket for a flagged package.
    Create {
        /// Project root path.
        #[arg(long, default_value = ".")]
        project_root: String,
        /// Package ecosystem ("crates.io", "PyPI", "npm", ...).
        #[arg(long)]
        ecosystem: String,
        /// Package name.
        #[arg(long)]
        package: String,
        /// Lockfile-resolved version that triggered the review.
        #[arg(long)]
        version: Option<String>,
        /// Triggering signal kind (e.g., "manual:operator-spotted",
        /// "vigilance:typosquat-proximity").
        #[arg(long)]
        signal: String,
        /// Free-text rationale for opening the ticket.
        #[arg(long)]
        note: String,
        /// Operator handle. Defaults to `$NEUROGRIM_OPERATOR` or
        /// "unknown" if unset.
        #[arg(long)]
        operator: Option<String>,
    },

    /// List review tickets.
    List {
        #[arg(long, default_value = ".")]
        project_root: String,
        /// Show only open tickets (default: show all).
        #[arg(long)]
        open_only: bool,
    },

    /// Resolve an open review ticket.
    Resolve {
        #[arg(long, default_value = ".")]
        project_root: String,
        /// Ticket id to resolve.
        #[arg(long)]
        id: String,
        /// One of accept | reject | pin-to-last-good | no-action.
        ///
        /// 2026-04-26 PRE-RELEASE C9 fix: validated at CLI parse time
        /// via PossibleValuesParser so typos like `--decision yolo`
        /// fail fast with clap's standard "invalid value" error
        /// instead of reaching the sensory layer.
        #[arg(
            long,
            value_parser = clap::builder::PossibleValuesParser::new([
                "accept", "reject", "pin-to-last-good", "no-action",
            ]),
        )]
        decision: String,
        /// Resolution rationale (required, non-empty).
        #[arg(long)]
        note: String,
        /// Operator handle.
        #[arg(long)]
        operator: Option<String>,
        /// For pin-to-last-good: the version we pinned FROM.
        #[arg(long)]
        from_version: Option<String>,
        /// For pin-to-last-good: the version we pinned TO.
        #[arg(long)]
        to_version: Option<String>,
    },
}

pub async fn run(subcommand: ScaReviewCmd) -> Result<()> {
    match subcommand {
        ScaReviewCmd::Create {
            project_root,
            ecosystem,
            package,
            version,
            signal,
            note,
            operator,
        } => cmd_create(
            &project_root,
            &ecosystem,
            &package,
            version.as_deref(),
            &signal,
            &note,
            operator.as_deref(),
        ),
        ScaReviewCmd::List {
            project_root,
            open_only,
        } => cmd_list(&project_root, open_only),
        ScaReviewCmd::Resolve {
            project_root,
            id,
            decision,
            note,
            operator,
            from_version,
            to_version,
        } => cmd_resolve(
            &project_root,
            &id,
            &decision,
            &note,
            operator.as_deref(),
            from_version.as_deref(),
            to_version.as_deref(),
        ),
    }
}

fn cmd_create(
    project_root: &str,
    ecosystem: &str,
    package: &str,
    version: Option<&str>,
    signal: &str,
    note: &str,
    operator: Option<&str>,
) -> Result<()> {
    let op = resolve_operator(operator);
    let id = neurogrim_sensory::supply_chain_review::cli_create(
        Path::new(project_root),
        ecosystem,
        package,
        version,
        signal,
        note,
        &op,
    )
    .context("sca-review create")?;
    println!(
        "{} {} {}@{} [{}] — opened by {}",
        "✓ ticket created:".green(),
        id,
        package,
        version.unwrap_or("?"),
        ecosystem,
        op,
    );
    Ok(())
}

fn cmd_list(project_root: &str, open_only: bool) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let count = neurogrim_sensory::supply_chain_review::cli_list(
        Path::new(project_root),
        open_only,
        &mut out,
    )
    .context("sca-review list")?;
    eprintln!(
        "({} {} ticket{})",
        count,
        if open_only { "open" } else { "total" },
        if count == 1 { "" } else { "s" }
    );
    Ok(())
}

fn cmd_resolve(
    project_root: &str,
    id: &str,
    decision: &str,
    note: &str,
    operator: Option<&str>,
    from_version: Option<&str>,
    to_version: Option<&str>,
) -> Result<()> {
    let op = resolve_operator(operator);
    neurogrim_sensory::supply_chain_review::cli_resolve(
        Path::new(project_root),
        id,
        decision,
        note,
        &op,
        from_version,
        to_version,
    )
    .context("sca-review resolve")?;
    println!(
        "{} {} resolved as {} by {}",
        "✓".green(),
        id,
        decision,
        op,
    );
    Ok(())
}

fn resolve_operator(cli_arg: Option<&str>) -> String {
    if let Some(v) = cli_arg {
        if !v.trim().is_empty() {
            return v.to_string();
        }
    }
    std::env::var("NEUROGRIM_OPERATOR").unwrap_or_else(|_| "unknown".to_string())
}
