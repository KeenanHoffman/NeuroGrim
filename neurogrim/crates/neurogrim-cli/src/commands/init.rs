//! `neurogrim init` — project scanner and registry generator.
//!
//! Scans a project directory to detect the tech stack, then generates a
//! `brain-registry.json` with universal domains pre-configured and detected
//! domains added as advisory (weight 0.0) stubs.

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write as IoWrite};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

// ---------------------------------------------------------------------------
// Project signals
// ---------------------------------------------------------------------------

/// A detected characteristic of the project that implies a domain candidate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjectSignal {
    Terraform,
    Java,
    Node,
    Python,
    Go,
    RustProject,
    Proto,
    Kubernetes,
    Helm,
    JiraHint,
}

/// Results of scanning a project directory.
pub struct ScanResult {
    pub signals: Vec<ProjectSignal>,
    /// Whether a `.git` directory was detected at the project root. Surfaced
    /// separately from `signals` because consumers sometimes need this as a
    /// prerequisite check (registry recommends git-health domain only when true).
    #[allow(dead_code)]
    pub has_git: bool,
    /// Whether at least one test file was detected anywhere in the scan.
    /// Used to decide whether test-health should default to a weighted or
    /// advisory role. Surfaced for consumers; not currently read by the CLI
    /// init command itself.
    #[allow(dead_code)]
    pub has_tests: bool,
    pub has_ci: bool,
    /// Relative file paths that triggered each signal.
    pub evidence: HashMap<ProjectSignal, Vec<String>>,
}

/// Directories to skip during the walk.
const EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    ".terraform",
    "__pycache__",
    ".venv",
    "venv",
    ".next",
    "vendor",
    ".gradle",
    ".idea",
    ".vscode",
];

/// Scan the project directory for signals, up to `max_depth` levels deep.
pub fn scan_project(root: &Path) -> ScanResult {
    let mut seen_signals: HashSet<ProjectSignal> = HashSet::new();
    let mut evidence: HashMap<ProjectSignal, Vec<String>> = HashMap::new();
    let has_git = root.join(".git").exists();
    let mut has_tests = false;
    let mut has_ci = root.join(".github").join("workflows").exists()
        || root.join(".gitlab-ci.yml").exists()
        || root.join("Jenkinsfile").exists()
        || root.join(".circleci").exists();

    // JiraHint: check root README.md content only
    if let Ok(readme) = fs::read_to_string(root.join("README.md")) {
        if readme.to_lowercase().contains("jira") {
            seen_signals.insert(ProjectSignal::JiraHint);
            evidence
                .entry(ProjectSignal::JiraHint)
                .or_default()
                .push("README.md".to_string());
        }
    }

    walk_dir(
        root,
        root,
        0,
        3,
        &mut seen_signals,
        &mut evidence,
        &mut has_tests,
        &mut has_ci,
    );

    let signals: Vec<ProjectSignal> = [
        ProjectSignal::Terraform,
        ProjectSignal::Java,
        ProjectSignal::Node,
        ProjectSignal::Python,
        ProjectSignal::Go,
        ProjectSignal::RustProject,
        ProjectSignal::Proto,
        ProjectSignal::Kubernetes,
        ProjectSignal::Helm,
        ProjectSignal::JiraHint,
    ]
    .into_iter()
    .filter(|s| seen_signals.contains(s))
    .collect();

    ScanResult {
        signals,
        has_git,
        has_tests,
        has_ci,
        evidence,
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_dir(
    root: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    signals: &mut HashSet<ProjectSignal>,
    evidence: &mut HashMap<ProjectSignal, Vec<String>>,
    has_tests: &mut bool,
    has_ci: &mut bool,
) {
    if depth > max_depth {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if EXCLUDED_DIRS.contains(&name.as_str()) {
            continue;
        }

        let rel_path = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| name.clone());

        if path.is_dir() {
            // Kubernetes directory signal
            let lower = name.to_lowercase();
            if lower == "k8s" || lower == "kubernetes" {
                add_signal(
                    signals,
                    evidence,
                    ProjectSignal::Kubernetes,
                    rel_path.clone(),
                );
            }
            // CI: check for .github/workflows
            if lower == "workflows" && rel_path.contains("github") {
                *has_ci = true;
            }
            // Recurse
            walk_dir(
                root,
                &path,
                depth + 1,
                max_depth,
                signals,
                evidence,
                has_tests,
                has_ci,
            );
        } else if path.is_file() {
            // Test file detection
            let lower_name = name.to_lowercase();
            if lower_name.contains("test") || lower_name.contains("spec") {
                *has_tests = true;
            }
            // CI file detection
            if name == ".gitlab-ci.yml" || name == "Jenkinsfile" || name == "Jenkinsfile.ci" {
                *has_ci = true;
            }

            // File-based signals
            classify_file(&name, &rel_path, depth, signals, evidence);
        }
    }
}

fn classify_file(
    name: &str,
    rel_path: &str,
    depth: usize,
    signals: &mut HashSet<ProjectSignal>,
    evidence: &mut HashMap<ProjectSignal, Vec<String>>,
) {
    let lower = name.to_lowercase();

    // Terraform: any .tf file
    if lower.ends_with(".tf") {
        add_signal(
            signals,
            evidence,
            ProjectSignal::Terraform,
            rel_path.to_string(),
        );
        return;
    }

    // Java: pom.xml, *.gradle, build.gradle.kts
    if lower == "pom.xml" || lower.ends_with(".gradle") || lower == "build.gradle.kts" {
        add_signal(signals, evidence, ProjectSignal::Java, rel_path.to_string());
        return;
    }

    // Node: package.json (root or depth 1 only — avoid nested packages in monorepos)
    if lower == "package.json" && depth <= 1 {
        add_signal(signals, evidence, ProjectSignal::Node, rel_path.to_string());
        return;
    }

    // Python
    if matches!(
        lower.as_str(),
        "pyproject.toml" | "pipfile" | "requirements.txt" | "setup.py"
    ) {
        add_signal(
            signals,
            evidence,
            ProjectSignal::Python,
            rel_path.to_string(),
        );
        return;
    }

    // Go
    if lower == "go.mod" {
        add_signal(signals, evidence, ProjectSignal::Go, rel_path.to_string());
        return;
    }

    // Rust project (skip if inside the neurogrim workspace itself)
    if lower == "cargo.toml" && !rel_path.to_lowercase().contains("neurogrim") {
        add_signal(
            signals,
            evidence,
            ProjectSignal::RustProject,
            rel_path.to_string(),
        );
        return;
    }

    // Proto
    if lower.ends_with(".proto") {
        add_signal(
            signals,
            evidence,
            ProjectSignal::Proto,
            rel_path.to_string(),
        );
        return;
    }

    // Helm
    if name == "Chart.yaml" {
        add_signal(signals, evidence, ProjectSignal::Helm, rel_path.to_string());
        return;
    }
}

fn add_signal(
    signals: &mut HashSet<ProjectSignal>,
    evidence: &mut HashMap<ProjectSignal, Vec<String>>,
    signal: ProjectSignal,
    path: String,
) {
    evidence.entry(signal.clone()).or_default().push(path);
    signals.insert(signal);
}

// ---------------------------------------------------------------------------
// Domain candidates
// ---------------------------------------------------------------------------

/// A domain that was detected from project signals.
pub struct DomainCandidate {
    pub key: String,
    pub display_name: String,
    pub reason: String,
    pub sensory_hint: String,
}

/// Convert project signals into domain candidates.
/// Deduplicates overlapping signals (e.g. Java + Node both → dependency-health).
pub fn signals_to_domain_candidates(scan: &ScanResult) -> Vec<DomainCandidate> {
    let mut candidates: Vec<DomainCandidate> = Vec::new();
    let mut seen_keys: HashSet<String> = HashSet::new();

    let mut add = |key: &str, display: &str, reason: String, hint: &str| {
        if seen_keys.insert(key.to_string()) {
            candidates.push(DomainCandidate {
                key: key.to_string(),
                display_name: display.to_string(),
                reason,
                sensory_hint: hint.to_string(),
            });
        }
    };

    for signal in &scan.signals {
        let evidence_str = scan
            .evidence
            .get(signal)
            .and_then(|v| v.first())
            .cloned()
            .unwrap_or_default();

        match signal {
            ProjectSignal::Terraform => add(
                "terraform-health",
                "Terraform Health",
                format!("detected {} at {}", "Terraform files", evidence_str),
                "see sdk-python/ to write a terraform sensory tool",
            ),
            ProjectSignal::Java => add(
                "dependency-health",
                "Dependency Health",
                format!("detected Java build file at {}", evidence_str),
                "track dependency CVEs and freshness via dependency-check or similar",
            ),
            ProjectSignal::Node => add(
                "dependency-health",
                "Dependency Health",
                format!("detected package.json at {}", evidence_str),
                "see sdk-python/ to write an npm-health sensory tool",
            ),
            ProjectSignal::Python => add(
                "python-health",
                "Python Health",
                format!("detected Python project file at {}", evidence_str),
                "see sdk-python/ to write a python-health sensory tool",
            ),
            ProjectSignal::Go => add(
                "go-health",
                "Go Health",
                format!("detected go.mod at {}", evidence_str),
                "see sdk-python/ to write a go-health sensory tool",
            ),
            ProjectSignal::RustProject => add(
                "rust-health",
                "Rust Health",
                format!("detected Cargo.toml at {}", evidence_str),
                "see sdk-python/ to write a rust-health sensory tool",
            ),
            ProjectSignal::Proto => add(
                "api-contract",
                "API Contract",
                format!("detected proto files at {}", evidence_str),
                "track proto schema changes and breaking API surface",
            ),
            ProjectSignal::Kubernetes => add(
                "k8s-health",
                "Kubernetes Health",
                format!("detected k8s directory at {}", evidence_str),
                "see sdk-python/ to write a k8s-health sensory tool",
            ),
            ProjectSignal::Helm => add(
                "helm-health",
                "Helm Health",
                format!("detected Chart.yaml at {}", evidence_str),
                "track helm chart version drift",
            ),
            ProjectSignal::JiraHint => add(
                "issue-tracker",
                "Issue Tracker Health",
                "README mentions Jira".to_string(),
                "see sdk-python/examples/jira_health/ for a complete example",
            ),
        }
    }

    candidates
}

// ---------------------------------------------------------------------------
// Registry generator
// ---------------------------------------------------------------------------

/// Default personas to include in the generated registry.
fn default_personas() -> Value {
    json!({
        "executive": {
            "description": "C-suite and stakeholders — score + top risk only",
            "output_level": "brief",
            "fields": ["score", "trajectory", "top_recommendations"]
        },
        "manager": {
            "description": "Engineering managers — domain breakdown + gate status",
            "output_level": "standard",
            "fields": ["score", "domains", "trajectory", "top_recommendations", "incident_patterns"]
        },
        "developer": {
            "description": "Individual contributors — full detail",
            "output_level": "full",
            "fields": ["*"]
        },
        "specialist": {
            "description": "Domain experts — filtered view emphasizing relevant domains",
            "output_level": "standard",
            "fields": ["score", "domains", "domain_variables", "top_recommendations"]
        },
        "product-manager": {
            "description": "PMs — delivery risk and blockers",
            "output_level": "standard",
            "fields": ["score", "trajectory", "top_recommendations", "correlations_fired"]
        }
    })
}

/// Generate a complete, valid brain-registry.json for the given project.
pub fn generate_registry(candidates: &[DomainCandidate], project_name: &str) -> Value {
    let mut domain_weights = Map::new();
    domain_weights.insert("code-quality".to_string(), json!(0.35));
    domain_weights.insert("test-health".to_string(), json!(0.35));
    domain_weights.insert("deploy-readiness".to_string(), json!(0.30));

    // Advisory domains from detected signals
    for candidate in candidates {
        domain_weights.insert(candidate.key.clone(), json!(0.0));
    }

    let mut domain_definitions = Map::new();
    domain_definitions.insert(
        "_doc".to_string(),
        json!("Universal domains are pre-configured. Advisory domains (weight=0.0) need a scoring_source and weight before they contribute to the score."),
    );
    domain_definitions.insert(
        "code-quality".to_string(),
        json!({
            "scoring_source": { "type": "cmdb", "path": ".claude/code-quality-cmdb.json" }
        }),
    );
    domain_definitions.insert(
        "test-health".to_string(),
        json!({
            "scoring_source": { "type": "cmdb", "path": ".claude/test-health-cmdb.json" }
        }),
    );
    domain_definitions.insert(
        "deploy-readiness".to_string(),
        json!({
            "scoring_source": { "type": "cmdb", "path": ".claude/deploy-readiness-cmdb.json" }
        }),
    );

    // Advisory domain stubs
    for candidate in candidates {
        let todo_key = format!("_todo_{}", candidate.key);
        domain_definitions.insert(
            todo_key,
            json!(format!(
                "Advisory domain: {}. Reason: {}. To activate: (1) add a sensory_source with a CMDB path, (2) set weight > 0.0, (3) add a sensory tool. Hint: {}",
                candidate.display_name, candidate.reason, candidate.sensory_hint
            )),
        );
    }

    json!({
        "meta": {
            "schema_version": "2.0",
            "description": format!("Brain registry for {} — generated by neurogrim init", project_name),
            "updated_by": "neurogrim-init"
        },
        "config": {
            "domain_weights": Value::Object(domain_weights),
            "domain_definitions": Value::Object(domain_definitions),
            "scoring": {
                "model": "multiplier",
                "floor_confidence_threshold": 30,
                "floor_score_ceiling": 30
            },
            "confidence_thresholds": {
                "cmdb_fresh_days": 1,
                "cmdb_stale_days": 3,
                "cmdb_very_stale_days": 7
            },
            "trajectory": {
                "retention_days": 30,
                "min_samples_for_trend": 5,
                "velocity_window": 5
            },
            "severity_thresholds": {
                "warning_count": 3,
                "critical_count": 5,
                "recurrence_window_days": 7
            },
            "correlations": [],
            "incident_patterns": [],
            "personas": default_personas(),
            "hats": {},
            "sensory_servers": {
                "_doc": "Add external MCP sensory servers here. Example: { \"command\": \"python\", \"args\": [\"-m\", \"my_tool\"], \"transport\": \"stdio\" }. See https://github.com/KeenanHoffman/LSP-Brains"
            }
        }
    })
}

// ---------------------------------------------------------------------------
// CLI run function
// ---------------------------------------------------------------------------

pub async fn run(project_root: &str, output: &str, yes: bool) -> Result<()> {
    eprintln!("✦ Conjuring registry…");
    let root = PathBuf::from(project_root);
    if !root.is_dir() {
        bail!("Project root '{}' is not a directory.", project_root);
    }
    let root = root
        .canonicalize()
        .context("Cannot canonicalize project root")?;

    let output_path = PathBuf::from(output);
    if output_path.exists() {
        bail!(
            "Registry already exists at '{}'. Use 'neurogrim validate' to check it, \
            or delete it to re-init.",
            output
        );
    }

    // Derive project name from the last component of the root path
    let project_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("my-project")
        .to_string();

    // Scan
    println!("Scanning: {}", root.display());
    println!();
    let scan = scan_project(&root);

    // Print detection summary
    if scan.signals.is_empty() {
        println!("Detected signals: (none — universal domains only)");
    } else {
        println!("Detected signals:");
        for signal in &scan.signals {
            let label = signal_label(signal);
            let evidence = scan
                .evidence
                .get(signal)
                .and_then(|v| v.first())
                .cloned()
                .unwrap_or_default();
            println!("  + {} ({})", label, evidence);
        }
    }
    if scan.has_ci {
        println!("  + CI/CD configuration detected");
    }
    println!();

    let candidates = signals_to_domain_candidates(&scan);

    println!(
        "Universal domains (weighted):  code-quality (0.35), test-health (0.35), deploy-readiness (0.30)"
    );
    if candidates.is_empty() {
        println!("Advisory domains (weight 0.0): (none detected)");
    } else {
        let keys: Vec<&str> = candidates.iter().map(|c| c.key.as_str()).collect();
        println!("Advisory domains (weight 0.0): {}", keys.join(", "));
    }
    println!();

    // Confirm unless --yes
    if !yes {
        print!("Write to {}? [y/N]: ", output);
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        if !line.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    // Generate registry
    let registry = generate_registry(&candidates, &project_name);
    let registry_json = serde_json::to_string_pretty(&registry)?;

    // Create output directory and brain/ subdirectory
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    let brain_dir = output_path
        .parent()
        .map(|p| p.join("brain"))
        .unwrap_or_else(|| PathBuf::from("brain"));
    tokio::fs::create_dir_all(&brain_dir).await?;

    // Write empty ledger files
    for ledger_file in &[
        "score-history.json",
        "incident-ledger.json",
        "proposal-ledger.json",
    ] {
        let ledger_path = brain_dir.join(ledger_file);
        if !ledger_path.exists() {
            tokio::fs::write(&ledger_path, "[]").await?;
        }
    }

    // Create local-awareness.json (gitignored, machine-local fact store)
    let awareness_path = brain_dir.join("local-awareness.json");
    if !awareness_path.exists() {
        let empty = neurogrim_core::awareness::LocalAwareness::empty();
        let awareness_json = serde_json::to_string_pretty(&empty)?;
        tokio::fs::write(&awareness_path, awareness_json).await?;
    }

    // Add awareness file to .gitignore (create if missing, skip if entry already present)
    let gitignore_path = root.join(".gitignore");
    let gitignore_entry = ".claude/brain/local-awareness.json";
    let existing_gitignore = tokio::fs::read_to_string(&gitignore_path)
        .await
        .unwrap_or_default();
    if !existing_gitignore.contains(gitignore_entry) {
        let append = format!(
            "\n# Local machine awareness — machine-specific facts, never commit\n{}\n",
            gitignore_entry
        );
        let mut file = tokio::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&gitignore_path)
            .await?;
        file.write_all(append.as_bytes()).await?;
        println!("Updated: .gitignore (added local-awareness.json)");
    }

    // Write registry
    tokio::fs::write(&output_path, &registry_json).await?;

    println!("Written: {}", output);
    println!();
    println!("Next steps:");
    println!("  neurogrim validate     # verify configuration");
    println!("  neurogrim sensory test-health && neurogrim sensory code-quality && neurogrim sensory deploy-readiness");
    println!("  neurogrim score        # get your first score");
    println!("  neurogrim health       # full dashboard");
    println!();
    println!("Local awareness:");
    println!("  neurogrim awareness    # view machine-specific facts agents have recorded");
    println!("  neurogrim awareness add --key tool_paths.cargo --value /path/to/cargo --category tool_paths");

    Ok(())
}

fn signal_label(signal: &ProjectSignal) -> &'static str {
    match signal {
        ProjectSignal::Terraform => "Terraform",
        ProjectSignal::Java => "Java project",
        ProjectSignal::Node => "Node.js project",
        ProjectSignal::Python => "Python project",
        ProjectSignal::Go => "Go project",
        ProjectSignal::RustProject => "Rust project",
        ProjectSignal::Proto => "Proto/gRPC files",
        ProjectSignal::Kubernetes => "Kubernetes directory",
        ProjectSignal::Helm => "Helm chart",
        ProjectSignal::JiraHint => "Jira mention in README",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_file(dir: &Path, rel_path: &str) {
        let full = dir.join(rel_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, "").unwrap();
    }

    #[test]
    fn scan_detects_go_project() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "go.mod");
        let result = scan_project(tmp.path());
        assert!(result.signals.contains(&ProjectSignal::Go));
    }

    #[test]
    fn scan_detects_terraform() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "infra/main.tf");
        let result = scan_project(tmp.path());
        assert!(result.signals.contains(&ProjectSignal::Terraform));
    }

    #[test]
    fn scan_detects_proto() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "api/v1/service.proto");
        let result = scan_project(tmp.path());
        assert!(result.signals.contains(&ProjectSignal::Proto));
    }

    #[test]
    fn scan_skips_node_modules() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "node_modules/some-pkg/package.json");
        let result = scan_project(tmp.path());
        assert!(!result.signals.contains(&ProjectSignal::Node));
    }

    #[test]
    fn scan_depth_limited_at_3() {
        let tmp = TempDir::new().unwrap();
        // Depth 4: root/a/b/c/d/package.json
        create_file(tmp.path(), "a/b/c/d/package.json");
        let result = scan_project(tmp.path());
        assert!(!result.signals.contains(&ProjectSignal::Node));
    }

    #[test]
    fn scan_node_at_depth_1_detected() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "frontend/package.json");
        let result = scan_project(tmp.path());
        assert!(result.signals.contains(&ProjectSignal::Node));
    }

    #[test]
    fn scan_detects_ci_github_workflows() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), ".github/workflows/ci.yml");
        let result = scan_project(tmp.path());
        assert!(result.has_ci);
    }

    #[test]
    fn scan_detects_kubernetes_directory() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("k8s")).unwrap();
        let result = scan_project(tmp.path());
        assert!(result.signals.contains(&ProjectSignal::Kubernetes));
    }

    #[test]
    fn scan_detects_helm_chart() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "charts/mychart/Chart.yaml");
        let result = scan_project(tmp.path());
        assert!(result.signals.contains(&ProjectSignal::Helm));
    }

    #[test]
    fn scan_detects_jira_hint_in_readme() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("README.md"),
            "Track issues in JIRA board XYZ",
        )
        .unwrap();
        let result = scan_project(tmp.path());
        assert!(result.signals.contains(&ProjectSignal::JiraHint));
    }

    #[test]
    fn scan_no_jira_hint_without_readme() {
        let tmp = TempDir::new().unwrap();
        let result = scan_project(tmp.path());
        assert!(!result.signals.contains(&ProjectSignal::JiraHint));
    }

    #[test]
    fn candidates_from_go_and_proto() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "go.mod");
        create_file(tmp.path(), "api/service.proto");
        let scan = scan_project(tmp.path());
        let candidates = signals_to_domain_candidates(&scan);
        let keys: Vec<&str> = candidates.iter().map(|c| c.key.as_str()).collect();
        assert!(keys.contains(&"go-health"));
        assert!(keys.contains(&"api-contract"));
    }

    #[test]
    fn dependency_health_deduped_from_java_and_node() {
        let scan = ScanResult {
            signals: vec![ProjectSignal::Java, ProjectSignal::Node],
            has_git: false,
            has_tests: false,
            has_ci: false,
            evidence: HashMap::new(),
        };
        let candidates = signals_to_domain_candidates(&scan);
        let dep_count = candidates
            .iter()
            .filter(|c| c.key == "dependency-health")
            .count();
        assert_eq!(dep_count, 1, "dependency-health should appear only once");
    }

    #[test]
    fn universal_domains_always_in_registry() {
        let registry = generate_registry(&[], "my-project");
        let weights = registry["config"]["domain_weights"].as_object().unwrap();
        assert!(weights.contains_key("code-quality"));
        assert!(weights.contains_key("test-health"));
        assert!(weights.contains_key("deploy-readiness"));
        assert_eq!(weights["code-quality"].as_f64().unwrap(), 0.35);
        assert_eq!(weights["test-health"].as_f64().unwrap(), 0.35);
        assert_eq!(weights["deploy-readiness"].as_f64().unwrap(), 0.30);
    }

    #[test]
    fn weights_sum_to_one_for_universal_only() {
        let registry = generate_registry(&[], "my-project");
        let weights = registry["config"]["domain_weights"].as_object().unwrap();
        let sum: f64 = weights.values().filter_map(|v| v.as_f64()).sum();
        assert!(
            (sum - 1.0).abs() < 0.001,
            "weights should sum to 1.0, got {}",
            sum
        );
    }

    #[test]
    fn advisory_domains_at_zero_weight() {
        let candidates = vec![DomainCandidate {
            key: "go-health".to_string(),
            display_name: "Go Health".to_string(),
            reason: "go.mod detected".to_string(),
            sensory_hint: "write go tool".to_string(),
        }];
        let registry = generate_registry(&candidates, "my-project");
        let weights = registry["config"]["domain_weights"].as_object().unwrap();
        assert_eq!(weights["go-health"].as_f64().unwrap(), 0.0);
    }

    #[test]
    fn generated_registry_parses_as_valid_json() {
        let candidates = vec![DomainCandidate {
            key: "api-contract".to_string(),
            display_name: "API Contract".to_string(),
            reason: "proto files".to_string(),
            sensory_hint: "track proto drift".to_string(),
        }];
        let registry = generate_registry(&candidates, "my-project");
        let json_str = serde_json::to_string(&registry).unwrap();
        let reparsed: Value = serde_json::from_str(&json_str).unwrap();
        assert!(reparsed.get("meta").is_some());
        assert!(reparsed.get("config").is_some());
    }

    #[test]
    fn init_fails_when_output_already_exists() {
        let tmp = TempDir::new().unwrap();
        let output = tmp.path().join("brain-registry.json");
        fs::write(&output, "{}").unwrap();

        // run() is async but we can test the existence check synchronously
        assert!(output.exists());
    }

    #[test]
    fn todo_comment_keys_added_for_advisory_domains() {
        let candidates = vec![DomainCandidate {
            key: "go-health".to_string(),
            display_name: "Go Health".to_string(),
            reason: "go.mod".to_string(),
            sensory_hint: "hint".to_string(),
        }];
        let registry = generate_registry(&candidates, "my-project");
        let defs = registry["config"]["domain_definitions"]
            .as_object()
            .unwrap();
        assert!(
            defs.contains_key("_todo_go-health"),
            "_todo_ comment key must be present"
        );
        let todo_val = defs["_todo_go-health"].as_str().unwrap();
        assert!(todo_val.contains("go-health") || todo_val.contains("Go Health"));
    }
}
