//! `neurogrim explain <topic>` — bundled methodology primer (v3.2 Phase B).
//!
//! Eight self-contained topic files ship inside the CLI binary via
//! `include_str!`. This command prints the requested topic's content
//! to stdout. With no topic, lists available topics.
//!
//! Source of truth: the per-topic markdown files at
//! `data/explain/<topic>.md`. Each carries a version-stamped HTML
//! comment header. The thin index at `docs/AGENT-PRIMER.md` exists for
//! GitHub-side human browsing; the binary ships a self-contained copy.
//!
//! Output is plain markdown — no rendering. Agents read raw markdown
//! natively and the behavior is identical across TTY and pipe.

use anyhow::Result;
use clap::Args as ClapArgs;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Topic to print. Omit to list available topics.
    pub topic: Option<String>,

    /// Print the bundled spec version + canonical-source path.
    #[arg(long)]
    pub version: bool,
}

// --- Bundled topic content --------------------------------------------

const TOPIC_METHODOLOGY: &str = include_str!("../../data/explain/methodology.md");
const TOPIC_DOMAIN: &str = include_str!("../../data/explain/domain.md");
const TOPIC_SENSOR: &str = include_str!("../../data/explain/sensor.md");
const TOPIC_HAT: &str = include_str!("../../data/explain/hat.md");
const TOPIC_SCORING: &str = include_str!("../../data/explain/scoring.md");
const TOPIC_FEDERATION: &str = include_str!("../../data/explain/federation.md");
const TOPIC_CLI: &str = include_str!("../../data/explain/cli.md");
const TOPIC_CULTURE: &str = include_str!("../../data/explain/culture.md");

/// Spec/methodology version this bundle was compiled against. Matches
/// the version header in each `data/explain/*.md`. Bumped manually when
/// methodology evolves enough to invalidate prior agent guidance.
const BUNDLED_VERSION: &str = "v3.2";

/// Where the canonical source lives in the source tree (for human
/// audit / pull-request review of methodology changes).
const CANONICAL_SOURCE: &str =
    "neurogrim/crates/neurogrim-cli/data/explain/<topic>.md";

/// All bundled topics, in the order shown by the no-topic listing.
fn topics() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        (
            "methodology",
            "What is LSP Brains; the overlay framing; the 5-piece model",
            TOPIC_METHODOLOGY,
        ),
        (
            "domain",
            "Anatomy of a domain; weight tiers; when to add one",
            TOPIC_DOMAIN,
        ),
        (
            "sensor",
            "Sensor authoring contract; CMDB envelope; score formula patterns",
            TOPIC_SENSOR,
        ),
        (
            "hat",
            "The 8 declared hats and when to wear each",
            TOPIC_HAT,
        ),
        (
            "scoring",
            "Unified score, confidence, trajectory, floor gates",
            TOPIC_SCORING,
        ),
        (
            "federation",
            "A2A peers, fractal composition, read-only siblings",
            TOPIC_FEDERATION,
        ),
        (
            "cli",
            "All commands grouped by purpose (introspection / authoring / execution / bookkeeping)",
            TOPIC_CLI,
        ),
        (
            "culture",
            "culture.yaml — five values as floor-only invariants",
            TOPIC_CULTURE,
        ),
    ]
}

pub async fn run(args: Args) -> Result<()> {
    if args.version {
        println!("Bundled methodology version: {}", BUNDLED_VERSION);
        println!("Canonical source: {}", CANONICAL_SOURCE);
        return Ok(());
    }

    match args.topic.as_deref() {
        None => print_topic_list(),
        Some(name) => print_topic(name)?,
    }
    Ok(())
}

fn print_topic_list() {
    println!("neurogrim explain — bundled methodology primer ({})", BUNDLED_VERSION);
    println!();
    println!("Available topics:");
    for (name, summary, _) in topics() {
        println!("  {:<13} {}", name, summary);
    }
    println!();
    println!("Run `neurogrim explain <topic>` for any topic.");
    println!("Run `neurogrim explain --version` for bundle metadata.");
}

fn print_topic(name: &str) -> Result<()> {
    for (key, _, body) in topics() {
        if *key == name {
            print!("{}", body);
            return Ok(());
        }
    }
    let names: Vec<&str> = topics().iter().map(|(k, _, _)| *k).collect();
    anyhow::bail!(
        "unknown topic '{name}'. Available: {}. Run `neurogrim explain` to list with summaries.",
        names.join(", ")
    );
}

// --- Tests ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_topics_have_non_empty_bodies() {
        for (name, _summary, body) in topics() {
            assert!(
                body.len() > 200,
                "topic '{name}' bundled body is too short ({} bytes)",
                body.len()
            );
        }
    }

    #[test]
    fn all_topics_carry_version_header() {
        for (name, _summary, body) in topics() {
            let first = body.lines().next().unwrap_or("");
            assert!(
                first.starts_with("<!-- topic:") && first.contains(BUNDLED_VERSION),
                "topic '{name}' is missing the bundled-version header marker; first line: {first:?}"
            );
        }
    }

    #[test]
    fn topic_count_is_8() {
        assert_eq!(topics().len(), 8);
    }

    #[test]
    fn topic_names_are_unique() {
        let mut names: Vec<&str> = topics().iter().map(|(k, _, _)| *k).collect();
        names.sort();
        let n = names.len();
        names.dedup();
        assert_eq!(n, names.len(), "duplicate topic name");
    }

    #[test]
    fn methodology_topic_mentions_overlay() {
        assert!(TOPIC_METHODOLOGY.contains("overlay"));
        assert!(TOPIC_METHODOLOGY.contains("nervous system"));
    }

    #[test]
    fn domain_topic_describes_weight_tiers() {
        assert!(TOPIC_DOMAIN.contains("Weighted"));
        assert!(TOPIC_DOMAIN.contains("Advisory"));
        assert!(TOPIC_DOMAIN.contains("Stub"));
    }

    #[test]
    fn cli_topic_lists_command_families() {
        assert!(TOPIC_CLI.contains("Introspection"));
        assert!(TOPIC_CLI.contains("Authoring"));
        assert!(TOPIC_CLI.contains("Execution"));
        assert!(TOPIC_CLI.contains("Bookkeeping"));
    }
}
