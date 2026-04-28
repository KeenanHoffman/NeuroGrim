use anyhow::Result;
use neurogrim_core::registry::BrainRegistry;

pub async fn run(registry_path: &str) -> Result<()> {
    let json = tokio::fs::read_to_string(registry_path).await?;
    let registry = BrainRegistry::from_json(&json)?;

    // Weight sum check
    let weight_sum: f64 = registry
        .config
        .domain_weights
        .values()
        .filter(|w| **w > 0.0)
        .sum();
    // An all-advisory registry (every domain at weight 0.0) is valid
    // per spec principle #2 and is flagged separately for clarity.
    let advisory_only = weight_sum == 0.0;
    let weight_ok = advisory_only || (weight_sum - 1.0).abs() <= 0.01;

    // Domain definitions check
    let mut missing_defs = Vec::new();
    for dk in registry.config.domain_weights.keys() {
        if !registry.config.domain_definitions.contains_key(dk) {
            missing_defs.push(dk.clone());
        }
    }

    // Scoring config
    let model = &registry.config.scoring.model;

    println!("Registry Validation: {}", registry_path);
    println!("  Schema version: {}", registry.meta.schema_version);
    println!("  Domains: {}", registry.config.domain_weights.len());
    let weight_label = if advisory_only {
        "(all-advisory)"
    } else if weight_ok {
        "(valid)"
    } else {
        "(INVALID)"
    };
    println!("  Weight sum: {:.3} {}", weight_sum, weight_label);
    println!("  Scoring model: {:?}", model);
    println!("  Hats: {}", registry.config.hats.len());
    println!("  Correlations: {}", registry.config.correlations.len());
    println!(
        "  Incident patterns: {}",
        registry.config.incident_patterns.len()
    );
    println!("  Human personas: {}", registry.config.human_personas.len());
    println!(
        "  Sensory servers: {}",
        registry.config.sensory_servers.len()
    );

    // v3.3 F3: surface the autonomy block alongside the other counts so
    // operators can see at a glance that they have safety declarations
    // (or that they are missing).
    let autonomy = registry.config.autonomy.as_object();
    let action_types_count = autonomy
        .and_then(|a| a.get("action_types"))
        .and_then(|v| v.as_object())
        .map(|m| m.len())
        .unwrap_or(0);
    let safety_invariants_count = autonomy
        .and_then(|a| a.get("safety_invariants"))
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);
    let levels_count = autonomy
        .and_then(|a| a.get("levels"))
        .and_then(|v| v.as_object())
        .map(|m| m.len())
        .unwrap_or(0);
    println!(
        "  Autonomy: {} levels, {} action_types, {} safety_invariants",
        levels_count, action_types_count, safety_invariants_count
    );

    if !missing_defs.is_empty() {
        println!(
            "  WARN: Domains without definitions: {}",
            missing_defs.join(", ")
        );
    }

    // Confidence thresholds
    let ct = &registry.config.confidence_thresholds;
    println!(
        "  Confidence thresholds: fresh={}d, stale={}d, very_stale={}d",
        ct.cmdb_fresh_days, ct.cmdb_stale_days, ct.cmdb_very_stale_days
    );

    // Trajectory config
    let tc = &registry.config.trajectory;
    println!(
        "  Trajectory: retention={}d, min_samples={}, velocity_window={}",
        tc.retention_days, tc.min_samples_for_trend, tc.velocity_window
    );

    match registry.validate() {
        Ok(()) => {
            println!("\nResult: VALID");
            Ok(())
        }
        Err(e) => {
            println!("\nResult: INVALID — {}", e);
            std::process::exit(1);
        }
    }
}
