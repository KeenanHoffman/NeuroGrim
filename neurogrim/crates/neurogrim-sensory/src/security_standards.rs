//! Security standards sensory tool.
//!
//! Scans a project's file tree for compliance evidence across 5 SOC2 Common Criteria
//! control groups. Each group scores up to 20 points (100 total).
//!
//! This is evidence-based scoring — absence of evidence does not mean the project is
//! insecure. It means no evidence is visible in the file tree.

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SecurityStandardsServer {
    tool_router: ToolRouter<Self>,
}
impl SecurityStandardsServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckSecurityStandardsParams {
    pub project_root: String,
}

#[tool_router]
impl SecurityStandardsServer {
    #[tool(
        description = "Check security standards compliance evidence: SOC2 CC, ISO27001 Annex A, NIST CSF. \
        Scans for vulnerability disclosure policy, secrets management, dependency scanning, \
        access controls, and SAST tooling. Returns CMDB-envelope JSON."
    )]
    async fn check_security_standards(
        &self,
        Parameters(p): Parameters<CheckSecurityStandardsParams>,
    ) -> String {
        serde_json::to_string_pretty(&analyze_security_standards(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for SecurityStandardsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Security standards compliance evidence sensory tool (SOC2 CC / ISO27001 / NIST CSF).".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub async fn analyze_security_standards(project_root: &str) -> Value {
    let root = Path::new(project_root);
    let mut findings = Vec::new();
    let mut score: i32 = 0;
    let mut extras: Vec<(&str, Value)> = Vec::new();

    // ── Pre-load content for checks that need it ──────────────────────────────

    // Read .gitignore content (Group 2)
    let gitignore_content = tokio::fs::read_to_string(root.join(".gitignore"))
        .await
        .unwrap_or_default()
        .to_lowercase();

    // Read all .github/workflows/*.yml content (Groups 3 and 5)
    let mut workflow_content = String::new();
    let workflows_dir = root.join(".github/workflows");
    let mut has_workflow_files = false;
    if workflows_dir.exists() {
        if let Ok(mut entries) = tokio::fs::read_dir(&workflows_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "yml" || ext == "yaml" {
                    has_workflow_files = true;
                    if let Ok(text) = tokio::fs::read_to_string(&path).await {
                        workflow_content.push_str(&text.to_lowercase());
                        workflow_content.push('\n');
                    }
                }
            }
        }
    }

    // ── Group 1: Vulnerability Disclosure Policy (CC2.2, CC9.1) ─────────────
    // ISO27001: A.5.1, A.6.8 | NIST CSF: GV.RM-01, RS.CO-02
    let has_security_policy =
        root.join("SECURITY.md").exists() || root.join(".github/SECURITY.md").exists();
    extras.push(("has_security_policy", Value::Bool(has_security_policy)));
    if has_security_policy {
        score += 20;
        findings.push(Finding {
            name: "vulnerability_disclosure_policy".into(),
            status: "found".into(),
            points: 20,
            detail: Some("SECURITY.md present — vulnerability disclosure policy documented [SOC2: CC2.2, CC9.1 | ISO27001: A.5.1, A.6.8 | NIST CSF: GV.RM-01, RS.CO-02]".into()),
        });
    } else {
        findings.push(Finding {
            name: "vulnerability_disclosure_policy".into(),
            status: "missing".into(),
            points: 0,
            detail: Some("No SECURITY.md found — create SECURITY.md or .github/SECURITY.md [SOC2: CC2.2, CC9.1]".into()),
        });
    }

    // ── Group 2: Secrets & Credential Management (CC6.1) ────────────────────
    // ISO27001: A.7.1, A.8.20 | NIST CSF: PR.AA-01, PR.DS-02
    let has_env_template = root.join(".env.example").exists()
        || root.join(".env.template").exists()
        || root.join(".env.sample").exists();

    let secret_patterns = [
        "*.pem",
        "*.key",
        ".env",
        "credentials",
        "*.secret",
        "*.pfx",
        "*.p12",
    ];
    let gitignore_excludes_secrets = secret_patterns
        .iter()
        .any(|p| gitignore_content.contains(p));

    let secrets_score =
        if has_env_template { 10 } else { 0 } + if gitignore_excludes_secrets { 10 } else { 0 };
    let has_secrets_management = secrets_score > 0;
    extras.push((
        "has_secrets_management",
        Value::Bool(has_secrets_management),
    ));
    score += secrets_score;

    if has_env_template {
        findings.push(Finding {
            name: "env_template".into(),
            status: "found".into(),
            points: 10,
            detail: Some(".env.example present — credential template documents expected secrets [SOC2: CC6.1]".into()),
        });
    } else {
        findings.push(Finding {
            name: "env_template".into(),
            status: "missing".into(),
            points: 0,
            detail: Some("No .env.example found — add a template to document expected environment variables [SOC2: CC6.1]".into()),
        });
    }
    if gitignore_excludes_secrets {
        findings.push(Finding {
            name: "gitignore_excludes_secrets".into(),
            status: "found".into(),
            points: 10,
            detail: Some(".gitignore excludes secret file patterns (*.pem, *.key, .env, etc.) [SOC2: CC6.1 | ISO27001: A.7.1 | NIST CSF: PR.DS-02]".into()),
        });
    } else {
        findings.push(Finding {
            name: "gitignore_excludes_secrets".into(),
            status: "missing".into(),
            points: 0,
            detail: Some(".gitignore does not exclude secret file patterns — add *.pem, *.key, .env, credentials [SOC2: CC6.1]".into()),
        });
    }

    // ── Group 3: Dependency Vulnerability Management (CC7.1) ─────────────────
    // ISO27001: A.7.3, A.8.8 | NIST CSF: ID.RA-01, PR.PS-06
    let has_dep_tool = root.join(".github/dependabot.yml").exists()
        || root.join("renovate.json").exists()
        || root.join(".renovaterc").exists()
        || root.join("renovate.json5").exists()
        || root.join(".github/renovate.json").exists();

    let dep_audit_patterns = [
        "audit",
        "trivy",
        "snyk",
        "grype",
        "pip-audit",
        "cargo audit",
        "npm audit",
        "yarn audit",
        "osv-scanner",
    ];
    let has_dep_scanning_ci = dep_audit_patterns
        .iter()
        .any(|p| workflow_content.contains(p));

    let dep_score = if has_dep_tool { 10 } else { 0 } + if has_dep_scanning_ci { 10 } else { 0 };
    let has_dependency_management = dep_score > 0;
    extras.push((
        "has_dependency_management",
        Value::Bool(has_dependency_management),
    ));
    score += dep_score;

    if has_dep_tool {
        findings.push(Finding {
            name: "dependency_update_tool".into(),
            status: "found".into(),
            points: 10,
            detail: Some("Dependabot or Renovate configured — automated dependency vulnerability monitoring [SOC2: CC7.1 | ISO27001: A.8.8 | NIST CSF: ID.RA-01]".into()),
        });
    } else {
        findings.push(Finding {
            name: "dependency_update_tool".into(),
            status: "missing".into(),
            points: 0,
            detail: Some("No Dependabot or Renovate config found — add .github/dependabot.yml or renovate.json [SOC2: CC7.1]".into()),
        });
    }
    if has_dep_scanning_ci {
        findings.push(Finding {
            name: "ci_dependency_audit".into(),
            status: "found".into(),
            points: 10,
            detail: Some("CI workflow runs dependency audit (trivy/snyk/audit/grype/osv-scanner) [SOC2: CC7.1 | ISO27001: A.7.3 | NIST CSF: PR.PS-06]".into()),
        });
    } else {
        findings.push(Finding {
            name: "ci_dependency_audit".into(),
            status: "missing".into(),
            points: 0,
            detail: Some("No dependency audit step found in CI workflows — add trivy, snyk, or cargo/npm audit [SOC2: CC7.1]".into()),
        });
    }

    // ── Group 4: Access Control & Change Authorization (CC6.3, CC8.1) ────────
    // ISO27001: A.5.3, A.8.2 | NIST CSF: PR.AA-05, PR.PS-04
    let has_codeowners = root.join("CODEOWNERS").exists()
        || root.join(".github/CODEOWNERS").exists()
        || root.join(".gitlab/CODEOWNERS").exists();
    extras.push(("has_codeowners", Value::Bool(has_codeowners)));

    if has_codeowners {
        score += 15;
        findings.push(Finding {
            name: "codeowners".into(),
            status: "found".into(),
            points: 15,
            detail: Some("CODEOWNERS present — code ownership and required reviewers defined [SOC2: CC6.3, CC8.1 | ISO27001: A.5.3 | NIST CSF: PR.AA-05]".into()),
        });
    } else {
        findings.push(Finding {
            name: "codeowners".into(),
            status: "missing".into(),
            points: 0,
            detail: Some("No CODEOWNERS file — add CODEOWNERS or .github/CODEOWNERS to define required reviewers [SOC2: CC6.3, CC8.1]".into()),
        });
    }
    if has_workflow_files {
        score += 5;
        findings.push(Finding {
            name: "ci_workflows".into(),
            status: "found".into(),
            points: 5,
            detail: Some("CI/CD workflows present — systematic change deployment process defined [SOC2: CC8.1 | ISO27001: A.8.2 | NIST CSF: PR.PS-04]".into()),
        });
    } else {
        findings.push(Finding {
            name: "ci_workflows".into(),
            status: "missing".into(),
            points: 0,
            detail: Some("No CI workflow files found in .github/workflows/ [SOC2: CC8.1]".into()),
        });
    }

    // ── Group 5: Static Security Analysis / SAST (CC7.1) ────────────────────
    // ISO27001: A.7.3, A.8.25 | NIST CSF: PR.DS-08, DE.CM-01
    let sast_patterns = [
        "codeql",
        "semgrep",
        "sast",
        "sonarqube",
        "sonarcloud",
        "checkmarx",
        "veracode",
        "snyk code",
        "bearer",
        "horusec",
    ];
    let has_sast = sast_patterns.iter().any(|p| workflow_content.contains(p));
    extras.push(("has_sast", Value::Bool(has_sast)));
    if has_sast {
        score += 20;
        findings.push(Finding {
            name: "sast".into(),
            status: "found".into(),
            points: 20,
            detail: Some("SAST tool detected in CI workflow (CodeQL/Semgrep/Sonar/Checkmarx/etc.) [SOC2: CC7.1 | ISO27001: A.7.3, A.8.25 | NIST CSF: PR.DS-08, DE.CM-01]".into()),
        });
    } else {
        findings.push(Finding {
            name: "sast".into(),
            status: "missing".into(),
            points: 0,
            detail: Some("No SAST tool found in CI workflows — add CodeQL, Semgrep, or Sonar [SOC2: CC7.1 | ISO27001: A.8.25]".into()),
        });
    }

    // ── Aggregate exports ─────────────────────────────────────────────────────
    let controls_evidenced = [
        has_security_policy,
        has_secrets_management,
        has_dependency_management,
        has_codeowners || has_workflow_files,
        has_sast,
    ]
    .iter()
    .filter(|&&v| v)
    .count() as u8;
    extras.push(("controls_evidenced", Value::from(controls_evidenced)));

    // ── Control references map (SOC2 / ISO27001 / NIST CSF) ──────────────────
    extras.push((
        "control_references",
        json!({
            "vulnerability_disclosure": {
                "soc2":     ["CC2.2", "CC9.1"],
                "iso27001": ["A.5.1", "A.6.8"],
                "nist_csf": ["GV.RM-01", "RS.CO-02"]
            },
            "secrets_management": {
                "soc2":     ["CC6.1"],
                "iso27001": ["A.7.1", "A.8.20"],
                "nist_csf": ["PR.AA-01", "PR.DS-02"]
            },
            "dependency_management": {
                "soc2":     ["CC7.1"],
                "iso27001": ["A.7.3", "A.8.8"],
                "nist_csf": ["ID.RA-01", "PR.PS-06"]
            },
            "access_control": {
                "soc2":     ["CC6.3", "CC8.1"],
                "iso27001": ["A.5.3", "A.8.2"],
                "nist_csf": ["PR.AA-05", "PR.PS-04"]
            },
            "sast": {
                "soc2":     ["CC7.1"],
                "iso27001": ["A.7.3", "A.8.25"],
                "nist_csf": ["PR.DS-08", "DE.CM-01"]
            }
        }),
    ));

    build_cmdb(
        "check-security-standards",
        score.clamp(0, 100) as u8,
        findings,
        Some(extras),
    )
}
