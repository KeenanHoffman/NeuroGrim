//! Agent-behavior sensor — wraps the Python `abv-run` harness so the Brain
//! can score its own agents as a regular CMDB-backed domain.
//!
//! This is the first sensor in the ecosystem that delegates to an out-of-
//! process orchestrator. The CMDB production itself happens in the Python
//! harness (`D:/Brains/agent-behavior-runner/`); this module is a thin
//! operational wrapper that:
//!
//! 1. Validates preconditions before invoking the harness (scenarios dir
//!    exists, proxy env vars set, etc.) and emits a degraded CMDB on
//!    failure rather than panicking.
//! 2. Shells out to `abv-run scenarios <dir> --ledger … --result-file …
//!    --profile sandbox` with a generous timeout ceiling (10 min — the
//!    harness respects its own per-scenario timeouts internally).
//! 3. Passes the harness's stdout (an `agent-behavior-cmdb.json`) through
//!    unchanged when the run succeeds.
//!
//! **Why a wrapper and not just `Command::new` inline in main.rs?**
//! So every sensor — in-process or subprocess — has the same dispatch shape
//! in `run_sensory`. Graceful-degradation findings live here, not in the
//! CLI glue.
//!
//! **Scope (v1):** read-only. The sensor does not author scenarios, edit
//! skills, or write CMDBs anywhere other than stdout. Feedback-ledger writes
//! are the harness's responsibility; we only point it at the right path.

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Hard ceiling on how long `abv-run` is allowed to run from this wrapper.
/// The harness respects per-scenario `timeout_seconds` internally; this is
/// the outer backstop if the harness itself deadlocks or the process-
/// management layer loses its mind. 10 minutes accommodates 5 scenarios ×
/// 3 trials × 3 calls at real API latencies; longer runs should invoke
/// `abv-run` directly and pipe to the CMDB file manually.
const ABV_RUN_TIMEOUT: Duration = Duration::from_secs(600);

// ---------------------------------------------------------------------------
// MCP server
// ---------------------------------------------------------------------------

/// MCP tool surface for agent-behavior. One tool — `check_agent_behavior` —
/// that runs the sensor and returns the CMDB JSON. Included for symmetry
/// with every other sensor in `neurogrim-sensory`; most operators will use
/// the `neurogrim cast agent-behavior` CLI dispatch instead.
#[derive(Debug, Clone)]
pub struct AgentBehaviorServer {
    tool_router: ToolRouter<Self>,
}

impl AgentBehaviorServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for AgentBehaviorServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectRootParams {
    /// Path to the project root (the directory whose `.claude/agent-behavior-
    /// scenarios/` holds the scenario YAMLs to run).
    pub project_root: String,
}

#[tool_router]
impl AgentBehaviorServer {
    #[tool(
        description = "Score the project's agent-behavior by running the \
            scenario library under .claude/agent-behavior-scenarios/ \
            through the abv-run harness. Returns a CMDB-envelope JSON. \
            Requires ABV_PROXY_URL + ABV_PROXY_TOKEN env vars and \
            `abv-run` on PATH. See LSP-Brains spec §15 for the contract \
            this tool implements."
    )]
    async fn check_agent_behavior(
        &self,
        Parameters(p): Parameters<ProjectRootParams>,
    ) -> String {
        match analyze_agent_behavior(&p.project_root).await {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
            Err(e) => serde_json::json!({ "error": e.to_string(), "score": 0 }).to_string(),
        }
    }
}

impl ServerHandler for AgentBehaviorServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Agent-behavior sensor — invokes the abv-run harness to score \
                 agent behavior against a scenario library. Spec §15."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Sensor entry point
// ---------------------------------------------------------------------------

/// Primary sensor entry point. Returns a CMDB-envelope JSON. Never raises
/// on infrastructure absence — missing scenarios / missing `abv-run` /
/// missing env vars all produce a degraded CMDB (`score: 0`) with an
/// explanatory finding, matching the `docker_topology.rs` graceful-
/// degradation contract.
pub async fn analyze_agent_behavior(project_root: &str) -> anyhow::Result<Value> {
    let root = Path::new(project_root);
    let scenarios_dir = root.join(".claude").join("agent-behavior-scenarios");
    let ledger_path = root
        .join(".claude")
        .join("brain")
        .join("agent-behavior-feedback.jsonl");
    let result_path = root
        .join(".claude")
        .join("brain")
        .join("agent-behavior-latest.json");

    let mut findings: Vec<Finding> = Vec::new();

    // --- pre-flight 1: scenarios dir -----------------------------------
    if !scenarios_dir.is_dir() {
        findings.push(Finding {
            name: "scenarios_dir".into(),
            status: "missing".into(),
            points: -100,
            detail: Some(format!(
                "expected {}; author scenarios per LSP-Brains spec §15.2",
                scenarios_dir.display()
            )),
        });
        return Ok(build_cmdb("cast-agent-behavior", 0, findings, None, None));
    }

    // Is the directory empty? Harness would no-op; we say so explicitly.
    let any_scenarios = std::fs::read_dir(&scenarios_dir)
        .map(|d| {
            d.filter_map(Result::ok)
                .any(|e| {
                    e.path()
                        .extension()
                        .map(|x| x == "yaml" || x == "yml")
                        .unwrap_or(false)
                })
        })
        .unwrap_or(false);
    if !any_scenarios {
        findings.push(Finding {
            name: "scenarios_dir".into(),
            status: "empty".into(),
            points: -100,
            detail: Some(format!(
                "{} has no *.yaml files",
                scenarios_dir.display()
            )),
        });
        return Ok(build_cmdb("cast-agent-behavior", 0, findings, None, None));
    }

    // --- pre-flight 2: proxy env ---------------------------------------
    let have_url = std::env::var("ABV_PROXY_URL").is_ok();
    let have_tok = std::env::var("ABV_PROXY_TOKEN").is_ok();
    if !have_url || !have_tok {
        findings.push(Finding {
            name: "abv_proxy_env".into(),
            status: "missing".into(),
            points: -100,
            detail: Some(
                "ABV_PROXY_URL and/or ABV_PROXY_TOKEN not set; \
                 issue a scope token via `proxy-cli issue --label cast-agent-behavior` \
                 and export both. See agent-behavior-runner/README.md."
                    .into(),
            ),
        });
        return Ok(build_cmdb("cast-agent-behavior", 0, findings, None, None));
    }

    // --- ensure ledger + result dirs exist ------------------------------
    // The harness creates its own parents but we do it here too so the
    // graceful-degradation path is consistent even if the run errors out.
    if let Some(p) = ledger_path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    if let Some(p) = result_path.parent() {
        let _ = std::fs::create_dir_all(p);
    }

    // --- invoke abv-run -------------------------------------------------
    let scenarios_s = scenarios_dir.to_string_lossy().into_owned();
    let ledger_s = ledger_path.to_string_lossy().into_owned();
    let result_s = result_path.to_string_lossy().into_owned();

    let fut = Command::new("abv-run")
        .args([
            "scenarios",
            &scenarios_s,
            "--ledger",
            &ledger_s,
            "--result-file",
            &result_s,
            "--profile",
            "sandbox",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let out = match timeout(ABV_RUN_TIMEOUT, fut).await {
        Err(_) => {
            findings.push(Finding {
                name: "abv_run".into(),
                status: "timeout".into(),
                points: -100,
                detail: Some(format!(
                    "abv-run exceeded {} seconds; invoke it directly for long runs",
                    ABV_RUN_TIMEOUT.as_secs()
                )),
            });
            return Ok(build_cmdb("cast-agent-behavior", 0, findings, None, None));
        }
        Ok(Err(e)) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                findings.push(Finding {
                    name: "abv_run".into(),
                    status: "missing".into(),
                    points: -100,
                    detail: Some(
                        "abv-run not on PATH; `pip install -e D:/Brains/agent-behavior-runner`"
                            .into(),
                    ),
                });
                return Ok(build_cmdb("cast-agent-behavior", 0, findings, None, None));
            }
            anyhow::bail!("abv-run spawn failed: {e}");
        }
        Ok(Ok(o)) => o,
    };

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        // Trim + truncate for the CMDB finding; never pass multi-kB stderr.
        let detail: String = stderr.trim().chars().take(300).collect();
        findings.push(Finding {
            name: "abv_run".into(),
            status: "failed".into(),
            points: -100,
            detail: Some(format!("abv-run exited {}: {}", out.status, detail)),
        });
        return Ok(build_cmdb("cast-agent-behavior", 0, findings, None, None));
    }

    // --- success: parse + pass through ---------------------------------
    let stdout = String::from_utf8_lossy(&out.stdout);
    let cmdb: Value = serde_json::from_str(&stdout).map_err(|e| {
        anyhow::anyhow!(
            "abv-run produced non-JSON stdout (first 200 chars: {:?}): {}",
            stdout.chars().take(200).collect::<String>(),
            e
        )
    })?;
    Ok(cmdb)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn missing_scenarios_dir_produces_degraded_cmdb() {
        // Empty tempdir: no `.claude/agent-behavior-scenarios/` at all.
        let dir = tempdir().unwrap();
        let cmdb = analyze_agent_behavior(dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(cmdb["score"], 0);
        assert_eq!(cmdb["meta"]["updated_by"], "cast-agent-behavior");
        let findings = cmdb["findings"].as_array().unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f["name"] == "scenarios_dir" && f["status"] == "missing"),
            "expected scenarios_dir:missing finding, got {:?}",
            findings
        );
    }

    #[tokio::test]
    async fn empty_scenarios_dir_produces_degraded_cmdb() {
        let dir = tempdir().unwrap();
        let scen = dir.path().join(".claude").join("agent-behavior-scenarios");
        std::fs::create_dir_all(&scen).unwrap();
        // No YAML files inside.
        let cmdb = analyze_agent_behavior(dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(cmdb["score"], 0);
        let findings = cmdb["findings"].as_array().unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f["name"] == "scenarios_dir" && f["status"] == "empty"),
            "expected scenarios_dir:empty finding"
        );
    }

    #[tokio::test]
    async fn missing_proxy_env_produces_degraded_cmdb() {
        // Hostile test: we can't touch real env (global state would leak
        // into other tests); instead we rely on the ordered pre-flights
        // and check that a scenarios-present dir still trips the env
        // check when envs are absent. Ugly but honest: if ABV_PROXY_URL
        // happens to be set in the test runner's env (rare) this test
        // degrades to checking that the downstream abv-run absence is
        // surfaced. We assert the weaker form — that SOME missing-
        // infra finding is emitted.
        let dir = tempdir().unwrap();
        let scen = dir.path().join(".claude").join("agent-behavior-scenarios");
        std::fs::create_dir_all(&scen).unwrap();
        std::fs::write(
            scen.join("noop.yaml"),
            "id: noop\nversion: '1'\ntarget: {kind: general, scope: 'placeholder for test'}\nprompt: 'hi'\nrubric:\n  - id: a\n    weight: 100\n    description: 'placeholder criterion — at least twenty chars'\n",
        )
        .unwrap();
        let cmdb = analyze_agent_behavior(dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(cmdb["score"], 0);
        let findings = cmdb["findings"].as_array().unwrap();
        let has_infra_miss = findings.iter().any(|f| {
            let name = f["name"].as_str().unwrap_or("");
            let status = f["status"].as_str().unwrap_or("");
            matches!(
                (name, status),
                ("abv_proxy_env", "missing")
                    | ("abv_run", "missing")
                    | ("abv_run", "failed")
                    | ("abv_run", "timeout"),
            )
        });
        assert!(
            has_infra_miss,
            "expected abv_proxy_env:missing OR abv_run:{{missing|failed|timeout}} finding, got {:?}",
            findings
        );
    }
}
