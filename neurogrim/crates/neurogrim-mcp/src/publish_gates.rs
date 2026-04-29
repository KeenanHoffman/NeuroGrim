//! Publish-gates manifest loader + validator (S12-G-3, v4.0).
//!
//! Reads `<brain>/.claude/brain/publish-gates.yaml`, validates it
//! against the embedded `publish-gates-v1.schema.json` Draft-07
//! schema, and deserializes the result into the typed
//! [`PublishGatesConfig`] view that the publish-gate runner
//! (S12-G-4) and the doctor check (this stage) consume.
//!
//! ## Layered validation
//!
//! Two layers, intentionally overlapping:
//!
//! 1. **JSON Schema** — closed vocabulary, kebab-case `id` pattern,
//!    `if/then` rule for "manual gates require `instructions`",
//!    timeout bounds. Authoritative contract; the schema is the
//!    single source of truth for shape.
//! 2. **Typed `serde_yaml::from_value`** — produces the ergonomic
//!    [`PublishGatesConfig`] view. Does NOT use
//!    `deny_unknown_fields` because the schema already covers that
//!    discipline; the typed layer's job is convenient access, not
//!    re-validation.
//!
//! Mirrors the `neurogrim-sensory::trust_budget` pattern (schema
//! vendored under the crate's `data/schemas/`, compiled inline,
//! errors emitted per validation failure).
//!
//! ## Error semantics
//!
//! - YAML parse failure → `Err(PublishGatesError::Yaml(_))` — single
//!   error, full parse-error text.
//! - Schema-validation failures → `Err(PublishGatesError::Schema(_))`
//!   carrying one [`SchemaIssue`] per validation error (path +
//!   message). The doctor check emits one `Finding` per issue.
//! - Duplicate gate IDs → `Err(PublishGatesError::DuplicateIds(_))`
//!   listing the offenders. The schema can't easily express
//!   uniqueness, so this is post-validate in Rust.
//!
//! Successful return = (parses, schema-valid, no duplicate IDs).

use jsonschema::{Draft, JSONSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

/// Embedded Draft-07 schema. v4.0 — vendored next to the module so
/// it resolves in `cargo publish` tarballs (mirrors trust_budget's
/// 3.2.2 vendoring decision).
const PUBLISH_GATES_SCHEMA_JSON: &str =
    include_str!("../data/schemas/publish-gates-v1.schema.json");

/// Default expected manifest path relative to a Brain's project root.
pub const PUBLISH_GATES_MANIFEST_RELPATH: &str = ".claude/brain/publish-gates.yaml";

// ── Typed view ───────────────────────────────────────────────────────────

/// Top-level manifest. One per Brain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishGatesConfig {
    pub schema_version: String,
    pub gates: Vec<Gate>,
}

/// Single gate declaration. Fields not relevant to a given
/// `gate_type` are `None`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Gate {
    pub id: String,
    pub gate_type: GateType,
    pub description: String,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub blocking: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub timeout_seconds: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub check_command: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub instructions: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub operator_required: Option<bool>,
}

/// Closed vocabulary of gate kinds. Mirrors the schema enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum GateType {
    /// Runner executes `check_command`, exit 0 = pass.
    Automated,
    /// Runner prints `instructions`, prompts operator y/n.
    Manual,
    /// Runner invokes the Playwright harness (S12-G-5).
    E2e,
}

// ── Errors ───────────────────────────────────────────────────────────────

/// Single schema-validation error. Path is JSON-Pointer style
/// (e.g. `/gates/0/id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaIssue {
    pub path: String,
    pub message: String,
}

impl fmt::Display for SchemaIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// All ways a manifest can fail to load.
#[derive(Debug)]
pub enum PublishGatesError {
    /// YAML syntax error or non-mapping root.
    Yaml(String),
    /// One or more JSON-Schema validation failures.
    Schema(Vec<SchemaIssue>),
    /// Two or more gates declared the same `id`.
    DuplicateIds(Vec<String>),
    /// Manifest file missing on disk (only returned by [`load_publish_gates`]).
    NotFound,
    /// Manifest file present but unreadable (permissions, EIO, etc).
    Io(String),
}

impl fmt::Display for PublishGatesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublishGatesError::Yaml(s) => write!(f, "YAML parse failed: {s}"),
            PublishGatesError::Schema(issues) => {
                writeln!(
                    f,
                    "publish-gates manifest failed schema validation ({} issue(s)):",
                    issues.len()
                )?;
                for i in issues {
                    writeln!(f, "  - {i}")?;
                }
                Ok(())
            }
            PublishGatesError::DuplicateIds(ids) => {
                write!(f, "duplicate gate id(s): {}", ids.join(", "))
            }
            PublishGatesError::NotFound => {
                write!(f, "publish-gates.yaml not found")
            }
            PublishGatesError::Io(s) => write!(f, "I/O error reading manifest: {s}"),
        }
    }
}

impl std::error::Error for PublishGatesError {}

// ── Public API ───────────────────────────────────────────────────────────

/// Load + validate a manifest file. Convenience wrapper around
/// [`validate_publish_gates_yaml`] that also handles the
/// missing-file case.
pub fn load_publish_gates(path: &Path) -> Result<PublishGatesConfig, PublishGatesError> {
    let text = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(PublishGatesError::NotFound);
        }
        Err(e) => return Err(PublishGatesError::Io(e.to_string())),
    };
    validate_publish_gates_yaml(&text)
}

/// Parse YAML text → schema-validate → typed view. Single-shot.
pub fn validate_publish_gates_yaml(yaml_text: &str) -> Result<PublishGatesConfig, PublishGatesError> {
    // ── Phase 1: YAML → serde_json::Value ─────────────────────────────
    // We round-trip through serde_json::Value because the jsonschema
    // crate validates serde_json values directly. serde_yaml::Value
    // can be re-serialized into a serde_json::Value via serde, but
    // going through a single conversion is simplest.
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_text)
        .map_err(|e| PublishGatesError::Yaml(e.to_string()))?;
    let json_value: JsonValue = serde_json::to_value(&yaml_value)
        .map_err(|e| PublishGatesError::Yaml(format!("YAML→JSON conversion failed: {e}")))?;

    // ── Phase 2: schema validation ────────────────────────────────────
    let schema = compile_schema_inline()
        .ok_or_else(|| PublishGatesError::Yaml(
            "embedded schema failed to compile — this is a NeuroGrim bug".to_string(),
        ))?;
    if let Err(errs) = schema.validate(&json_value) {
        let mut issues: Vec<SchemaIssue> = Vec::new();
        for err in errs {
            issues.push(SchemaIssue {
                path: err.instance_path.to_string(),
                message: err.to_string(),
            });
        }
        // Sort for deterministic output (some validators emit errors in
        // hash-iteration order).
        issues.sort_by(|a, b| a.path.cmp(&b.path).then(a.message.cmp(&b.message)));
        return Err(PublishGatesError::Schema(issues));
    }

    // ── Phase 3: typed deserialize ────────────────────────────────────
    let config: PublishGatesConfig = serde_json::from_value(json_value)
        .map_err(|e| PublishGatesError::Yaml(format!(
            "schema-valid manifest failed typed deserialize — likely a schema/typed-view drift bug: {e}"
        )))?;

    // ── Phase 4: post-validate uniqueness ─────────────────────────────
    let mut seen: BTreeMap<&str, u32> = BTreeMap::new();
    for g in &config.gates {
        *seen.entry(g.id.as_str()).or_insert(0) += 1;
    }
    let dups: Vec<String> = seen
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(id, _)| (*id).to_string())
        .collect();
    if !dups.is_empty() {
        return Err(PublishGatesError::DuplicateIds(dups));
    }

    Ok(config)
}

/// Compile the embedded schema once. Returns `None` only if the
/// vendored schema itself is malformed (a NeuroGrim bug).
fn compile_schema_inline() -> Option<JSONSchema> {
    let parsed: JsonValue = serde_json::from_str(PUBLISH_GATES_SCHEMA_JSON).ok()?;
    JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&parsed)
        .ok()
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Minimal manifest exercising the simplest valid shape: one
    /// automated gate with the required fields.
    const MIN_AUTOMATED: &str = r#"
schema_version: "1"
gates:
  - id: tests-pass
    gate_type: automated
    description: All tests green via `neurogrim test`
    check_command: "neurogrim test"
"#;

    /// Full example with one of each gate_type, exercising optional
    /// fields and the `if/then` branching for manual.
    const FULL_EXAMPLE: &str = r#"
schema_version: "1"
gates:
  - id: tests-pass
    gate_type: automated
    description: All tests green
    blocking: true
    timeout_seconds: 120
    check_command: "neurogrim test"
  - id: dashboard-loads-locally
    gate_type: manual
    description: Operator visits dashboard and verifies render
    operator_required: true
    instructions: |
      1. Run `neurogrim ui --allow-mutations`
      2. Navigate to /brains/<id>/federation
      3. Verify peer list renders without errors
  - id: e2e-smoke
    gate_type: e2e
    description: Playwright smoke specs
    blocking: true
    timeout_seconds: 180
"#;

    #[test]
    fn loads_minimal_valid_config() {
        let cfg = validate_publish_gates_yaml(MIN_AUTOMATED).expect("min should validate");
        assert_eq!(cfg.schema_version, "1");
        assert_eq!(cfg.gates.len(), 1);
        assert_eq!(cfg.gates[0].id, "tests-pass");
        assert_eq!(cfg.gates[0].gate_type, GateType::Automated);
        assert_eq!(cfg.gates[0].check_command.as_deref(), Some("neurogrim test"));
    }

    #[test]
    fn loads_full_example_with_all_three_gate_types() {
        let cfg = validate_publish_gates_yaml(FULL_EXAMPLE).expect("full example should validate");
        assert_eq!(cfg.gates.len(), 3);
        assert_eq!(cfg.gates[0].gate_type, GateType::Automated);
        assert_eq!(cfg.gates[1].gate_type, GateType::Manual);
        assert_eq!(cfg.gates[2].gate_type, GateType::E2e);
        // Manual gate's instructions populated
        assert!(cfg.gates[1]
            .instructions
            .as_ref()
            .unwrap()
            .contains("Run `neurogrim ui --allow-mutations`"));
    }

    #[test]
    fn rejects_missing_schema_version() {
        let yaml = r#"
gates:
  - id: tests-pass
    gate_type: automated
    description: x
    check_command: "x"
"#;
        let err = validate_publish_gates_yaml(yaml).expect_err("missing schema_version");
        match err {
            PublishGatesError::Schema(issues) => {
                assert!(
                    issues.iter().any(|i| i.message.contains("schema_version")),
                    "expected schema_version error, got: {issues:?}"
                );
            }
            other => panic!("expected Schema error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_gate_type() {
        let yaml = r#"
schema_version: "1"
gates:
  - id: weird
    gate_type: telepathic
    description: x
"#;
        let err = validate_publish_gates_yaml(yaml).expect_err("unknown gate_type");
        match err {
            PublishGatesError::Schema(issues) => {
                assert!(
                    issues.iter().any(|i| i.path.ends_with("/gate_type")),
                    "expected /gate_type error, got: {issues:?}"
                );
            }
            other => panic!("expected Schema error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_kebab_case_id() {
        // Underscores, capitals, leading digit — all invalid.
        for bad_id in ["Tests_Pass", "TestsPass", "1tests-pass", "tests_pass"] {
            let yaml = format!(
                r#"
schema_version: "1"
gates:
  - id: {bad_id}
    gate_type: automated
    description: x
    check_command: "x"
"#
            );
            let err =
                validate_publish_gates_yaml(&yaml).expect_err("bad id should fail");
            match err {
                PublishGatesError::Schema(issues) => {
                    assert!(
                        issues.iter().any(|i| i.path.ends_with("/id")),
                        "id={bad_id}: expected /id pattern error, got: {issues:?}"
                    );
                }
                other => panic!("id={bad_id}: expected Schema error, got {other:?}"),
            }
        }
    }

    #[test]
    fn manual_gate_requires_instructions() {
        let yaml = r#"
schema_version: "1"
gates:
  - id: smoke-render
    gate_type: manual
    description: operator verifies render
"#;
        let err = validate_publish_gates_yaml(yaml).expect_err("manual without instructions");
        match err {
            PublishGatesError::Schema(_) => { /* OK — schema's allOf branch fires */ }
            other => panic!("expected Schema error, got {other:?}"),
        }
    }

    #[test]
    fn automated_gate_requires_check_command() {
        let yaml = r#"
schema_version: "1"
gates:
  - id: tests-pass
    gate_type: automated
    description: x
"#;
        let err =
            validate_publish_gates_yaml(yaml).expect_err("automated without check_command");
        match err {
            PublishGatesError::Schema(_) => {}
            other => panic!("expected Schema error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let yaml = r#"
schema_version: "1"
gates:
  - id: tests-pass
    gate_type: automated
    description: x
    check_command: "x"
mystery_field: oops
"#;
        let err =
            validate_publish_gates_yaml(yaml).expect_err("unknown top-level field");
        match err {
            PublishGatesError::Schema(issues) => {
                assert!(
                    issues
                        .iter()
                        .any(|i| i.message.contains("mystery_field")
                            || i.message.contains("Additional properties")),
                    "expected additionalProperties error mentioning mystery_field; got: {issues:?}"
                );
            }
            other => panic!("expected Schema error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_gate_field() {
        let yaml = r#"
schema_version: "1"
gates:
  - id: tests-pass
    gate_type: automated
    description: x
    check_command: "x"
    rocket_fuel: explosive
"#;
        let err =
            validate_publish_gates_yaml(yaml).expect_err("unknown gate field");
        match err {
            PublishGatesError::Schema(issues) => {
                assert!(
                    issues.iter().any(|i| i
                        .message
                        .to_lowercase()
                        .contains("additional propert")),
                    "expected additionalProperties error; got: {issues:?}"
                );
            }
            other => panic!("expected Schema error, got {other:?}"),
        }
    }

    #[test]
    fn detects_duplicate_gate_ids() {
        let yaml = r#"
schema_version: "1"
gates:
  - id: tests-pass
    gate_type: automated
    description: first
    check_command: "neurogrim test"
  - id: tests-pass
    gate_type: automated
    description: second
    check_command: "neurogrim test --slow"
"#;
        let err = validate_publish_gates_yaml(yaml).expect_err("dup ids should fail");
        match err {
            PublishGatesError::DuplicateIds(ids) => {
                assert_eq!(ids, vec!["tests-pass".to_string()]);
            }
            other => panic!("expected DuplicateIds, got {other:?}"),
        }
    }

    #[test]
    fn rejects_oversize_timeout() {
        let yaml = r#"
schema_version: "1"
gates:
  - id: slow
    gate_type: automated
    description: x
    check_command: "x"
    timeout_seconds: 99999
"#;
        let err = validate_publish_gates_yaml(yaml).expect_err("timeout > max");
        match err {
            PublishGatesError::Schema(issues) => {
                assert!(
                    issues.iter().any(|i| i.path.ends_with("/timeout_seconds")),
                    "expected timeout_seconds error, got: {issues:?}"
                );
            }
            other => panic!("expected Schema error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_zero_timeout() {
        let yaml = r#"
schema_version: "1"
gates:
  - id: tests-pass
    gate_type: automated
    description: x
    check_command: "x"
    timeout_seconds: 0
"#;
        let err = validate_publish_gates_yaml(yaml).expect_err("timeout < min");
        match err {
            PublishGatesError::Schema(_) => {}
            other => panic!("expected Schema error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_empty_gates_list() {
        let yaml = r#"
schema_version: "1"
gates: []
"#;
        let err = validate_publish_gates_yaml(yaml).expect_err("gates: [] should fail");
        match err {
            PublishGatesError::Schema(_) => {}
            other => panic!("expected Schema error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_yaml_syntax_error() {
        let yaml = "schema_version: \"1\"\ngates:\n  - id: tests-pass\n    gate_type: automated\n  description: bad indent\n";
        let err = validate_publish_gates_yaml(yaml).expect_err("malformed yaml");
        match err {
            PublishGatesError::Yaml(_) => {}
            other => panic!("expected Yaml error, got {other:?}"),
        }
    }

    #[test]
    fn load_publish_gates_returns_not_found_when_file_missing() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("does-not-exist.yaml");
        let err = load_publish_gates(&nonexistent).expect_err("missing file");
        match err {
            PublishGatesError::NotFound => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn load_publish_gates_round_trip_valid_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("publish-gates.yaml");
        fs::write(&path, MIN_AUTOMATED).unwrap();
        let cfg = load_publish_gates(&path).expect("should load");
        assert_eq!(cfg.gates.len(), 1);
        assert_eq!(cfg.gates[0].id, "tests-pass");
    }

    #[test]
    fn schema_v1_is_only_accepted_value() {
        let yaml = r#"
schema_version: "2"
gates:
  - id: tests-pass
    gate_type: automated
    description: x
    check_command: "x"
"#;
        let err =
            validate_publish_gates_yaml(yaml).expect_err("schema_version: 2 not yet defined");
        match err {
            PublishGatesError::Schema(issues) => {
                assert!(
                    issues.iter().any(|i| i.path.ends_with("/schema_version")),
                    "expected schema_version error, got: {issues:?}"
                );
            }
            other => panic!("expected Schema error, got {other:?}"),
        }
    }
}
