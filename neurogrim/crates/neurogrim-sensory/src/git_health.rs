//! Git health sensory tool — the first dynamic sensory tool.

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct GitHealthServer {
    // rmcp #[tool_router] macro accesses this through generated dispatch — rustc can't see the uses
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl GitHealthServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckGitHealthParams {
    /// Path to the project root
    pub project_root: String,
}

#[tool_router]
impl GitHealthServer {
    #[tool(
        description = "Check git repository health: uncommitted changes, commit frequency, branch hygiene. Returns CMDB-envelope JSON."
    )]
    async fn check_git_health(&self, Parameters(p): Parameters<CheckGitHealthParams>) -> String {
        match analyze_git_health(&p.project_root).await {
            Ok(result) => serde_json::to_string_pretty(&result).unwrap_or_default(),
            Err(e) => serde_json::json!({"error": e.to_string(), "score": 0}).to_string(),
        }
    }
}

impl ServerHandler for GitHealthServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Git health sensory tool for NeuroGrim.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

/// Analyze git health and produce a CMDB envelope.
pub async fn analyze_git_health(project_root: &str) -> anyhow::Result<Value> {
    let root = Path::new(project_root);
    let mut findings = Vec::new();
    let mut score: i32 = 100;
    let mut extras: Vec<(&str, Value)> = Vec::new();

    if !root.join(".git").exists() {
        findings.push(Finding {
            name: "git_repo".into(),
            status: "missing".into(),
            points: -100,
            detail: Some("Not a git repository".into()),
        });
        return Ok(build_cmdb("check-git-health", 0, findings, None, None));
    }

    let dirty = git_dirty_count(project_root).await.unwrap_or(0);
    extras.push(("uncommitted_changes", Value::from(dirty)));
    if dirty > 0 {
        let d = (dirty as i32 * 3).min(30);
        score -= d;
        findings.push(Finding {
            name: "uncommitted_changes".into(),
            status: "dirty".into(),
            points: -d,
            detail: Some(format!("{} changes", dirty)),
        });
    }

    let untracked = git_untracked_count(project_root).await.unwrap_or(0);
    extras.push(("untracked_files", Value::from(untracked)));
    if untracked > 5 {
        let d = ((untracked as i32 - 5) * 2).min(20);
        score -= d;
    }

    let c24 = git_commit_count(project_root, "24 hours")
        .await
        .unwrap_or(0);
    let c7d = git_commit_count(project_root, "7 days").await.unwrap_or(0);
    extras.push(("commits_24h", Value::from(c24)));
    extras.push(("commits_7d", Value::from(c7d)));
    if c7d == 0 {
        score -= 15;
    }

    let div = git_branch_divergence(project_root).await.unwrap_or(0);
    extras.push(("branch_divergence", Value::from(div)));
    if div > 20 {
        score -= ((div as i32 - 20) / 5).min(20);
    }

    if !root.join(".gitignore").exists() {
        score -= 10;
    }

    Ok(build_cmdb(
        "check-git-health",
        score.clamp(0, 100) as u8,
        findings,
        Some(extras),
        None,
    ))
}

async fn git_cmd(root: &str, args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        anyhow::bail!("git failed")
    }
}
async fn git_dirty_count(r: &str) -> anyhow::Result<u32> {
    Ok(git_cmd(r, &["status", "--porcelain"])
        .await?
        .lines()
        .filter(|l| !l.starts_with("??"))
        .count() as u32)
}
async fn git_untracked_count(r: &str) -> anyhow::Result<u32> {
    Ok(git_cmd(r, &["status", "--porcelain"])
        .await?
        .lines()
        .filter(|l| l.starts_with("??"))
        .count() as u32)
}
async fn git_commit_count(r: &str, since: &str) -> anyhow::Result<u32> {
    Ok(git_cmd(
        r,
        &[
            "rev-list",
            "--count",
            "HEAD",
            &format!("--since={} ago", since),
        ],
    )
    .await?
    .parse()
    .unwrap_or(0))
}
async fn git_branch_divergence(r: &str) -> anyhow::Result<u32> {
    for b in &["main", "master"] {
        if let Ok(s) = git_cmd(r, &["rev-list", "--count", &format!("{}..HEAD", b)]).await {
            if let Ok(n) = s.parse() {
                return Ok(n);
            }
        }
    }
    Ok(0)
}
