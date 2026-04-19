//! Human-readable display output with optional ANSI colors.

use colored::*;
use neurogrim_core::agent_output::AgentOutput;
use neurogrim_core::types::{ScoreLabel, TrajectoryClassification};

/// Display the single-line score output.
pub fn display_score(output: &AgentOutput, plain: bool) {
    let score = output.score;
    let label = ScoreLabel::from_score(score, 75, 50);
    let score_str = format!("{}/100", score);

    let colored_score = if plain {
        score_str.clone()
    } else {
        match label {
            ScoreLabel::Green => score_str.green().bold().to_string(),
            ScoreLabel::Yellow => score_str.yellow().bold().to_string(),
            ScoreLabel::Red => score_str.red().bold().to_string(),
        }
    };

    println!("NeuroGrim Score: {}", colored_score);

    // Domain breakdown
    let mut domains: Vec<_> = output.domains.iter().collect();
    domains.sort_by(|a, b| a.0.cmp(b.0));

    for (name, d) in &domains {
        let eff_label = ScoreLabel::from_score(d.effective_score, 75, 50);
        let eff_str = format!("{}", d.effective_score);
        let colored_eff = if plain {
            eff_str.clone()
        } else {
            match eff_label {
                ScoreLabel::Green => eff_str.green().to_string(),
                ScoreLabel::Yellow => eff_str.yellow().to_string(),
                ScoreLabel::Red => eff_str.red().to_string(),
            }
        };

        let conf_note = if d.confidence < 100 {
            format!(" (confidence: {}%)", d.confidence)
        } else {
            String::new()
        };

        println!(
            "  {} {} raw:{} eff:{}{}",
            domain_icon(d.effective_score),
            name,
            d.score,
            colored_eff,
            conf_note
        );
    }

    // Trajectory
    if let Some(ref traj) = output.trajectory {
        let class_str = classification_display(&traj.classification);
        println!(
            "  Trajectory: {} (velocity: {:+.1}, samples: {})",
            class_str, traj.velocity, traj.samples
        );
    }

    // Floor constraint
    // (checked via scorecard, not in agent output directly — show if score seems capped)
}

/// Display the full health dashboard.
pub fn display_health(output: &AgentOutput, plain: bool) {
    let divider = if plain {
        "---"
    } else {
        "───────────────────────────────────"
    };

    println!("{}", divider);
    display_score(output, plain);
    println!("{}", divider);

    // Correlations
    if !output.correlations_fired.is_empty() {
        println!("\nCorrelations:");
        for c in &output.correlations_fired {
            if plain {
                println!("  ! {}", c.description);
            } else {
                println!("  {} {}", "!".red(), c.description);
            }
        }
    }

    // Incidents
    if !output.incident_patterns.is_empty() {
        println!("\nIncident Patterns:");
        for inc in &output.incident_patterns {
            if plain {
                let sev = match inc.severity.as_str() {
                    "critical" => "CRITICAL",
                    "warning" => "WARNING",
                    _ => "INFO",
                };
                println!("  [{}] {} (x{})", sev, inc.name, inc.recurrence_count);
            } else {
                match inc.severity.as_str() {
                    "critical" => println!(
                        "  [{}] {} (x{})",
                        "CRITICAL".red().bold(),
                        inc.name,
                        inc.recurrence_count
                    ),
                    "warning" => println!(
                        "  [{}] {} (x{})",
                        "WARNING".yellow().bold(),
                        inc.name,
                        inc.recurrence_count
                    ),
                    _ => println!(
                        "  [{}] {} (x{})",
                        "INFO".dimmed(),
                        inc.name,
                        inc.recurrence_count
                    ),
                };
            }
            if !inc.hypothesis.is_empty() {
                println!("    {}", inc.hypothesis);
            }
        }
    }

    // Domain variables (non-empty)
    if !output.domain_variables.is_empty() {
        println!("\nDomain Variables:");
        let mut vars: Vec<_> = output.domain_variables.iter().collect();
        vars.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in vars.iter().take(10) {
            println!("  {} = {}", k, v);
        }
        if vars.len() > 10 {
            println!("  ... and {} more", vars.len() - 10);
        }
    }

    // Recommendations
    if !output.top_recommendations.is_empty() {
        println!("\nRecommendations:");
        for (i, rec) in output.top_recommendations.iter().enumerate() {
            println!(
                "  {}. [{}] {} — {}",
                i + 1,
                rec.status,
                rec.gate,
                rec.command
            );
        }
    }

    println!("{}", divider);
}

/// Display trajectory trend analysis.
pub fn display_trend(output: &AgentOutput, _plain: bool) {
    // `_plain` is accepted for API symmetry with display_score/display_health.
    // Trend output is currently plain-only (no color surface); the flag is a
    // placeholder so callers can pass it uniformly. When colorized classifications
    // land, rename to `plain` and branch on it.
    println!("Trajectory Analysis");
    println!();

    // Unified
    if let Some(ref traj) = output.trajectory {
        let class_str = classification_display(&traj.classification);
        println!(
            "  Unified: {} (velocity: {:+.1}, acceleration: {:+.1}, samples: {})",
            class_str, traj.velocity, traj.acceleration, traj.samples
        );
    } else {
        println!("  Unified: no trajectory data");
    }

    // Per-domain
    let mut domains: Vec<_> = output.domains.iter().collect();
    domains.sort_by(|a, b| a.0.cmp(b.0));

    for (name, d) in &domains {
        if let Some(ref traj) = d.trajectory {
            let class_str = classification_display(&traj.classification);
            println!(
                "  {}: {} (velocity: {:+.1}, acceleration: {:+.1}, samples: {})",
                name, class_str, traj.velocity, traj.acceleration, traj.samples
            );
        } else {
            println!("  {}: no data", name);
        }
    }
}

fn domain_icon(score: u8) -> &'static str {
    if score >= 75 {
        "+"
    } else if score >= 50 {
        "~"
    } else {
        "-"
    }
}

fn classification_display(c: &TrajectoryClassification) -> String {
    match c {
        TrajectoryClassification::Improving => "improving ^".to_string(),
        TrajectoryClassification::Degrading => "degrading v".to_string(),
        TrajectoryClassification::Stable => "stable =".to_string(),
        TrajectoryClassification::Volatile => "volatile ~".to_string(),
        TrajectoryClassification::NoData => "no-data".to_string(),
    }
}
