//! `cargo xtask sca-check` — consolidated SCA gate for the workspace.
//!
//! Runs:
//! - **`cargo audit`** against the workspace `Cargo.lock` (Rust deps, RustSec advisories)
//! - **`npm audit --json`** against every `package.json` (with sibling `package-lock.json`)
//!   under the workspace, excluding `node_modules`. Parses JSON output and aggregates.
//!
//! Exit codes:
//! - `0` — clean OR all findings below the severity floor
//! - `1` — findings at or above the floor (default: `moderate`)
//! - `2` — tool not installed, missing lockfile, or other infra error

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use colored::Colorize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Severity floor — findings at or above this level fail the check.
    /// Allowed: low, moderate, high, critical. Default: moderate.
    #[arg(long, default_value = "moderate")]
    severity: String,

    /// Don't fail on missing tools (cargo-audit, npm). Useful in
    /// dev environments where you want partial coverage.
    #[arg(long)]
    skip_missing_tools: bool,

    /// Path to scan from. Defaults to the workspace root (one level
    /// up from `xtask/`).
    #[arg(long)]
    root: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Severity {
    Info,
    Low,
    Moderate,
    High,
    Critical,
}

impl Severity {
    fn from_str(s: &str) -> Result<Severity> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "info" => Severity::Info,
            "low" => Severity::Low,
            "moderate" | "medium" => Severity::Moderate,
            "high" => Severity::High,
            "critical" => Severity::Critical,
            other => anyhow::bail!("unknown severity: {other}"),
        })
    }
}

pub fn run(args: Args) -> Result<()> {
    let floor = Severity::from_str(&args.severity)
        .with_context(|| format!("invalid --severity: {}", args.severity))?;

    let root = match args.root {
        Some(p) => p,
        None => find_workspace_root()?,
    };

    println!("{}", "=== cargo xtask sca-check ===".bold());
    println!("Workspace root: {}", root.display());
    println!("Severity floor: {:?}", floor);
    println!();

    let mut errors = 0usize;
    let mut warnings = 0usize;

    // 1) cargo audit — Rust workspace
    match run_cargo_audit(&root, args.skip_missing_tools) {
        AuditOutcome::Clean => println!("{}", "  ✓ cargo audit: no advisories".green()),
        AuditOutcome::Findings { high_or_above, total } => {
            println!(
                "  {} cargo audit: {} advisories ({} at high/critical)",
                "✗".red(),
                total,
                high_or_above
            );
            errors += high_or_above;
            warnings += total.saturating_sub(high_or_above);
        }
        AuditOutcome::ToolMissing => {
            if args.skip_missing_tools {
                println!(
                    "  {} cargo-audit not installed — skipped (install with `cargo install cargo-audit`)",
                    "!".yellow()
                );
            } else {
                eprintln!(
                    "{}",
                    "  ✗ cargo-audit not installed. Install: `cargo install cargo-audit`. Or pass --skip-missing-tools."
                        .red()
                );
                std::process::exit(2);
            }
        }
        AuditOutcome::Error(msg) => {
            eprintln!("  {} cargo audit error: {msg}", "✗".red());
            errors += 1;
        }
    }

    // 2) npm audit — every package.json + package-lock.json under the
    //    workspace (excluding node_modules)
    let npm_lockfiles = find_npm_lockfiles(&root);
    if npm_lockfiles.is_empty() {
        println!("  {} no npm lockfiles found in workspace — skipping npm audit", "·".dimmed());
    } else {
        println!(
            "  {} found {} npm lockfile(s); auditing each",
            "·".dimmed(),
            npm_lockfiles.len()
        );
        for lockdir in &npm_lockfiles {
            match run_npm_audit(lockdir, floor, args.skip_missing_tools) {
                AuditOutcome::Clean => println!(
                    "  {} npm audit ({}): no findings at or above {:?}",
                    "✓".green(),
                    lockdir.display(),
                    floor
                ),
                AuditOutcome::Findings { high_or_above, total } => {
                    println!(
                        "  {} npm audit ({}): {} findings ({} at or above {:?})",
                        "!".yellow(),
                        lockdir.display(),
                        total,
                        high_or_above,
                        floor
                    );
                    if high_or_above > 0 {
                        errors += high_or_above;
                    }
                    warnings += total.saturating_sub(high_or_above);
                }
                AuditOutcome::ToolMissing => {
                    if args.skip_missing_tools {
                        println!(
                            "  {} npm not installed — skipped",
                            "!".yellow()
                        );
                    } else {
                        eprintln!("{}", "  ✗ npm not installed. Or pass --skip-missing-tools.".red());
                        std::process::exit(2);
                    }
                }
                AuditOutcome::Error(msg) => {
                    eprintln!("  {} npm audit error: {msg}", "✗".red());
                    errors += 1;
                }
            }
        }
    }

    println!();
    if errors > 0 {
        println!(
            "{}",
            format!("Result: FAIL — {errors} findings at or above {floor:?} ({warnings} below)")
                .red()
                .bold()
        );
        std::process::exit(1);
    } else if warnings > 0 {
        println!(
            "{}",
            format!("Result: PASS — {warnings} findings below severity floor ({floor:?}); review when convenient")
                .yellow()
        );
    } else {
        println!("{}", "Result: PASS — no findings".green().bold());
    }
    Ok(())
}

#[derive(Debug)]
enum AuditOutcome {
    Clean,
    Findings { high_or_above: usize, total: usize },
    ToolMissing,
    Error(String),
}

/// Walk up from CARGO_MANIFEST_DIR to find the workspace root (the
/// directory whose `Cargo.toml` declares `[workspace]`).
fn find_workspace_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut dir = manifest_dir.as_path();
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.is_file() {
            let contents = std::fs::read_to_string(&candidate).unwrap_or_default();
            if contents.contains("[workspace]") {
                return Ok(dir.to_path_buf());
            }
        }
        dir = dir
            .parent()
            .ok_or_else(|| anyhow::anyhow!("could not find workspace root above xtask"))?;
    }
}

fn run_cargo_audit(root: &Path, _skip_missing: bool) -> AuditOutcome {
    let output = Command::new("cargo")
        .arg("audit")
        .arg("--json")
        .current_dir(root)
        .output();

    let out = match output {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return AuditOutcome::ToolMissing,
        Err(e) => return AuditOutcome::Error(format!("spawn cargo: {e}")),
    };

    // cargo-audit returns non-zero when it finds advisories but the
    // JSON output still parses. A genuine "tool missing" surfaces as
    // either "no such subcommand: audit" or "no such command: audit"
    // on stderr (cargo phrasing varies by version).
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains("no such subcommand")
        || stderr.contains("no such command")
        || stderr.contains("not installed")
    {
        return AuditOutcome::ToolMissing;
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.is_empty() {
        // cargo audit ran but produced no output — likely a transient.
        return AuditOutcome::Error(format!(
            "cargo audit exited {} with no output. Stderr: {}",
            out.status,
            stderr.trim()
        ));
    }

    // Parse JSON. cargo-audit's schema: { "vulnerabilities": { "count": N, "list": [...] }, ... }
    let parsed: serde_json::Value = match serde_json::from_str(&stdout) {
        Ok(v) => v,
        Err(e) => return AuditOutcome::Error(format!("parse cargo-audit JSON: {e}")),
    };
    let count = parsed
        .get("vulnerabilities")
        .and_then(|v| v.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    if count == 0 {
        return AuditOutcome::Clean;
    }

    // cargo-audit's advisory entries don't carry a normalized severity
    // field across all advisory sources; treat all as "high" for
    // reporting purposes (Rust ecosystem advisories tend to be
    // genuinely meaningful when they fire).
    AuditOutcome::Findings {
        high_or_above: count,
        total: count,
    }
}

fn run_npm_audit(lockdir: &Path, floor: Severity, _skip_missing: bool) -> AuditOutcome {
    // On Windows, `npm` is installed as `npm.cmd` (a batch wrapper); the
    // bare name doesn't resolve via Command::new's exe lookup. Try the
    // .cmd suffix first when on Windows, fall through to bare name.
    let npm_exe = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let output = Command::new(npm_exe)
        .arg("audit")
        .arg("--json")
        .current_dir(lockdir)
        .output();

    let out = match output {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return AuditOutcome::ToolMissing,
        Err(e) => return AuditOutcome::Error(format!("spawn npm: {e}")),
    };

    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return AuditOutcome::Error(format!("npm audit empty stdout. stderr: {}", stderr.trim()));
    }

    let parsed: serde_json::Value = match serde_json::from_str(&stdout) {
        Ok(v) => v,
        Err(e) => return AuditOutcome::Error(format!("parse npm-audit JSON: {e}")),
    };

    // npm audit JSON schema: { "metadata": { "vulnerabilities": { "info": N, "low": N, "moderate": N, "high": N, "critical": N, "total": N } } }
    let m = parsed
        .get("metadata")
        .and_then(|v| v.get("vulnerabilities"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let total = m.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    if total == 0 {
        return AuditOutcome::Clean;
    }
    let critical = m.get("critical").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let high = m.get("high").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let moderate = m.get("moderate").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let low = m.get("low").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let info = m.get("info").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let at_or_above = match floor {
        Severity::Critical => critical,
        Severity::High => high + critical,
        Severity::Moderate => moderate + high + critical,
        Severity::Low => low + moderate + high + critical,
        Severity::Info => info + low + moderate + high + critical,
    };

    AuditOutcome::Findings {
        high_or_above: at_or_above,
        total,
    }
}

fn find_npm_lockfiles(root: &Path) -> Vec<PathBuf> {
    use walkdir::WalkDir;
    let mut out = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Skip node_modules + target + .git for performance.
            let name = e.file_name().to_string_lossy();
            !(name == "node_modules" || name == "target" || name == ".git")
        })
        .flatten()
    {
        if entry.file_name() == "package-lock.json" {
            if let Some(parent) = entry.path().parent() {
                out.push(parent.to_path_buf());
            }
        }
    }
    out
}
