//! Fixture-library loader for supply-chain calibration.
//!
//! Reads `fixture.toml` files from `tests/supply-chain-fixtures/`
//! per the format documented at the library README. Each fixture
//! exercises one of the three supply-chain sensors against a
//! deterministic input + expected-output pair.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Parsed `fixture.toml`. Field shapes mirror the README.
#[derive(Debug, Clone, Deserialize)]
pub struct FixtureMetadata {
    /// Stable kebab-case id; matches the directory name.
    pub id: String,
    /// One of "known-bad" | "known-good" | "edge-case".
    pub label: FixtureLabel,
    /// "1" | "2" | "3" — which layer this fixture exercises.
    pub layer: String,
    /// Human-readable description.
    pub description: String,
    /// Optional grouping tag for reports.
    #[serde(default)]
    pub attack_pattern: Option<String>,
    /// Optional list of relevant URLs / advisory ids.
    #[serde(default)]
    pub references: Vec<String>,
    /// Optional author handle.
    #[serde(default)]
    pub author: Option<String>,
    /// Optional authoring date (ISO-8601).
    #[serde(default)]
    pub authored_at: Option<String>,
    /// Layer-specific expected outputs.
    #[serde(default)]
    pub expected: ExpectedOutputs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FixtureLabel {
    KnownBad,
    KnownGood,
    EdgeCase,
}

impl FixtureLabel {
    pub fn as_str(&self) -> &'static str {
        match self {
            FixtureLabel::KnownBad => "known-bad",
            FixtureLabel::KnownGood => "known-good",
            FixtureLabel::EdgeCase => "edge-case",
        }
    }
}

/// Per-fixture expected outputs. Fields are layer-specific; the
/// loader tolerates fields irrelevant to the fixture's `layer`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExpectedOutputs {
    // ── Layer 1 ────────────────────────────────────────────────
    /// Advisory IDs the sensor MUST detect as unaccepted.
    #[serde(default)]
    pub advisory_ids: Vec<String>,
    /// Advisory IDs the sensor MUST detect as accepted (filtered
    /// by the accepted-advisories file).
    #[serde(default)]
    pub accepted_advisory_ids: Vec<String>,
    /// If set, the sensor's `sensor_status` extra MUST equal this.
    #[serde(default)]
    pub sensor_status: Option<String>,
    /// Minimum score the sensor SHOULD report.
    #[serde(default)]
    pub min_score: Option<u32>,
    /// Maximum score the sensor SHOULD report.
    #[serde(default)]
    pub max_score: Option<u32>,

    // ── Layer 2 ────────────────────────────────────────────────
    /// List of expected vigilance findings. Each must match a
    /// finding produced by the sensor (kind + package).
    #[serde(default)]
    pub findings: Vec<ExpectedFinding>,
    /// List of finding kinds that MUST NOT fire on this fixture.
    /// Used for false-positive control.
    #[serde(default)]
    pub forbidden_findings: Vec<ExpectedFinding>,

    // ── Layer 3 ────────────────────────────────────────────────
    /// Reference decision: "accept" | "reject" | "pin-to-last-good"
    /// | "no-action".
    #[serde(default)]
    pub reference_decision: Option<String>,
    /// Reference rationale (prose).
    #[serde(default)]
    pub reference_rationale: Option<String>,
    /// Fixture-author's self-assessed confidence in the reference
    /// decision (0.0-1.0).
    #[serde(default)]
    pub fixture_author_confidence: Option<f64>,
    /// Alternate decisions the fixture-author considers defensible.
    #[serde(default)]
    pub defensible_alternatives: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExpectedFinding {
    pub kind: String,
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub ecosystem: Option<String>,
}

/// One discovered fixture: its directory + parsed metadata.
#[derive(Debug, Clone)]
pub struct LoadedFixture {
    pub dir: PathBuf,
    pub metadata: FixtureMetadata,
}

impl LoadedFixture {
    /// Path to the fixture's `metadata.json` (Layer 2) or other
    /// layer-specific artifacts.
    pub fn artifact_path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn id(&self) -> &str {
        &self.metadata.id
    }

    pub fn layer(&self) -> &str {
        &self.metadata.layer
    }
}

/// Load one fixture from a directory containing `fixture.toml`.
pub fn load_one(dir: &Path) -> Result<LoadedFixture> {
    let toml_path = dir.join("fixture.toml");
    let raw = fs::read_to_string(&toml_path)
        .with_context(|| format!("read {}", toml_path.display()))?;
    let metadata: FixtureMetadata =
        toml::from_str(&raw).with_context(|| format!("parse {}", toml_path.display()))?;
    // Sanity: id should match directory name.
    if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
        if name != metadata.id {
            tracing::warn!(
                "fixture id {:?} does not match directory name {:?}",
                metadata.id,
                name
            );
        }
    }
    Ok(LoadedFixture {
        dir: dir.to_path_buf(),
        metadata,
    })
}

/// Discover all fixtures under a layer-specific subdirectory of
/// the fixture library. Skips entries that don't have a
/// `fixture.toml`.
pub fn discover_layer(library_root: &Path, layer: &str) -> Vec<LoadedFixture> {
    let layer_dir = library_root.join(format!("layer-{}", layer));
    let mut out = Vec::new();
    let entries = match fs::read_dir(&layer_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!(
                "fixture: layer-{} dir read failed ({}): {:#}",
                layer,
                layer_dir.display(),
                e
            );
            return out;
        }
    };
    for entry_result in entries {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("fixture.toml").is_file() {
            continue;
        }
        match load_one(&path) {
            Ok(f) => out.push(f),
            Err(e) => {
                tracing::warn!(
                    "fixture: skip {} ({:#})",
                    path.display(),
                    e
                );
            }
        }
    }
    // Stable order: by id ascending.
    out.sort_by(|a, b| a.metadata.id.cmp(&b.metadata.id));
    out
}

/// Discover the default fixture library under `<repo_root>/tests/supply-chain-fixtures/`.
/// Convenience helper for the in-repo integration test.
pub fn default_library_root_from_manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(Path::new("."))
        .parent()
        .unwrap_or(Path::new("."))
        .join("tests")
        .join("supply-chain-fixtures")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_fixture_toml() {
        let raw = r#"
id = "test-fixture-001"
label = "known-good"
layer = "1"
description = "A minimal fixture for testing."
"#;
        let m: FixtureMetadata = toml::from_str(raw).unwrap();
        assert_eq!(m.id, "test-fixture-001");
        assert_eq!(m.label, FixtureLabel::KnownGood);
        assert_eq!(m.layer, "1");
        assert!(m.references.is_empty());
        assert!(m.expected.advisory_ids.is_empty());
    }

    #[test]
    fn parse_layer_1_fixture_with_advisories() {
        let raw = r#"
id = "rust-with-rustsec"
label = "known-bad"
layer = "1"
description = "Rust crate with a known RUSTSEC advisory."
attack_pattern = "advisory-match"
references = ["https://rustsec.org/advisories/RUSTSEC-2024-0436.html"]

[expected]
advisory_ids = ["RUSTSEC-2024-0436"]
"#;
        let m: FixtureMetadata = toml::from_str(raw).unwrap();
        assert_eq!(m.label, FixtureLabel::KnownBad);
        assert_eq!(m.expected.advisory_ids, vec!["RUSTSEC-2024-0436"]);
        assert_eq!(m.attack_pattern.as_deref(), Some("advisory-match"));
    }

    #[test]
    fn parse_layer_2_fixture_with_findings() {
        let raw = r#"
id = "litellm-synthetic"
label = "known-bad"
layer = "2"
description = "Synthetic publish-cadence pattern."
attack_pattern = "publish-cadence-acceleration"

[expected]
findings = [
  { kind = "publish-cadence-acceleration", package = "litellm-synthetic", ecosystem = "PyPI" },
]
forbidden_findings = [
  { kind = "exfil-indicator" },
]
min_score = 60
max_score = 90
"#;
        let m: FixtureMetadata = toml::from_str(raw).unwrap();
        assert_eq!(m.expected.findings.len(), 1);
        assert_eq!(m.expected.findings[0].kind, "publish-cadence-acceleration");
        assert_eq!(m.expected.forbidden_findings.len(), 1);
        assert_eq!(m.expected.min_score, Some(60));
        assert_eq!(m.expected.max_score, Some(90));
    }

    #[test]
    fn parse_layer_3_fixture_with_reference_decision() {
        let raw = r#"
id = "post-dormancy-pin"
label = "known-bad"
layer = "3"
description = "Operator should pin-to-last-good after post-dormancy."

[expected]
reference_decision = "pin-to-last-good"
reference_rationale = "Conservative posture pending upstream context."
fixture_author_confidence = 0.75
defensible_alternatives = ["no-action"]
"#;
        let m: FixtureMetadata = toml::from_str(raw).unwrap();
        assert_eq!(
            m.expected.reference_decision.as_deref(),
            Some("pin-to-last-good")
        );
        assert!(m.expected.reference_rationale.is_some());
        assert_eq!(m.expected.fixture_author_confidence, Some(0.75));
        assert_eq!(m.expected.defensible_alternatives, vec!["no-action"]);
    }

    #[test]
    fn discover_returns_empty_for_nonexistent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let fixtures = discover_layer(dir.path(), "1");
        assert!(fixtures.is_empty());
    }

    #[test]
    fn discover_loads_only_dirs_with_fixture_toml() {
        let dir = tempfile::tempdir().unwrap();
        let layer1 = dir.path().join("layer-1");
        std::fs::create_dir_all(&layer1).unwrap();
        // Real fixture.
        let f1 = layer1.join("fixture-001");
        std::fs::create_dir(&f1).unwrap();
        std::fs::write(
            f1.join("fixture.toml"),
            r#"id = "fixture-001"
label = "known-good"
layer = "1"
description = "."
"#,
        )
        .unwrap();
        // Bare dir without fixture.toml.
        let f2 = layer1.join("not-a-fixture");
        std::fs::create_dir(&f2).unwrap();
        // Markdown file that isn't a directory.
        std::fs::write(layer1.join("README.md"), "# layer-1").unwrap();

        let fixtures = discover_layer(dir.path(), "1");
        assert_eq!(fixtures.len(), 1);
        assert_eq!(fixtures[0].metadata.id, "fixture-001");
    }

    #[test]
    fn discover_returns_stable_order() {
        let dir = tempfile::tempdir().unwrap();
        let layer1 = dir.path().join("layer-1");
        std::fs::create_dir_all(&layer1).unwrap();
        for id in &["zeta", "alpha", "mu"] {
            let f = layer1.join(id);
            std::fs::create_dir(&f).unwrap();
            std::fs::write(
                f.join("fixture.toml"),
                format!(
                    r#"id = "{}"
label = "known-good"
layer = "1"
description = "."
"#,
                    id
                ),
            )
            .unwrap();
        }
        let fixtures = discover_layer(dir.path(), "1");
        let ids: Vec<&str> = fixtures.iter().map(|f| f.metadata.id.as_str()).collect();
        assert_eq!(ids, vec!["alpha", "mu", "zeta"]);
    }
}
