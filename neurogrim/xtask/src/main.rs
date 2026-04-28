//! Workspace build / verification orchestration. Invoked via:
//!
//! ```bash
//! cargo xtask <subcommand>
//! ```
//!
//! The `cargo xtask` alias is wired in `.cargo/config.toml`.
//!
//! Subcommands:
//! - `sca-check` — runs `cargo audit` + `npm audit` across the workspace
//!   and aggregates findings. Returns non-zero exit on findings above the
//!   configured severity floor (default: moderate).

use anyhow::Result;
use clap::{Parser, Subcommand};

mod sca_check;

#[derive(Parser, Debug)]
#[command(name = "xtask", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run cargo audit + npm audit across the workspace; report
    /// vulnerabilities at or above the configured severity floor.
    ///
    /// Wraps the discipline captured in the bundled
    /// `dependency-discipline` skill — agents installing deps in a
    /// NeuroGrim-aware project are expected to run this before merge.
    /// CI gates on the same command.
    ScaCheck(sca_check::Args),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::ScaCheck(args) => sca_check::run(args),
    }
}
