use anyhow::Result;
use motherbrain_core::registry::BrainRegistry;

pub async fn run(registry_path: &str) -> Result<()> {
    let json = tokio::fs::read_to_string(registry_path).await?;
    let registry = BrainRegistry::from_json(&json)?;

    // Weight sum check
    let weight_sum: f64 = registry.config.domain_weights.values().filter(|w| **w > 0.0).sum();
    let weight_ok = (weight_sum - 1.0).abs() <= 0.01;

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
    println!("  Weight sum: {:.3} {}", weight_sum, if weight_ok { "(valid)" } else { "(INVALID)" });
    println!("  Scoring model: {:?}", model);
    println!("  Hats: {}", registry.config.hats.len());
    println!("  Correlations: {}", registry.config.correlations.len());
    println!("  Incident patterns: {}", registry.config.incident_patterns.len());
    println!("  Personas: {}", registry.config.personas.len());
    println!("  Sensory servers: {}", registry.config.sensory_servers.len());

    if !missing_defs.is_empty() {
        println!("  WARN: Domains without definitions: {}", missing_defs.join(", "));
    }

    // Confidence thresholds
    let ct = &registry.config.confidence_thresholds;
    println!("  Confidence thresholds: fresh={}d, stale={}d, very_stale={}d", ct.cmdb_fresh_days, ct.cmdb_stale_days, ct.cmdb_very_stale_days);

    // Trajectory config
    let tc = &registry.config.trajectory;
    println!("  Trajectory: retention={}d, min_samples={}, velocity_window={}", tc.retention_days, tc.min_samples_for_trend, tc.velocity_window);

    match registry.validate() {
        Ok(()) => { println!("\nResult: VALID"); Ok(()) }
        Err(e) => { println!("\nResult: INVALID — {}", e); std::process::exit(1); }
    }
}
