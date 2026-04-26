//! `neurogrim sca-calibrate` — supply-chain calibration runner.
//!
//! Discovers the fixture library at `<project_root>/tests/supply-
//! chain-fixtures/`, runs calibration against each layer, emits a
//! JSON report to stdout (or `--output` path), and prints a
//! human-friendly summary to stderr.
//!
//! `--check-promotion-ready` returns exit code 0 if the report
//! indicates promotion-readiness (≥30 fixtures per layer + L3
//! human-agreement data sufficient), 1 otherwise. v1 always
//! returns 1 by design — we ship the framework + ~10 fixtures
//! per layer + 0 L3 triage history.

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use neurogrim_sensory::supply_chain_calibration::{
    run_calibration, CalibrationReport, OverallStatus,
};
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
pub struct ScaCalibrateArgs {
    /// Project root path. The fixture library is read from
    /// `<project_root>/tests/supply-chain-fixtures/`.
    #[arg(long, default_value = ".")]
    pub project_root: String,

    /// Output path for the JSON report. If omitted, prints to
    /// stdout. The conventional path is
    /// `.claude/supply-chain-calibration-report.json`.
    #[arg(long, short)]
    pub output: Option<String>,

    /// Custom fixture library directory. Overrides the default
    /// `<project_root>/tests/supply-chain-fixtures/`.
    #[arg(long)]
    pub fixtures_dir: Option<String>,

    /// If set, exit non-zero unless the report indicates
    /// promotion-readiness. v1 always returns 1 by design (we
    /// lack ≥30 fixtures + ≥30 days of L3 triage data). Wires
    /// into CI as a forcing function: "do not promote any
    /// supply-chain domain past advisory weight yet."
    #[arg(long)]
    pub check_promotion_ready: bool,
}

pub async fn run(args: ScaCalibrateArgs) -> Result<()> {
    let project_root = PathBuf::from(&args.project_root);
    let fixtures_dir = args
        .fixtures_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| project_root.join("tests").join("supply-chain-fixtures"));

    if !fixtures_dir.is_dir() {
        // 2026-04-26 PRE-RELEASE C7 fix: report the canonicalized
        // absolute path so operators running from a different CWD
        // can see exactly where the CLI looked.
        let canonical = fixtures_dir
            .canonicalize()
            .ok()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(canonicalize failed; path may not exist)".to_string());
        anyhow::bail!(
            "fixture library not found at {} (resolved to {}). \
             See docs/supply-chain-calibration.md § Running calibration for setup. \
             Pass --fixtures-dir to point at a non-default location.",
            fixtures_dir.display(),
            canonical
        );
    }

    eprintln!(
        "{} {}",
        "✦ Calibrating against fixture library:".cyan(),
        fixtures_dir.display()
    );

    // Calibration internally runs the SCA sensor (which is async)
    // by spinning up a current-thread runtime. We're already inside
    // the CLI's async runtime, so wrap in `spawn_blocking` to give
    // the calibration its own OS thread (and thus its own runtime).
    let fixtures_dir_clone = fixtures_dir.clone();
    let report = tokio::task::spawn_blocking(move || run_calibration(&fixtures_dir_clone))
        .await
        .context("calibration task join failed")?
        .context("supply-chain calibration run failed")?;

    print_summary(&report);

    let json = serde_json::to_string_pretty(&report).context("serialize report")?;
    match &args.output {
        Some(path) => {
            // Ensure parent directory exists.
            if let Some(parent) = Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("mkdir {}", parent.display()))?;
                }
            }
            std::fs::write(path, &json).with_context(|| format!("write report to {}", path))?;
            eprintln!("{} {}", "✓ report written:".green(), path);
        }
        None => {
            println!("{}", json);
        }
    }

    if args.check_promotion_ready {
        if report.promotion_ready.ready {
            eprintln!("{}", "✓ promotion-ready: all gates met.".green().bold());
            std::process::exit(0);
        } else {
            eprintln!(
                "\n{} promotion-readiness gate FAILED. Gaps:",
                "✗".red().bold()
            );
            for gap in &report.promotion_ready.gaps {
                eprintln!("  - {}", gap);
            }
            eprintln!(
                "\n{}",
                "v1 expectation: this gate fails by design until calibration evidence accumulates. \
                 See docs/supply-chain-calibration.md for the path to ready."
                    .yellow()
            );
            std::process::exit(1);
        }
    }

    Ok(())
}

fn print_summary(report: &CalibrationReport) {
    eprintln!();
    eprintln!("{}", "=== Supply-Chain Calibration Summary ===".bold());
    eprintln!("Run id:     {}", report.run_id);
    eprintln!("Started:    {}", report.started_at.format("%Y-%m-%d %H:%M:%SZ"));
    eprintln!("Finished:   {}", report.finished_at.format("%Y-%m-%d %H:%M:%SZ"));
    eprintln!();

    print_layer_summary(
        "Layer 1 (Mechanical SCA)",
        &report.layer_1,
        report.layer_1.target_fp_rate,
        report.layer_1.target_fn_rate,
    );
    print_layer_summary(
        "Layer 2 (Vigilance)",
        &report.layer_2,
        report.layer_2.target_fp_rate,
        report.layer_2.target_fn_rate,
    );
    print_l3_summary(&report.layer_3);

    eprintln!();
    let overall_color = match report.overall_status {
        OverallStatus::Pass => "green",
        OverallStatus::PassWithSampleSizeWarning => "yellow",
        OverallStatus::TargetMiss => "red",
        OverallStatus::RedMiss => "red",
        OverallStatus::NoFixtures => "yellow",
    };
    let overall_str = format!("Overall status: {}", report.overall_status.as_str());
    eprintln!(
        "{}",
        match overall_color {
            "green" => overall_str.green().bold(),
            "yellow" => overall_str.yellow().bold(),
            _ => overall_str.red().bold(),
        }
    );
    eprintln!();
    eprintln!("Validity note: {}", report.statistical_validity_note);
    eprintln!();
    if report.promotion_ready.ready {
        eprintln!("{}", "Promotion-readiness: ✓ ready".green().bold());
    } else {
        eprintln!(
            "{} (gaps: {})",
            "Promotion-readiness: ✗ not ready".red().bold(),
            report.promotion_ready.gaps.len()
        );
        for gap in &report.promotion_ready.gaps {
            eprintln!("  - {}", gap);
        }
    }
}

fn print_layer_summary(
    name: &str,
    layer: &neurogrim_sensory::supply_chain_calibration::LayerReport,
    _target_fp: f64,
    _target_fn: f64,
) {
    let status_label = format!("[{}]", layer.status.as_str());
    let colored_status = match layer.status {
        neurogrim_sensory::supply_chain_calibration::LayerStatus::Pass => status_label.green(),
        neurogrim_sensory::supply_chain_calibration::LayerStatus::PassWithSampleSizeWarning => {
            status_label.yellow()
        }
        neurogrim_sensory::supply_chain_calibration::LayerStatus::TargetMiss => status_label.red(),
        neurogrim_sensory::supply_chain_calibration::LayerStatus::RedMiss => {
            status_label.red().bold()
        }
        neurogrim_sensory::supply_chain_calibration::LayerStatus::FrameworkOnly => {
            status_label.yellow()
        }
        neurogrim_sensory::supply_chain_calibration::LayerStatus::NoFixtures => {
            status_label.yellow()
        }
    };
    eprintln!(
        "{}: {} fixtures evaluated, {} errored. {} TP / {} TN / {} FP / {} FN. {}",
        name.bold(),
        layer.fixtures_evaluated,
        layer.fixtures_errored,
        layer.tp_count,
        layer.tn_count,
        layer.fp_count,
        layer.fn_count,
        colored_status
    );
    if let (Some(fp), Some(fnr)) = (layer.fp_rate, layer.fn_rate) {
        eprintln!(
            "  fp_rate={:.1}% (target ≤{:.1}%), fn_rate={:.1}% (target ≤{:.1}%)",
            fp * 100.0,
            layer.target_fp_rate * 100.0,
            fnr * 100.0,
            layer.target_fn_rate * 100.0,
        );
    }
}

fn print_l3_summary(layer: &neurogrim_sensory::supply_chain_calibration::LayerReport) {
    let label = "Layer 3 (Agent-assisted Review)";
    eprintln!(
        "{}: {} fixtures discovered. {}",
        label.bold(),
        layer.sample_size,
        format!("[{}]", layer.status.as_str()).yellow()
    );
    if let Some(reason) = layer.human_agreement_data.as_deref() {
        eprintln!("  human_agreement_data: {}", reason);
    }
    if let Some(c) = layer.fixtures_with_reference_decision {
        eprintln!("  fixtures with reference_decision: {}/{}", c, layer.sample_size);
    }
}
