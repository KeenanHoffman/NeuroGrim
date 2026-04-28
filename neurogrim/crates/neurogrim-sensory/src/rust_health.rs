//! Rust health sensory tool — workspace + lint + MSRV discipline.
//!
//! Static-only checks (no subprocess execution): detects presence of
//! Rust-specific config files and discipline indicators. Intentionally
//! avoids running `cargo clippy` / `cargo audit` to stay aligned with
//! the §16.2 anti-scanner-chain-compromise principle (the binary that
//! produces the signal becomes part of the trust surface). Future
//! iterations may add an opt-in `--with-cargo` flag for live tool
//! invocation; v1 ships static-only.

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RustHealthServer {
    tool_router: ToolRouter<Self>,
}

impl RustHealthServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckRustHealthParams {
    /// Path to the project root
    pub project_root: String,
}

#[tool_router]
impl RustHealthServer {
    #[tool(
        description = "Check Rust health: workspace presence, MSRV pin, lint discipline (rustfmt + clippy + cargo-deny), unified [lints] table, CI integration. Returns CMDB-envelope JSON."
    )]
    async fn check_rust_health(&self, Parameters(p): Parameters<CheckRustHealthParams>) -> String {
        serde_json::to_string_pretty(&analyze_rust_health(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for RustHealthServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Rust health sensory tool.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

/// Find the workspace's Cargo.toml. Returns the workspace dir.
///
/// Probes (in order):
///   1. `<project_root>/Cargo.toml`
///   2. `<project_root>/neurogrim/Cargo.toml` (NeuroGrim convention —
///      workspace lives in a subdirectory)
///
/// Returns the directory containing Cargo.toml, or None if not a Rust
/// project.
fn find_workspace_root(project_root: &Path) -> Option<PathBuf> {
    let candidates = [project_root.to_path_buf(), project_root.join("neurogrim")];
    candidates
        .into_iter()
        .find(|p| p.join("Cargo.toml").is_file())
}

pub async fn analyze_rust_health(project_root: &str) -> Value {
    let root = Path::new(project_root);
    let mut findings: Vec<Finding> = Vec::new();
    let mut score: i32 = 0;
    let mut extras: Vec<(&str, Value)> = Vec::new();

    let workspace = match find_workspace_root(root) {
        Some(w) => w,
        None => {
            findings.push(Finding {
                name: "cargo_toml".into(),
                status: "missing".into(),
                points: 0,
                detail: Some(
                    "No Cargo.toml found at root or in neurogrim/ subdir — not a Rust project"
                        .into(),
                ),
            });
            extras.push(("is_rust_project", Value::Bool(false)));
            return build_cmdb("check-rust-health", 0, findings, Some(extras), None);
        }
    };

    extras.push(("is_rust_project", Value::Bool(true)));
    extras.push((
        "workspace_path",
        Value::String(
            workspace
                .strip_prefix(root)
                .unwrap_or(&workspace)
                .to_string_lossy()
                .into_owned(),
        ),
    ));

    let cargo_toml = workspace.join("Cargo.toml");
    let cargo_toml_text = std::fs::read_to_string(&cargo_toml).unwrap_or_default();

    // Foundation: Cargo.toml present (20)
    score += 20;
    findings.push(Finding {
        name: "cargo_toml".into(),
        status: "found".into(),
        points: 20,
        detail: None,
    });

    // Cargo.lock committed (10)
    let has_lock = workspace.join("Cargo.lock").is_file();
    extras.push(("has_cargo_lock", Value::Bool(has_lock)));
    if has_lock {
        score += 10;
        findings.push(Finding {
            name: "cargo_lock".into(),
            status: "found".into(),
            points: 10,
            detail: None,
        });
    }

    // MSRV pin (15) — rust-toolchain file OR rust-version field in Cargo.toml
    let has_toolchain_file = workspace.join("rust-toolchain.toml").is_file()
        || workspace.join("rust-toolchain").is_file();
    let has_rust_version = cargo_toml_text.contains("rust-version");
    let has_msrv = has_toolchain_file || has_rust_version;
    extras.push(("has_msrv_pin", Value::Bool(has_msrv)));
    if has_msrv {
        score += 15;
        findings.push(Finding {
            name: "msrv_pin".into(),
            status: "found".into(),
            points: 15,
            detail: Some(if has_toolchain_file {
                "rust-toolchain file present".into()
            } else {
                "Cargo.toml has rust-version field".into()
            }),
        });
    }

    // Rustfmt config (10)
    let has_rustfmt =
        workspace.join("rustfmt.toml").is_file() || workspace.join(".rustfmt.toml").is_file();
    extras.push(("has_rustfmt_config", Value::Bool(has_rustfmt)));
    if has_rustfmt {
        score += 10;
        findings.push(Finding {
            name: "rustfmt_config".into(),
            status: "found".into(),
            points: 10,
            detail: None,
        });
    }

    // Clippy config (15) — explicit clippy.toml, [lints.clippy] subsection,
    // OR inline-table form `clippy = { ... }` inside a `[lints]` /
    // `[workspace.lints]` table.
    let has_clippy_file =
        workspace.join("clippy.toml").is_file() || workspace.join(".clippy.toml").is_file();
    let has_clippy_subsection = cargo_toml_text.contains("[lints.clippy]")
        || cargo_toml_text.contains("[workspace.lints.clippy]");
    let has_lints_with_clippy = (cargo_toml_text.contains("[lints]")
        || cargo_toml_text.contains("[workspace.lints]"))
        && cargo_toml_text.contains("clippy");
    let has_clippy_lints = has_clippy_subsection || has_lints_with_clippy;
    let has_clippy_config = has_clippy_file || has_clippy_lints;
    extras.push(("has_clippy_config", Value::Bool(has_clippy_config)));
    if has_clippy_config {
        score += 15;
        findings.push(Finding {
            name: "clippy_config".into(),
            status: "found".into(),
            points: 15,
            detail: Some(if has_clippy_file {
                "clippy.toml present".into()
            } else {
                "Cargo.toml [lints.clippy] table".into()
            }),
        });
    }

    // cargo-deny config (10) — supply-chain discipline
    let has_deny = workspace.join("deny.toml").is_file();
    extras.push(("has_cargo_deny", Value::Bool(has_deny)));
    if has_deny {
        score += 10;
        findings.push(Finding {
            name: "cargo_deny_config".into(),
            status: "found".into(),
            points: 10,
            detail: None,
        });
    }

    // [lints] table (10) — Rust 2024+ unified lint config. Accepts both
    // top-level forms (`[lints]`, `[workspace.lints]`) and deeply-nested
    // subsection forms (`[lints.clippy]`, `[workspace.lints.clippy]`).
    let has_lints_table = cargo_toml_text.contains("[lints]")
        || cargo_toml_text.contains("[workspace.lints]")
        || cargo_toml_text.contains("[lints.")
        || cargo_toml_text.contains("[workspace.lints.");
    extras.push(("has_lints_table", Value::Bool(has_lints_table)));
    if has_lints_table {
        score += 10;
        findings.push(Finding {
            name: "lints_table".into(),
            status: "found".into(),
            points: 10,
            detail: Some("Rust 2024+ unified [lints] config".into()),
        });
    }

    // CI integration (10) — .github/workflows OR .gitlab-ci.yml at workspace OR root
    let has_ci = workspace.join(".github/workflows").is_dir()
        || root.join(".github/workflows").is_dir()
        || workspace.join(".gitlab-ci.yml").is_file()
        || root.join(".gitlab-ci.yml").is_file();
    extras.push(("has_ci", Value::Bool(has_ci)));
    if has_ci {
        score += 10;
        findings.push(Finding {
            name: "ci_integration".into(),
            status: "found".into(),
            points: 10,
            detail: None,
        });
    }

    build_cmdb(
        "check-rust-health",
        score.clamp(0, 100) as u8,
        findings,
        Some(extras),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn rust_health_returns_zero_for_non_rust_project() {
        let tmp = TempDir::new().unwrap();
        let result = analyze_rust_health(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["score"], 0);
        assert_eq!(result["is_rust_project"], false);
    }

    #[tokio::test]
    async fn rust_health_finds_cargo_toml_at_root() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let result = analyze_rust_health(tmp.path().to_str().unwrap()).await;
        assert!(result["score"].as_u64().unwrap() >= 20);
        assert_eq!(result["is_rust_project"], true);
    }

    #[tokio::test]
    async fn rust_health_finds_neurogrim_workspace_subdir() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("neurogrim")).unwrap();
        std::fs::write(
            tmp.path().join("neurogrim/Cargo.toml"),
            "[workspace]\nmembers = [\"foo\"]\n",
        )
        .unwrap();
        let result = analyze_rust_health(tmp.path().to_str().unwrap()).await;
        assert!(result["score"].as_u64().unwrap() >= 20);
        assert_eq!(result["is_rust_project"], true);
    }

    #[tokio::test]
    async fn rust_health_detects_msrv_via_cargo_toml() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\nrust-version = \"1.75\"\n",
        )
        .unwrap();
        let result = analyze_rust_health(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["has_msrv_pin"], true);
    }

    #[tokio::test]
    async fn rust_health_detects_msrv_via_toolchain_file() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"stable\"\n",
        )
        .unwrap();
        let result = analyze_rust_health(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["has_msrv_pin"], true);
    }

    #[tokio::test]
    async fn rust_health_detects_workspace_lints_clippy() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"foo\"]\n[workspace.lints.clippy]\nall = \"warn\"\n",
        )
        .unwrap();
        let result = analyze_rust_health(tmp.path().to_str().unwrap()).await;
        assert_eq!(result["has_clippy_config"], true);
        assert_eq!(result["has_lints_table"], true);
    }

    #[tokio::test]
    async fn rust_health_full_score_with_all_signals() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            r#"[workspace]
members = ["foo"]

[workspace.package]
rust-version = "1.75"

[workspace.lints]
clippy = { all = "warn" }
"#,
        )
        .unwrap();
        std::fs::write(tmp.path().join("Cargo.lock"), "# Auto-generated\n").unwrap();
        std::fs::write(tmp.path().join("rustfmt.toml"), "edition = \"2021\"\n").unwrap();
        std::fs::write(tmp.path().join("deny.toml"), "[advisories]\n").unwrap();
        std::fs::create_dir_all(tmp.path().join(".github/workflows")).unwrap();
        std::fs::write(
            tmp.path().join(".github/workflows/ci.yml"),
            "name: CI\non: [push]\n",
        )
        .unwrap();
        let result = analyze_rust_health(tmp.path().to_str().unwrap()).await;
        // 20 (cargo) + 10 (lock) + 15 (msrv) + 10 (rustfmt) + 15 (clippy)
        // + 10 (deny) + 10 (lints) + 10 (ci) = 100
        assert_eq!(result["score"], 100);
    }
}
