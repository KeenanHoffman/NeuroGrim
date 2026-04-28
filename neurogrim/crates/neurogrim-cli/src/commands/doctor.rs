//! `neurogrim doctor` — validate Brain configuration without scoring (v3.2 Phase A.2).
//!
//! Distinct from neighboring commands:
//!   - `neurogrim validate` — registry shape only (schema, weight sum)
//!   - `neurogrim health` / `score` — runs the scoring pipeline; reads CMDBs
//!   - `neurogrim doctor` — reads the registry + on-disk filesystem, checks
//!     declared-vs-actual alignment, reports configuration problems an
//!     agent should fix before relying on the Brain.
//!
//! Read-only. No ledger writes. No scoring. Exit codes:
//!   0 — clean (no findings)
//!   1 — warnings (Brain is usable but has degraded posture)
//!   2 — errors (Brain is misconfigured; downstream commands will misbehave)
//!
//! Findings are grouped into 6 check families (see `run`). Each finding
//! carries a severity + a single-line actionable message. The output is
//! intentionally compact so an agent can act on it without re-reading.

use anyhow::{Context, Result};
use colored::*;
use neurogrim_core::registry::BrainRegistry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub category: &'static str,
    pub message: String,
}

impl Finding {
    fn err(category: &'static str, message: impl Into<String>) -> Self {
        Finding {
            severity: Severity::Error,
            category,
            message: message.into(),
        }
    }
    fn warn(category: &'static str, message: impl Into<String>) -> Self {
        Finding {
            severity: Severity::Warn,
            category,
            message: message.into(),
        }
    }
    #[allow(dead_code)]
    fn info(category: &'static str, message: impl Into<String>) -> Self {
        Finding {
            severity: Severity::Info,
            category,
            message: message.into(),
        }
    }
}

/// Entry point for `neurogrim doctor`.
pub async fn run(registry_path: &str, plain: bool) -> Result<()> {
    if plain {
        colored::control::set_override(false);
    }

    let mut findings: Vec<Finding> = Vec::new();

    // Step 1: load + parse the registry. If this fails the rest of the
    // checks can't run; report the error and exit 2 immediately.
    let registry = match load_registry(registry_path).await {
        Ok(r) => r,
        Err(e) => {
            findings.push(Finding::err(
                "registry-parse",
                format!("cannot parse {}: {}", registry_path, e),
            ));
            print_findings(&findings, plain, registry_path);
            std::process::exit(2);
        }
    };

    let project_root = derive_project_root(registry_path);

    // Schema-level validation (reuses BrainRegistry::validate).
    findings.extend(check_validate(&registry));
    findings.extend(check_definitions_alignment(&registry));
    findings.extend(check_principle_map_alignment(&registry));
    findings.extend(check_cmdb_paths(&registry, &project_root));
    findings.extend(check_culture_yaml(&project_root));
    findings.extend(check_federation_ports(&registry));

    let exit = print_findings(&findings, plain, registry_path);
    std::process::exit(exit);
}

/// Translate the registry path to the project root: strip `.claude/...` if
/// present, else use the registry's parent's parent (matches `BrainContext`).
fn derive_project_root(registry_path: &str) -> PathBuf {
    let p = Path::new(registry_path);
    if let Some(parent) = p.parent() {
        if let Some(grandparent) = parent.parent() {
            return grandparent.to_path_buf();
        }
        return parent.to_path_buf();
    }
    PathBuf::from(".")
}

async fn load_registry(registry_path: &str) -> Result<BrainRegistry> {
    let json = tokio::fs::read_to_string(registry_path)
        .await
        .with_context(|| format!("read {registry_path}"))?;
    BrainRegistry::from_json(&json).with_context(|| format!("parse {registry_path}"))
}

// --- Check 1: schema-level validate ----------------------------------

fn check_validate(reg: &BrainRegistry) -> Vec<Finding> {
    match reg.validate() {
        Ok(()) => Vec::new(),
        Err(e) => vec![Finding::err("schema-validate", format!("{}", e))],
    }
}

// --- Check 2: domain_weights keys ⊆ domain_definitions keys ----------

fn check_definitions_alignment(reg: &BrainRegistry) -> Vec<Finding> {
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

fn check_principle_map_alignment(reg: &BrainRegistry) -> Vec<Finding> {
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

fn check_cmdb_paths(reg: &BrainRegistry, project_root: &Path) -> Vec<Finding> {
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

fn check_culture_yaml(project_root: &Path) -> Vec<Finding> {
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

fn check_federation_ports(reg: &BrainRegistry) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(children) = reg.config.extra.get("children").and_then(|v| v.as_object()) else {
        return findings;
    };

    let mut by_port: HashMap<u16, Vec<String>> = HashMap::new();
    for (id, val) in children {
        let endpoint = val.get("a2a_endpoint").and_then(|v| v.as_str());
        let Some(endpoint) = endpoint else {
            continue;
        };
        if let Some(port) = parse_port(endpoint) {
            by_port.entry(port).or_default().push(id.clone());
        }
    }
    for (port, ids) in by_port {
        if ids.len() > 1 {
            let mut sorted = ids;
            sorted.sort();
            findings.push(Finding::err(
                "federation-ports",
                format!(
                    "port {port} is shared by federation children {:?}; \
                     each peer must own a unique port",
                    sorted
                ),
            ));
        }
    }
    findings
}

/// Pull the port out of an A2A endpoint URL like
/// `http://localhost:8421/a2a/v1/`. Returns None on shapes the function
/// can't read (which is fine — the check only fires on confirmed clashes).
fn parse_port(endpoint: &str) -> Option<u16> {
    // Find `://`, then find the host:port segment up to next `/`.
    let after_scheme = endpoint.split("://").nth(1)?;
    let host_port = after_scheme.split('/').next()?;
    // host_port is `localhost:8421` or `127.0.0.1:8421` or just `localhost`.
    let port_str = host_port.rsplit(':').next()?;
    port_str.parse::<u16>().ok()
}

// --- Output -----------------------------------------------------------

fn print_findings(findings: &[Finding], plain: bool, registry_path: &str) -> i32 {
    let mut errors = 0;
    let mut warns = 0;

    let header = format!("neurogrim doctor — {}", registry_path);
    if plain {
        println!("{}", header);
    } else {
        println!("{}", header.bold());
    }

    if findings.is_empty() {
        let msg = "  ✓ no findings — Brain configuration looks clean";
        if plain {
            println!("{}", msg);
        } else {
            println!("{}", msg.green());
        }
        return 0;
    }

    // Group by severity for display order: Errors first, then Warnings,
    // then Infos.
    let mut sorted: Vec<&Finding> = findings.iter().collect();
    sorted.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.category.cmp(b.category)));

    for f in sorted {
        let (sym, color) = match f.severity {
            Severity::Error => ("✗", "red"),
            Severity::Warn => ("!", "yellow"),
            Severity::Info => ("i", "cyan"),
        };
        let line = format!("  {} [{}] {}", sym, f.category, f.message);
        let colored_line = if plain {
            line
        } else {
            match color {
                "red" => line.red().to_string(),
                "yellow" => line.yellow().to_string(),
                _ => line.cyan().to_string(),
            }
        };
        println!("{}", colored_line);
        match f.severity {
            Severity::Error => errors += 1,
            Severity::Warn => warns += 1,
            Severity::Info => {}
        }
    }

    println!();
    let summary = format!(
        "{} error{}, {} warning{}",
        errors,
        if errors == 1 { "" } else { "s" },
        warns,
        if warns == 1 { "" } else { "s" }
    );
    if plain {
        println!("{}", summary);
    } else if errors > 0 {
        println!("{}", summary.red().bold());
    } else if warns > 0 {
        println!("{}", summary.yellow().bold());
    } else {
        println!("{}", summary.dimmed());
    }

    if errors > 0 {
        2
    } else if warns > 0 {
        1
    } else {
        0
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
        // Weight 0.0 + no definition = advisory placeholder posture
        // (legitimate `domain new --type stub` shape). Warn, not error.
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
        // Weight > 0 + no definition = silent score corruption. Error.
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
        // project_root is a tmpdir with no actual CMDB → expect 1 warning.
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
        let f = check_federation_ports(&r);
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
        assert!(check_federation_ports(&r).is_empty());
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
    fn derive_project_root_strips_dotclaude() {
        let p = derive_project_root("/foo/bar/.claude/brain-registry.json");
        assert_eq!(p, PathBuf::from("/foo/bar"));
    }
}
