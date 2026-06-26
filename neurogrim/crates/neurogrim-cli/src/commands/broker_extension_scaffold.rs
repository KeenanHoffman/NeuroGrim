//! A.0.4 — `neurogrim broker-extension-scaffold` — Tier 1 extension template emitter.
//!
//! Generates ready-to-edit TOML config templates for the broker extension
//! system shipped in A.0.1. Operators write these to
//! `.claude/brain/broker/extensions/<broker-id>/<config-name>.toml`; the
//! substrate's [`neurogrim_brokers::ExtensionRegistry::discover_from_disk`]
//! discovers + applies them at host boot.
//!
//! ## V1 templates
//!
//! Per `docs/BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md` and the plan's
//! Gate 4-derived A.0.4 minimum set:
//!
//! - `fact` — workspace fact (key/value, with category)
//! - `terminal-rec` — workspace terminal recommendation (pattern + advice)
//! - `pipeline` — workspace declarative pipeline (read-only fact return)
//! - `sensor` — sensory broker Tier 1 declarative pattern
//!   - Sub-pattern: `file-presence`, `glob-count`, or `cmdb-derived`
//!
//! ## Usage
//!
//! ```bash
//! neurogrim broker-extension-scaffold \
//!     --broker workspace --kind fact --name team-conventions
//!
//! neurogrim broker-extension-scaffold \
//!     --broker sensory --kind sensor --pattern file-presence --name doc-quality
//! ```
//!
//! Emits TOML to stdout by default; with `--out <path>` writes to disk.

use anyhow::{anyhow, Result};
use std::path::PathBuf;

#[derive(Debug)]
pub struct ExtensionScaffoldArgs {
    /// Target broker (e.g., `workspace`, `sensory`). Operator-facing
    /// labeling; routing decision is via `kind` (and `pattern` for
    /// sensors). Kept in the struct so the CLI surface reads cleanly and
    /// future Tier 1 patterns can route by `broker` if multiple brokers
    /// share a kind name.
    #[allow(dead_code)]
    pub broker: String,
    /// Extension kind (e.g., `fact`, `terminal-rec`, `pipeline`, `sensor`)
    pub kind: String,
    /// Name used for the config file + (where applicable) the extension's
    /// `name`/`broker_id` field. Snake-case-or-kebab-case recommended.
    pub name: String,
    /// For `--kind sensor`: which Tier 1 pattern
    /// (`file-presence`, `glob-count`, `cmdb-derived`). Required for
    /// `--kind sensor`; ignored otherwise.
    pub pattern: Option<String>,
    /// Optional output path; default is stdout. If set, the parent dir
    /// is created and the TOML written to `<out>/<name>.toml` (or to
    /// `<out>` directly if the path ends in `.toml`).
    pub out: Option<PathBuf>,
}

pub fn run(args: ExtensionScaffoldArgs) -> Result<()> {
    let template = build_template(&args)?;

    // Emit
    if let Some(out_path) = args.out.as_ref() {
        let final_path = if out_path.extension().and_then(|s| s.to_str()) == Some("toml") {
            out_path.clone()
        } else {
            out_path.join(format!("{}.toml", args.name))
        };
        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&final_path, &template)?;
        eprintln!("wrote extension template to {}", final_path.display());
    } else {
        print!("{}", template);
    }
    Ok(())
}

fn build_template(args: &ExtensionScaffoldArgs) -> Result<String> {
    match args.kind.as_str() {
        "fact" => Ok(template_workspace_fact(&args.name)),
        "terminal-rec" | "terminal-recommendation" => {
            Ok(template_workspace_terminal_rec(&args.name))
        }
        "pipeline" => Ok(template_workspace_pipeline(&args.name)),
        "sensor" | "sensor-decl" | "sensor-declaration" => {
            let pattern = args.pattern.as_deref().unwrap_or("file-presence");
            template_sensor(&args.name, pattern)
        }
        other => Err(anyhow!(
            "unknown extension kind `{}`; expected one of: fact | terminal-rec | pipeline | sensor",
            other
        )),
    }
}

fn template_workspace_fact(name: &str) -> String {
    format!(
        r#"# Workspace fact extension — {name}.
# Discovered by the substrate at host boot:
#   <project_root>/.claude/brain/broker/extensions/workspace/{name}.toml
#
# Operator-declared facts are injected into the workspace broker's
# LocalAwareness store at boot. Agents query them via:
#   workspace/get-fact {{ key = "..." }}
# Facts persist across runs in `.claude/brain/local-awareness.json`.

[extension]
schema_version = "1"
authored_by = "operator"
# Optional free-text description for audit / future-you:
# description = "Team-wide deployment region"

[[facts]]
key = "deployment.primary_region"
value = "us-west-2"
# category: one of `tool-paths` | `environment` | `patterns` | `constraints` | `general`
category = "general"
# Optional note explaining WHY this fact is here:
# note = "Locked in by ops team 2026-Q2; do not change without ops review."

# Add more facts below — each as its own [[facts]] block.
# [[facts]]
# key = "deployment.staging_region"
# value = "us-east-1"
# category = "general"
"#,
        name = name,
    )
}

fn template_workspace_terminal_rec(name: &str) -> String {
    format!(
        r#"# Workspace terminal-recommendation extension — {name}.
# Discovered by the substrate at host boot:
#   <project_root>/.claude/brain/broker/extensions/workspace/{name}.toml
#
# Each [[terminal_recommendations]] block becomes a gotcha visible to
# agents via `workspace/get-terminal-profile`. Use these to encode local
# shell quirks so the agent doesn't burn cycles rediscovering them.

[extension]
schema_version = "1"
authored_by = "operator"

[[terminal_recommendations]]
# Regex applied to the action / command the agent is about to run.
# When it matches, the recommendation text surfaces in the agent's
# terminal-profile query result.
pattern = "^bash -c .*head"
recommendation = "Git Bash on this machine lacks `head`/`tail`/`wc`. Use PowerShell's `Get-Content -TotalCount N` instead."

[[terminal_recommendations]]
pattern = "^aws .*"
recommendation = "Use 'aws-vault exec prod -- aws ...' for production credentials; raw `aws` will fail with no creds."
"#,
        name = name,
    )
}

fn template_workspace_pipeline(name: &str) -> String {
    format!(
        r#"# Workspace pipeline extension — {name}.
# Discovered by the substrate at host boot:
#   <project_root>/.claude/brain/broker/extensions/workspace/{name}.toml
#
# Adds a NEW Surfaced pipeline to the workspace broker. V1 pipelines are
# data-only: they return the value of a workspace fact when dispatched.
# For computational pipelines, author a Tier 2 Rust broker instead.

[extension]
schema_version = "1"
authored_by = "operator"

[[pipelines]]
# Pipeline name appended to the workspace broker's catalog as
# `workspace/{name}` (snake-case the operator-supplied name).
name = "{name}"
description = "Returns the operator-declared <FACT-KEY> fact."
# When dispatched, returns the current value of this fact key.
returns_fact_key = "deployment.primary_region"
"#,
        name = name,
    )
}

fn template_sensor(name: &str, pattern: &str) -> Result<String> {
    let body = match pattern {
        "file-presence" | "file_presence" | "file-presence-score" => template_sensor_file_presence(name),
        "glob-count" | "glob_count" => template_sensor_glob_count(name),
        "cmdb-derived" | "cmdb_derived" => template_sensor_cmdb_derived(name),
        other => {
            return Err(anyhow!(
                "unknown sensor pattern `{}`; expected: file-presence | glob-count | cmdb-derived",
                other
            ));
        }
    };
    Ok(body)
}

fn template_sensor_file_presence(name: &str) -> String {
    format!(
        r#"# Sensory broker Tier 1 extension — {name} (file_presence_score).
# Discovered by the substrate at host boot:
#   <project_root>/.claude/brain/broker/extensions/sensory/{name}.toml
#
# Generates a sensor that scores based on presence + (optional) freshness
# of required files. Score is linear in the fraction of required files
# present; freshness boosts/penalties layered on top when configured.

[extension]
schema_version = "1"
authored_by = "operator"

[sensor]
# Broker id this sensor registers as (must be unique across the cluster).
broker_id = "sensor-{name}"
# Role: sensors are always [sense].
role = "sense"
# Domain name this sensor reports under (matches scoring engine).
domain = "{name}"
# Tier 1 pattern selector — picks the engine implementation.
pattern = "file_presence_score"
description = "Score based on presence of required documentation files."

[sensor.config]
# Files that must exist for full score. Paths are relative to project root.
required_files = [
    "README.md",
    "docs/ARCHITECTURE.md",
    "CONTRIBUTING.md",
]
# Scoring shape: `linear` (each file = 100/N points) | `all-or-nothing` (100 if all present, 0 otherwise)
scoring = "linear"
# Optional: penalize stale files. Files older than `freshness_window_days`
# get `freshness_penalty` deducted from their per-file contribution.
freshness_window_days = 90
freshness_penalty = 25  # percentage points
"#,
        name = name,
    )
}

fn template_sensor_glob_count(name: &str) -> String {
    format!(
        r#"# Sensory broker Tier 1 extension — {name} (glob_count).
# Discovered by the substrate at host boot:
#   <project_root>/.claude/brain/broker/extensions/sensory/{name}.toml
#
# Generates a sensor that scores based on the count of files matching a
# glob pattern. Use this for things like "how many TODOs / FIXMEs are
# left", "how many test files exist", "how big is the migration backlog".

[extension]
schema_version = "1"
authored_by = "operator"

[sensor]
broker_id = "sensor-{name}"
role = "sense"
domain = "{name}"
pattern = "glob_count"
description = "Score based on count of files matching a glob pattern."

[sensor.config]
# Glob pattern (relative to project root).
glob = "**/*.todo.md"
# Scoring shape:
#   `inverse` — fewer matches = higher score (0 matches = 100, ramp down)
#   `direct`  — more matches = higher score (capped at ceiling)
scoring = "inverse"
# Ceiling: matches above this are clamped (avoid score going negative or
# blowing past 100).
ceiling = 50
"#,
        name = name,
    )
}

fn template_sensor_cmdb_derived(name: &str) -> String {
    format!(
        r#"# Sensory broker Tier 1 extension — {name} (cmdb_derived).
# Discovered by the substrate at host boot:
#   <project_root>/.claude/brain/broker/extensions/sensory/{name}.toml
#
# Generates a sensor whose score is a function of OTHER sensors' CMDBs.
# Useful for composite domains like "release-readiness = min(deploy,
# security, test-health)".

[extension]
schema_version = "1"
authored_by = "operator"

[sensor]
broker_id = "sensor-{name}"
role = "sense"
domain = "{name}"
pattern = "cmdb_derived"
description = "Composite score derived from sibling sensor CMDBs."

[sensor.config]
# Sibling CMDB sources by their sensor name (resolves to
# `.claude/<sensor-name>-cmdb.json`).
sources = [
    "deploy-readiness",
    "security-standards",
    "test-health",
]
# Function applied to the source scores. One of:
#   `min`     — pessimistic (release-readiness style)
#   `max`     — optimistic
#   `mean`    — average
#   `median`  — robust to outliers
combinator = "min"
"#,
        name = name,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(broker: &str, kind: &str, name: &str) -> ExtensionScaffoldArgs {
        ExtensionScaffoldArgs {
            broker: broker.to_string(),
            kind: kind.to_string(),
            name: name.to_string(),
            pattern: None,
            out: None,
        }
    }

    #[test]
    fn workspace_fact_template_has_required_sections() {
        let args = fixture("workspace", "fact", "team-conventions");
        let body = build_template(&args).unwrap();
        assert!(body.contains("[extension]"));
        assert!(body.contains("schema_version = \"1\""));
        assert!(body.contains("[[facts]]"));
        assert!(body.contains("team-conventions"));
    }

    #[test]
    fn workspace_terminal_rec_template_emits_pattern_and_recommendation() {
        let args = fixture("workspace", "terminal-rec", "team-shell-quirks");
        let body = build_template(&args).unwrap();
        assert!(body.contains("[[terminal_recommendations]]"));
        assert!(body.contains("pattern = "));
        assert!(body.contains("recommendation = "));
    }

    #[test]
    fn workspace_pipeline_template_includes_pipeline_name() {
        let args = fixture("workspace", "pipeline", "get-deploy-region");
        let body = build_template(&args).unwrap();
        assert!(body.contains("[[pipelines]]"));
        assert!(body.contains("name = \"get-deploy-region\""));
        assert!(body.contains("returns_fact_key"));
    }

    #[test]
    fn sensor_file_presence_template_is_complete() {
        let mut args = fixture("sensory", "sensor", "doc-quality");
        args.pattern = Some("file-presence".to_string());
        let body = build_template(&args).unwrap();
        assert!(body.contains("pattern = \"file_presence_score\""));
        assert!(body.contains("required_files"));
        assert!(body.contains("scoring = "));
    }

    #[test]
    fn sensor_glob_count_template_is_complete() {
        let mut args = fixture("sensory", "sensor", "todo-backlog");
        args.pattern = Some("glob-count".to_string());
        let body = build_template(&args).unwrap();
        assert!(body.contains("pattern = \"glob_count\""));
        assert!(body.contains("glob = "));
        assert!(body.contains("ceiling"));
    }

    #[test]
    fn sensor_cmdb_derived_template_is_complete() {
        let mut args = fixture("sensory", "sensor", "release-readiness");
        args.pattern = Some("cmdb-derived".to_string());
        let body = build_template(&args).unwrap();
        assert!(body.contains("pattern = \"cmdb_derived\""));
        assert!(body.contains("sources = "));
        assert!(body.contains("combinator"));
    }

    #[test]
    fn sensor_default_pattern_is_file_presence() {
        let args = fixture("sensory", "sensor", "default-pattern");
        let body = build_template(&args).unwrap();
        assert!(body.contains("file_presence_score"));
    }

    #[test]
    fn unknown_kind_rejected() {
        let args = fixture("workspace", "nonexistent-kind", "x");
        let err = build_template(&args).unwrap_err();
        assert!(err.to_string().contains("unknown extension kind"));
    }

    #[test]
    fn unknown_sensor_pattern_rejected() {
        let mut args = fixture("sensory", "sensor", "x");
        args.pattern = Some("nonexistent-pattern".to_string());
        let err = build_template(&args).unwrap_err();
        assert!(err.to_string().contains("unknown sensor pattern"));
    }

    #[test]
    fn out_path_writes_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut args = fixture("workspace", "fact", "test-facts");
        args.out = Some(tmp.path().to_path_buf());
        run(args).unwrap();
        let written = tmp.path().join("test-facts.toml");
        assert!(written.exists());
        let body = std::fs::read_to_string(&written).unwrap();
        assert!(body.contains("[extension]"));
    }
}
