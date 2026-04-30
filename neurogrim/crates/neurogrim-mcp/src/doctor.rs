//! Configuration auditor for a Brain registry (v3.2 Phase A.2; relocated
//! from neurogrim-cli to neurogrim-mcp in v3.2.1 so the MCP `doctor`
//! tool and the `neurogrim doctor` CLI command share a single source
//! of truth).
//!
//! Read-only. No ledger writes. No scoring. Returns a list of
//! `Finding`s; the caller (CLI or MCP tool) maps that to text or JSON
//! output and an appropriate exit-code/severity-summary.
//!
//! Six check families:
//!   1. registry validates against schema (reuses BrainRegistry::validate)
//!   2. domain_weights keys ⊆ domain_definitions keys (severity by weight)
//!   3. principle_map keys ⊆ domain_definitions keys
//!   4. every scoring_source.path resolves to a readable file
//!   5. culture.yaml exists at expected path
//!   6. federation children have unique A2A ports

use neurogrim_core::registry::BrainRegistry;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub category: &'static str,
    pub message: String,
}

impl Finding {
    pub fn err(category: &'static str, message: impl Into<String>) -> Self {
        Finding {
            severity: Severity::Error,
            category,
            message: message.into(),
        }
    }
    pub fn warn(category: &'static str, message: impl Into<String>) -> Self {
        Finding {
            severity: Severity::Warn,
            category,
            message: message.into(),
        }
    }
    #[allow(dead_code)]
    pub fn info(category: &'static str, message: impl Into<String>) -> Self {
        Finding {
            severity: Severity::Info,
            category,
            message: message.into(),
        }
    }
}

/// Run all check families and return aggregated findings. The caller
/// decides how to render them and what exit code to emit.
///
/// v3.3 F3: added `check_autonomy` for autonomy-block schema correctness.
/// v4.0 S12-G-3: added `check_publish_gates` for `publish-gates.yaml`
///               schema correctness (advisory-during-rollout).
pub fn audit(registry: &BrainRegistry, project_root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    findings.extend(check_validate(registry));
    findings.extend(check_definitions_alignment(registry));
    findings.extend(check_principle_map_alignment(registry));
    findings.extend(check_cmdb_paths(registry, project_root));
    findings.extend(check_culture_yaml(project_root));
    findings.extend(check_federation_ports(registry, project_root));
    findings.extend(check_autonomy(registry));
    findings.extend(check_publish_gates(project_root));
    findings.extend(check_queue_config(project_root));
    findings
}

// --- Check 1: schema-level validate ----------------------------------

pub fn check_validate(reg: &BrainRegistry) -> Vec<Finding> {
    match reg.validate() {
        Ok(()) => Vec::new(),
        Err(e) => vec![Finding::err("schema-validate", format!("{}", e))],
    }
}

// --- Check 2: domain_weights keys ⊆ domain_definitions keys ----------

pub fn check_definitions_alignment(reg: &BrainRegistry) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (k, w) in &reg.config.domain_weights {
        if reg.config.domain_definitions.contains_key(k) {
            continue;
        }
        // Severity depends on whether the domain contributes to the score:
        //   - Weighted (w > 0) without a definition → Error: scoring falls
        //     to no_file_score (default 0) and pulls the unified score
        //     down silently.
        //   - Advisory (w == 0) without a definition → Warn: this is the
        //     "declared intent, sensor not yet authored" placeholder
        //     posture (`neurogrim domain new --type stub` will produce
        //     this shape and is a legitimate v3.2 v1 starting point).
        if *w > 0.0 {
            findings.push(Finding::err(
                "definitions",
                format!(
                    "domain '{k}' has weight {w} but no entry in domain_definitions; \
                     scoring will fall back to no_file_score (0) and degrade the \
                     unified score silently"
                ),
            ));
        } else {
            findings.push(Finding::warn(
                "definitions",
                format!(
                    "domain '{k}' is declared advisory (weight 0.0) but has no \
                     domain_definitions entry; sensor authoring is still pending"
                ),
            ));
        }
    }
    findings
}

// --- Check 3: principle_map keys ⊆ domain_definitions keys -----------

pub fn check_principle_map_alignment(reg: &BrainRegistry) -> Vec<Finding> {
    let mut findings = Vec::new();
    for k in reg.config.principle_map.keys() {
        if !reg.config.domain_definitions.contains_key(k) {
            findings.push(Finding::warn(
                "principle-map",
                format!(
                    "principle_map has '{k}' but no domain_definition; \
                     remove the orphan or add a definition"
                ),
            ));
        }
    }
    findings
}

// --- Check 4: every scoring_source.path resolves to a readable file --

pub fn check_cmdb_paths(reg: &BrainRegistry, project_root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (k, def) in &reg.config.domain_definitions {
        let Some(src) = def.scoring_source.as_ref() else {
            continue;
        };
        if src.source_type.as_str() != "cmdb" {
            // a2a / function sources don't have local paths.
            continue;
        }
        let Some(rel) = src.path.as_ref() else {
            findings.push(Finding::warn(
                "cmdb-paths",
                format!("domain '{k}' has scoring_source.type='cmdb' but no path"),
            ));
            continue;
        };
        let full = project_root.join(rel);
        if !full.is_file() {
            findings.push(Finding::warn(
                "cmdb-paths",
                format!(
                    "domain '{k}' CMDB missing at {} (will score as no_file_score \
                     until refreshed: `neurogrim sensory {k} --project-root .`)",
                    full.display()
                ),
            ));
        }
    }
    findings
}

// --- Check 5: culture.yaml present -----------------------------------

pub fn check_culture_yaml(project_root: &Path) -> Vec<Finding> {
    let path = project_root.join(".claude").join("culture.yaml");
    if path.is_file() {
        Vec::new()
    } else {
        vec![Finding::warn(
            "culture",
            format!(
                "{}: not found; the byte-identical-across-federation invariant is broken",
                path.display()
            ),
        )]
    }
}

// --- Check 6: federation children have unique A2A ports -------------

/// v3.3 F7: walks `config.children` AND each child's `brain_path` registry
/// recursively, collecting `port → list of "owner_id" labels` for every
/// peer at any level. Detects port conflicts across the full federation
/// tree, not just direct children.
///
/// `project_root` is the directory containing the registry being walked
/// (used to resolve relative `brain_path` entries). `visited` tracks
/// already-walked registry paths to prevent infinite recursion on
/// pathological cyclic federations.
pub fn collect_transitive_ports(
    reg: &BrainRegistry,
    project_root: &Path,
) -> HashMap<u16, Vec<String>> {
    let mut by_port: HashMap<u16, Vec<String>> = HashMap::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    walk_children(
        reg,
        project_root,
        "",
        &mut by_port,
        &mut visited,
    );
    by_port
}

fn walk_children(
    reg: &BrainRegistry,
    project_root: &Path,
    prefix: &str,
    by_port: &mut HashMap<u16, Vec<String>>,
    visited: &mut HashSet<PathBuf>,
) {
    let Some(children) = reg.config.extra.get("children").and_then(|v| v.as_object()) else {
        return;
    };
    for (id, val) in children {
        let label = if prefix.is_empty() {
            id.clone()
        } else {
            format!("{prefix}/{id}")
        };

        // Record this child's port at the current level.
        if let Some(endpoint) = val.get("a2a_endpoint").and_then(|v| v.as_str()) {
            if let Some(port) = parse_port(endpoint) {
                by_port.entry(port).or_default().push(label.clone());
            }
        }

        // Recurse into the child's own registry if it's reachable on disk.
        let Some(brain_path) = val.get("brain_path").and_then(|v| v.as_str()) else {
            continue;
        };
        let child_root = project_root.join(brain_path);
        let child_registry_path = child_root.join(".claude").join("brain-registry.json");
        let canonical = child_registry_path
            .canonicalize()
            .unwrap_or_else(|_| child_registry_path.clone());
        if !visited.insert(canonical) {
            continue; // already walked this registry; cycle guard
        }
        let Ok(json) = std::fs::read_to_string(&child_registry_path) else {
            continue; // peer not on disk; just skip (still counted at this level)
        };
        let Ok(child_reg) = BrainRegistry::from_json(&json) else {
            continue;
        };
        walk_children(&child_reg, &child_root, &label, by_port, visited);
    }
}

pub fn check_federation_ports(reg: &BrainRegistry, project_root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let by_port = collect_transitive_ports(reg, project_root);
    for (port, ids) in by_port {
        if ids.len() > 1 {
            let mut sorted = ids;
            sorted.sort();
            findings.push(Finding::err(
                "federation-ports",
                format!(
                    "port {port} is shared by federation peers {:?}; \
                     each peer must own a unique port (transitive across the whole tree)",
                    sorted
                ),
            ));
        }
    }
    findings
}

// --- Check 7: autonomy block schema correctness (v3.3 F3) -----------

/// The four canonical autonomy levels. Adding new ones requires a
/// methodology change, not a registry edit.
const CANONICAL_LEVELS: &[&str] = &["auto", "notify", "approve", "blocked"];

/// Validate the `autonomy` block in `config.extra`:
///
/// - `levels` should declare the four canonical levels with `description`
///   + `requires_approval`
/// - Each `action_types[].default_level` must reference an existing level
/// - Each `safety_invariants[]` entry should have a `rule` + at least one
///   of `minimum_level` / `enforced_level`, and any level it references
///   must exist in `levels`
/// - Both `minimum_level` AND `enforced_level` on the same invariant is
///   ambiguous — pick one (warn)
/// - `description` recommended on action_types + safety_invariants (warn
///   when missing — operators reading the registry six months later need
///   to know why a rule exists)
/// - Warn on unknown top-level keys inside `autonomy.action_types[]` or
///   `autonomy.safety_invariants[]` (catches v3.2.2-era invented fields
///   like `autonomy_bias`)
pub fn check_autonomy(reg: &BrainRegistry) -> Vec<Finding> {
    let mut findings = Vec::new();
    // BrainConfig.autonomy is a typed `serde_json::Value`, defaulting to
    // Value::Null when absent. Only proceed when it's an object.
    let Some(autonomy) = reg.config.autonomy.as_object() else {
        return findings;
    };

    // 1. Levels — collect the set of level names this registry declares.
    let declared_levels: std::collections::HashSet<&str> = autonomy
        .get("levels")
        .and_then(|v| v.as_object())
        .map(|m| m.keys().map(|s| s.as_str()).collect())
        .unwrap_or_default();

    // Soft-warn on missing canonical levels.
    if !declared_levels.is_empty() {
        for canon in CANONICAL_LEVELS {
            if !declared_levels.contains(canon) {
                findings.push(Finding::warn(
                    "autonomy",
                    format!(
                        "autonomy.levels does not declare the canonical level '{canon}'; \
                         agents MAY default to a tighter level when reasoning about it"
                    ),
                ));
            }
        }
    }

    // 2. Action types — every default_level must reference an existing level.
    if let Some(action_types) = autonomy.get("action_types").and_then(|v| v.as_object()) {
        for (action, body) in action_types {
            let Some(body) = body.as_object() else {
                findings.push(Finding::err(
                    "autonomy",
                    format!("autonomy.action_types.{action} is not an object"),
                ));
                continue;
            };
            // Every action MUST have default_level
            let Some(level) = body.get("default_level").and_then(|v| v.as_str()) else {
                findings.push(Finding::err(
                    "autonomy",
                    format!(
                        "autonomy.action_types.{action} missing required field `default_level`"
                    ),
                ));
                continue;
            };
            if !declared_levels.is_empty() && !declared_levels.contains(level) {
                findings.push(Finding::err(
                    "autonomy",
                    format!(
                        "autonomy.action_types.{action}.default_level = '{level}' is not \
                         declared in autonomy.levels"
                    ),
                ));
            } else if declared_levels.is_empty()
                && !CANONICAL_LEVELS.contains(&level)
            {
                findings.push(Finding::err(
                    "autonomy",
                    format!(
                        "autonomy.action_types.{action}.default_level = '{level}' is not a \
                         canonical level (auto / notify / approve / blocked)"
                    ),
                ));
            }
            if !body.contains_key("description") {
                findings.push(Finding::warn(
                    "autonomy",
                    format!(
                        "autonomy.action_types.{action} is missing `description` — operators \
                         auditing the registry need to know what this action class covers"
                    ),
                ));
            }
            // Warn on unknown fields (catches v3.2.2-era invented fields like `autonomy_bias`).
            for key in body.keys() {
                if !matches!(
                    key.as_str(),
                    "default_level" | "blast_radius" | "reversible" | "description"
                ) {
                    findings.push(Finding::warn(
                        "autonomy",
                        format!(
                            "autonomy.action_types.{action} has unknown field '{key}'; \
                             schema is closed (default_level, blast_radius, reversible, \
                             description). Field will be ignored at runtime."
                        ),
                    ));
                }
            }
        }
    }

    // 3. Safety invariants
    if let Some(invariants) = autonomy
        .get("safety_invariants")
        .and_then(|v| v.as_array())
    {
        for (i, inv) in invariants.iter().enumerate() {
            let Some(inv) = inv.as_object() else {
                findings.push(Finding::err(
                    "autonomy",
                    format!("autonomy.safety_invariants[{i}] is not an object"),
                ));
                continue;
            };
            // rule is required
            let rule_label = inv
                .get("rule")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("[{i}]"));
            if !inv.contains_key("rule") {
                findings.push(Finding::err(
                    "autonomy",
                    format!("autonomy.safety_invariants[{i}] missing required field `rule`"),
                ));
            }
            let has_min = inv.contains_key("minimum_level");
            let has_enf = inv.contains_key("enforced_level");
            if !has_min && !has_enf {
                findings.push(Finding::err(
                    "autonomy",
                    format!(
                        "autonomy.safety_invariants[{rule_label}] needs at least one of \
                         `minimum_level` or `enforced_level` — invariant has no effect otherwise"
                    ),
                ));
            }
            if has_min && has_enf {
                findings.push(Finding::warn(
                    "autonomy",
                    format!(
                        "autonomy.safety_invariants[{rule_label}] has BOTH `minimum_level` \
                         and `enforced_level` — semantics are ambiguous; pick one"
                    ),
                ));
            }
            for level_field in &["minimum_level", "enforced_level"] {
                if let Some(level) = inv.get(*level_field).and_then(|v| v.as_str()) {
                    if !declared_levels.is_empty() && !declared_levels.contains(level) {
                        findings.push(Finding::err(
                            "autonomy",
                            format!(
                                "autonomy.safety_invariants[{rule_label}].{level_field} = \
                                 '{level}' is not declared in autonomy.levels"
                            ),
                        ));
                    } else if declared_levels.is_empty()
                        && !CANONICAL_LEVELS.contains(&level)
                    {
                        findings.push(Finding::err(
                            "autonomy",
                            format!(
                                "autonomy.safety_invariants[{rule_label}].{level_field} = \
                                 '{level}' is not a canonical level"
                            ),
                        ));
                    }
                }
            }
            if !inv.contains_key("description") {
                findings.push(Finding::warn(
                    "autonomy",
                    format!(
                        "autonomy.safety_invariants[{rule_label}] is missing `description` — \
                         auditors WILL ask why this rule exists"
                    ),
                ));
            }
            // Warn on unknown fields
            for key in inv.keys() {
                if !matches!(
                    key.as_str(),
                    "rule" | "minimum_level" | "enforced_level" | "description"
                ) {
                    findings.push(Finding::warn(
                        "autonomy",
                        format!(
                            "autonomy.safety_invariants[{rule_label}] has unknown field \
                             '{key}'; schema is closed (rule, minimum_level, enforced_level, \
                             description). Field will be ignored at runtime."
                        ),
                    ));
                }
            }
        }
    }

    findings
}

/// Pull the port out of an A2A endpoint URL like
/// `http://localhost:8421/a2a/v1/`. Returns None on shapes the function
/// can't read (which is fine — the check only fires on confirmed clashes).
pub(crate) fn parse_port(endpoint: &str) -> Option<u16> {
    let after_scheme = endpoint.split("://").nth(1)?;
    let host_port = after_scheme.split('/').next()?;
    let port_str = host_port.rsplit(':').next()?;
    port_str.parse::<u16>().ok()
}

// --- Check 8: publish-gates.yaml schema correctness (v4.0 S12-G-3) --

/// Validate `<project_root>/.claude/brain/publish-gates.yaml` against
/// the embedded `publish-gates-v1.schema.json`.
///
/// Severity model — adopters are rolling onto v4.0 publish-gates
/// progressively:
///
/// - **Missing file** → no finding. The file is opt-in. Brains that
///   don't run their own publish pipeline (e.g., python-starter, a
///   private adopter that hasn't authored gates yet) shouldn't see a
///   warning forever. NeuroGrim's S12-G-7 self-hosting milestone makes
///   it required for *NeuroGrim itself*, but the adopter contract
///   stays advisory.
/// - **YAML parse failure** → Error. Once authored, the file MUST be
///   structurally well-formed; a broken manifest silently breaks every
///   subsequent `publish-gate run`.
/// - **Schema validation failure** → Error per validation issue. The
///   schema is the contract; drift means the runner can't trust the
///   declared shape.
/// - **Duplicate gate IDs** → Error. The runner uses `id` as the
///   ledger primary key; duplicates corrupt audit trails.
pub fn check_publish_gates(project_root: &Path) -> Vec<Finding> {
    let path = project_root
        .join(".claude")
        .join("brain")
        .join("publish-gates.yaml");
    match crate::publish_gates::load_publish_gates(&path) {
        Ok(_) => Vec::new(),
        Err(crate::publish_gates::PublishGatesError::NotFound) => {
            // Opt-in posture during v4.0 rollout — no finding.
            Vec::new()
        }
        Err(crate::publish_gates::PublishGatesError::Yaml(msg)) => {
            vec![Finding::err(
                "publish-gates-syntax",
                format!("{}: YAML parse failed: {msg}", path.display()),
            )]
        }
        Err(crate::publish_gates::PublishGatesError::Io(msg)) => {
            vec![Finding::err(
                "publish-gates-syntax",
                format!("{}: I/O error: {msg}", path.display()),
            )]
        }
        Err(crate::publish_gates::PublishGatesError::Schema(issues)) => issues
            .into_iter()
            .map(|i| {
                Finding::err(
                    "publish-gates-schema",
                    format!("{}: {i}", path.display()),
                )
            })
            .collect(),
        Err(crate::publish_gates::PublishGatesError::DuplicateIds(ids)) => vec![Finding::err(
            "publish-gates-schema",
            format!(
                "{}: duplicate gate id(s): {} — `id` is the ledger primary key; \
                 each gate must have a unique kebab-case identifier",
                path.display(),
                ids.join(", ")
            ),
        )],
    }
}

// --- Check 9: queue-config.yaml schema correctness (S13-B-3 v2) ----

/// Validate `<project_root>/.claude/brain/queue-config.yaml` against
/// the schema in `neurogrim_core::queue_config::QueueConfig`.
///
/// Severity model:
///
/// - **Missing file** → silent (no finding). The file is opt-in;
///   adopters who only use JSONL topics never need to author one.
/// - **Parse / schema failure** → `Error`. A misconfigured topic
///   would default to JSONL and silently violate the operator's
///   intent (e.g., `ack_required` topics that should have been
///   SQLite-backed will silently lose ack semantics). Loud failure
///   beats silent fallback.
pub fn check_queue_config(project_root: &Path) -> Vec<Finding> {
    let path = project_root
        .join(".claude")
        .join("brain")
        .join("queue-config.yaml");
    match neurogrim_core::queue_config::QueueConfig::from_path(&path) {
        Ok(_) => Vec::new(),
        Err(e) => vec![Finding::err(
            "queue-config",
            format!("{}: {e:#}", path.display()),
        )],
    }
}

// --- Tests ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(json: &str) -> BrainRegistry {
        BrainRegistry::from_json(json).expect("fixture should parse")
    }

    const MIN_VALID: &str = r#"{
        "meta": {"schema_version": "2", "description": "test", "updated_by": "test"},
        "config": {
            "domain_weights": {"a": 1.0},
            "domain_definitions": {
                "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a-cmdb.json"}}
            }
        }
    }"#;

    #[test]
    fn check_validate_clean() {
        let r = fixture(MIN_VALID);
        assert!(check_validate(&r).is_empty());
    }

    #[test]
    fn check_validate_catches_bad_weight_sum() {
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2", "description": "x", "updated_by": "x"},
                "config": {"domain_weights": {"a": 0.5, "b": 0.3}}
            }"#,
        );
        let f = check_validate(&r);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Error);
    }

    #[test]
    fn check_definitions_warns_on_advisory_orphan() {
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2", "description": "x", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0, "future-domain": 0.0},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    }
                }
            }"#,
        );
        let f = check_definitions_alignment(&r);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Warn);
        assert!(f[0].message.contains("future-domain"));
    }

    #[test]
    fn check_definitions_errors_on_weighted_orphan() {
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2", "description": "x", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 0.5, "weighted-orphan": 0.5},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    }
                }
            }"#,
        );
        let f = check_definitions_alignment(&r);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Error);
        assert!(f[0].message.contains("weighted-orphan"));
    }

    #[test]
    fn check_principle_map_warns_on_orphan() {
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2", "description": "x", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0},
                    "principle_map": {"a": "A", "ghost": "Ghost"},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    }
                }
            }"#,
        );
        let f = check_principle_map_alignment(&r);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Warn);
        assert!(f[0].message.contains("ghost"));
    }

    #[test]
    fn check_cmdb_paths_warns_when_file_missing() {
        let r = fixture(MIN_VALID);
        let tmp = tempfile::TempDir::new().unwrap();
        let f = check_cmdb_paths(&r, tmp.path());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Warn);
    }

    #[test]
    fn check_cmdb_paths_clean_when_file_exists() {
        let r = fixture(MIN_VALID);
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        std::fs::write(tmp.path().join(".claude/a-cmdb.json"), "{}").unwrap();
        let f = check_cmdb_paths(&r, tmp.path());
        assert!(f.is_empty(), "got: {:?}", f);
    }

    #[test]
    fn check_culture_yaml_warns_when_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let f = check_culture_yaml(tmp.path());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Warn);
    }

    #[test]
    fn check_culture_yaml_clean_when_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        std::fs::write(tmp.path().join(".claude/culture.yaml"), "values: []").unwrap();
        let f = check_culture_yaml(tmp.path());
        assert!(f.is_empty());
    }

    #[test]
    fn check_federation_ports_catches_clash() {
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2.1", "description": "x", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    },
                    "children": {
                        "p1": {"a2a_endpoint": "http://localhost:8421/a2a/v1/"},
                        "p2": {"a2a_endpoint": "http://127.0.0.1:8421/a2a/v1/"}
                    }
                }
            }"#,
        );
        let tmp = tempfile::TempDir::new().unwrap();
        let f = check_federation_ports(&r, tmp.path());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Error);
        assert!(f[0].message.contains("8421"));
    }

    #[test]
    fn check_federation_ports_clean_when_unique() {
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2.1", "description": "x", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    },
                    "children": {
                        "p1": {"a2a_endpoint": "http://localhost:8421/a2a/v1/"},
                        "p2": {"a2a_endpoint": "http://localhost:8422/a2a/v1/"}
                    }
                }
            }"#,
        );
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(check_federation_ports(&r, tmp.path()).is_empty());
    }

    #[test]
    fn check_federation_ports_catches_transitive_clash() {
        // v3.3 F7: parent has 1 child at 8421; that child's own registry
        // declares ANOTHER child at 8422; parent declares a 2nd direct
        // child also at 8422. The transitive walker should catch the clash
        // between (parent → child2) and (parent → child1 → grandchild)
        // even though they live at different levels.
        let tmp = tempfile::TempDir::new().unwrap();

        // Set up child1 with its own registry that declares a grandchild at 8422.
        let child1_dir = tmp.path().join("child1");
        std::fs::create_dir_all(child1_dir.join(".claude")).unwrap();
        std::fs::write(
            child1_dir.join(".claude/brain-registry.json"),
            r#"{
                "meta": {"schema_version": "2.1", "description": "child1", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    },
                    "children": {
                        "grandchild": {
                            "a2a_endpoint": "http://localhost:8422/a2a/v1/",
                            "brain_path": "../grandchild"
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        // Parent: declares child1 at 8421 + child2 at 8422 (clashes with grandchild).
        let parent = fixture(
            r#"{
                "meta": {"schema_version": "2.1", "description": "parent", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    },
                    "children": {
                        "child1": {
                            "a2a_endpoint": "http://localhost:8421/a2a/v1/",
                            "brain_path": "child1"
                        },
                        "child2": {
                            "a2a_endpoint": "http://localhost:8422/a2a/v1/",
                            "brain_path": "child2"
                        }
                    }
                }
            }"#,
        );
        let f = check_federation_ports(&parent, tmp.path());
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("8422"));
        assert!(f[0].message.contains("child1/grandchild"));
        assert!(f[0].message.contains("child2"));
    }

    #[test]
    fn parse_port_handles_localhost() {
        assert_eq!(parse_port("http://localhost:8421/a2a/v1/"), Some(8421));
        assert_eq!(parse_port("http://127.0.0.1:8424/"), Some(8424));
        assert_eq!(parse_port("https://example.com:443/path"), Some(443));
    }

    #[test]
    fn parse_port_returns_none_on_no_port() {
        assert_eq!(parse_port("http://localhost/path"), None);
    }

    #[test]
    fn audit_runs_all_check_families() {
        // Smoke test: against MIN_VALID + a tmp dir, audit runs to completion
        // and returns 0 errors but ≥1 warn (missing CMDB + missing culture).
        let r = fixture(MIN_VALID);
        let tmp = tempfile::TempDir::new().unwrap();
        let findings = audit(&r, tmp.path());
        let errors = findings.iter().filter(|f| f.severity == Severity::Error).count();
        assert_eq!(errors, 0, "got: {:?}", findings);
        assert!(findings.len() >= 2, "got: {:?}", findings);
    }

    // --- F3: autonomy schema checks --------------------------------

    #[test]
    fn check_autonomy_clean_when_no_block() {
        let r = fixture(MIN_VALID);
        assert!(check_autonomy(&r).is_empty());
    }

    #[test]
    fn check_autonomy_clean_on_well_formed_block() {
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2.1", "description": "x", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    },
                    "autonomy": {
                        "levels": {
                            "auto": {"description": "auto", "requires_approval": false},
                            "notify": {"description": "notify", "requires_approval": false},
                            "approve": {"description": "approve", "requires_approval": true},
                            "blocked": {"description": "blocked", "requires_approval": true}
                        },
                        "action_types": {
                            "submit-application": {
                                "default_level": "approve",
                                "blast_radius": "high",
                                "reversible": false,
                                "description": "submitting an application"
                            }
                        },
                        "safety_invariants": [
                            {
                                "rule": "agents-must-not-submit-without-approval",
                                "minimum_level": "approve",
                                "description": "applications carry operator identity"
                            }
                        ]
                    }
                }
            }"#,
        );
        assert!(check_autonomy(&r).is_empty(), "got: {:?}", check_autonomy(&r));
    }

    #[test]
    fn check_autonomy_catches_invented_field() {
        // F3 worked example: the v3.2.2 agent invented `autonomy_bias` which
        // is silently accepted at runtime. Doctor should warn.
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2.1", "description": "x", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    },
                    "autonomy": {
                        "action_types": {
                            "submit-application": {
                                "default_level": "approve",
                                "autonomy_bias": "invented field"
                            }
                        }
                    }
                }
            }"#,
        );
        let f = check_autonomy(&r);
        assert!(
            f.iter().any(|x| x.message.contains("autonomy_bias")),
            "got: {:?}",
            f
        );
    }

    #[test]
    fn check_autonomy_catches_unknown_level_reference() {
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2.1", "description": "x", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    },
                    "autonomy": {
                        "levels": {
                            "auto": {"description": "auto", "requires_approval": false},
                            "approve": {"description": "approve", "requires_approval": true}
                        },
                        "safety_invariants": [
                            {
                                "rule": "x",
                                "minimum_level": "supervisor",
                                "description": "y"
                            }
                        ]
                    }
                }
            }"#,
        );
        let f = check_autonomy(&r);
        assert!(
            f.iter().any(|x| x.severity == Severity::Error
                && x.message.contains("supervisor")),
            "got: {:?}",
            f
        );
    }

    #[test]
    fn check_autonomy_warns_on_missing_description() {
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2.1", "description": "x", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    },
                    "autonomy": {
                        "safety_invariants": [
                            { "rule": "x", "minimum_level": "approve" }
                        ]
                    }
                }
            }"#,
        );
        let f = check_autonomy(&r);
        assert!(f.iter().any(|x| x.message.contains("description")));
    }

    #[test]
    fn check_autonomy_errors_on_invariant_without_level() {
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2.1", "description": "x", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    },
                    "autonomy": {
                        "safety_invariants": [
                            { "rule": "x", "description": "y" }
                        ]
                    }
                }
            }"#,
        );
        let f = check_autonomy(&r);
        assert!(
            f.iter().any(|x| x.severity == Severity::Error
                && x.message.contains("minimum_level")
                && x.message.contains("enforced_level")),
            "got: {:?}",
            f
        );
    }

    #[test]
    fn check_autonomy_warns_on_both_min_and_enforced() {
        let r = fixture(
            r#"{
                "meta": {"schema_version": "2.1", "description": "x", "updated_by": "x"},
                "config": {
                    "domain_weights": {"a": 1.0},
                    "domain_definitions": {
                        "a": {"scoring_source": {"type": "cmdb", "path": ".claude/a.json"}}
                    },
                    "autonomy": {
                        "safety_invariants": [
                            {
                                "rule": "x",
                                "minimum_level": "approve",
                                "enforced_level": "blocked",
                                "description": "y"
                            }
                        ]
                    }
                }
            }"#,
        );
        let f = check_autonomy(&r);
        assert!(
            f.iter().any(|x| x.severity == Severity::Warn
                && x.message.contains("ambiguous")),
            "got: {:?}",
            f
        );
    }

    // --- check_publish_gates (v4.0 S12-G-3) ---------------------------

    use tempfile::TempDir;

    #[test]
    fn check_publish_gates_missing_file_emits_no_finding() {
        let tmp = TempDir::new().unwrap();
        // No .claude/brain/publish-gates.yaml present — opt-in posture.
        let f = check_publish_gates(tmp.path());
        assert!(
            f.is_empty(),
            "missing manifest should be silent during v4.0 rollout; got: {f:?}"
        );
    }

    #[test]
    fn check_publish_gates_clean_manifest_emits_no_finding() {
        let tmp = TempDir::new().unwrap();
        let brain = tmp.path().join(".claude").join("brain");
        std::fs::create_dir_all(&brain).unwrap();
        std::fs::write(
            brain.join("publish-gates.yaml"),
            r#"
schema_version: "1"
gates:
  - id: tests-pass
    gate_type: automated
    description: All tests green
    check_command: "neurogrim test"
"#,
        )
        .unwrap();
        let f = check_publish_gates(tmp.path());
        assert!(f.is_empty(), "clean manifest should emit no findings; got: {f:?}");
    }

    #[test]
    fn check_publish_gates_malformed_yaml_returns_error() {
        let tmp = TempDir::new().unwrap();
        let brain = tmp.path().join(".claude").join("brain");
        std::fs::create_dir_all(&brain).unwrap();
        // Bad indent → serde_yaml parse error
        std::fs::write(
            brain.join("publish-gates.yaml"),
            "schema_version: \"1\"\ngates:\n  - id: tests-pass\n    gate_type: automated\n  description: bad indent\n",
        )
        .unwrap();
        let f = check_publish_gates(tmp.path());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Error);
        assert_eq!(f[0].category, "publish-gates-syntax");
    }

    #[test]
    fn check_publish_gates_schema_invalid_emits_error_per_issue() {
        let tmp = TempDir::new().unwrap();
        let brain = tmp.path().join(".claude").join("brain");
        std::fs::create_dir_all(&brain).unwrap();
        // Two issues: missing schema_version + unknown gate_type.
        std::fs::write(
            brain.join("publish-gates.yaml"),
            r#"
gates:
  - id: weird
    gate_type: telepathic
    description: x
"#,
        )
        .unwrap();
        let f = check_publish_gates(tmp.path());
        assert!(f.len() >= 2, "expected ≥2 findings; got: {f:?}");
        for finding in &f {
            assert_eq!(finding.severity, Severity::Error);
            assert_eq!(finding.category, "publish-gates-schema");
        }
    }

    #[test]
    fn check_publish_gates_duplicate_ids_emits_single_error() {
        let tmp = TempDir::new().unwrap();
        let brain = tmp.path().join(".claude").join("brain");
        std::fs::create_dir_all(&brain).unwrap();
        std::fs::write(
            brain.join("publish-gates.yaml"),
            r#"
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
"#,
        )
        .unwrap();
        let f = check_publish_gates(tmp.path());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Error);
        assert_eq!(f[0].category, "publish-gates-schema");
        assert!(f[0].message.contains("duplicate gate id"));
        assert!(f[0].message.contains("tests-pass"));
    }

    // --- queue-config.yaml (S13-B-3 v2) -------------------------------

    #[test]
    fn check_queue_config_clean_when_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let f = check_queue_config(tmp.path());
        assert!(f.is_empty(), "missing file is opt-in; no finding");
    }

    #[test]
    fn check_queue_config_clean_when_valid() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude/brain")).unwrap();
        std::fs::write(
            tmp.path().join(".claude/brain/queue-config.yaml"),
            r#"schema_version: "1"
topics:
  pc-state/alerts:
    backend: sqlite
    ack_required: true
"#,
        )
        .unwrap();
        let f = check_queue_config(tmp.path());
        assert!(f.is_empty(), "valid config should produce no findings");
    }

    #[test]
    fn check_queue_config_errors_on_bad_schema_version() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude/brain")).unwrap();
        std::fs::write(
            tmp.path().join(".claude/brain/queue-config.yaml"),
            r#"schema_version: "99""#,
        )
        .unwrap();
        let f = check_queue_config(tmp.path());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Error);
        assert_eq!(f[0].category, "queue-config");
        assert!(f[0].message.contains("schema_version"));
    }

    #[test]
    fn check_queue_config_errors_on_ack_required_with_jsonl() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude/brain")).unwrap();
        std::fs::write(
            tmp.path().join(".claude/brain/queue-config.yaml"),
            r#"schema_version: "1"
topics:
  pc-state/alerts:
    backend: jsonl
    ack_required: true
"#,
        )
        .unwrap();
        let f = check_queue_config(tmp.path());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Error);
        assert!(f[0].message.contains("ack_required"));
    }
}
