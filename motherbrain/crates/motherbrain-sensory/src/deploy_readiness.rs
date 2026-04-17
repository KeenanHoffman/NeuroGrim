//! Deploy readiness sensory tool.

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DeployReadinessServer {
    tool_router: ToolRouter<Self>,
}
impl DeployReadinessServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckDeployReadinessParams {
    pub project_root: String,
}

#[tool_router]
impl DeployReadinessServer {
    #[tool(
        description = "Check deploy readiness: CI/CD, containers, version control. Returns CMDB-envelope JSON."
    )]
    async fn check_deploy_readiness(
        &self,
        Parameters(p): Parameters<CheckDeployReadinessParams>,
    ) -> String {
        serde_json::to_string_pretty(&analyze_deploy_readiness(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for DeployReadinessServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Deploy readiness sensory tool.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub async fn analyze_deploy_readiness(project_root: &str) -> Value {
    let root = Path::new(project_root);
    let mut findings = Vec::new();
    let mut score: i32 = 0;
    let mut extras: Vec<(&str, Value)> = Vec::new();

    let ci = [
        (".github/workflows", "GitHub Actions"),
        (".gitlab-ci.yml", "GitLab CI"),
        ("Jenkinsfile", "Jenkins"),
        ("azure-pipelines.yml", "Azure Pipelines"),
    ];
    let found: Vec<&str> = ci
        .iter()
        .filter(|(f, _)| root.join(f).exists())
        .map(|(_, n)| *n)
        .collect();
    extras.push(("has_ci", Value::Bool(!found.is_empty())));
    if !found.is_empty() {
        score += 30;
        findings.push(Finding {
            name: "ci_config".into(),
            status: "found".into(),
            points: 30,
            detail: Some(found.join(", ")),
        });
    }

    if ["Dockerfile", "docker-compose.yml", "compose.yml"]
        .iter()
        .any(|f| root.join(f).exists())
    {
        score += 20;
    }
    if root.join(".git").exists() {
        score += 15;
    }
    if root.join(".gitignore").exists() {
        score += 10;
    }
    if ["terraform", "pulumi"]
        .iter()
        .any(|f| root.join(f).exists())
        || root.join("cdk.json").exists()
    {
        score += 15;
    }
    if root.join(".env.example").exists() || root.join(".env.template").exists() {
        score += 10;
    }

    build_cmdb(
        "check-deploy-readiness",
        score.clamp(0, 100) as u8,
        findings,
        Some(extras),
    )
}
