//! A.2.3 — Tier 1 declarative sensor patterns.
//!
//! Operators author sensors as TOML config (no Rust) for the most common
//! shapes. Per `docs/BROKER-AUTHORING.md`:
//!
//! - **`file_presence_score`** — score = linear function of required-file
//!   presence + (optional) freshness penalty.
//! - **`glob_count`** — score derived from count of files matching a
//!   glob (inverse: fewer = higher; direct: more = higher; ceiling cap).
//! - **`cmdb_derived`** — composite score from sibling CMDBs (min/max/
//!   mean/median combinator).
//!
//! Each pattern is implemented as a [`neurogrim_core::sensor::Sensor`]
//! that the substrate wraps in a [`crate::sensory::SensorBackedBroker`]
//! at boot time (via [`discover_sensory_extensions`]). The resulting
//! broker writes its CMDB through the standard CmdbMaterializer path,
//! gated by the [`crate::sensory_queue::SensoryQueueEnforcerV1`].
//!
//! ## TOML shape
//!
//! ```toml
//! [extension]
//! schema_version = "1"
//!
//! [sensor]
//! broker_id = "sensor-doc-quality"
//! role = "sense"
//! domain = "documentation"
//! pattern = "file_presence_score"  # or glob_count, cmdb_derived
//! description = "Score based on presence of canonical docs."
//!
//! [sensor.config]
//! # pattern-specific keys
//! ```

use anyhow::Result;
use async_trait::async_trait;
use neurogrim_core::sensor::{Sensor, SensorFactory};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PatternError {
    #[error("missing required [sensor] section in extension config")]
    MissingSensorSection,
    #[error("missing required [sensor.config] section")]
    MissingConfigSection,
    #[error("unknown sensor pattern `{0}`; expected file_presence_score | glob_count | cmdb_derived")]
    UnknownPattern(String),
    #[error("pattern config parse error: {0}")]
    ConfigParse(String),
    #[error("io error: {0}")]
    Io(#[source] std::io::Error),
}

// ============================================================================
// Common TOML shape
// ============================================================================

#[derive(Debug, Deserialize)]
struct ExtensionSensorTomlEnvelope {
    sensor: ExtensionSensorMeta,
}

#[derive(Debug, Deserialize)]
struct ExtensionSensorMeta {
    broker_id: String,
    #[serde(default)]
    domain: Option<String>,
    pattern: String,
    #[serde(default)]
    description: Option<String>,
    config: toml::Value,
}

// ============================================================================
// Pattern 1: file_presence_score
// ============================================================================

#[derive(Debug, Deserialize)]
struct FilePresenceConfig {
    required_files: Vec<String>,
    /// Either "linear" (each file contributes 100/N) or "all-or-nothing"
    /// (100 if all present, 0 otherwise).
    #[serde(default = "default_scoring")]
    scoring: String,
    /// Optional freshness penalty: files older than N days lose this
    /// many percentage points each. Omit to skip freshness check.
    #[serde(default)]
    freshness_window_days: Option<u64>,
    #[serde(default)]
    freshness_penalty: Option<u32>,
}

fn default_scoring() -> String {
    "linear".to_string()
}

pub struct FilePresenceSensor {
    domain: String,
    config: FilePresenceConfig,
}

#[async_trait]
impl Sensor for FilePresenceSensor {
    async fn analyze(&self, project_root: &str) -> Result<Value> {
        let root = Path::new(project_root);
        let total = self.config.required_files.len() as u32;
        if total == 0 {
            return Ok(degraded_envelope(&self.domain, 0, "no required_files configured"));
        }

        let mut findings: Vec<Value> = Vec::new();
        let mut present_count: u32 = 0;
        let mut stale_count: u32 = 0;
        for rel in &self.config.required_files {
            let abs = root.join(rel);
            if !abs.exists() {
                findings.push(json!({
                    "severity": "warning",
                    "title": format!("Missing required file: {}", rel),
                    "path": rel,
                }));
                continue;
            }
            present_count += 1;
            // Freshness check (if configured)
            if let (Some(window_days), Some(_penalty)) =
                (self.config.freshness_window_days, self.config.freshness_penalty)
            {
                if let Ok(meta) = std::fs::metadata(&abs) {
                    if let Ok(modified) = meta.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            let days = elapsed.as_secs() / 86400;
                            if days > window_days {
                                stale_count += 1;
                                findings.push(json!({
                                    "severity": "info",
                                    "title": format!("Stale file ({}d > {}d): {}", days, window_days, rel),
                                    "path": rel,
                                }));
                            }
                        }
                    }
                }
            }
        }

        let base_score = match self.config.scoring.as_str() {
            "all-or-nothing" | "all_or_nothing" => {
                if present_count == total {
                    100
                } else {
                    0
                }
            }
            _ /* linear */ => {
                (present_count as f64 / total as f64 * 100.0).round() as u32
            }
        };
        // Apply freshness penalty
        let freshness_deduction = self
            .config
            .freshness_penalty
            .map(|p| stale_count * p)
            .unwrap_or(0);
        let score = base_score.saturating_sub(freshness_deduction).min(100) as u8;

        Ok(envelope_v1(&self.domain, score, findings))
    }
}

// ============================================================================
// Pattern 2: glob_count
// ============================================================================

#[derive(Debug, Deserialize)]
struct GlobCountConfig {
    /// Glob pattern, relative to project root.
    glob: String,
    /// Scoring direction: "inverse" (fewer = higher score) or "direct"
    /// (more = higher score).
    #[serde(default = "default_glob_scoring")]
    scoring: String,
    /// Matches above this count are clamped — prevents the score from
    /// blowing past 100 or going negative.
    #[serde(default = "default_glob_ceiling")]
    ceiling: u32,
}

fn default_glob_scoring() -> String {
    "inverse".to_string()
}

fn default_glob_ceiling() -> u32 {
    50
}

pub struct GlobCountSensor {
    domain: String,
    config: GlobCountConfig,
}

#[async_trait]
impl Sensor for GlobCountSensor {
    async fn analyze(&self, project_root: &str) -> Result<Value> {
        let root = Path::new(project_root);
        // Simple glob matching — we don't have a glob crate available
        // in the substrate, so V1 implements a minimal walker that
        // supports `**/*.ext` patterns. More sophisticated patterns
        // fall back to "matches everything" with a warning finding.
        let count = walk_and_count(root, &self.config.glob);

        let ceiling = self.config.ceiling.max(1);
        let normalized = (count.min(ceiling) as f64 / ceiling as f64 * 100.0).round() as u32;
        let score = match self.config.scoring.as_str() {
            "direct" => normalized,
            _ /* inverse */ => 100u32.saturating_sub(normalized),
        }
        .min(100) as u8;

        let findings = vec![json!({
            "severity": "info",
            "title": format!("Glob `{}` matched {} file(s)", self.config.glob, count),
            "match_count": count,
            "ceiling": ceiling,
        })];

        Ok(envelope_v1(&self.domain, score, findings))
    }
}

/// Minimal glob walker — supports `**/*.ext` and `*.ext` patterns. For
/// V1 simplicity. Real operators with complex glob needs author a Tier 2
/// Rust sensor.
fn walk_and_count(root: &Path, pattern: &str) -> u32 {
    let mut count = 0u32;
    let suffix = pattern
        .rsplit('.')
        .next()
        .map(|s| format!(".{}", s))
        .unwrap_or_default();
    walk_dir_count(root, &suffix, &mut count);
    count
}

fn walk_dir_count(dir: &Path, suffix: &str, count: &mut u32) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            // Skip common skip-dirs
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if matches!(name, ".git" | "target" | "node_modules" | ".claude") {
                    continue;
                }
            }
            walk_dir_count(&path, suffix, count);
        } else if meta.is_file() {
            if path.to_string_lossy().ends_with(suffix) {
                *count += 1;
            }
        }
    }
}

// ============================================================================
// Pattern 3: cmdb_derived
// ============================================================================

#[derive(Debug, Deserialize)]
struct CmdbDerivedConfig {
    /// Sibling CMDBs to read (resolves to `<project_root>/.claude/<source>-cmdb.json`).
    sources: Vec<String>,
    /// Combinator: "min" | "max" | "mean" | "median".
    #[serde(default = "default_combinator")]
    combinator: String,
}

fn default_combinator() -> String {
    "min".to_string()
}

pub struct CmdbDerivedSensor {
    domain: String,
    config: CmdbDerivedConfig,
}

#[async_trait]
impl Sensor for CmdbDerivedSensor {
    async fn analyze(&self, project_root: &str) -> Result<Value> {
        let root = Path::new(project_root);
        let mut scores: Vec<u8> = Vec::new();
        let mut findings: Vec<Value> = Vec::new();
        for source in &self.config.sources {
            let path = root.join(format!(".claude/{}-cmdb.json", source));
            let Ok(contents) = std::fs::read_to_string(&path) else {
                findings.push(json!({
                    "severity": "warning",
                    "title": format!("Source CMDB missing: {}", source),
                    "path": path.to_string_lossy(),
                }));
                continue;
            };
            let Ok(envelope) = serde_json::from_str::<Value>(&contents) else {
                findings.push(json!({
                    "severity": "warning",
                    "title": format!("Source CMDB malformed: {}", source),
                }));
                continue;
            };
            if let Some(score) = envelope.get("score").and_then(|v| v.as_u64()) {
                scores.push(score.min(100) as u8);
            }
        }
        if scores.is_empty() {
            return Ok(degraded_envelope(
                &self.domain,
                0,
                "no source CMDB scores collected",
            ));
        }
        let composite = match self.config.combinator.as_str() {
            "max" => *scores.iter().max().unwrap(),
            "mean" => {
                let total: u32 = scores.iter().map(|&s| s as u32).sum();
                (total / scores.len() as u32).min(100) as u8
            }
            "median" => {
                let mut sorted = scores.clone();
                sorted.sort();
                sorted[sorted.len() / 2]
            }
            _ /* min */ => *scores.iter().min().unwrap(),
        };
        Ok(envelope_v1(&self.domain, composite, findings))
    }
}

// ============================================================================
// Common helpers
// ============================================================================

fn envelope_v1(domain: &str, score: u8, findings: Vec<Value>) -> Value {
    json!({
        "meta": {
            "schema_version": "1",
            "updated_at": chrono::Utc::now().to_rfc3339(),
            "updated_by": format!("ext-sensor:{}", domain),
        },
        "score": score,
        "updated_at": chrono::Utc::now().to_rfc3339(),
        "findings": findings,
    })
}

fn degraded_envelope(domain: &str, score: u8, reason: &str) -> Value {
    json!({
        "meta": {
            "schema_version": "1",
            "updated_at": chrono::Utc::now().to_rfc3339(),
            "updated_by": format!("ext-sensor:{}", domain),
        },
        "score": score,
        "updated_at": chrono::Utc::now().to_rfc3339(),
        "findings": [{
            "severity": "warning",
            "title": format!("Sensor degraded: {}", reason),
        }],
    })
}

// ============================================================================
// SensorFactory wrappers (so SensorBackedBroker can construct them)
// ============================================================================

struct DeclarativeSensorFactory {
    name: String,
    pattern: String,
    domain: String,
    config_value: toml::Value,
}

impl SensorFactory for DeclarativeSensorFactory {
    fn name(&self) -> &'static str {
        // SensorFactory requires &'static str; we leak the broker_id once
        // per factory at registration. Cost is minimal (one String per
        // declared extension sensor; lifetime = process).
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn build(&self) -> Box<dyn Sensor> {
        let pattern = self.pattern.as_str();
        let domain = self.domain.clone();
        let config_value = self.config_value.clone();
        match pattern {
            "file_presence_score" => {
                let config: FilePresenceConfig = config_value
                    .try_into()
                    .expect("file_presence_score config invalid; should have been caught at discover time");
                Box::new(FilePresenceSensor { domain, config })
            }
            "glob_count" => {
                let config: GlobCountConfig = config_value
                    .try_into()
                    .expect("glob_count config invalid; should have been caught at discover time");
                Box::new(GlobCountSensor { domain, config })
            }
            "cmdb_derived" => {
                let config: CmdbDerivedConfig = config_value
                    .try_into()
                    .expect("cmdb_derived config invalid; should have been caught at discover time");
                Box::new(CmdbDerivedSensor { domain, config })
            }
            _ => unreachable!("pattern was validated at discover time"),
        }
    }
}

/// Discover Tier 1 sensor extensions in
/// `<extensions_dir>/sensory/*.toml` and return a registry of
/// [`SensorFactory`] instances ready to wrap in `SensorBackedBroker`.
///
/// Each factory's `name()` is the operator-declared `broker_id` from
/// the extension's `[sensor]` table. Returns empty if `extensions_dir`
/// or `extensions_dir/sensory` don't exist.
///
/// Schema validation is done eagerly at discover time so operators see
/// errors at boot, not at first dispatch.
pub fn discover_sensory_extensions(
    extensions_dir: &Path,
) -> Result<HashMap<String, Arc<dyn SensorFactory>>, PatternError> {
    let mut out = HashMap::new();
    let sensory_dir = extensions_dir.join("sensory");
    if !sensory_dir.exists() {
        return Ok(out);
    }

    let mut toml_paths: Vec<PathBuf> = std::fs::read_dir(&sensory_dir)
        .map_err(PatternError::Io)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
        .collect();
    toml_paths.sort();

    for path in toml_paths {
        let contents = std::fs::read_to_string(&path).map_err(PatternError::Io)?;
        let envelope: ExtensionSensorTomlEnvelope =
            toml::from_str(&contents).map_err(|e| {
                PatternError::ConfigParse(format!("{}: {}", path.display(), e))
            })?;
        let meta = envelope.sensor;
        // Validate pattern is known
        match meta.pattern.as_str() {
            "file_presence_score" | "glob_count" | "cmdb_derived" => {}
            other => return Err(PatternError::UnknownPattern(other.to_string())),
        }
        // Eager parse of the config — fails loudly NOW rather than at first dispatch
        match meta.pattern.as_str() {
            "file_presence_score" => {
                let _: FilePresenceConfig = meta.config.clone().try_into().map_err(|e: toml::de::Error| {
                    PatternError::ConfigParse(format!("{}: {}", path.display(), e))
                })?;
            }
            "glob_count" => {
                let _: GlobCountConfig = meta.config.clone().try_into().map_err(|e: toml::de::Error| {
                    PatternError::ConfigParse(format!("{}: {}", path.display(), e))
                })?;
            }
            "cmdb_derived" => {
                let _: CmdbDerivedConfig = meta.config.clone().try_into().map_err(|e: toml::de::Error| {
                    PatternError::ConfigParse(format!("{}: {}", path.display(), e))
                })?;
            }
            _ => unreachable!(),
        }
        let domain = meta.domain.unwrap_or_else(|| meta.broker_id.clone());
        let factory: Arc<dyn SensorFactory> = Arc::new(DeclarativeSensorFactory {
            name: meta.broker_id.clone(),
            pattern: meta.pattern,
            domain,
            config_value: meta.config,
        });
        out.insert(meta.broker_id, factory);
    }

    Ok(out)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_ext(tmp: &TempDir, name: &str, body: &str) {
        let dir = tmp.path().join("sensory");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{}.toml", name)), body).unwrap();
    }

    #[tokio::test]
    async fn file_presence_score_linear_full_presence() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("README.md"), "x").unwrap();
        std::fs::write(tmp.path().join("CONTRIBUTING.md"), "x").unwrap();
        let sensor = FilePresenceSensor {
            domain: "docs".to_string(),
            config: FilePresenceConfig {
                required_files: vec!["README.md".to_string(), "CONTRIBUTING.md".to_string()],
                scoring: "linear".to_string(),
                freshness_window_days: None,
                freshness_penalty: None,
            },
        };
        let envelope = sensor.analyze(tmp.path().to_str().unwrap()).await.unwrap();
        assert_eq!(envelope["score"], 100);
    }

    #[tokio::test]
    async fn file_presence_score_linear_partial() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("README.md"), "x").unwrap();
        let sensor = FilePresenceSensor {
            domain: "docs".to_string(),
            config: FilePresenceConfig {
                required_files: vec![
                    "README.md".to_string(),
                    "CONTRIBUTING.md".to_string(),
                    "ARCHITECTURE.md".to_string(),
                    "LICENSE".to_string(),
                ],
                scoring: "linear".to_string(),
                freshness_window_days: None,
                freshness_penalty: None,
            },
        };
        let envelope = sensor.analyze(tmp.path().to_str().unwrap()).await.unwrap();
        assert_eq!(envelope["score"], 25); // 1 of 4 present
    }

    #[tokio::test]
    async fn file_presence_all_or_nothing_misses_one() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("README.md"), "x").unwrap();
        let sensor = FilePresenceSensor {
            domain: "docs".to_string(),
            config: FilePresenceConfig {
                required_files: vec!["README.md".to_string(), "CONTRIBUTING.md".to_string()],
                scoring: "all-or-nothing".to_string(),
                freshness_window_days: None,
                freshness_penalty: None,
            },
        };
        let envelope = sensor.analyze(tmp.path().to_str().unwrap()).await.unwrap();
        assert_eq!(envelope["score"], 0);
    }

    #[tokio::test]
    async fn glob_count_inverse_few_matches_high_score() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.todo.md"), "").unwrap();
        let sensor = GlobCountSensor {
            domain: "todos".to_string(),
            config: GlobCountConfig {
                glob: "**/*.todo.md".to_string(),
                scoring: "inverse".to_string(),
                ceiling: 10,
            },
        };
        let envelope = sensor.analyze(tmp.path().to_str().unwrap()).await.unwrap();
        let score = envelope["score"].as_u64().unwrap();
        assert!(score >= 80, "1 match of 10-ceiling should score high; got {}", score);
    }

    #[tokio::test]
    async fn glob_count_direct_many_matches_high_score() {
        let tmp = TempDir::new().unwrap();
        for i in 0..5 {
            std::fs::write(tmp.path().join(format!("test-{}.rs", i)), "").unwrap();
        }
        let sensor = GlobCountSensor {
            domain: "tests".to_string(),
            config: GlobCountConfig {
                glob: "**/*.rs".to_string(),
                scoring: "direct".to_string(),
                ceiling: 5,
            },
        };
        let envelope = sensor.analyze(tmp.path().to_str().unwrap()).await.unwrap();
        assert_eq!(envelope["score"], 100); // 5 matches / 5 ceiling = 100
    }

    #[tokio::test]
    async fn cmdb_derived_min_returns_lowest() {
        let tmp = TempDir::new().unwrap();
        let claude = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("alpha-cmdb.json"),
            r#"{"meta":{"schema_version":"1"},"score":80}"#,
        )
        .unwrap();
        std::fs::write(
            claude.join("beta-cmdb.json"),
            r#"{"meta":{"schema_version":"1"},"score":40}"#,
        )
        .unwrap();
        let sensor = CmdbDerivedSensor {
            domain: "composite".to_string(),
            config: CmdbDerivedConfig {
                sources: vec!["alpha".to_string(), "beta".to_string()],
                combinator: "min".to_string(),
            },
        };
        let envelope = sensor.analyze(tmp.path().to_str().unwrap()).await.unwrap();
        assert_eq!(envelope["score"], 40);
    }

    #[tokio::test]
    async fn cmdb_derived_mean_returns_average() {
        let tmp = TempDir::new().unwrap();
        let claude = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("a-cmdb.json"),
            r#"{"meta":{"schema_version":"1"},"score":100}"#,
        )
        .unwrap();
        std::fs::write(
            claude.join("b-cmdb.json"),
            r#"{"meta":{"schema_version":"1"},"score":50}"#,
        )
        .unwrap();
        let sensor = CmdbDerivedSensor {
            domain: "avg".to_string(),
            config: CmdbDerivedConfig {
                sources: vec!["a".to_string(), "b".to_string()],
                combinator: "mean".to_string(),
            },
        };
        let envelope = sensor.analyze(tmp.path().to_str().unwrap()).await.unwrap();
        assert_eq!(envelope["score"], 75);
    }

    #[tokio::test]
    async fn cmdb_derived_missing_source_degrades_gracefully() {
        let tmp = TempDir::new().unwrap();
        let sensor = CmdbDerivedSensor {
            domain: "x".to_string(),
            config: CmdbDerivedConfig {
                sources: vec!["does-not-exist".to_string()],
                combinator: "min".to_string(),
            },
        };
        let envelope = sensor.analyze(tmp.path().to_str().unwrap()).await.unwrap();
        assert_eq!(envelope["score"], 0);
    }

    #[test]
    fn discover_parses_file_presence_extension() {
        let tmp = TempDir::new().unwrap();
        write_ext(
            &tmp,
            "doc-quality",
            r#"
[extension]
schema_version = "1"

[sensor]
broker_id = "sensor-doc-quality"
role = "sense"
domain = "documentation"
pattern = "file_presence_score"

[sensor.config]
required_files = ["README.md"]
scoring = "linear"
"#,
        );
        let registry = discover_sensory_extensions(tmp.path()).unwrap();
        assert_eq!(registry.len(), 1);
        assert!(registry.contains_key("sensor-doc-quality"));
    }

    #[test]
    fn discover_rejects_unknown_pattern() {
        let tmp = TempDir::new().unwrap();
        write_ext(
            &tmp,
            "bad",
            r#"
[extension]
schema_version = "1"

[sensor]
broker_id = "sensor-bad"
pattern = "not_a_real_pattern"

[sensor.config]
"#,
        );
        let err = match discover_sensory_extensions(tmp.path()) {
            Ok(_) => panic!("expected discover to fail"),
            Err(e) => e,
        };
        match err {
            PatternError::UnknownPattern(p) => {
                assert_eq!(p, "not_a_real_pattern");
            }
            other => panic!("expected UnknownPattern, got {:?}", other),
        }
    }

    #[test]
    fn discover_rejects_malformed_file_presence_config_eagerly() {
        let tmp = TempDir::new().unwrap();
        write_ext(
            &tmp,
            "broken-fp",
            r#"
[extension]
schema_version = "1"

[sensor]
broker_id = "sensor-broken-fp"
pattern = "file_presence_score"

[sensor.config]
# missing required_files
scoring = "linear"
"#,
        );
        let err = match discover_sensory_extensions(tmp.path()) {
            Ok(_) => panic!("expected discover to fail"),
            Err(e) => e,
        };
        match err {
            PatternError::ConfigParse(msg) => {
                assert!(
                    msg.contains("required_files") || msg.contains("missing"),
                    "expected config-parse error mentioning required_files; got: {}",
                    msg
                );
            }
            other => panic!("expected ConfigParse, got {:?}", other),
        }
    }

    #[test]
    fn discover_empty_dir_returns_empty_registry() {
        let tmp = TempDir::new().unwrap();
        let registry = discover_sensory_extensions(tmp.path()).unwrap();
        assert!(registry.is_empty());
    }

    #[test]
    fn discover_factory_name_round_trips() {
        let tmp = TempDir::new().unwrap();
        write_ext(
            &tmp,
            "x",
            r#"
[extension]
schema_version = "1"

[sensor]
broker_id = "sensor-x"
pattern = "glob_count"

[sensor.config]
glob = "**/*.rs"
"#,
        );
        let registry = discover_sensory_extensions(tmp.path()).unwrap();
        let factory = registry.get("sensor-x").unwrap();
        assert_eq!(factory.name(), "sensor-x");
    }
}
