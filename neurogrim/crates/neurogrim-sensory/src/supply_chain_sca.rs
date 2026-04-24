//! Supply-chain SCA (Software Composition Analysis) sensor — Rust ecosystem.
//!
//! Epic E-SC-2 of the supply-chain security scaffolding. Parses the
//! project's `Cargo.lock`, queries OSV.dev for advisories, cross-
//! references a pinned local clone of `rustsec/advisory-db`, honors
//! operator-accepted advisories, and emits a standard CMDB envelope.
//!
//! # Design posture
//!
//! **No external scanner binaries.** The trust surface is the
//! `neurogrim` binary itself + the `cargo-lock` crate (RustSec-
//! maintained) + `reqwest` + `toml` + `sha2` + OSV.dev HTTPS + the
//! pinned `vendor/rustsec-advisory-db/` submodule. No `trivy`, no
//! `grype`, no `cargo-audit` binary, no `osv-scanner`. External-
//! tool output may be piped in as an optional cross-check (E-SC-2
//! Step 10) but never as a primary data source.
//!
//! This design was adopted on 2026-04-24 in response to a PyPI
//! supply-chain incident where a trojanized security-scanner binary
//! was the attack vector (LiteLLM 1.82.7/.8; HN item 47501426).
//!
//! # Implementation status
//!
//! - Step 3-4 (current): module scaffold + `lockfile.rs` parser.
//!   End-to-end CLI dispatch returns a stub CMDB with
//!   `total_packages_scanned` populated from the real lockfile.
//! - Step 5: `osv.rs` batch client + file-backed cache.
//! - Step 6: `rustsec.rs` local advisory-db TOML reader.
//! - Step 7: `accepted.rs` operator-accepted-advisories reader.
//! - Step 8: `scoring.rs` count-based scoring model.
//! - Step 9: MCP wrapper + `SupplyChainScaServer` struct.
//!
//! See `~/.claude/plans/parallel-hugging-eich-e-sc-2.md` for the full
//! per-epic plan.

pub mod accepted;
pub mod lockfile;
pub mod osv;
pub mod rustsec;
pub mod scoring;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

/// A package identified in the project's dependency graph.
///
/// Only packages sourced from `crates.io` are included — git deps,
/// local path deps, and alternative registries are excluded at
/// lockfile-enumeration time. OSV coverage does not extend to those
/// sources, and our scoring model does not have a sensible way to
/// represent "we can't check this."
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Package {
    pub name: String,
    pub version: String,
}

/// An advisory affecting a specific package version.
///
/// Sourced from either OSV.dev (primary) or our pinned local clone
/// of `rustsec/advisory-db` (fallback + cross-reference).
#[derive(Debug, Clone, Serialize)]
pub struct Advisory {
    /// Advisory identifier (e.g., `RUSTSEC-2024-0436`, `GHSA-...`).
    pub id: String,
    /// The affected package.
    pub package: Package,
    /// Short summary if known (may be empty; OSV batch responses do
    /// not include summary text — retrieved via `/v1/vulns/<id>`).
    pub summary: Option<String>,
    /// Where we discovered this advisory.
    pub source: AdvisorySource,
    /// RustSec-specific: "unmaintained" / "notice" / etc. for
    /// non-CVE advisories. `None` for actual vulnerabilities.
    pub informational: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdvisorySource {
    /// Found via `POST https://api.osv.dev/v1/querybatch`.
    Osv,
    /// Found via local `vendor/rustsec-advisory-db/` TOML scan.
    RustsecLocal,
}

// =========================================================================
// MCP server wrapper
// =========================================================================
//
// Exposes the sensor via the same MCP-tool surface the other built-in
// sensors use. `neurogrim serve` registers a `SupplyChainScaServer`
// alongside the existing 12 domain servers; Claude Code sessions can
// then call `check_supply_chain_sca` as a structured MCP tool.
//
// The shape mirrors `security_standards.rs` 1:1 — change both together
// if rmcp's macro-generated API evolves.

#[derive(Debug, Clone)]
pub struct SupplyChainScaServer {
    tool_router: ToolRouter<Self>,
}

impl SupplyChainScaServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for SupplyChainScaServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckSupplyChainScaParams {
    /// Filesystem path to the project's root directory. Must contain
    /// a `Cargo.lock` (direct children) and optionally a
    /// `.claude/supply-chain-accepted-advisories.toml` + a
    /// `vendor/rustsec-advisory-db/` submodule for local cross-
    /// reference. When `.claude/` or `vendor/` live one level up
    /// (e.g., cargo workspace in a subdirectory), the sensor falls
    /// back to `<project_root>/../` automatically.
    pub project_root: String,
}

#[tool_router]
impl SupplyChainScaServer {
    #[tool(
        description = "Run native-Rust supply-chain SCA against a project's Cargo.lock. \
        No external scanner binaries. Queries OSV.dev for crates.io-sourced \
        dependencies (batched + 24h cached; override with NEUROGRIM_OSV_NO_CACHE=1), \
        cross-references a pinned local clone of rustsec/advisory-db, honors \
        `.claude/supply-chain-accepted-advisories.toml` for operator triage. \
        Returns CMDB-envelope JSON with a count-based score: 0 unaccepted advisories \
        = 100, 1 = 75, 2 = 50, 3 = 25, 4+ = 0."
    )]
    async fn check_supply_chain_sca(
        &self,
        Parameters(p): Parameters<CheckSupplyChainScaParams>,
    ) -> String {
        serde_json::to_string_pretty(&analyze_supply_chain_sca(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for SupplyChainScaServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Native-Rust supply-chain SCA sensor. OSV.dev + \
                 RustSec-advisory-db (pinned submodule) + operator-accepted \
                 advisories. No external scanner binaries.".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

/// Primary sensor entry point.
///
/// Orchestrates: lockfile parse → OSV batch query (cached) → RustSec
/// local cross-check → accepted-advisory filter → scoring → CMDB
/// envelope.
///
/// Always returns a valid CMDB envelope. Infrastructure failures
/// (lockfile missing, OSV unreachable, etc.) are reported as findings
/// with a conservative score, not as panics.
pub async fn analyze_supply_chain_sca(project_root: &str) -> Value {
    let root = Path::new(project_root);

    // Step 4 — lockfile enumeration is fully wired.
    let packages = match lockfile::parse(root) {
        Ok(pkgs) => pkgs,
        Err(e) => {
            // Conservative: if we can't even read Cargo.lock, the
            // sensor can't report on anything. Score 0 (honest
            // unknown) + a finding that names the failure.
            let findings = vec![crate::cmdb::Finding {
                name: "lockfile_read_failed".to_string(),
                status: "error".to_string(),
                points: 0,
                detail: Some(format!("failed to parse Cargo.lock: {e:#}")),
            }];
            let extras: Vec<(&str, Value)> = vec![
                ("total_packages_scanned", json!(0)),
                ("sensor_status", json!("lockfile_unreadable")),
            ];
            return crate::cmdb::build_cmdb(
                "supply-chain-sca",
                0,
                findings,
                Some(extras),
            );
        }
    };

    // Step 5 wires the OSV client (batch query + 24h file cache).
    // RustSec cross-check + accepted-filter + scoring remain stubbed;
    // Steps 6-8 fill them in.
    let cache_dir = root.join(".claude").join("brain").join("cache").join("osv");
    let osv_result = match osv::query_batch(&packages, &cache_dir).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("OSV query_batch returned Err (should be rare): {:#}", e);
            osv::OsvQueryResult::default()
        }
    };
    let rustsec_advisories = rustsec::scan_local(&packages, root).unwrap_or_default();
    let accepted = accepted::read(root).unwrap_or_default();

    // Union OSV + RustSec, deduplicating by (advisory id, package
    // name, package version). When both sources surface the same
    // advisory, prefer the entry with a non-empty `informational`
    // field — OSV batch responses don't carry that metadata but
    // RustSec-local does, so preferring "richer" lets the Finding
    // detail distinguish `unmaintained` notices from real CVEs.
    let all_advisories: Vec<Advisory> = {
        use std::collections::HashMap;
        let mut by_key: HashMap<(String, String, String), Advisory> = HashMap::new();
        for adv in osv_result
            .advisories
            .iter()
            .cloned()
            .chain(rustsec_advisories.into_iter())
        {
            let key = (
                adv.id.clone(),
                adv.package.name.clone(),
                adv.package.version.clone(),
            );
            match by_key.entry(key) {
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(adv);
                }
                std::collections::hash_map::Entry::Occupied(mut o) => {
                    let existing_has_info = o
                        .get()
                        .informational
                        .as_deref()
                        .is_some_and(|s| !s.is_empty());
                    let new_has_info = adv
                        .informational
                        .as_deref()
                        .is_some_and(|s| !s.is_empty());
                    // Upgrade only if we're gaining information.
                    if !existing_has_info && new_has_info {
                        o.insert(adv);
                    }
                }
            }
        }
        by_key.into_values().collect()
    };

    // Identify advisories that OSV did NOT return — the "OSV miss,
    // locally caught" signal that motivates keeping the submodule
    // pinned. Surfacing the IDs (not just the count) lets the
    // operator see exactly what slipped through OSV's ingestion
    // pipeline.
    //
    // Note: we compute this against the ORIGINAL OSV ID set, not by
    // inspecting `.source` on the unioned list. The dedup above
    // prefers entries with richer metadata (RustSec-local carries
    // `informational`, OSV doesn't), so a RustSec entry can "win"
    // even for an ID OSV also knew about. This counter must reflect
    // "did OSV see it at all," not "which source won the dedup."
    let osv_ids: std::collections::HashSet<&str> =
        osv_result.advisories.iter().map(|a| a.id.as_str()).collect();
    let osv_missed: Vec<&Advisory> = all_advisories
        .iter()
        .filter(|a| !osv_ids.contains(a.id.as_str()))
        .collect();
    let rustsec_only_count = osv_missed.len();
    let rustsec_only_ids: Vec<Value> = osv_missed
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "package": a.package.name,
                "version": a.package.version,
                "informational": a.informational,
            })
        })
        .collect();

    let (score, findings, extras_from_scoring) =
        scoring::compute(&all_advisories, &accepted, packages.len());

    // Compose the full extras vec: scoring contributions + lockfile
    // stats + OSV metadata + RustSec-local metadata.
    let mut extras: Vec<(&str, Value)> = extras_from_scoring;
    extras.push(("total_packages_scanned", json!(packages.len())));
    extras.push(("sensor_status", json!("ok")));
    extras.push(("osv_cache_hits", json!(osv_result.cache_hits)));
    extras.push(("osv_live_queries", json!(osv_result.live_queries)));
    extras.push((
        "osv_oldest_cache_age_seconds",
        match osv_result.oldest_cache_age_seconds {
            Some(s) => json!(s),
            None => Value::Null,
        },
    ));
    extras.push(("osv_reachable", json!(osv_result.osv_reachable)));
    extras.push(("osv_cache_bypassed", json!(osv_result.cache_bypassed)));
    extras.push(("rustsec_local_unique_hits", json!(rustsec_only_count)));
    extras.push(("rustsec_local_unique_ids", json!(rustsec_only_ids)));
    extras.push((
        "_impl_status",
        json!("Steps 1-8 complete: OSV + RustSec-local + accepted-list + scoring live. \
               MCP wrapper (Step 9) and integration tests (Step 10) pending."),
    ));

    crate::cmdb::build_cmdb("supply-chain-sca", score, findings, Some(extras))
}
