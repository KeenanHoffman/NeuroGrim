//! `neurogrim domain new <name>` — scaffold a new domain (v3.2 Phase C).
//!
//! Mirrors `neurogrim skill new` and `neurogrim federation register`
//! UX: kebab-case validation, idempotent re-registration with
//! `--force`, "next steps" output pointing at follow-on commands.
//!
//! Three sensor implementation modes:
//!
//! - `--type stub` (default) — registry mutation + stub CMDB only.
//!   The domain is declared; the sensor is "not yet authored." This
//!   is the legitimate v1 starting point per the v3.2 methodology
//!   primer (`neurogrim explain domain`).
//!
//! - `--type python` — additionally scaffolds
//!   `<directory>/sensory/check_<name>.py` with a `SensoryTool`
//!   skeleton. Operator edits the `analyze()` function to populate
//!   findings.
//!
//! - `--type rust` is intentionally unsupported. Adding a Rust sensor
//!   requires editing source in `neurogrim-sensory` and `neurogrim-cli`
//!   crates — a contributor task, not an adopter task. NeuroGrim
//!   contributors author Rust sensors by hand using `explain sensor`
//!   as guidance.
//!
//! Registry mutation is atomic: load → modify in memory → write back
//! pretty-printed JSON. Same pattern as `federation::register`.

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::fs;

use crate::commands::init_scaffold::stub_cmdb_json;

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
        /// Promote later by editing the registry directly.
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum SensorType {
    /// Registry + stub CMDB only; no sensor file.
    Stub,
    /// Registry + stub CMDB + Python sensor skeleton.
    Python,
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
            cmd_new(
                &name,
                description.as_deref(),
                weight,
                r#type,
                &registry,
                &directory,
                force,
            )
            .await
        }
    }
}

/// Validate a domain name matches the kebab-case convention used by
/// every existing domain (e.g., `test-health`, `code-quality`,
/// `supply-chain-vigilance`).
///
/// Same shape as `commands::skill::validate_name`. Factored separately
/// because the error messaging is domain-specific; the rules are
/// identical.
pub(crate) fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("domain name cannot be empty");
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_lowercase() {
        bail!(
            "domain name must start with a lowercase letter; got '{name}'. \
             Convention: kebab-case (e.g., 'test-coverage', 'supply-chain-vigilance')."
        );
    }
    for c in name.chars() {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            bail!(
                "domain name must contain only lowercase letters, digits, and \
                 hyphens; got '{name}' (offending char: '{c}'). \
                 Convention: kebab-case."
            );
        }
    }
    if name.contains("--") {
        bail!("domain name must not contain consecutive hyphens; got '{name}'");
    }
    if name.ends_with('-') {
        bail!("domain name must not end with a hyphen; got '{name}'");
    }
    Ok(())
}

/// Convert kebab-case to Title Case: "test-coverage" → "Test Coverage".
pub(crate) fn humanize(name: &str) -> String {
    name.split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(ch) => ch.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

async fn cmd_new(
    name: &str,
    description: Option<&str>,
    weight: f64,
    sensor_type: SensorType,
    registry_rel: &str,
    directory: &str,
    force: bool,
) -> Result<()> {
    validate_name(name).with_context(|| format!("invalid domain name '{name}'"))?;

    let project_root = PathBuf::from(directory);
    if !project_root.is_dir() {
        bail!(
            "directory '{directory}' is not a directory. Pass --directory \
             <project-root> or run from inside a project."
        );
    }
    let registry_pb = project_root.join(registry_rel);
    if !registry_pb.is_file() {
        bail!(
            "registry not found at {}. Run `neurogrim init --template <kind>` \
             to scaffold a Brain first.",
            registry_pb.display()
        );
    }

    let display = description
        .map(|s| s.to_string())
        .unwrap_or_else(|| humanize(name));
    let cmdb_rel = format!(".claude/{name}-cmdb.json");

    // Load + mutate the registry in memory; atomic-write back at the end.
    let registry_text = fs::read_to_string(&registry_pb).await?;
    let mut registry: Value = serde_json::from_str(&registry_text)
        .with_context(|| format!("failed to parse {} as JSON", registry_pb.display()))?;

    let was_existing = registry_has_domain(&registry, name);
    if was_existing && !force {
        bail!(
            "domain '{name}' is already registered in {}. Pass --force to \
             overwrite the registry entries + stub CMDB, or pick a different name.",
            registry_pb.display()
        );
    }

    let config = registry
        .get_mut("config")
        .ok_or_else(|| anyhow!("registry has no `config` block"))?
        .as_object_mut()
        .ok_or_else(|| anyhow!("registry's `config` is not an object"))?;

    // 1. domain_weights
    let weights = config
        .entry("domain_weights".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow!("config.domain_weights is not an object"))?;
    weights.insert(name.to_string(), json!(weight));

    // 2. principle_map
    let pm = config
        .entry("principle_map".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow!("config.principle_map is not an object"))?;
    pm.insert(name.to_string(), json!(display));

    // 3. domain_definitions
    let defs = config
        .entry("domain_definitions".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow!("config.domain_definitions is not an object"))?;
    defs.insert(
        name.to_string(),
        json!({
            "scoring_source": {
                "type": "cmdb",
                "path": cmdb_rel,
            },
            "exported_variables": {}
        }),
    );

    let serialized = serde_json::to_string_pretty(&registry)? + "\n";
    fs::write(&registry_pb, serialized)
        .await
        .with_context(|| format!("failed to write registry at {}", registry_pb.display()))?;

    // 4. Stub CMDB. Reuse init_scaffold::stub_cmdb_json so the shape stays
    //    identical to what `neurogrim init` produces for declared domains.
    let cmdb_path = project_root.join(&cmdb_rel);
    if let Some(parent) = cmdb_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    if cmdb_path.exists() && !force {
        // Idempotent re-registration without --force: registry was already
        // updated above; we tolerate an existing CMDB without rewriting it.
        // This matches the federation::register pattern.
    } else {
        let content = stub_cmdb_json(name)?;
        fs::write(&cmdb_path, content)
            .await
            .with_context(|| format!("failed to write {}", cmdb_path.display()))?;
    }

    // 5. Optional Python sensor skeleton.
    let mut sensor_path: Option<PathBuf> = None;
    if sensor_type == SensorType::Python {
        let path = project_root
            .join("sensory")
            .join(format!("check_{}.py", name.replace('-', "_")));
        if path.exists() && !force {
            bail!(
                "{} already exists. Pass --force to overwrite, or remove it first.",
                path.display()
            );
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let py = render_python_sensor_skeleton(name, &display);
        fs::write(&path, py)
            .await
            .with_context(|| format!("failed to write {}", path.display()))?;
        sensor_path = Some(path);
    }

    print_next_steps(name, &display, weight, &registry_pb, &cmdb_path, sensor_path.as_ref(), was_existing);
    Ok(())
}

fn registry_has_domain(registry: &Value, name: &str) -> bool {
    registry
        .get("config")
        .and_then(|c| c.get("domain_weights"))
        .and_then(|w| w.get(name))
        .is_some()
}

/// Render the Python sensor skeleton. Mirrors the contract documented in
/// `data/explain/sensor.md`: `analyze(project_root) -> dict` returning a
/// CMDB envelope.
fn render_python_sensor_skeleton(domain: &str, display: &str) -> String {
    format!(
        r#""""Sensor: check-{domain}. Measures the '{domain}' domain ({display}).

Scaffolded by `neurogrim domain new {domain} --type python`.

Replace the TODO blocks below with real logic. The returned dict MUST
match the CMDB envelope schema:

  {{
    "meta": {{ "schema_version": "1", "updated_by": "...", "updated_at": "..." }},
    "score": 0..100,
    "updated_at": "...",
    "findings": [
      {{ "name": "...", "status": "...", "points": int, "detail": "..." }}
    ]
  }}

Run via:

  py -3 sensory/check_{domain_underscored}.py . > .claude/{domain}-cmdb.json

See `neurogrim explain sensor` for the full authoring contract.
"""
import json
import sys
from datetime import datetime, timezone


def analyze(project_root: str) -> dict:
    findings: list[dict] = []
    score = 100

    # TODO — read project state, append findings, adjust score.
    # Examples:
    #   - Walk a directory tree, count violations, subtract per finding.
    #   - Parse a config file, check for required keys.
    #   - Read a JSON artifact (lint output, coverage report) and tally.
    #
    # Each finding shape:
    #   {{
    #     "name": "stable_identifier",  # snake_case
    #     "status": "pass" | "warn" | "error" | "info",
    #     "points": -2,                  # contribution to score
    #     "detail": "human-readable explanation"
    #   }}

    score = max(0, min(100, score))

    return {{
        "meta": {{
            "schema_version": "1",
            "updated_by": "check-{domain}",
            "updated_at": _now(),
        }},
        "score": score,
        "updated_at": _now(),
        "findings": findings,
    }}


def _now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


if __name__ == "__main__":
    project_root = sys.argv[1] if len(sys.argv) > 1 else "."
    print(json.dumps(analyze(project_root), indent=2))
"#,
        domain = domain,
        display = display,
        domain_underscored = domain.replace('-', "_"),
    )
}

fn print_next_steps(
    name: &str,
    display: &str,
    weight: f64,
    registry: &PathBuf,
    cmdb: &PathBuf,
    sensor: Option<&PathBuf>,
    was_existing: bool,
) {
    let action = if was_existing { "Updated" } else { "Registered" };
    let posture = if weight > 0.0 {
        format!("weight {weight}")
    } else {
        "advisory (weight 0.0)".to_string()
    };
    eprintln!("{action} domain '{name}' as {posture} — {display}");
    eprintln!("  Registry:  {}", registry.display());
    eprintln!("  Stub CMDB: {}", cmdb.display());
    if let Some(p) = sensor {
        eprintln!("  Sensor:    {}", p.display());
    }
    eprintln!();
    eprintln!("Next steps:");
    if let Some(p) = sensor {
        eprintln!("  1. Open {} and implement analyze().", p.display());
        eprintln!("     `neurogrim explain sensor` covers the contract.");
        eprintln!(
            "  2. Refresh the CMDB: py -3 sensory/check_{}.py . > .claude/{name}-cmdb.json",
            name.replace('-', "_")
        );
    } else {
        eprintln!("  1. Author a sensor that emits the CMDB envelope shape:");
        eprintln!("     `neurogrim explain sensor` describes the contract.");
        eprintln!(
            "  2. Refresh the CMDB into {} once the sensor exists.",
            cmdb.display()
        );
    }
    eprintln!("  3. Verify the domain shows up: `neurogrim agent --prose`");
    eprintln!("  4. Validate registry shape: `neurogrim doctor`");
    eprintln!("  5. Read the methodology if needed: `neurogrim explain domain`");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn minimal_registry_json() -> String {
        json!({
            "meta": {"schema_version": "2", "description": "test", "updated_by": "test"},
            "tools": {},
            "data_sources": {},
            "config": {
                "domain_weights": {"existing": 1.0},
                "principle_map": {"existing": "Existing"},
                "domain_definitions": {
                    "existing": {"scoring_source": {"type": "cmdb", "path": ".claude/existing-cmdb.json"}}
                }
            }
        })
        .to_string()
    }

    #[test]
    fn validate_name_accepts_canonical() {
        for n in ["foo", "foo-bar", "supply-chain-vigilance", "x123"] {
            validate_name(n).unwrap_or_else(|e| panic!("'{n}' should be valid: {e}"));
        }
    }

    #[test]
    fn validate_name_rejects_bad_inputs() {
        for n in ["", "Foo", "foo_bar", "1foo", "foo--bar", "foo-"] {
            assert!(validate_name(n).is_err(), "'{n}' should be rejected");
        }
    }

    #[test]
    fn humanize_kebab_case() {
        assert_eq!(humanize("test-coverage"), "Test Coverage");
        assert_eq!(
            humanize("supply-chain-vigilance"),
            "Supply Chain Vigilance"
        );
        assert_eq!(humanize("foo"), "Foo");
    }

    #[tokio::test]
    async fn cmd_new_stub_registers_in_three_sections() {
        let tmp = TempDir::new().unwrap();
        let registry_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&registry_dir).unwrap();
        let registry_path = registry_dir.join("brain-registry.json");
        std::fs::write(&registry_path, minimal_registry_json()).unwrap();

        cmd_new(
            "new-domain",
            Some("My Custom Domain"),
            0.0,
            SensorType::Stub,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            false,
        )
        .await
        .unwrap();

        let updated: Value =
            serde_json::from_str(&std::fs::read_to_string(&registry_path).unwrap()).unwrap();
        assert_eq!(updated["config"]["domain_weights"]["new-domain"], 0.0);
        assert_eq!(
            updated["config"]["principle_map"]["new-domain"],
            "My Custom Domain"
        );
        assert_eq!(
            updated["config"]["domain_definitions"]["new-domain"]["scoring_source"]["type"],
            "cmdb"
        );
        assert_eq!(
            updated["config"]["domain_definitions"]["new-domain"]["scoring_source"]["path"],
            ".claude/new-domain-cmdb.json"
        );

        // Stub CMDB exists.
        let cmdb_path = tmp.path().join(".claude/new-domain-cmdb.json");
        assert!(cmdb_path.is_file());
        let cmdb: Value =
            serde_json::from_str(&std::fs::read_to_string(&cmdb_path).unwrap()).unwrap();
        assert_eq!(cmdb["score"], 50);
    }

    #[tokio::test]
    async fn cmd_new_python_scaffolds_sensor() {
        let tmp = TempDir::new().unwrap();
        let registry_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&registry_dir).unwrap();
        std::fs::write(
            registry_dir.join("brain-registry.json"),
            minimal_registry_json(),
        )
        .unwrap();

        cmd_new(
            "py-domain",
            None,
            0.0,
            SensorType::Python,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            false,
        )
        .await
        .unwrap();

        let sensor_path = tmp.path().join("sensory/check_py_domain.py");
        assert!(sensor_path.is_file());
        let py = std::fs::read_to_string(&sensor_path).unwrap();
        assert!(py.contains("check-py-domain"));
        assert!(py.contains("def analyze("));
        assert!(py.contains("schema_version"));
    }

    #[tokio::test]
    async fn cmd_new_default_description_humanizes_name() {
        let tmp = TempDir::new().unwrap();
        let registry_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&registry_dir).unwrap();
        std::fs::write(
            registry_dir.join("brain-registry.json"),
            minimal_registry_json(),
        )
        .unwrap();

        cmd_new(
            "test-coverage",
            None,
            0.0,
            SensorType::Stub,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            false,
        )
        .await
        .unwrap();

        let updated: Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.path().join(".claude/brain-registry.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            updated["config"]["principle_map"]["test-coverage"],
            "Test Coverage"
        );
    }

    #[tokio::test]
    async fn cmd_new_refuses_existing_without_force() {
        let tmp = TempDir::new().unwrap();
        let registry_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&registry_dir).unwrap();
        std::fs::write(
            registry_dir.join("brain-registry.json"),
            minimal_registry_json(),
        )
        .unwrap();

        // 'existing' is in the fixture; reject without --force.
        let err = cmd_new(
            "existing",
            None,
            0.5,
            SensorType::Stub,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            false,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("already registered"));
    }

    #[tokio::test]
    async fn cmd_new_force_overwrites_existing() {
        let tmp = TempDir::new().unwrap();
        let registry_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&registry_dir).unwrap();
        std::fs::write(
            registry_dir.join("brain-registry.json"),
            minimal_registry_json(),
        )
        .unwrap();

        cmd_new(
            "existing",
            Some("Newly Renamed"),
            0.7,
            SensorType::Stub,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            true, // force
        )
        .await
        .unwrap();

        let updated: Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.path().join(".claude/brain-registry.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated["config"]["domain_weights"]["existing"], 0.7);
        assert_eq!(updated["config"]["principle_map"]["existing"], "Newly Renamed");
    }

    #[tokio::test]
    async fn cmd_new_rejects_invalid_name() {
        let tmp = TempDir::new().unwrap();
        let registry_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&registry_dir).unwrap();
        std::fs::write(
            registry_dir.join("brain-registry.json"),
            minimal_registry_json(),
        )
        .unwrap();

        let err = cmd_new(
            "BadName",
            None,
            0.0,
            SensorType::Stub,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            false,
        )
        .await
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("kebab-case") || msg.contains("lowercase"),
            "got: {msg}"
        );
    }

    #[tokio::test]
    async fn cmd_new_fails_when_registry_missing() {
        let tmp = TempDir::new().unwrap();
        let err = cmd_new(
            "x",
            None,
            0.0,
            SensorType::Stub,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            false,
        )
        .await
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("not found"), "got: {msg}");
    }
}
