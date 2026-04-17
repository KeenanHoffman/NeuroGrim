//! Code quality sensory tool.

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct CodeQualityServer {
    tool_router: ToolRouter<Self>,
}
impl CodeQualityServer {
    pub fn new() -> Self { Self { tool_router: Self::tool_router() } }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckCodeQualityParams { pub project_root: String }

#[tool_router]
impl CodeQualityServer {
    #[tool(description = "Check code quality: README, lint configs, editor config. Returns CMDB-envelope JSON.")]
    async fn check_code_quality(&self, Parameters(p): Parameters<CheckCodeQualityParams>) -> String {
        serde_json::to_string_pretty(&analyze_code_quality(&p.project_root).await).unwrap_or_default()
    }
}

impl ServerHandler for CodeQualityServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo { instructions: Some("Code quality sensory tool.".into()), capabilities: ServerCapabilities::builder().enable_tools().build(), ..Default::default() }
    }
}

pub async fn analyze_code_quality(project_root: &str) -> Value {
    let root = Path::new(project_root);
    let mut findings = Vec::new();
    let mut score: i32 = 0;
    let mut extras: Vec<(&str, Value)> = Vec::new();

    let has_readme = root.join("README.md").exists() || root.join("README").exists();
    extras.push(("has_readme", Value::Bool(has_readme)));
    if has_readme { score += 20; findings.push(Finding { name: "readme".into(), status: "found".into(), points: 20, detail: None }); }

    let lint_configs = [".eslintrc",".eslintrc.json","eslint.config.js",".pylintrc","pyproject.toml","rustfmt.toml",".rubocop.yml",".golangci.yml","biome.json"];
    let lc = lint_configs.iter().filter(|f| root.join(f).exists()).count();
    extras.push(("has_lint_config", Value::Bool(lc > 0)));
    if lc > 0 { score += 25; findings.push(Finding { name: "lint_config".into(), status: "found".into(), points: 25, detail: Some(format!("{} config(s)", lc)) }); }

    if [".prettierrc",".prettierrc.json",".clang-format","rustfmt.toml"].iter().any(|f| root.join(f).exists()) { score += 15; }
    if root.join(".editorconfig").exists() { score += 10; }
    if root.join("LICENSE").exists() || root.join("LICENSE.md").exists() { score += 10; }
    if root.join(".gitignore").exists() { score += 10; }
    if root.join(".github/workflows").exists() { score += 10; }

    build_cmdb("check-code-quality", score.clamp(0, 100) as u8, findings, Some(extras))
}
