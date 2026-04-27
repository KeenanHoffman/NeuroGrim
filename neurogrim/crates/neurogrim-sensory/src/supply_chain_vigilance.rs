//! Supply-chain vigilance — Layer 2 deep-signal sensors.
//!
//! Epic E-SC-5 of the supply-chain security scaffolding. Implements
//! the LSP-Brains v2.6 §16.3 "Layer 2 — Vigilance" contract: seven
//! advisory sub-sensors that detect publishing-behavior patterns
//! preceding a confirmed compromise.
//!
//! # The seven sub-sensors
//!
//! Group A — registry metadata only (cheap, fast, well-scoped):
//! 1. **publish_cadence** — step-function changes in release frequency
//!    (e.g., dormant 365+ days then 3 releases in a week, or 10x
//!    acceleration over the historical median).
//! 2. **maintainer_delta** — new maintainer added within a configured
//!    window (default 30 days). Compares against per-package cached
//!    historical state.
//! 3. **transitive_surface_delta** — dep-count delta between adjacent
//!    versions (e.g., a patch release that suddenly pulls in 40 new
//!    transitive deps).
//! 4. **typosquat_proximity** — Levenshtein ≤ 1 to top-1000 popular
//!    packages per registry. Static asset (compiled-in lists),
//!    refreshed quarterly.
//!
//! Group B — cryptographic / cross-source verification:
//! 5. **signature_gap** — sigstore / GPG / trusted-publisher presence
//!    vs prior version. Per-ecosystem (crates.io trusted-publishing,
//!    PyPI PEP 740 attestations, npm sigstore-attestation).
//! 6. **binary_reproducibility** — registry-tarball hash vs source-
//!    tag hash. Useful for the subset of packages with clean source
//!    repo links; informational on the rest.
//!
//! Group C — source-content scanning (security-loaded):
//! 7. **exfil_indicator** — static-analysis heuristics on package
//!    source: base64 strings, dynamic eval/exec, subprocess/Process,
//!    network endpoints added in a recent version. Per-language regex
//!    sets. Directly motivated by the LiteLLM 2026-04-23 payload.
//!    Tarball download + safe-extraction discipline (no execution,
//!    no install hooks fire).
//!
//! # Design posture
//!
//! Same as Layer 1 (`supply_chain_sca`): **no external scanner
//! binaries.** Trust surface is `neurogrim` itself + `reqwest`
//! (rustls + native-roots) + `serde_json` + `sha2` + `chrono` + per-
//! ecosystem registry HTTPS endpoints (crates.io / PyPI / npm-
//! registry). All endpoints documented in `audit/TOOL-TRUST-NOTES.md`
//! per the 2026-04-25 E-SC-5 entry.
//!
//! # Caching + state
//!
//! - Registry-metadata fetches: 7-day file-backed cache at
//!   `.claude/brain/cache/vigilance/registry/<ecosystem>/<sha256>.json`.
//!   `NEUROGRIM_VIGILANCE_NO_CACHE=1` env override forces fresh
//!   queries.
//! - Per-package historical state: `.claude/brain/cache/vigilance/
//!   state/<ecosystem>/<sha256>.json` per dep. Used by
//!   `maintainer_delta` and `transitive_surface_delta` for cross-run
//!   comparison.
//!
//! # Scoring
//!
//! Count-based v1 (mirrors Layer 1's rubric). Each sub-sensor finding
//! deducts a fixed amount from a starting 100. Score caps at 0.
//! Severity-weighted scoring is an E-SC-8 calibration candidate.
//!
//! # Default weight
//!
//! Spec §16.3 MUST: domain `supply-chain-vigilance` defaults to
//! `domain_weights: 0.0` (advisory) in v1. Promotion past advisory
//! requires §15.5-equivalent calibration evidence.

pub mod binary_reproducibility;
pub mod exfil_indicator;
pub mod maintainer_delta;
pub mod publish_cadence;
pub mod registry;
pub mod scan_with_metadata;
pub mod scoring;
pub mod signature_gap;
pub mod state;
pub mod transitive_surface;
pub mod typosquat;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

use crate::supply_chain_sca::{lockfile, Package};

// =========================================================================
// MCP server wrapper
// =========================================================================

#[derive(Debug, Clone)]
pub struct SupplyChainVigilanceServer {
    tool_router: ToolRouter<Self>,
}

impl SupplyChainVigilanceServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for SupplyChainVigilanceServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckSupplyChainVigilanceParams {
    /// Filesystem path to the project's root directory. Must contain a
    /// supported lockfile (Cargo.lock / uv.lock / requirements*.txt /
    /// package-lock.json / yarn.lock / pnpm-lock.yaml). When `.claude/`
    /// lives one level up (workspace-subdir layouts), the sensor falls
    /// back automatically — same behavior as supply-chain-sca.
    pub project_root: String,
}

#[tool_router]
impl SupplyChainVigilanceServer {
    #[tool(
        description = "Run native-Rust supply-chain Layer 2 vigilance \
        (LSP-Brains v2.6 §16.3). Seven advisory sub-sensors detect publishing-\
        behavior patterns preceding compromise: publish_cadence (step-function \
        release-frequency changes), maintainer_delta (new maintainer in window), \
        transitive_surface_delta (dep-count surge), typosquat_proximity \
        (Levenshtein ≤1 to top-1000), signature_gap (attestation drop), \
        binary_reproducibility (tarball-vs-source hash), exfil_indicator \
        (base64/eval/subprocess/network static analysis). NO external scanner \
        binaries. Queries each ecosystem's registry JSON API directly \
        (crates.io/PyPI/npm-registry) over HTTPS with 7-day file cache. \
        Override cache via NEUROGRIM_VIGILANCE_NO_CACHE=1. Returns CMDB envelope \
        with count-based score (0 findings=100, deducted per finding). Default \
        domain weight 0.0 (advisory) per §16.3 — promotion past advisory \
        requires §15.5-equivalent calibration evidence."
    )]
    async fn check_supply_chain_vigilance(
        &self,
        Parameters(p): Parameters<CheckSupplyChainVigilanceParams>,
    ) -> String {
        serde_json::to_string_pretty(&analyze_supply_chain_vigilance(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for SupplyChainVigilanceServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Native-Rust supply-chain Layer 2 vigilance sensor. Seven deep-\
                 signal sub-sensors. Direct registry JSON-API queries with 7-day \
                 cache. NO external scanner binaries. Advisory weight by default."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

/// Primary sensor entry point.
///
/// Orchestrates: lockfile parse (reuses Layer 1's multi-ecosystem
/// dispatch) → for each package, fetch registry metadata (cached) +
/// load prior state → run all 7 sub-sensors → aggregate findings →
/// scoring → CMDB envelope.
///
/// Always returns a valid CMDB envelope. Infrastructure failures
/// (lockfile missing, registry unreachable, etc.) are reported as
/// findings + a degraded-coverage flag, not propagated as panics.
pub async fn analyze_supply_chain_vigilance(project_root: &str) -> Value {
    let root = Path::new(project_root);

    // Reuse Layer 1's lockfile dispatch — vigilance scans the same
    // dependency graph, just with different sensors.
    let detected = lockfile::detect(root);
    if detected.is_empty() {
        let findings = vec![crate::cmdb::Finding {
            name: "lockfile_read_failed".to_string(),
            status: "error".to_string(),
            points: 0,
            detail: Some(
                "no supported lockfile found at project_root \
                 (looked for: Cargo.lock, uv.lock, requirements*.txt, \
                 package-lock.json, yarn.lock, pnpm-lock.yaml). \
                 Run a lockfile generator and re-run."
                    .to_string(),
            ),
        }];
        let extras: Vec<(&str, Value)> = vec![
            ("total_packages_scanned", json!(0)),
            ("sensor_status", json!("lockfile_unreadable")),
            ("ecosystems_scanned", json!(Vec::<String>::new())),
        ];
        return crate::cmdb::build_cmdb("supply-chain-vigilance", 0, findings, Some(extras));
    }

    // Parse + dedupe across all detected lockfiles.
    let mut packages: Vec<Package> = Vec::new();
    let mut packages_by_ecosystem: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut parse_errors: Vec<String> = Vec::new();
    for detection in &detected {
        match lockfile::parse_detected(detection, root) {
            Ok(pkgs) => {
                for pkg in pkgs {
                    *packages_by_ecosystem.entry(pkg.ecosystem).or_insert(0) += 1;
                    packages.push(pkg);
                }
            }
            Err(e) => {
                parse_errors.push(format!("{detection:?}: {e:#}"));
            }
        }
    }
    {
        let mut seen: std::collections::HashSet<(String, String, &'static str)> =
            std::collections::HashSet::new();
        packages.retain(|p| seen.insert((p.name.clone(), p.version.clone(), p.ecosystem)));
        packages_by_ecosystem.clear();
        for pkg in &packages {
            *packages_by_ecosystem.entry(pkg.ecosystem).or_insert(0) += 1;
        }
    }

    if packages.is_empty() && !parse_errors.is_empty() {
        let findings = vec![crate::cmdb::Finding {
            name: "lockfile_read_failed".to_string(),
            status: "error".to_string(),
            points: 0,
            detail: Some(format!(
                "lockfile(s) detected but all parses failed: {}",
                parse_errors.join(" | ")
            )),
        }];
        let extras: Vec<(&str, Value)> = vec![
            ("total_packages_scanned", json!(0)),
            ("sensor_status", json!("lockfile_unreadable")),
        ];
        return crate::cmdb::build_cmdb("supply-chain-vigilance", 0, findings, Some(extras));
    }

    // Cache + state directories.
    let cache_dir = root
        .join(".claude")
        .join("brain")
        .join("cache")
        .join("vigilance");
    let registry_cache_dir = cache_dir.join("registry");
    let state_dir = cache_dir.join("state");

    let cache_behavior = registry::cache_behavior_from_env();

    // Fetch registry metadata for every package (cached).
    let metadata_result =
        registry::fetch_all(&packages, &registry_cache_dir, cache_behavior).await;

    // Run each sub-sensor against the metadata + state. Each returns
    // a Vec<scoring::VigilanceFinding>; we flatten into one list.
    let mut all_findings: Vec<scoring::VigilanceFinding> = Vec::new();

    // Group A — registry-metadata only.
    all_findings.extend(typosquat::scan(&packages));
    all_findings.extend(publish_cadence::scan(&packages, &metadata_result));
    all_findings.extend(transitive_surface::scan(
        &packages,
        &metadata_result,
        &state_dir,
    ));
    all_findings.extend(maintainer_delta::scan(
        &packages,
        &metadata_result,
        &state_dir,
    ));

    // Group B — cryptographic verification.
    // signature_gap is sync (registry-metadata only); leave it
    // sequential. binary_reproducibility + exfil_indicator (Group C)
    // are I/O-bound async; run them concurrently for measurable
    // wall-clock improvement on cold-cache scans.
    //
    // 2026-04-26 PRE-RELEASE Round 2 R2-3 fix (D2-D3): the two
    // async sensors operate on disjoint cache subpaths
    // (`<cache_dir>/repro/` vs `<cache_dir>/exfil/`) so concurrent
    // execution introduces no write conflicts. tokio::join! awaits
    // both and returns when both complete; the underlying reqwest
    // connection pool bounds per-host concurrency.
    all_findings.extend(signature_gap::scan(
        &packages,
        &metadata_result,
        &state_dir,
    ));
    let (binary_repro_findings, exfil_findings) = tokio::join!(
        binary_reproducibility::scan(&packages, &metadata_result, &cache_dir, cache_behavior),
        exfil_indicator::scan(&packages, &cache_dir, cache_behavior),
    );
    all_findings.extend(binary_repro_findings);

    // Group C — source-content scanning.
    all_findings.extend(exfil_findings);

    // Persist updated per-package state for next run's deltas.
    state::persist_after_scan(&packages, &metadata_result, &state_dir);

    // Auto-create Layer 3 review tickets for findings that don't
    // already have an open ticket. Per the 2026-04-26 E-SC-6
    // AskUserQuestion lock: auto-create from Layer 2 findings is
    // ON. The dedup key is (ecosystem, package, finding_kind) —
    // repeated scans don't multiply tickets.
    //
    // 2026-04-26 PRE-RELEASE-ASSESSMENT A2 fix: bridge failures
    // were previously logged at warn level only and the count
    // discarded; operators reading the CMDB had no visibility.
    // We now surface the failure as a SensorDegradation finding
    // (weight 0; informational; skipped by the strict gate per
    // its kind == 'sensor-degradation' rule).
    let auto_create_outcome = crate::supply_chain_review::auto_create_from_vigilance(
        &all_findings,
        root,
    );
    let auto_created_tickets = match &auto_create_outcome {
        Ok(n) => *n,
        Err(e) => {
            tracing::warn!("vigilance: auto-create review tickets failed: {:#}", e);
            0
        }
    };
    if let Err(e) = &auto_create_outcome {
        all_findings.push(scoring::VigilanceFinding {
            kind: scoring::VigilanceKind::SensorDegradation,
            package: Package {
                name: "supply-chain-review-bridge".to_string(),
                version: "n/a".to_string(),
                ecosystem: "internal",
            },
            summary: format!("Layer 3 auto-create failed: {:#}", e),
            evidence: None,
            confidence: 0.0,
        });
    }

    // Score + build CMDB envelope.
    let score = scoring::score(&all_findings);
    let _ = auto_created_tickets; // Currently surfaced via supply-chain-review CMDB; reserved for future vigilance-side extras.

    scoring::build_cmdb_envelope(
        score,
        &all_findings,
        &packages,
        &packages_by_ecosystem,
        &metadata_result,
        &parse_errors,
    )
}
