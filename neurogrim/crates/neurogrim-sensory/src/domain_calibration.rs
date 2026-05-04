//! Domain calibration — the meta-observer (LSP-Brains v2.8 §17.9).
//!
//! Reads all per-domain calibration ledgers under `.claude/brain/`
//! and reports calibration health (open vs triaged counts, ledger
//! freshness). Emits envelope-supplied confidence per the
//! tuple-aware (has_ever_fired, last_triage_age) signal in §17.9.
//!
//! # Bootstrap-loop closure
//!
//! Per spec §17.9, the domain-calibration sensor's OWN calibration
//! trigger MUST be hard-coded `Manual`. This sensor doesn't enforce
//! the recursion-guard itself — that's C7's job in `build_scorecard`.
//! Here we just acknowledge that domain-calibration's own ledger
//! exists alongside other domains' ledgers and treat them uniformly
//! when scanning the directory.
//!
//! # Score model (v1)
//!
//! `score = clamp(0, 100, 100 - 10 × open_pending_total)`. Same
//! shape as `supply-chain-review` (§16.4 reference impl). A
//! healthy ecosystem produces 0 entries → score 100.
//!
//! # Confidence model (§17.9)
//!
//! Tuple of (has_ever_fired, last_triage_age):
//!
//! - `has_ever_fired = false` → 100. No signal yet; full confidence
//!   that the calibration story is healthy (or simply absent).
//! - `has_ever_fired = true, last_triage_age = None` → 50. Pending
//!   entries exist but operator hasn't triaged any. Low confidence
//!   — calibration backlog is unattended.
//! - `has_ever_fired = true, last_triage_age = Some(age)` →
//!   `confidence_from_cache_age(age_seconds, 7.0)`. Fresh triage =
//!   high; ancient triage = decayed. 7-day TTL anchors the curve.

use crate::cmdb::{build_cmdb, Finding};
use neurogrim_core::calibration_ledger::{fold, read_all, LedgerEntry};
use neurogrim_core::confidence::confidence_from_cache_age;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

// =========================================================================
// MCP server wrapper (mirrors supply_chain_review)
// =========================================================================

#[derive(Debug, Clone)]
pub struct DomainCalibrationServer {
    // rmcp #[tool_router] macro accesses this through generated dispatch — rustc can't see the uses
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl DomainCalibrationServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for DomainCalibrationServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckDomainCalibrationParams {
    /// Filesystem path to the project's root directory. Reads
    /// calibration ledgers under `.claude/brain/*-calibration-ledger*.jsonl`.
    pub project_root: String,
}

#[tool_router]
impl DomainCalibrationServer {
    #[tool(
        description = "Run the native-Rust domain-calibration meta-observer (LSP-Brains v2.8 §17.9). \
        Reads all .claude/brain/*-calibration-ledger*.jsonl files and reports per-domain \
        calibration health. Score model (v1): 100 - 10 × open_pending_total, floor 0. \
        Default weight 0.0 (advisory) per §17.9 — promotion past advisory requires \
        §15.5-equivalent calibration evidence. Emits envelope-supplied confidence per \
        the tuple-aware (has_ever_fired, last_triage_age) signal — distinguishes 'no signal yet' \
        (high confidence) from 'recently triaged' (high confidence) from 'pending-unanswered' \
        (low confidence). Per §17.9, this sensor's OWN calibration trigger MUST be hard-coded \
        Manual; the recursion guard is enforced in build_scorecard (E-B2-2 C7)."
    )]
    async fn check_domain_calibration(
        &self,
        Parameters(p): Parameters<CheckDomainCalibrationParams>,
    ) -> String {
        serde_json::to_string_pretty(&analyze_domain_calibration(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for DomainCalibrationServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Native-Rust domain-calibration meta-observer (§17.9). Reads all per-domain \
                 calibration ledgers and reports calibration health. Advisory weight by default; \
                 promotion past advisory requires §15.5-equivalent calibration evidence. \
                 Sensor's own calibration trigger is hard-coded Manual per §17.9 (recursion \
                 guard against bootstrap-loop)."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// =========================================================================
// Sensor entry point
// =========================================================================

/// Resolve the `.claude/` root, accepting either the project-root
/// directly or its parent. Mirrors `supply_chain_review.rs::resolve_claude_root`.
fn resolve_claude_root(project_root: &Path) -> PathBuf {
    if project_root.join(".claude").is_dir() {
        project_root.to_path_buf()
    } else if project_root
        .parent()
        .map(|p| p.join(".claude").is_dir())
        .unwrap_or(false)
    {
        project_root.parent().unwrap().to_path_buf()
    } else {
        project_root.to_path_buf()
    }
}

/// Extract the domain name from a calibration-ledger filename.
/// Supports both base form (`<domain>-calibration-ledger.jsonl`)
/// and §17.7 rotation form (`<domain>-calibration-ledger-<year>.jsonl`).
///
/// Returns `None` if the filename doesn't contain `-calibration-ledger`.
fn extract_domain_from_filename(filename: &str) -> Option<&str> {
    let base = filename.strip_suffix(".jsonl")?;
    let idx = base.find("-calibration-ledger")?;
    let prefix = &base[..idx];
    if prefix.is_empty() {
        None
    } else {
        Some(prefix)
    }
}

/// Aggregated state across all ledgers; rolled up at end into the
/// CMDB extras + score + confidence computations.
#[derive(Debug, Default)]
struct AggregatedState {
    /// Distinct domain names that have at least one ledger file.
    domains_with_ledgers: std::collections::BTreeSet<String>,
    /// Distinct domains with at least one currently-open pending entry.
    domains_with_open_pending: std::collections::BTreeSet<String>,
    /// Total open pending entries (across all domains).
    open_pending_total: usize,
    /// Total triaged entries (across all domains, all-time).
    triaged_total: usize,
    /// Most-recent triaged entry's ts across all domains. None if
    /// no triages exist anywhere.
    last_triage_ts: Option<f64>,
    /// Whether ANY ledger has at least one pending entry (open or
    /// superseded). False = "no signal yet"; true = "calibration
    /// backlog has accrued at least once".
    has_ever_fired: bool,
}

/// Primary sensor entry point. Reads all calibration ledgers + emits
/// CMDB envelope.
///
/// Async signature for caller consistency with other sensory entry
/// points (`analyze_supply_chain_review`, etc.). The body is fully
/// synchronous (small file reads through `std::fs`); no actual
/// async I/O happens in v1.
///
/// # No "no peer CMDBs" sentinel here (V5-MOD-2 Phase 4.5, 2026-05-02)
///
/// `coherence` ships a sentinel that returns `score: 0` when its
/// registry-declared correlations can't be evaluated against any
/// peer CMDBs. **`domain-calibration` intentionally does NOT carry
/// the same sentinel.** This sensor reads
/// `*-calibration-ledger.jsonl` files (evidence of fires); their
/// absence is the legitimate "nothing has fired yet" state, where
/// the correct answer is `score: 100, has_ever_fired: false,
/// confidence: 100` (see line 304's `_impl_status` comment). The
/// signal is surfaced via `domains_scanned` extras, not via the
/// score. The plan-critic Subagent 2 finding ("same fix-pattern
/// applies") was overreach — recon at Phase 4.5 confirmed only
/// `coherence` has the false-positive-green issue.
pub async fn analyze_domain_calibration(project_root: &str) -> Value {
    let root = Path::new(project_root);
    let claude_root = resolve_claude_root(root);
    let brain_dir = claude_root.join(".claude").join("brain");

    let mut state = AggregatedState::default();
    let mut findings: Vec<Finding> = Vec::new();

    if brain_dir.is_dir() {
        if let Ok(read_dir) = std::fs::read_dir(&brain_dir) {
            // Sort for deterministic output (BTreeMap by path).
            let mut paths: Vec<PathBuf> = read_dir
                .filter_map(|r| r.ok().map(|e| e.path()))
                .collect();
            paths.sort();
            for path in paths {
                let name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n,
                    None => continue,
                };
                // Filter: must contain -calibration-ledger AND end .jsonl
                if !name.contains("-calibration-ledger") || !name.ends_with(".jsonl") {
                    continue;
                }
                let domain_name = match extract_domain_from_filename(name) {
                    Some(d) => d.to_string(),
                    None => continue,
                };

                let ledger_entries = match read_all(&path) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(
                            "domain-calibration: read {} failed: {:#}",
                            path.display(),
                            e
                        );
                        continue;
                    }
                };

                state.domains_with_ledgers.insert(domain_name.clone());

                if ledger_entries
                    .iter()
                    .any(|e| matches!(e, LedgerEntry::Pending(_)))
                {
                    state.has_ever_fired = true;
                }

                let folded = fold(&ledger_entries);
                let open_count = folded.open_pending.len();
                let triaged_count = folded.triaged.len();

                if open_count > 0 {
                    state.domains_with_open_pending.insert(domain_name.clone());
                }
                state.open_pending_total += open_count;
                state.triaged_total += triaged_count;

                // Track most-recent triaged ts across all ledgers.
                if let Some(latest_in_this) =
                    folded.triaged.iter().map(|t| t.ts).fold(None, |acc, ts| {
                        Some(acc.map_or(ts, |a: f64| a.max(ts)))
                    })
                {
                    state.last_triage_ts = Some(
                        state
                            .last_triage_ts
                            .map_or(latest_in_this, |x| x.max(latest_in_this)),
                    );
                }

                // Per-domain finding (one per ledger seen).
                let (status, points, detail) = if open_count > 0 {
                    (
                        "warning".to_string(),
                        -((10 * open_count) as i32).min(100),
                        format!(
                            "{} open pending, {} triaged historically",
                            open_count, triaged_count
                        ),
                    )
                } else if triaged_count > 0 {
                    (
                        "ok".to_string(),
                        0,
                        format!(
                            "clean — {} entries triaged historically; no open pending",
                            triaged_count
                        ),
                    )
                } else {
                    (
                        "info".to_string(),
                        0,
                        "ledger present but empty".to_string(),
                    )
                };
                findings.push(Finding {
                    name: format!("ledger:{}", domain_name),
                    status,
                    points,
                    detail: Some(detail),
                });
            }
        }
    }

    // Score: 100 - 10 × open_pending_total, clamped [0, 100].
    let score: u8 = ((100_i32) - (10_i32).saturating_mul(state.open_pending_total as i32))
        .clamp(0, 100) as u8;

    // Tuple-aware confidence per §17.9.
    let now_secs = chrono::Utc::now().timestamp() as f64
        + chrono::Utc::now().timestamp_subsec_nanos() as f64 / 1e9;
    let last_triage_age_seconds: Option<u64> = state
        .last_triage_ts
        .map(|ts| (now_secs - ts).max(0.0) as u64);

    let confidence: u8 = if !state.has_ever_fired {
        // No signal ever — full confidence in "everything is fine"
        // (or simply absent — the sensor distinguishes by surfacing
        // domains_scanned in extras).
        100
    } else if let Some(age_secs) = last_triage_age_seconds {
        // Has fired AND has triages — fresh-or-aged based on triage age.
        // Falls back to 50 only if Confidence::new fails (shouldn't).
        confidence_from_cache_age(Some(age_secs), 7.0).unwrap_or(50)
    } else {
        // Has fired but NO triages anywhere — operator hasn't
        // responded. Calibration backlog is unattended; low confidence.
        50
    };

    let mut extras: Vec<(&str, Value)> = vec![
        (
            "domains_scanned",
            json!(state.domains_with_ledgers.iter().collect::<Vec<_>>()),
        ),
        (
            "domains_with_open_pending",
            json!(state.domains_with_open_pending.iter().collect::<Vec<_>>()),
        ),
        ("open_pending_total", json!(state.open_pending_total)),
        ("triaged_total", json!(state.triaged_total)),
        ("has_ever_fired", json!(state.has_ever_fired)),
        (
            "last_triage_age_seconds",
            match last_triage_age_seconds {
                Some(s) => json!(s),
                None => Value::Null,
            },
        ),
        (
            "_impl_status",
            json!(
                "E-B2-2 C4: domain-calibration meta-observer (§17.9). \
                 Tuple-aware (has_ever_fired, last_triage_age) confidence; \
                 score = 100 - 10 × open_pending_total, floor 0. \
                 Default weight 0.0 (advisory) per §17.9."
            ),
        ),
    ];

    if findings.is_empty() {
        // No ledgers found at all — surface explicitly so operators
        // know the sensor ran. (Score will be 100 because nothing
        // is open.)
        findings.push(Finding {
            name: "no_ledgers".to_string(),
            status: "info".to_string(),
            points: 0,
            detail: Some(
                "No calibration ledgers found under .claude/brain/. \
                 Sensor reports score 100 (nothing to triage). \
                 Confidence 100 (has_ever_fired=false; no signal yet)."
                    .to_string(),
            ),
        });
        extras.push((
            "ledger_dir_missing",
            json!(!brain_dir.is_dir()),
        ));
    }

    build_cmdb(
        "domain-calibration",
        score,
        findings,
        Some(extras),
        Some(confidence),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_domain_from_filename_base_form() {
        assert_eq!(
            extract_domain_from_filename("test-health-calibration-ledger.jsonl"),
            Some("test-health")
        );
        assert_eq!(
            extract_domain_from_filename("supply-chain-sca-calibration-ledger.jsonl"),
            Some("supply-chain-sca")
        );
        assert_eq!(
            extract_domain_from_filename("a-calibration-ledger.jsonl"),
            Some("a")
        );
    }

    #[test]
    fn extract_domain_from_filename_rotation_form() {
        // §17.7 rotation: <domain>-calibration-ledger-<year>.jsonl.
        // The "-<year>" suffix is part of the post-marker tail, so
        // the prefix-extraction returns the same domain name.
        assert_eq!(
            extract_domain_from_filename("test-health-calibration-ledger-2026.jsonl"),
            Some("test-health")
        );
    }

    #[test]
    fn extract_domain_from_filename_rejects_non_calibration() {
        assert_eq!(extract_domain_from_filename("foo.jsonl"), None);
        assert_eq!(extract_domain_from_filename("not-a-calibration-thing.json"), None);
        // Empty prefix (would be calibration-ledger.jsonl with no domain).
        assert_eq!(extract_domain_from_filename("-calibration-ledger.jsonl"), None);
    }

    #[test]
    fn extract_domain_from_filename_rejects_non_jsonl() {
        assert_eq!(
            extract_domain_from_filename("test-health-calibration-ledger.json"),
            None
        );
        assert_eq!(
            extract_domain_from_filename("test-health-calibration-ledger.txt"),
            None
        );
    }
}
