//! Domain scaffolder (v3.2 Phase C; relocated from neurogrim-cli to
//! neurogrim-mcp in v3.2.1 so the MCP `domain_new` tool and the
//! `neurogrim domain new` CLI command share a single source of truth).
//!
//! `scaffold_domain` mutates a Brain's `brain-registry.json` (adds
//! entries to `domain_weights`, `principle_map`, `domain_definitions`),
//! creates a stub CMDB at `.claude/<name>-cmdb.json`, and optionally
//! scaffolds a Python sensor skeleton at
//! `<directory>/sensory/check_<name>.py`.
//!
//! Registry mutation is atomic: load → modify in memory → write back
//! pretty-printed JSON. Same pattern as `federation::register`.
//!
//! Three sensor implementation modes:
//!   - `Stub` (default) — registry + CMDB only; sensor authoring deferred
//!   - `Python` — also scaffolds the Python sensor skeleton
//!   - Rust is intentionally NOT supported here. Adding a built-in Rust
//!     sensor edits source in `neurogrim-sensory` and `neurogrim-cli` —
//!     contributor work documented in `explain sensor`, not adopter work.

use anyhow::{anyhow, bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::fs;

/// Sensor implementation type for `scaffold_domain`. Plain `serde`
/// (de)serialization so the MCP tool can accept it as a string argument.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SensorType {
    /// Registry + stub CMDB only; no sensor file.
    Stub,
    /// Registry + stub CMDB + Python sensor skeleton.
    Python,
}

/// What `scaffold_domain` actually did. The CLI uses this to print
/// "next steps" output; the MCP tool uses it to build a structured
/// response.
#[derive(Debug, Clone, Serialize)]
pub struct ScaffoldOutcome {
    pub name: String,
    pub display_name: String,
    pub weight: f64,
    pub registry_path: PathBuf,
    pub cmdb_path: PathBuf,
    pub sensor_path: Option<PathBuf>,
    /// True when the domain was already in the registry (re-register
    /// path with `--force`); false when it was newly added.
    pub was_existing: bool,
}

/// Validate a domain name matches the kebab-case convention used by
/// every existing domain (e.g., `test-health`, `code-quality`,
/// `supply-chain-vigilance`). Same rules as `commands::skill::validate_name`
/// — error messaging differs but the regex is identical.
pub fn validate_name(name: &str) -> Result<()> {
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
pub fn humanize(name: &str) -> String {
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

/// Scaffold a new domain. Returns details of what was done so the
/// caller can render appropriate output (CLI prints "next steps";
/// MCP returns JSON).
///
/// v3.3 F10: `sensor_intent` (when supplied) is recorded as a
/// `_todo_<name>` field on the domain's definition entry. Captures
/// the operator's intent for what a future sensor will observe;
/// useful when re-reading the registry months later.
pub async fn scaffold_domain(
    name: &str,
    description: Option<&str>,
    weight: f64,
    sensor_type: SensorType,
    registry_rel: &str,
    directory: &str,
    force: bool,
    sensor_intent: Option<&str>,
) -> Result<ScaffoldOutcome> {
    validate_name(name).with_context(|| format!("invalid domain name '{name}'"))?;

    let project_root = PathBuf::from(directory);
    if !project_root.is_dir() {
        bail!(
            "directory '{directory}' is not a directory. Pass a project root, \
             or run from inside a project."
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
    let mut domain_def = json!({
        "scoring_source": {
            "type": "cmdb",
            "path": cmdb_rel,
        },
        "exported_variables": {}
    });
    // v3.3 F10: optional `_todo_<name>` field carries operator-supplied
    // sensor intent. The leading underscore makes it a documentation key
    // (the registry's custom deserializer skips it during validation).
    if let Some(intent) = sensor_intent {
        if let Some(obj) = domain_def.as_object_mut() {
            obj.insert(format!("_todo_{name}"), json!(intent));
        }
    }
    defs.insert(name.to_string(), domain_def);

    let serialized = serde_json::to_string_pretty(&registry)? + "\n";
    fs::write(&registry_pb, serialized)
        .await
        .with_context(|| format!("failed to write registry at {}", registry_pb.display()))?;

    // 4. Stub CMDB.
    let cmdb_path = project_root.join(&cmdb_rel);
    if let Some(parent) = cmdb_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    if !(cmdb_path.exists() && !force) {
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

    Ok(ScaffoldOutcome {
        name: name.to_string(),
        display_name: display,
        weight,
        registry_path: registry_pb,
        cmdb_path,
        sensor_path,
        was_existing,
    })
}

fn registry_has_domain(registry: &Value, name: &str) -> bool {
    registry
        .get("config")
        .and_then(|c| c.get("domain_weights"))
        .and_then(|w| w.get(name))
        .is_some()
}

/// Build a stub CMDB JSON for a domain. Score 50, low_confidence: true,
/// single descriptive finding. Mirrors the python-starter + job-hunt
/// pattern (honest "unknown" per spec principle #2). Reused by
/// `neurogrim init --template` (via cli's init_scaffold) and by
/// `neurogrim domain new` (via this module).
pub fn stub_cmdb_json(domain: &str) -> Result<String> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let cmdb = json!({
        "meta": {
            "updated_by": "neurogrim-init",
            "updated_at": now,
            "source": format!("Stub CMDB authored by `neurogrim init` template scaffolder. Score 50 = honest 'unknown' per spec principle #2. Sensor not yet authored for '{domain}'."),
            "schema_version": "1"
        },
        "score": 50,
        "updated_at": now,
        "findings": [{
            "name": format!("{domain}:stub"),
            "status": "info",
            "points": 0,
            "detail": format!("Domain '{domain}' declared in registry; sensor not yet authored. Score 50 reflects honest unknown until a sensory tool produces real signal.")
        }],
        "exported_variables": {
            format!("{domain}:low_confidence"): true,
            format!("{domain}:sensor_authored"): false
        }
    });
    Ok(serde_json::to_string_pretty(&cmdb)? + "\n")
}

/// Render the Python sensor skeleton.
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
        assert_eq!(humanize("supply-chain-vigilance"), "Supply Chain Vigilance");
        assert_eq!(humanize("foo"), "Foo");
    }

    #[test]
    fn stub_cmdb_json_has_required_shape() {
        let s = stub_cmdb_json("my-domain").unwrap();
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["score"], 50);
        assert_eq!(v["meta"]["schema_version"], "1");
        assert!(v["findings"][0]["name"].as_str().unwrap().starts_with("my-domain:"));
    }

    #[tokio::test]
    async fn scaffold_stub_registers_in_three_sections() {
        let tmp = TempDir::new().unwrap();
        let registry_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&registry_dir).unwrap();
        std::fs::write(registry_dir.join("brain-registry.json"), minimal_registry_json()).unwrap();

        let outcome = scaffold_domain(
            "new-domain",
            Some("My Custom Domain"),
            0.0,
            SensorType::Stub,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            false,
            None,
        )
        .await
        .unwrap();

        assert!(!outcome.was_existing);
        assert_eq!(outcome.display_name, "My Custom Domain");
        assert!(outcome.sensor_path.is_none());
        assert!(outcome.cmdb_path.is_file());

        let updated: Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.path().join(".claude/brain-registry.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated["config"]["domain_weights"]["new-domain"], 0.0);
        assert_eq!(
            updated["config"]["principle_map"]["new-domain"],
            "My Custom Domain"
        );
    }

    #[tokio::test]
    async fn scaffold_with_sensor_intent_writes_todo_field() {
        // F10: --sensor-intent appears as `_todo_<name>` on the domain definition.
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        std::fs::write(
            tmp.path().join(".claude/brain-registry.json"),
            minimal_registry_json(),
        )
        .unwrap();

        scaffold_domain(
            "with-intent",
            Some("Domain With Intent"),
            0.0,
            SensorType::Stub,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            false,
            Some("Sensor (when authored) reads X and reports Y."),
        )
        .await
        .unwrap();

        let updated: Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.path().join(".claude/brain-registry.json")).unwrap(),
        )
        .unwrap();
        let def = &updated["config"]["domain_definitions"]["with-intent"];
        assert_eq!(
            def["_todo_with-intent"],
            "Sensor (when authored) reads X and reports Y."
        );
    }

    #[tokio::test]
    async fn scaffold_python_includes_sensor() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        std::fs::write(
            tmp.path().join(".claude/brain-registry.json"),
            minimal_registry_json(),
        )
        .unwrap();

        let outcome = scaffold_domain(
            "py-domain",
            None,
            0.0,
            SensorType::Python,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            false,
            None,
        )
        .await
        .unwrap();

        assert!(outcome.sensor_path.is_some());
        let sensor = outcome.sensor_path.unwrap();
        assert!(sensor.is_file());
        let py = std::fs::read_to_string(&sensor).unwrap();
        assert!(py.contains("def analyze("));
        assert!(py.contains("check-py-domain"));
    }

    #[tokio::test]
    async fn scaffold_refuses_existing_without_force() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        std::fs::write(
            tmp.path().join(".claude/brain-registry.json"),
            minimal_registry_json(),
        )
        .unwrap();

        let err = scaffold_domain(
            "existing",
            None,
            0.5,
            SensorType::Stub,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            false,
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("already registered"));
    }

    #[tokio::test]
    async fn scaffold_force_overwrites() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        std::fs::write(
            tmp.path().join(".claude/brain-registry.json"),
            minimal_registry_json(),
        )
        .unwrap();

        let outcome = scaffold_domain(
            "existing",
            Some("Renamed"),
            0.7,
            SensorType::Stub,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            true,
            None,
        )
        .await
        .unwrap();

        assert!(outcome.was_existing);
        let updated: Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.path().join(".claude/brain-registry.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated["config"]["domain_weights"]["existing"], 0.7);
        assert_eq!(updated["config"]["principle_map"]["existing"], "Renamed");
    }

    #[tokio::test]
    async fn scaffold_rejects_invalid_name() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        std::fs::write(
            tmp.path().join(".claude/brain-registry.json"),
            minimal_registry_json(),
        )
        .unwrap();

        let err = scaffold_domain(
            "BadName",
            None,
            0.0,
            SensorType::Stub,
            ".claude/brain-registry.json",
            tmp.path().to_str().unwrap(),
            false,
            None,
        )
        .await
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("kebab-case") || msg.contains("lowercase"), "got: {msg}");
    }
}
