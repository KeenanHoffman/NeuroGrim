//! Persona-based output filtering.
//!
//! 5 personas control output verbosity:
//! - executive: score + trajectory + top risk (5 lines max)
//! - manager: domain breakdown + trends + blockers
//! - developer: full detail (default)
//! - specialist: single-domain deep dive
//! - product-manager: delivery risk + blockers

use motherbrain_core::agent_output::AgentOutput;

/// Display output filtered by persona.
pub fn display_persona(output: &AgentOutput, persona: &str, plain: bool) {
    match persona {
        "executive" => display_executive(output, plain),
        "manager" => display_manager(output, plain),
        "developer" => super::display::display_health(output, plain),
        "specialist" => display_specialist(output, plain),
        "product-manager" => display_pm(output, plain),
        _ => {
            eprintln!("Unknown persona: {}. Using developer.", persona);
            super::display::display_health(output, plain);
        }
    }
}

/// Executive: score + trajectory + top risk. Max 5 lines.
fn display_executive(output: &AgentOutput, _plain: bool) {
    println!("Score: {}/100", output.score);

    if let Some(ref traj) = output.trajectory {
        let trend = match traj.classification {
            motherbrain_core::types::TrajectoryClassification::Improving => "improving",
            motherbrain_core::types::TrajectoryClassification::Degrading => "DEGRADING",
            motherbrain_core::types::TrajectoryClassification::Stable => "stable",
            motherbrain_core::types::TrajectoryClassification::Volatile => "VOLATILE",
            motherbrain_core::types::TrajectoryClassification::NoData => "insufficient data",
        };
        println!("Trend: {}", trend);
    }

    // Top risk: lowest-scoring domain
    if let Some((name, d)) = output.domains.iter().min_by_key(|(_, d)| d.effective_score) {
        if d.effective_score < 75 {
            println!("Risk: {} at {}/100", name, d.effective_score);
        }
    }

    if !output.incident_patterns.is_empty() {
        let critical_count = output
            .incident_patterns
            .iter()
            .filter(|i| i.severity == "critical")
            .count();
        if critical_count > 0 {
            println!("Incidents: {} critical", critical_count);
        }
    }
}

/// Manager: domain breakdown + trends + blockers.
fn display_manager(output: &AgentOutput, _plain: bool) {
    println!("Score: {}/100", output.score);
    println!();

    let mut domains: Vec<_> = output.domains.iter().collect();
    domains.sort_by(|a, b| a.0.cmp(b.0));

    println!("Domains:");
    for (name, d) in &domains {
        let trend = d
            .trajectory
            .as_ref()
            .map(|t| {
                format!(
                    " [{}]",
                    match t.classification {
                        motherbrain_core::types::TrajectoryClassification::Improving => "^",
                        motherbrain_core::types::TrajectoryClassification::Degrading => "v",
                        motherbrain_core::types::TrajectoryClassification::Stable => "=",
                        motherbrain_core::types::TrajectoryClassification::Volatile => "~",
                        motherbrain_core::types::TrajectoryClassification::NoData => "?",
                    }
                )
            })
            .unwrap_or_default();
        println!("  {} {}/100{}", name, d.effective_score, trend);
    }

    if !output.incident_patterns.is_empty() {
        println!("\nIncidents:");
        for inc in &output.incident_patterns {
            println!(
                "  [{}] {} (x{})",
                inc.severity, inc.name, inc.recurrence_count
            );
        }
    }

    if !output.dirty_gates.is_empty() {
        println!("\nBlocked gates: {}", output.dirty_gates.join(", "));
    }
}

/// Specialist: single-domain deep dive. Shows all detail for one domain.
fn display_specialist(output: &AgentOutput, _plain: bool) {
    // Find the most interesting domain (lowest scoring)
    let focus_domain = output
        .domains
        .iter()
        .min_by_key(|(_, d)| d.effective_score)
        .map(|(n, _)| n.clone());

    let domain_name = focus_domain.unwrap_or_else(|| "unknown".to_string());

    if let Some(d) = output.domains.get(&domain_name) {
        println!("Domain: {}", domain_name);
        println!("  Raw score: {}", d.score);
        println!("  Confidence: {}%", d.confidence);
        println!("  Effective: {}", d.effective_score);
        println!("  Weight: {:.2}", d.weight);

        if let Some(ref traj) = d.trajectory {
            println!(
                "  Trajectory: velocity={:+.1}, accel={:+.1}, class={:?}",
                traj.velocity, traj.acceleration, traj.classification
            );
        }

        // Related domain variables
        let prefix = format!("{}:", domain_name);
        let related_vars: Vec<_> = output
            .domain_variables
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .collect();
        if !related_vars.is_empty() {
            println!("  Variables:");
            for (k, v) in &related_vars {
                println!("    {} = {}", k, v);
            }
        }

        // Related incidents
        for inc in &output.incident_patterns {
            let is_related = inc.signals.keys().any(|k| k.starts_with(&prefix));
            if is_related {
                println!("  Incident: [{}] {}", inc.severity, inc.name);
            }
        }
    }
}

/// Product Manager: delivery risk + blockers + timeline.
fn display_pm(output: &AgentOutput, _plain: bool) {
    let can_ship = output.score >= 75 && output.dirty_gates.is_empty();
    println!("Ship ready: {}", if can_ship { "YES" } else { "NO" });
    println!("Score: {}/100", output.score);

    if let Some(ref traj) = output.trajectory {
        let direction = match traj.classification {
            motherbrain_core::types::TrajectoryClassification::Improving => "improving",
            motherbrain_core::types::TrajectoryClassification::Degrading => {
                "DEGRADING - needs attention"
            }
            _ => "stable",
        };
        println!("Trend: {}", direction);
    }

    if !output.dirty_gates.is_empty() {
        println!("\nBlockers:");
        for gate in &output.dirty_gates {
            println!("  - {} (dirty)", gate);
        }
    }

    if !output.incident_patterns.is_empty() {
        println!("\nRisks:");
        for inc in &output.incident_patterns {
            if inc.severity == "critical" || inc.severity == "warning" {
                println!("  - [{}] {}", inc.severity, inc.name);
            }
        }
    }
}
