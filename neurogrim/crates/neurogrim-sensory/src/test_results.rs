//! Test results sensory tool.

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
pub struct TestResultsServer {
    // rmcp #[tool_router] macro accesses this through generated dispatch — rustc can't see the uses
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}
impl TestResultsServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckTestHealthParams {
    pub project_root: String,
}

#[tool_router]
impl TestResultsServer {
    #[tool(
        description = "Check test health: test directories, configs, results, coverage. Returns CMDB-envelope JSON."
    )]
    async fn check_test_health(&self, Parameters(p): Parameters<CheckTestHealthParams>) -> String {
        serde_json::to_string_pretty(&analyze_test_health(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for TestResultsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Test health sensory tool.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub async fn analyze_test_health(project_root: &str) -> Value {
    let root = Path::new(project_root);
    let mut findings = Vec::new();
    let mut score: i32 = 0;
    let mut extras: Vec<(&str, Value)> = Vec::new();

    let test_dirs = ["tests", "test", "spec", "__tests__", "src/test"];
    let has_td = test_dirs.iter().any(|d| root.join(d).exists());
    extras.push(("has_test_directory", Value::Bool(has_td)));
    if has_td {
        score += 30;
        findings.push(Finding {
            name: "test_directory".into(),
            status: "found".into(),
            points: 30,
            detail: None,
        });
    }

    let test_configs = [
        "jest.config.js",
        "jest.config.ts",
        "vitest.config.ts",
        "pytest.ini",
        "conftest.py",
        "Cargo.toml",
        "phpunit.xml",
    ];
    let has_tc = test_configs.iter().any(|f| root.join(f).exists());
    extras.push(("has_test_config", Value::Bool(has_tc)));
    if has_tc {
        score += 20;
        findings.push(Finding {
            name: "test_config".into(),
            status: "found".into(),
            points: 20,
            detail: None,
        });
    }

    let has_results = root.join("test-results.xml").exists() || root.join("junit.xml").exists();
    extras.push(("has_test_results", Value::Bool(has_results)));
    if has_results {
        score += 20;
    }

    if [".nycrc", ".coveragerc", "codecov.yml"]
        .iter()
        .any(|f| root.join(f).exists())
    {
        score += 15;
    }
    if root.join(".github/workflows").exists() {
        score += 15;
    }

    build_cmdb(
        "check-test-health",
        score.clamp(0, 100) as u8,
        findings,
        Some(extras),
        None,
    )
}
