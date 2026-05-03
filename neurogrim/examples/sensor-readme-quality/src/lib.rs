//! # `sensor-readme-quality` — V5-MOD-2 third-party example
//!
//! This crate demonstrates the modularity claim of V5-MOD-2: a
//! third-party crate can ship a custom
//! [`neurogrim_core::sensor::Sensor`] impl that plugs into
//! NeuroGrim's `cast` dispatch **without forking
//! `neurogrim-core`, `neurogrim-sensory`, or `neurogrim-cli`**.
//! It depends only on the public contract surface in
//! `neurogrim-core`, registers itself at startup via the
//! consuming binary's [`neurogrim_core::sensor::SensorRegistry`],
//! and passes the Phase 5 conformance suite shipped with
//! `neurogrim-core`.
//!
//! Companion to V5-MOD-1's `scoring-source-prom` (HTTP-fetch
//! pattern). This example is the file-system pattern: zero env
//! vars, zero network, runs against any project that has — or
//! lacks — a README.
//!
//! ## What it does
//!
//! [`ReadmeQualitySensor`] reads `README.md` (or its common
//! casings — `Readme.md`, `readme.md`, `README`) at the project
//! root and scores documentation quality on a 0-100 scale via 6
//! heuristic features:
//!
//! | Feature                        | Points |
//! |--------------------------------|-------:|
//! | README file present            |     30 |
//! | First non-empty line is `# H1` |     15 |
//! | Has at least one `## Section`  |     15 |
//! | Body length ≥ 500 characters   |     15 |
//! | Has at least one code block    |     15 |
//! | Mentions install/usage/start   |     10 |
//! | **Total**                      |  **100** |
//!
//! Each feature produces a finding with `name` /
//! `status: "found" \| "missing"` / `points` / `detail`. The
//! score is the sum of `points` for "found" features.
//!
//! ## Failure modes (all preserved as `Ok(degraded envelope)`)
//!
//! The sensor follows the silent-degrade convention used by
//! 18 of the 21 built-in sensors:
//!
//! | Trigger                                  | Outcome                                          |
//! |------------------------------------------|--------------------------------------------------|
//! | No README file at any candidate path     | `Ok` envelope, score 0, finding `readme:missing` |
//! | README present but unreadable (IO error) | `Ok` envelope, score 0, finding `readme:read_error` |
//! | README empty (zero bytes)                | `Ok` envelope, score 30 (file-present points only) + `readme:empty` finding |
//!
//! Operator-visible behavior at the JSON level: identical to a
//! degraded built-in sensor.
//!
//! ## How a consuming binary registers the sensor
//!
//! ```ignore
//! use neurogrim_core::sensor::SensorRegistry;
//! use sensor_readme_quality::ReadmeQualitySensorFactory;
//!
//! fn build_registry() -> SensorRegistry {
//!     let mut registry = SensorRegistry::new();
//!     registry.register_all(neurogrim_sensory::built_in_factories());
//!     // Third-party README-quality factory.
//!     registry.register(Box::new(ReadmeQualitySensorFactory));
//!     registry
//! }
//! ```
//!
//! Once registered, a `brain-registry.json` domain entry referencing
//! `source_type: "cmdb"` (and a CMDB written via
//! `neurogrim cast readme-quality`) routes through this sensor.
//!
//! ## Conformance
//!
//! `tests/conformance.rs` runs the cross-crate suite from
//! [`neurogrim_core::sensor_conformance`] against
//! [`ReadmeQualitySensorFactory`]. Third-party plugin authors
//! should copy that test into their own crate as the canonical
//! contract check. If it passes, the impl honors the negative-path
//! discipline (no panics, never deadlocks, idempotent on identical
//! input, fast-fails on skeletal config) that every built-in
//! sensor satisfies.
//!
//! ## Why not the two-method dance
//!
//! `Sensor::analyze` is a single async method via `#[async_trait]`.
//! Unlike V5-MOD-1's `ScoringSource` (which exposes both `load` and
//! `load_inherent` to bypass future-boxing on the perf-critical
//! scoring path), sensors are slow IO at the seconds-per-call scale —
//! the ~50ns boxing overhead is rounding error. See
//! `neurogrim_core::sensor`'s rustdoc for the V5-MOD-2 Fork B
//! decision rationale.

use async_trait::async_trait;
use chrono::Utc;
use neurogrim_core::sensor::{Sensor, SensorFactory};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Stable wire-name for the `readme-quality` sensor. Must match the
/// `name()` returned by [`ReadmeQualitySensorFactory`] — the
/// V5-MOD-2 Phase 5 conformance suite enforces this consistency.
pub const SENSOR_NAME: &str = "readme-quality";

/// Common README filename casings, in lookup order.
const README_CANDIDATES: &[&str] =
    &["README.md", "Readme.md", "readme.md", "README"];

/// Third-party [`Sensor`] that reads the project's README and
/// scores documentation quality on a 0-100 scale via 6 heuristic
/// features (see crate-level rustdoc for the rubric).
///
/// Stateless — `Box::new(ReadmeQualitySensor)` per call. A
/// production third-party crate that needs caching (e.g., parsed
/// metadata across files) should put state on the factory instead.
pub struct ReadmeQualitySensor;

#[async_trait]
impl Sensor for ReadmeQualitySensor {
    async fn analyze(
        &self,
        project_root: &str,
    ) -> anyhow::Result<Value> {
        let root = Path::new(project_root);
        let now = Utc::now().to_rfc3339();
        let mut findings: Vec<Value> = Vec::new();
        let mut score: u32 = 0;

        // ── Step 1: locate the README ───────────────────────────────
        let readme_path = match find_readme(root) {
            Some(p) => p,
            None => {
                findings.push(json!({
                    "name": "readme:missing",
                    "status": "missing",
                    "points": 0,
                    "detail": format!(
                        "no README at any of {:?} under project root",
                        README_CANDIDATES,
                    ),
                }));
                return Ok(envelope(0, &now, findings));
            }
        };

        // ── Step 2: read it ─────────────────────────────────────────
        let content = match std::fs::read_to_string(&readme_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "readme-quality: read {} failed: {e}",
                    readme_path.display()
                );
                findings.push(json!({
                    "name": "readme:read_error",
                    "status": "missing",
                    "points": 0,
                    "detail": format!(
                        "{} present but unreadable: {e}",
                        readme_path.display()
                    ),
                }));
                return Ok(envelope(0, &now, findings));
            }
        };

        // README file present (+30).
        score += 30;
        findings.push(json!({
            "name": "readme_present",
            "status": "found",
            "points": 30,
            "detail": format!("README found at {}", readme_path.display()),
        }));

        // ── Step 3: empty-file shortcut ─────────────────────────────
        if content.trim().is_empty() {
            findings.push(json!({
                "name": "readme:empty",
                "status": "missing",
                "points": 0,
                "detail": "README file is empty (zero non-whitespace content)",
            }));
            return Ok(envelope(score as u8, &now, findings));
        }

        // ── Step 4: H1 heading ──────────────────────────────────────
        let first_line = content.lines().find(|l| !l.trim().is_empty());
        let has_h1 = first_line
            .map(|l| l.trim_start().starts_with("# "))
            .unwrap_or(false);
        if has_h1 {
            score += 15;
            findings.push(json!({
                "name": "h1_heading",
                "status": "found",
                "points": 15,
                "detail": "first non-empty line is `# Title`",
            }));
        } else {
            findings.push(json!({
                "name": "h1_heading",
                "status": "missing",
                "points": 0,
                "detail": format!(
                    "first non-empty line is not `# Title`: {}",
                    first_line.unwrap_or("(none)").chars().take(80).collect::<String>(),
                ),
            }));
        }

        // ── Step 5: at least one ## Section ─────────────────────────
        let section_count = content
            .lines()
            .filter(|l| l.trim_start().starts_with("## "))
            .count();
        if section_count > 0 {
            score += 15;
            findings.push(json!({
                "name": "section_heading",
                "status": "found",
                "points": 15,
                "detail": format!("{section_count} `## Section` heading(s)"),
            }));
        } else {
            findings.push(json!({
                "name": "section_heading",
                "status": "missing",
                "points": 0,
                "detail": "no `## Section` headings found",
            }));
        }

        // ── Step 6: substantive length ──────────────────────────────
        let len = content.trim().len();
        if len >= 500 {
            score += 15;
            findings.push(json!({
                "name": "substantive_length",
                "status": "found",
                "points": 15,
                "detail": format!("README is {len} chars (≥ 500)"),
            }));
        } else {
            findings.push(json!({
                "name": "substantive_length",
                "status": "missing",
                "points": 0,
                "detail": format!("README is {len} chars (< 500)"),
            }));
        }

        // ── Step 7: code block ──────────────────────────────────────
        let has_code = content.contains("```");
        if has_code {
            score += 15;
            findings.push(json!({
                "name": "code_block",
                "status": "found",
                "points": 15,
                "detail": "at least one ``` fenced code block",
            }));
        } else {
            findings.push(json!({
                "name": "code_block",
                "status": "missing",
                "points": 0,
                "detail": "no fenced code blocks",
            }));
        }

        // ── Step 8: install / usage / getting-started keyword ───────
        let lower = content.to_lowercase();
        let has_practical = lower.contains("install")
            || lower.contains("usage")
            || lower.contains("getting started");
        if has_practical {
            score += 10;
            findings.push(json!({
                "name": "practical_section",
                "status": "found",
                "points": 10,
                "detail": "mentions install / usage / getting started",
            }));
        } else {
            findings.push(json!({
                "name": "practical_section",
                "status": "missing",
                "points": 0,
                "detail": "no install / usage / getting-started reference",
            }));
        }

        Ok(envelope(score as u8, &now, findings))
    }
}

/// Factory for [`ReadmeQualitySensor`]. Stateless —
/// `build()` returns a fresh `Box::new(ReadmeQualitySensor)` per
/// call. A production third-party crate that needs to cache parser
/// state, HTTP clients, etc., would put the cache here.
pub struct ReadmeQualitySensorFactory;

impl SensorFactory for ReadmeQualitySensorFactory {
    fn name(&self) -> &'static str {
        SENSOR_NAME
    }

    fn build(&self) -> Box<dyn Sensor> {
        Box::new(ReadmeQualitySensor)
    }
}

// ────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────

fn find_readme(root: &Path) -> Option<PathBuf> {
    README_CANDIDATES
        .iter()
        .map(|name| root.join(name))
        .find(|p| p.is_file())
}

fn envelope(score: u8, now_rfc3339: &str, findings: Vec<Value>) -> Value {
    json!({
        "meta": {
            "schema_version": "1",
            "updated_at": now_rfc3339,
            "updated_by": SENSOR_NAME,
        },
        "score": score,
        "updated_at": now_rfc3339,
        "findings": findings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn missing_readme_yields_score_0_with_finding() {
        let dir = TempDir::new().unwrap();
        let env = ReadmeQualitySensor
            .analyze(dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(env["score"], 0);
        let findings = env["findings"].as_array().unwrap();
        assert!(
            findings.iter().any(|f| f["name"] == "readme:missing"),
            "expected `readme:missing` finding, got {findings:?}"
        );
    }

    #[tokio::test]
    async fn empty_readme_yields_score_30_only_present() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("README.md"), "").unwrap();
        let env = ReadmeQualitySensor
            .analyze(dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(env["score"], 30);
        let findings = env["findings"].as_array().unwrap();
        assert!(
            findings.iter().any(|f| f["name"] == "readme:empty"),
            "expected `readme:empty` finding"
        );
    }

    #[tokio::test]
    async fn perfect_readme_yields_score_100() {
        let dir = TempDir::new().unwrap();
        // Construct a 100-pt README:
        // - Has H1 (+15)
        // - Has section (+15)
        // - >= 500 chars (+15)
        // - Has code block (+15)
        // - Mentions install (+10)
        // - File present (+30) = 100
        let mut content = String::from("# My Project\n\n");
        content.push_str("## Installation\n\n");
        content.push_str("```bash\ncargo install my-project\n```\n\n");
        content.push_str(&"This is a long description of the project. ".repeat(15));
        std::fs::write(dir.path().join("README.md"), &content).unwrap();
        let env = ReadmeQualitySensor
            .analyze(dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(
            env["score"], 100,
            "perfect README must score 100; got envelope = {env:?}"
        );
    }

    #[tokio::test]
    async fn stub_readme_just_h1_yields_score_45() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("README.md"), "# My Project\n").unwrap();
        let env = ReadmeQualitySensor
            .analyze(dir.path().to_str().unwrap())
            .await
            .unwrap();
        // file present (30) + H1 (15) = 45.
        assert_eq!(env["score"], 45);
    }

    #[tokio::test]
    async fn lowercase_readme_filename_works() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("readme.md"), "# Title\n").unwrap();
        let env = ReadmeQualitySensor
            .analyze(dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(env["score"], 45);
    }

    #[tokio::test]
    async fn factory_builds_a_working_sensor() {
        let factory = ReadmeQualitySensorFactory;
        assert_eq!(factory.name(), "readme-quality");
        let sensor = factory.build();
        // Roundtrip through Box<dyn Sensor>.
        let dir = TempDir::new().unwrap();
        let env = sensor.analyze(dir.path().to_str().unwrap()).await.unwrap();
        assert_eq!(env["meta"]["updated_by"], "readme-quality");
        assert_eq!(env["meta"]["schema_version"], "1");
    }
}
