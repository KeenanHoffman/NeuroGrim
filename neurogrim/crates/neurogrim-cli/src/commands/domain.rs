//! `neurogrim domain new <name>` — scaffold a new domain (v3.2 Phase C).
//!
//! v3.2.1: this module is a thin clap + printing wrapper around
//! `neurogrim_mcp::domain::scaffold_domain`, which holds the canonical
//! mutation logic. The MCP `domain_new` tool calls the same function.

use anyhow::{Context, Result};
use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use neurogrim_mcp::domain::{scaffold_domain, ScaffoldOutcome, SensorType as McpSensorType};
use std::path::PathBuf;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub subcommand: DomainCmd,
}

#[derive(Subcommand, Debug)]
pub enum DomainCmd {
    /// Scaffold a new domain in this Brain's registry.
    ///
    /// Mutates `brain-registry.json` (adds entries to `domain_weights`,
    /// `principle_map`, `domain_definitions`), creates a stub CMDB at
    /// `.claude/<name>-cmdb.json`, and optionally scaffolds a Python
    /// sensor skeleton.
    New {
        /// Domain name (kebab-case). Must match `^[a-z][a-z0-9-]*$`.
        name: String,

        /// Humanized display name for `principle_map`. Defaults to a
        /// title-case version of the kebab-case name.
        #[arg(long)]
        description: Option<String>,

        /// Initial weight in `domain_weights`. Default 0.0 (advisory) —
        /// new domains should observe before promoting to weighted.
        #[arg(long, default_value_t = 0.0)]
        weight: f64,

        /// Sensor implementation type. Default `stub` (registry +
        /// CMDB only). `python` additionally scaffolds a Python
        /// sensor skeleton.
        #[arg(long, value_enum, default_value_t = SensorType::Stub)]
        r#type: SensorType,

        /// Path to the registry to mutate. Defaults to
        /// `.claude/brain-registry.json` relative to `--directory`.
        #[arg(long, default_value = ".claude/brain-registry.json")]
        registry: String,

        /// Project root containing `.claude/` (and `sensory/` for
        /// `--type python`). Defaults to CWD.
        #[arg(long, default_value = ".")]
        directory: String,

        /// Overwrite existing entries (registry + CMDB + sensor file).
        /// Default refuses to clobber.
        #[arg(long)]
        force: bool,
    },
}

/// CLI-side mirror of the canonical `neurogrim_mcp::domain::SensorType`
/// — clap's `ValueEnum` derive needs to live in the same crate as the
/// arg parser, so we keep a thin shadow here and convert across the
/// boundary.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum SensorType {
    Stub,
    Python,
}

impl From<SensorType> for McpSensorType {
    fn from(s: SensorType) -> Self {
        match s {
            SensorType::Stub => McpSensorType::Stub,
            SensorType::Python => McpSensorType::Python,
        }
    }
}

pub async fn run(args: Args) -> Result<()> {
    match args.subcommand {
        DomainCmd::New {
            name,
            description,
            weight,
            r#type,
            registry,
            directory,
            force,
        } => {
            let outcome = scaffold_domain(
                &name,
                description.as_deref(),
                weight,
                r#type.into(),
                &registry,
                &directory,
                force,
            )
            .await
            .with_context(|| format!("scaffolding domain '{name}'"))?;
            print_next_steps(&outcome);
            Ok(())
        }
    }
}

fn print_next_steps(outcome: &ScaffoldOutcome) {
    let action = if outcome.was_existing { "Updated" } else { "Registered" };
    let posture = if outcome.weight > 0.0 {
        format!("weight {}", outcome.weight)
    } else {
        "advisory (weight 0.0)".to_string()
    };
    eprintln!(
        "{action} domain '{}' as {posture} — {}",
        outcome.name, outcome.display_name
    );
    eprintln!("  Registry:  {}", outcome.registry_path.display());
    eprintln!("  Stub CMDB: {}", outcome.cmdb_path.display());
    if let Some(p) = outcome.sensor_path.as_ref() {
        eprintln!("  Sensor:    {}", p.display());
    }
    eprintln!();
    eprintln!("Next steps:");
    if let Some(p) = outcome.sensor_path.as_ref() {
        let py_module = py_module_name(&outcome.name, p);
        eprintln!("  1. Open {} and implement analyze().", p.display());
        eprintln!("     `neurogrim explain sensor` covers the contract.");
        eprintln!(
            "  2. Refresh the CMDB: py -3 sensory/check_{}.py . > .claude/{}-cmdb.json",
            py_module, outcome.name
        );
    } else {
        eprintln!("  1. Author a sensor that emits the CMDB envelope shape:");
        eprintln!("     `neurogrim explain sensor` describes the contract.");
        eprintln!(
            "  2. Refresh the CMDB into {} once the sensor exists.",
            outcome.cmdb_path.display()
        );
    }
    eprintln!("  3. Verify the domain shows up: `neurogrim agent --prose`");
    eprintln!("  4. Validate registry shape: `neurogrim doctor`");
    eprintln!("  5. Read the methodology if needed: `neurogrim explain domain`");
}

/// Derive the Python module name (with `_`s) from the sensor path's
/// filename. Robust against Windows / unix path differences.
fn py_module_name(domain: &str, _sensor_path: &PathBuf) -> String {
    domain.replace('-', "_")
}
