use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Custom deserializer that skips entries with string values (like `_doc` keys)
/// in the domain_definitions map. These are inline documentation, not real domains.
fn deserialize_domain_definitions<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, DomainDefinition>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: HashMap<String, serde_json::Value> = HashMap::deserialize(deserializer)?;
    let mut result = HashMap::new();
    for (key, value) in raw {
        if key.starts_with('_') {
            continue; // Skip documentation keys like _doc
        }
        match serde_json::from_value::<DomainDefinition>(value) {
            Ok(def) => {
                result.insert(key, def);
            }
            Err(_) => {
                // Skip entries that don't parse as DomainDefinition (e.g., string values)
                continue;
            }
        }
    }
    Ok(result)
}

#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("domain weights do not sum to 1.0 (±0.01): got {0}")]
    WeightSumInvalid(f64),
    #[error("no domains defined")]
    NoDomains,
    #[error("domain '{0}' has weight but no definition")]
    MissingDefinition(String),
    #[error("failed to parse registry: {0}")]
    ParseError(#[from] serde_json::Error),
}

/// Top-level Brain registry structure (brain-registry.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainRegistry {
    pub meta: RegistryMeta,
    #[serde(default)]
    pub tools: serde_json::Value,
    #[serde(default)]
    pub data_sources: serde_json::Value,
    pub config: BrainConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryMeta {
    pub schema_version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub updated_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainConfig {
    pub domain_weights: HashMap<String, f64>,
    #[serde(default)]
    pub advisory_domains: Vec<String>,
    #[serde(default)]
    pub principle_map: HashMap<String, String>,
    #[serde(default, deserialize_with = "deserialize_domain_definitions")]
    pub domain_definitions: HashMap<String, DomainDefinition>,
    #[serde(default)]
    pub scoring: ScoringConfig,
    #[serde(default)]
    pub gate_tiers: HashMap<String, GateTierConfig>,
    #[serde(default)]
    pub confidence_thresholds: ConfidenceThresholdConfig,
    #[serde(default)]
    pub staleness_thresholds: StalenessConfig,
    #[serde(default)]
    pub severity_thresholds: SeverityConfig,
    #[serde(default)]
    pub autonomy: serde_json::Value,
    #[serde(default)]
    pub trajectory: TrajectoryConfig,
    #[serde(default)]
    pub attention_budget: AttentionBudgetConfig,
    #[serde(default)]
    pub human_personas: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub hats: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub correlations: Vec<serde_json::Value>,
    #[serde(default)]
    pub incident_patterns: Vec<serde_json::Value>,
    #[serde(default)]
    pub sensory_servers: HashMap<String, SensoryServerConfig>,
    /// Brains-2.0 E-B2-2 §17.3 — global gate for the calibration-
    /// ledger writer. Defaults to `false` (writer is silent). Even
    /// when `true`, per-domain auto-fire requires a non-Manual
    /// `calibration_trigger` on the domain's definition. Both opt-ins
    /// must be set for an entry to be auto-created. Manual triage via
    /// the CLI is unaffected by this flag.
    #[serde(default)]
    pub enable_calibration_writes: bool,
    // Allow additional fields without breaking parsing
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainDefinition {
    #[serde(default)]
    pub scoring_source: Option<ScoringSourceConfig>,
    #[serde(default)]
    pub floor: Option<FloorConfig>,
    #[serde(default)]
    pub exported_variables: HashMap<String, ExportedVariable>,
    /// Brains-2.0 E-B2-2 §17.3 — per-domain calibration trigger
    /// configuration. None means "no auto-trigger; operator-only
    /// manual entries via CLI" (the safe default for new domains).
    /// Some(Manual) is the same posture explicitly stated. Other
    /// variants opt the domain into auto-fire on the build_scorecard
    /// hot path. See `crate::calibration_ledger::CalibrationTrigger`
    /// for the discriminated union.
    #[serde(default)]
    pub calibration_trigger: Option<crate::calibration_ledger::CalibrationTrigger>,
    // Allow additional fields
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Per-domain scoring-source configuration (the "what should we read"
/// part of a domain). The "how to read it" part is the
/// [`crate::scoring_source::ScoringSource`] trait (V5-MOD-1, 2026-05-02);
/// each `source_type` string here corresponds to a registered
/// `ScoringSource` factory.
///
/// **Renamed in v5.0.0** (V5-MOD-1 Phase 0): previously named
/// `ScoringSource` (a struct). The trait introduced in V5-MOD-1 Phase 1
/// took the bare name; this config struct gained the `Config` suffix
/// to disambiguate. Downstream crates that imported
/// `neurogrim_core::registry::ScoringSource` must update to
/// `ScoringSourceConfig` (semver-major break, intentional at the v5
/// boundary).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringSourceConfig {
    #[serde(rename = "type")]
    pub source_type: String,
    #[serde(default)]
    pub path: Option<String>,
    /// For `source_type = "a2a"`: base URL of the peer Brain's A2A endpoint
    /// (e.g. `http://127.0.0.1:8421/a2a/v1/`). Ignored for other source types.
    /// Spec §9 fractal composition — parent pulls child's AgentOutput via
    /// snapshot.requested at score time.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// For `source_type = "a2a"`: expected agent-output interface version
    /// the peer emits. Used for pre-flight version negotiation (spec §6).
    /// Defaults to "1" when unset. Ignored for other source types.
    #[serde(default)]
    pub interface_version: Option<String>,
    #[serde(default)]
    pub score_field: Option<String>,
    #[serde(default)]
    pub updated_at_field: Option<String>,
    #[serde(default)]
    pub no_file_score: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorConfig {
    pub min_score: u8,
    pub unified_cap: u8,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedVariable {
    pub field: String,
    #[serde(rename = "type")]
    pub var_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    #[serde(default = "default_model")]
    pub model: crate::types::ScoringModel,
    #[serde(default = "default_floor_threshold")]
    pub floor_confidence_threshold: u8,
    #[serde(default = "default_floor_ceiling")]
    pub floor_score_ceiling: u8,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        ScoringConfig {
            model: crate::types::ScoringModel::Multiplier,
            floor_confidence_threshold: 30,
            floor_score_ceiling: 30,
        }
    }
}

fn default_model() -> crate::types::ScoringModel {
    crate::types::ScoringModel::Multiplier
}
fn default_floor_threshold() -> u8 {
    30
}
fn default_floor_ceiling() -> u8 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateTierConfig {
    pub scoring_weight: f64,
    pub priority_weight: f64,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceThresholdConfig {
    #[serde(default = "default_fresh")]
    pub cmdb_fresh_days: f64,
    #[serde(default = "default_stale")]
    pub cmdb_stale_days: f64,
    #[serde(default = "default_very_stale")]
    pub cmdb_very_stale_days: f64,
}

impl Default for ConfidenceThresholdConfig {
    fn default() -> Self {
        ConfidenceThresholdConfig {
            cmdb_fresh_days: 1.0,
            cmdb_stale_days: 3.0,
            cmdb_very_stale_days: 7.0,
        }
    }
}

fn default_fresh() -> f64 {
    1.0
}
fn default_stale() -> f64 {
    3.0
}
fn default_very_stale() -> f64 {
    7.0
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StalenessConfig {
    #[serde(default = "default_gate_stale_hours")]
    pub gate_stale_hours: f64,
    #[serde(default = "default_yellow")]
    pub health_score_yellow: u8,
    #[serde(default = "default_red")]
    pub health_score_red: u8,
}

fn default_gate_stale_hours() -> f64 {
    4.0
}
fn default_yellow() -> u8 {
    75
}
fn default_red() -> u8 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeverityConfig {
    #[serde(default = "default_warning_count")]
    pub warning_count: u32,
    #[serde(default = "default_critical_count")]
    pub critical_count: u32,
    #[serde(default = "default_recurrence_window")]
    pub recurrence_window_days: u32,
}

fn default_warning_count() -> u32 {
    3
}
fn default_critical_count() -> u32 {
    5
}
fn default_recurrence_window() -> u32 {
    7
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryConfig {
    #[serde(default = "default_retention")]
    pub retention_days: u32,
    #[serde(default = "default_max_entries")]
    pub max_entries: u32,
    #[serde(default = "default_min_samples")]
    pub min_samples_for_trend: usize,
    #[serde(default = "default_velocity_window")]
    pub velocity_window: usize,
    #[serde(default)]
    pub classification_thresholds: ClassificationThresholds,
}

impl Default for TrajectoryConfig {
    fn default() -> Self {
        TrajectoryConfig {
            retention_days: 30,
            max_entries: 500,
            min_samples_for_trend: 5,
            velocity_window: 5,
            classification_thresholds: ClassificationThresholds::default(),
        }
    }
}

fn default_retention() -> u32 {
    30
}
fn default_max_entries() -> u32 {
    500
}
fn default_min_samples() -> usize {
    5
}
fn default_velocity_window() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationThresholds {
    #[serde(default = "default_improving")]
    pub improving: f64,
    #[serde(default = "default_degrading")]
    pub degrading: f64,
    #[serde(default = "default_volatile_stddev")]
    pub volatile_stddev: f64,
}

impl Default for ClassificationThresholds {
    fn default() -> Self {
        ClassificationThresholds {
            improving: 2.0,
            degrading: -2.0,
            volatile_stddev: 10.0,
        }
    }
}

fn default_improving() -> f64 {
    2.0
}
fn default_degrading() -> f64 {
    -2.0
}
fn default_volatile_stddev() -> f64 {
    10.0
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttentionBudgetConfig {
    #[serde(default = "default_max_recs")]
    pub max_recommendations: u32,
    #[serde(default = "default_per_domain_max")]
    pub per_domain_max: u32,
    #[serde(default)]
    pub persona_overrides: HashMap<String, u32>,
}

fn default_max_recs() -> u32 {
    5
}
fn default_per_domain_max() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensoryServerConfig {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_transport() -> String {
    "stdio".to_string()
}

impl BrainRegistry {
    /// Parse a brain-registry.json string.
    pub fn from_json(json: &str) -> Result<Self, RegistryError> {
        let registry: BrainRegistry = serde_json::from_str(json)?;
        Ok(registry)
    }

    /// Validate the registry configuration.
    pub fn validate(&self) -> Result<(), RegistryError> {
        if self.config.domain_weights.is_empty() {
            return Err(RegistryError::NoDomains);
        }

        // Weight sum of non-advisory domains. Advisory domains (weight 0.0)
        // are intentional and do NOT contribute to the sum.
        let weight_sum: f64 = self
            .config
            .domain_weights
            .iter()
            .filter(|(_, w)| **w > 0.0)
            .map(|(_, w)| w)
            .sum();

        // All-advisory registry (every domain is weight 0.0) is valid.
        // Spec principle #2 — "unknown is not good" — blesses this state
        // for registries that exist to observe without yet committing to
        // weighted scoring (ecosystem-level Brains, spec repos, starter
        // templates). The `NoDomains` check above still rejects the
        // degenerate "zero domains defined" case.
        if weight_sum == 0.0 {
            return Ok(());
        }

        if (weight_sum - 1.0).abs() > 0.01 {
            return Err(RegistryError::WeightSumInvalid(weight_sum));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical three-domain registry fixture matching the original starter-kit shape.
    ///
    /// The starter-kit itself was archived to `D:\Brains\archive\Moth-er-Br-AI-n\starter-kit\`
    /// on 2026-04-17, so these tests now use an inline fixture rather than reading from
    /// the repo filesystem. The fixture preserves the shape these tests were designed to
    /// validate: exactly 3 weighted core domains summing to 1.0, with `test-health`
    /// carrying a floor constraint (min_score=25, unified_cap=50).
    const THREE_DOMAIN_FIXTURE: &str = r#"{
        "meta": {
            "schema_version": "2",
            "description": "Three-domain baseline fixture (formerly starter-kit)",
            "updated_by": "hand-maintained"
        },
        "tools": {},
        "data_sources": {},
        "config": {
            "domain_weights": {
                "code-quality": 0.35,
                "test-health": 0.35,
                "deploy-readiness": 0.30
            },
            "advisory_domains": [],
            "principle_map": {
                "code-quality": "Code Quality",
                "test-health": "Test Coverage & Health",
                "deploy-readiness": "Deploy Readiness"
            },
            "domain_definitions": {
                "code-quality": {
                    "scoring_source": {
                        "type": "cmdb",
                        "path": ".claude/code-quality-cmdb.json"
                    }
                },
                "test-health": {
                    "scoring_source": {
                        "type": "cmdb",
                        "path": ".claude/test-health-cmdb.json"
                    },
                    "floor": {
                        "min_score": 25,
                        "unified_cap": 50,
                        "message": "Critical test health failure caps unified score"
                    }
                },
                "deploy-readiness": {
                    "scoring_source": {
                        "type": "cmdb",
                        "path": ".claude/deploy-readiness-cmdb.json"
                    }
                }
            }
        }
    }"#;

    #[test]
    fn parse_three_domain_registry() {
        let registry =
            BrainRegistry::from_json(THREE_DOMAIN_FIXTURE).expect("Failed to parse registry");

        assert_eq!(registry.meta.schema_version, "2");
        assert_eq!(registry.config.domain_weights.len(), 3);
        assert!(registry.config.domain_weights.contains_key("code-quality"));
        assert!(registry.config.domain_weights.contains_key("test-health"));
        assert!(registry
            .config
            .domain_weights
            .contains_key("deploy-readiness"));
    }

    #[test]
    fn validate_three_domain_registry() {
        let registry =
            BrainRegistry::from_json(THREE_DOMAIN_FIXTURE).expect("Failed to parse registry");
        registry.validate().expect("Registry validation failed");
    }

    #[test]
    fn weight_sum_validation() {
        let json = r#"{
            "meta": {"schema_version": "2", "description": "test", "updated_by": "test"},
            "config": {
                "domain_weights": {"a": 0.5, "b": 0.3}
            }
        }"#;
        let registry = BrainRegistry::from_json(json).unwrap();
        assert!(registry.validate().is_err());
    }

    #[test]
    fn validate_all_advisory_registry_is_ok() {
        // All domains advisory (weight 0.0). Valid per spec principle #2;
        // this is how ecosystem-level and spec-scoring Brains look.
        let json = r#"{
            "meta": {"schema_version": "2", "description": "all-advisory",
                     "updated_by": "test"},
            "config": {
                "domain_weights": {"a": 0.0, "b": 0.0, "c": 0.0}
            }
        }"#;
        let registry = BrainRegistry::from_json(json).unwrap();
        registry
            .validate()
            .expect("all-advisory registry should validate");
    }

    #[test]
    fn validate_zero_domains_still_errors() {
        // Degenerate case: zero domains defined. Must NOT be confused
        // with all-advisory. Separate error variant.
        let json = r#"{
            "meta": {"schema_version": "2", "description": "empty",
                     "updated_by": "test"},
            "config": {
                "domain_weights": {}
            }
        }"#;
        let registry = BrainRegistry::from_json(json).unwrap();
        match registry.validate() {
            Err(RegistryError::NoDomains) => {}
            other => panic!("expected NoDomains, got {other:?}"),
        }
    }

    #[test]
    fn validate_partial_weight_still_errors() {
        // Half-specified weights (sum 0.5 from weighted entries) is still
        // invalid — the advisory carve-out only applies when sum == 0.0
        // exactly.
        let json = r#"{
            "meta": {"schema_version": "2", "description": "partial",
                     "updated_by": "test"},
            "config": {
                "domain_weights": {"a": 0.3, "b": 0.2, "c": 0.0}
            }
        }"#;
        let registry = BrainRegistry::from_json(json).unwrap();
        match registry.validate() {
            Err(RegistryError::WeightSumInvalid(s)) => {
                assert!((s - 0.5).abs() < 0.01, "expected sum ≈ 0.5, got {s}");
            }
            other => panic!("expected WeightSumInvalid, got {other:?}"),
        }
    }

    #[test]
    fn floor_config_parsed() {
        let registry = BrainRegistry::from_json(THREE_DOMAIN_FIXTURE).unwrap();
        let test_health = registry
            .config
            .domain_definitions
            .get("test-health")
            .unwrap();
        let floor = test_health
            .floor
            .as_ref()
            .expect("test-health should have a floor");
        assert_eq!(floor.min_score, 25);
        assert_eq!(floor.unified_cap, 50);
    }
}
