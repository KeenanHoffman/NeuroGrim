//! `neurogrim doctor` — validate Brain configuration without scoring (v3.2 Phase A.2).
//!
//! v3.2.1: this module is a thin printing wrapper around
//! `neurogrim_mcp::doctor::audit`, which holds the canonical check
//! functions. The MCP `doctor` tool calls the same audit function and
//! returns the findings as JSON.
//!
//! Read-only. No ledger writes. No scoring. Exit codes:
//!   0 — clean (no findings)
//!   1 — warnings (Brain is usable but has degraded posture)
//!   2 — errors (Brain is misconfigured; downstream commands will misbehave)

use anyhow::{Context, Result};
use colored::*;
use neurogrim_core::registry::BrainRegistry;
use neurogrim_mcp::doctor::{audit, Finding, Severity};
use std::path::{Path, PathBuf};

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
    findings.extend(audit(&registry, &project_root));

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
    sorted.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.category.cmp(b.category))
    });

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
