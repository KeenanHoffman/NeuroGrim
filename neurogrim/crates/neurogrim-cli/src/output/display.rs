//! Human-readable display output with optional ANSI colors.

use colored::*;
use neurogrim_core::agent_output::AgentOutput;
use neurogrim_core::types::{ScoreLabel, TrajectoryClassification};

/// Format the top-line score string with optional unified-confidence
/// annotation. Extracted for testability (the `plain=true` path is
/// deterministic; the colored path embeds ANSI escape sequences).
///
/// E-B2-1 C9 — adds the `(confidence: N%)` suffix when
/// `unified_confidence < 100`. Suppresses when 100 to avoid the
/// redundant "everything is fresh" annotation. Color bands match
/// the score's own `ScoreLabel(75, 50)` thresholds — green ≥75,
/// yellow 50–74, red <50 — so confidence reads at the same glance
/// as score itself.
pub(crate) fn format_score_top_line(score: u8, unified_confidence: u8, plain: bool) -> String {
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

    // E-B2-1 C9 unified_confidence annotation. Aggregate freshness
    // signal: receivers SHOULD use this for peer-trust decisions
    // (a peer at score=85 / confidence=20 is a low-quality signal).
    let conf_note = if unified_confidence < 100 {
        let conf_str = format!(" (confidence: {}%)", unified_confidence);
        if plain {
            conf_str
        } else {
            let conf_label = ScoreLabel::from_score(unified_confidence, 75, 50);
            match conf_label {
                ScoreLabel::Green => conf_str.green().to_string(),
                ScoreLabel::Yellow => conf_str.yellow().to_string(),
                ScoreLabel::Red => conf_str.red().to_string(),
            }
        }
    } else {
        String::new()
    };

    format!("NeuroGrim Score: {}{}", colored_score, conf_note)
}

/// Display the single-line score output.
pub fn display_score(output: &AgentOutput, plain: bool) {
    println!(
        "{}",
        format_score_top_line(output.score, output.unified_confidence, plain)
    );
    if !plain {
        println!("  {}", "✦ a book of spells for AI agents".dimmed().italic());
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    // E-B2-1 C9 tests for format_score_top_line. We test only the
    // `plain=true` path because the colored path embeds ANSI escape
    // sequences whose presence depends on tty detection by the
    // `colored` crate — brittle to assert in unit tests.

    #[test]
    fn format_top_line_omits_confidence_when_full() {
        // unified_confidence == 100 → confidence annotation suppressed
        // to avoid noise. This is the steady-state for fresh data.
        let line = format_score_top_line(85, 100, true);
        assert_eq!(line, "NeuroGrim Score: 85/100");
    }

    #[test]
    fn format_top_line_includes_confidence_below_full() {
        // unified_confidence < 100 → annotation surfaces.
        let line = format_score_top_line(85, 75, true);
        assert_eq!(line, "NeuroGrim Score: 85/100 (confidence: 75%)");
    }

    #[test]
    fn format_top_line_shows_zero_confidence_explicitly() {
        // unified_confidence == 0 means "peer at v2.6 (no signal)" or
        // "all-advisory Brain". Operator MUST see this — never suppress
        // 0 the way we suppress 100.
        let line = format_score_top_line(75, 0, true);
        assert_eq!(line, "NeuroGrim Score: 75/100 (confidence: 0%)");
    }

    #[test]
    fn format_top_line_low_score_with_high_confidence() {
        // Honest red flag: low score, but the Brain is confident in it.
        // Operator should not dismiss this as "stale data".
        let line = format_score_top_line(20, 95, true);
        assert_eq!(line, "NeuroGrim Score: 20/100 (confidence: 95%)");
    }
}
