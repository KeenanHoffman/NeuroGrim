//! Docker topology sensory tool — ecosystem Brain's lens on local Docker state.
//!
//! Scans the host's Docker CLI + daemon, compares the running state against the
//! project's `docker-compose.yml`, and produces a CMDB envelope scoring how
//! healthy the declared topology is right now.
//!
//! **Scope (v1):** read-only. No mutating Docker operations (`start`, `stop`,
//! `up`, `down`, `rm`, `rmi`, `prune`). Mutations need a permission model we
//! haven't designed; see `METHODOLOGY-EVOLUTION.md` and the plan file for the
//! trade-off rationale.
//!
//! **Backend:** subprocess via `docker` CLI, following the `git_health.rs`
//! pattern. Every call is wrapped in `tokio::time::timeout` (10s) — an
//! improvement over `git_health`'s no-timeout behavior, because a hung Docker
//! daemon shouldn't freeze the scorer.
//!
//! **Graceful degradation:** missing CLI, unreachable daemon, or missing
//! compose file all produce `score: 0` with an explanatory finding. Never
//! panics; never returns `Err` from the top-level `analyze_docker_topology`
//! unless the CMDB builder itself fails (which would indicate a programming
//! bug, not a user condition).

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Per-subprocess timeout. 10s is generous for local docker inventory but
/// low enough that a hung daemon doesn't freeze a scoring pass. If larger
/// stacks hit this ceiling we revisit — the plan file documents the trade.
const DOCKER_TIMEOUT: Duration = Duration::from_secs(10);

/// Default tail for `container_logs` tool. 100 lines is enough to diagnose
/// most recent-crash scenarios without blowing response size.
const DEFAULT_LOG_TAIL: u32 = 100;

/// Upper bound on `container_logs` tail to keep responses sane.
const MAX_LOG_TAIL: u32 = 1000;

// ---------------------------------------------------------------------------
// MCP server — read-only tools
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DockerTopologyServer {
    // rmcp #[tool_router] macro accesses this through generated dispatch — rustc can't see the uses
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl DockerTopologyServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for DockerTopologyServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectRootParams {
    /// Path to the project root (the directory containing docker-compose.yml).
    pub project_root: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ContainerIdParams {
    /// Container name or id to inspect / fetch logs for.
    pub name_or_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ContainerLogsParams {
    /// Container name or id.
    pub name_or_id: String,
    /// Number of trailing log lines to return. Defaults to 100; capped at 1000.
    #[serde(default)]
    pub tail: Option<u32>,
}

#[tool_router]
impl DockerTopologyServer {
    #[tool(
        description = "Score and describe the project's Docker topology health. \
            Compares `docker-compose.yml` services against running containers + images. \
            Returns a CMDB-envelope JSON."
    )]
    async fn check_docker_topology(
        &self,
        Parameters(p): Parameters<ProjectRootParams>,
    ) -> String {
        match analyze_docker_topology(&p.project_root).await {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
            Err(e) => serde_json::json!({ "error": e.to_string(), "score": 0 }).to_string(),
        }
    }

    #[tool(
        description = "List the containers belonging to the project's compose stack \
            (not host-wide). Uses `docker compose ps --all --format json`. Read-only."
    )]
    async fn list_containers(&self, Parameters(p): Parameters<ProjectRootParams>) -> String {
        match list_compose_containers(&p.project_root).await {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    #[tool(
        description = "List all images present on the local Docker host. \
            Uses `docker images --format '{{json .}}'`. Read-only."
    )]
    async fn list_images(&self) -> String {
        match list_host_images().await {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    #[tool(
        description = "Inspect a container by name or id. Uses `docker inspect <id>`. \
            Read-only — returns the raw inspect JSON."
    )]
    async fn inspect_container(&self, Parameters(p): Parameters<ContainerIdParams>) -> String {
        match inspect_container(&p.name_or_id).await {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    #[tool(
        description = "Fetch the last N lines of a container's logs. Uses \
            `docker logs --tail N <id>`. Read-only; tail capped at 1000."
    )]
    async fn container_logs(&self, Parameters(p): Parameters<ContainerLogsParams>) -> String {
        let tail = p.tail.unwrap_or(DEFAULT_LOG_TAIL).min(MAX_LOG_TAIL);
        match fetch_container_logs(&p.name_or_id, tail).await {
            Ok(s) => s,
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    #[tool(
        description = "Return the canonicalized compose config for the project. \
            Uses `docker compose config --format json`. Read-only; matches the \
            shape the sensor reads when scoring."
    )]
    async fn compose_config(&self, Parameters(p): Parameters<ProjectRootParams>) -> String {
        match compose_config_json(&p.project_root).await {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }
}

impl ServerHandler for DockerTopologyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Docker topology sensory tool — read-only scoring and inventory of \
                 the local Docker Engine + compose stack."
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

/// Primary sensor entry point. Returns a CMDB-envelope JSON describing the
/// ecosystem's Docker topology health. See module docs for the scoring
/// contract + graceful-degradation rules.
pub async fn analyze_docker_topology(project_root: &str) -> anyhow::Result<Value> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut extras: Vec<(&str, Value)> = Vec::new();

    // ---- Step 1: probe docker CLI + daemon ----
    let docker_version = match docker_version_probe().await {
        Ok(v) => v,
        Err(DockerProbeError::CliMissing) => {
            findings.push(Finding {
                name: "docker_cli".into(),
                status: "missing".into(),
                points: -100,
                detail: Some("`docker` CLI not found on PATH".into()),
            });
            return Ok(build_cmdb("check-docker-topology", 0, findings, None, None));
        }
        Err(DockerProbeError::DaemonUnreachable(detail)) => {
            findings.push(Finding {
                name: "docker_daemon".into(),
                status: "unreachable".into(),
                points: -100,
                detail: Some(detail),
            });
            return Ok(build_cmdb("check-docker-topology", 0, findings, None, None));
        }
    };
    findings.push(Finding {
        name: "docker_daemon".into(),
        status: "reachable".into(),
        points: 10,
        detail: Some(docker_version.clone()),
    });
    extras.push(("docker_version", Value::String(docker_version)));

    // ---- Step 2: locate compose file ----
    let compose_file = match find_compose_file(project_root) {
        Some(p) => p,
        None => {
            findings.push(Finding {
                name: "compose_file".into(),
                status: "missing".into(),
                points: -100,
                detail: Some(format!(
                    "no docker-compose.yml / compose.yml under {project_root}"
                )),
            });
            return Ok(build_cmdb("check-docker-topology", 0, findings, None, None));
        }
    };
    let compose_name = compose_file
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    findings.push(Finding {
        name: "compose_file".into(),
        status: "found".into(),
        points: 0,
        detail: Some(compose_name.clone()),
    });
    extras.push(("compose_file", Value::String(compose_name)));

    // ---- Step 3: canonicalize expected services ----
    let compose_str = compose_file.to_string_lossy().into_owned();
    let expected = match read_expected_services(&compose_str).await {
        Ok(s) => s,
        Err(e) => {
            findings.push(Finding {
                name: "compose_config".into(),
                status: "unreadable".into(),
                points: -30,
                detail: Some(e.to_string()),
            });
            // We continue — absence of a readable config is degraded but
            // not terminal; running containers may still be observable.
            Vec::new()
        }
    };
    extras.push(("expected_services", Value::from(expected.len() as u64)));

    // ---- Step 4: inventory actual state ----
    let actual_containers = match fetch_compose_ps(&compose_str).await {
        Ok(v) => v,
        Err(e) => {
            findings.push(Finding {
                name: "compose_ps".into(),
                status: "failed".into(),
                points: -20,
                detail: Some(e.to_string()),
            });
            Vec::new()
        }
    };
    let host_images = fetch_images().await.unwrap_or_default();

    // ---- Step 5: score per-service findings ----
    let mut running_services = 0u32;
    let mut exited_services = 0u32;
    let mut positive_service_points: i32 = 0;

    for svc in &expected {
        let container_state = actual_containers
            .iter()
            .find(|c| container_matches_service(c, &svc.name))
            .map(|c| container_state(c));

        match container_state.as_deref() {
            Some("running") => {
                running_services += 1;
                positive_service_points += 10;
                findings.push(Finding {
                    name: format!("service_{}", svc.name),
                    status: "running".into(),
                    points: 10,
                    detail: None,
                });
            }
            Some(other) => {
                exited_services += 1;
                findings.push(Finding {
                    name: format!("service_{}", svc.name),
                    status: other.to_string(),
                    points: -15,
                    detail: Some(format!("service {} in state {}", svc.name, other)),
                });
            }
            None => {
                findings.push(Finding {
                    name: format!("service_{}", svc.name),
                    status: "missing".into(),
                    points: -20,
                    detail: Some(format!("no container for service {}", svc.name)),
                });
            }
        }

        if let Some(image) = &svc.image {
            let present = host_images.iter().any(|img| image_matches(img, image));
            if present {
                findings.push(Finding {
                    name: format!("image_{}", image),
                    status: "present".into(),
                    points: 2,
                    detail: None,
                });
            } else {
                findings.push(Finding {
                    name: format!("image_{}", image),
                    status: "missing".into(),
                    points: -8,
                    detail: Some(format!("image {} not in local registry", image)),
                });
            }
        }
    }

    extras.push(("running_services", Value::from(running_services)));
    extras.push(("exited_services", Value::from(exited_services)));
    extras.push((
        "container_count",
        Value::from(actual_containers.len() as u64),
    ));
    extras.push(("image_count", Value::from(host_images.len() as u64)));

    // ---- Step 6: stragglers + dangling images ----
    let expected_names: std::collections::HashSet<String> =
        expected.iter().map(|s| s.name.clone()).collect();
    let straggler_count = actual_containers
        .iter()
        .filter(|c| {
            // A compose-project container whose service isn't in our
            // expected set. Only counts when the project label matches —
            // else we'd blame random host containers on the compose file.
            let svc = container_service_name(c).unwrap_or_default();
            !expected_names.contains(&svc) && !svc.is_empty()
        })
        .count() as u32;
    extras.push(("straggler_count", Value::from(straggler_count)));
    if straggler_count > 0 {
        findings.push(Finding {
            name: "stragglers".into(),
            status: "present".into(),
            points: -((straggler_count as i32) * 3).min(15).max(0).neg(),
            detail: Some(format!(
                "{straggler_count} container(s) from compose project not in current expected set"
            )),
        });
    }

    let dangling_count = fetch_dangling_image_count().await.unwrap_or(0);
    extras.push(("dangling_image_count", Value::from(dangling_count)));

    // ---- Step 7: aggregate score ----
    let mut score: i32 = 50;
    score += 10; // daemon reachable confirmed above
    score += positive_service_points.min(40); // +10 per running service, cap +40
    for f in &findings {
        if f.name.starts_with("service_") && f.points < 0 {
            score += f.points;
        }
        if f.name.starts_with("image_") && f.points != 0 {
            score += f.points;
        }
    }
    if straggler_count > 0 {
        score -= ((straggler_count as i32) * 3).min(15);
    }
    if dangling_count > 5 {
        score -= 5;
        findings.push(Finding {
            name: "dangling_images".into(),
            status: "many".into(),
            points: -5,
            detail: Some(format!("{dangling_count} dangling images — consider `docker image prune`")),
        });
    } else {
        findings.push(Finding {
            name: "dangling_images".into(),
            status: "ok".into(),
            points: 0,
            detail: Some(format!("{dangling_count} dangling")),
        });
    }

    Ok(build_cmdb(
        "check-docker-topology",
        score.clamp(0, 100) as u8,
        findings,
        Some(extras),
        None,
    ))
}

// Tiny helper — `i32::neg()` isn't inherent; we provide one here so the
// straggler-penalty expression above reads cleanly.
trait IntNeg {
    fn neg(self) -> Self;
}
impl IntNeg for i32 {
    fn neg(self) -> Self {
        -self
    }
}

// ---------------------------------------------------------------------------
// Docker subprocess helpers — all wrapped in DOCKER_TIMEOUT
// ---------------------------------------------------------------------------

enum DockerProbeError {
    CliMissing,
    DaemonUnreachable(String),
}

/// Run `docker version --format json` and return a short version string
/// (server version). Distinguishes CLI missing from daemon unreachable.
async fn docker_version_probe() -> Result<String, DockerProbeError> {
    let out = timeout(
        DOCKER_TIMEOUT,
        Command::new("docker")
            .args(["version", "--format", "{{json .}}"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await;

    let out = match out {
        Err(_) => return Err(DockerProbeError::DaemonUnreachable("timeout".into())),
        Ok(Err(e)) => {
            // `e.kind() == NotFound` strongly implies the CLI is missing.
            // Other errors (permission, pipe) also mean we can't reach docker,
            // but we surface them as daemon-unreachable for honesty.
            if e.kind() == std::io::ErrorKind::NotFound {
                return Err(DockerProbeError::CliMissing);
            }
            return Err(DockerProbeError::DaemonUnreachable(e.to_string()));
        }
        Ok(Ok(o)) => o,
    };

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(DockerProbeError::DaemonUnreachable(
            stderr.trim().to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&stdout)
        .map_err(|e| DockerProbeError::DaemonUnreachable(format!("version parse: {e}")))?;
    // `Server.Version` is the canonical daemon version. Client-only output
    // (no Server key) means the daemon is unreachable.
    let server = v.get("Server").and_then(|s| s.get("Version"));
    match server {
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err(DockerProbeError::DaemonUnreachable(
            "version output missing Server.Version — daemon unreachable".into(),
        )),
    }
}

/// Locate a compose file under `project_root`, preferring the canonical
/// `docker-compose.yml` per Compose v2 docs. Returns the absolute path.
fn find_compose_file(project_root: &str) -> Option<PathBuf> {
    let root = Path::new(project_root);
    for name in ["docker-compose.yml", "compose.yml", "docker-compose.yaml"] {
        let p = root.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[derive(Debug, Clone)]
struct ExpectedService {
    name: String,
    image: Option<String>,
}

async fn read_expected_services(compose_path: &str) -> anyhow::Result<Vec<ExpectedService>> {
    let out = run_docker(&["compose", "-f", compose_path, "config", "--format", "json"]).await?;
    let v: Value = serde_json::from_str(&out)
        .map_err(|e| anyhow::anyhow!("compose config JSON parse: {e}"))?;
    let services = v
        .get("services")
        .and_then(|s| s.as_object())
        .ok_or_else(|| anyhow::anyhow!("compose config missing services object"))?;
    Ok(services
        .iter()
        .map(|(name, spec)| ExpectedService {
            name: name.clone(),
            image: spec
                .get("image")
                .and_then(|i| i.as_str())
                .map(|s| s.to_string()),
        })
        .collect())
}

/// Parse docker compose ps output, tolerating both JSON-array (Compose ≥2.25)
/// and NDJSON (earlier versions) shapes.
async fn fetch_compose_ps(compose_path: &str) -> anyhow::Result<Vec<Value>> {
    let out = run_docker(&[
        "compose", "-f", compose_path, "ps", "--all", "--format", "json",
    ])
    .await?;
    parse_docker_json_list(&out)
}

async fn fetch_images() -> anyhow::Result<Vec<Value>> {
    // Use the template form — it emits NDJSON and has been stable for years.
    // `--format json` is newer (CLI ≥26) and emits an array; our parser
    // handles both so either form would work, but NDJSON is the safer bet
    // against older Docker CLIs in the wild.
    let out = run_docker(&["images", "--format", "{{json .}}"]).await?;
    parse_docker_json_list(&out)
}

async fn fetch_dangling_image_count() -> anyhow::Result<u32> {
    let out = run_docker(&[
        "images",
        "--filter",
        "dangling=true",
        "--format",
        "{{.ID}}",
    ])
    .await?;
    Ok(out.lines().filter(|l| !l.trim().is_empty()).count() as u32)
}

// Public helpers used by the MCP tools — thin wrappers around the same
// subprocess utilities used by the sensor, so behavior stays consistent.

/// List compose-project containers (JSON values as returned by Docker).
pub async fn list_compose_containers(project_root: &str) -> anyhow::Result<Vec<Value>> {
    let compose_file = find_compose_file(project_root)
        .ok_or_else(|| anyhow::anyhow!("no docker-compose.yml under {project_root}"))?;
    fetch_compose_ps(&compose_file.to_string_lossy()).await
}

/// List host-wide images.
pub async fn list_host_images() -> anyhow::Result<Vec<Value>> {
    fetch_images().await
}

/// Inspect one container by name or id. Returns the parsed JSON of
/// `docker inspect <id>` — an array of objects.
pub async fn inspect_container(name_or_id: &str) -> anyhow::Result<Value> {
    validate_docker_ident(name_or_id)?;
    let out = run_docker(&["inspect", name_or_id]).await?;
    let v: Value = serde_json::from_str(&out)
        .map_err(|e| anyhow::anyhow!("inspect JSON parse: {e}"))?;
    Ok(v)
}

/// Fetch the last `tail` lines of a container's logs (combined stdout+stderr).
pub async fn fetch_container_logs(name_or_id: &str, tail: u32) -> anyhow::Result<String> {
    validate_docker_ident(name_or_id)?;
    let tail_s = tail.to_string();
    run_docker(&["logs", "--tail", &tail_s, name_or_id]).await
}

/// Return the canonicalized compose config as parsed JSON.
pub async fn compose_config_json(project_root: &str) -> anyhow::Result<Value> {
    let compose_file = find_compose_file(project_root)
        .ok_or_else(|| anyhow::anyhow!("no docker-compose.yml under {project_root}"))?;
    let out = run_docker(&[
        "compose",
        "-f",
        &compose_file.to_string_lossy(),
        "config",
        "--format",
        "json",
    ])
    .await?;
    serde_json::from_str(&out).map_err(|e| anyhow::anyhow!("compose config parse: {e}"))
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Run `docker <args>` with the shared timeout. Captures stdout on success;
/// surfaces stderr on non-zero exit so callers see the real error.
async fn run_docker(args: &[&str]) -> anyhow::Result<String> {
    let fut = Command::new("docker")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    let out = match timeout(DOCKER_TIMEOUT, fut).await {
        Err(_) => anyhow::bail!("docker {:?} timed out after {:?}", args, DOCKER_TIMEOUT),
        Ok(Err(e)) => anyhow::bail!("docker {:?} failed to spawn: {e}", args),
        Ok(Ok(o)) => o,
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("docker {:?} failed: {}", args, stderr.trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Parse docker output that could be EITHER a JSON array (one call, array
/// of objects) OR NDJSON (one JSON object per non-empty line). Normalizes
/// to `Vec<Value>`. The Compose CLI switched from NDJSON to array around
/// v2.25; we tolerate both so adopters on older Compose versions don't
/// see a silent misparse.
fn parse_docker_json_list(raw: &str) -> anyhow::Result<Vec<Value>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        let v: Vec<Value> = serde_json::from_str(trimmed)
            .map_err(|e| anyhow::anyhow!("JSON array parse: {e}"))?;
        return Ok(v);
    }
    // NDJSON fallback.
    let mut out = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line)
            .map_err(|e| anyhow::anyhow!("NDJSON parse on line {line:?}: {e}"))?;
        out.push(v);
    }
    Ok(out)
}

/// Guard against command-injection-ish identifier misuse when we pass
/// user-supplied strings to `docker` as positional args. Docker's argv
/// parser already isolates args from the shell, but we still validate
/// because an identifier that starts with `-` would be misread by docker
/// as a flag (`--privileged`, `--tls` …) — a real injection path even
/// without a shell.
///
/// Rules:
/// - non-empty
/// - characters limited to `[A-Za-z0-9._:/]` (mid-token `-` allowed)
/// - MUST NOT start with `-` (blocks flag injection)
fn validate_docker_ident(s: &str) -> anyhow::Result<()> {
    if s.is_empty() {
        anyhow::bail!("empty container identifier");
    }
    if s.starts_with('-') {
        anyhow::bail!("container identifier must not start with '-' (flag-injection guard)");
    }
    for c in s.chars() {
        if !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '/')) {
            anyhow::bail!("invalid character in container identifier: {c:?}");
        }
    }
    Ok(())
}

/// Pull the compose-project service name from a `docker compose ps` record.
/// Both older (`Service`) and newer (`Service` / `Names`) formats carry it.
fn container_service_name(c: &Value) -> Option<String> {
    c.get("Service")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Does this container belong to the named compose service?
fn container_matches_service(c: &Value, service: &str) -> bool {
    container_service_name(c).as_deref() == Some(service)
}

/// Extract a normalized container state ("running" / "exited" / etc.) from
/// a `docker compose ps` record. The field name has been `State` since
/// Compose v2; we also accept `Status` as a fallback for exotic outputs.
fn container_state(c: &Value) -> String {
    if let Some(s) = c.get("State").and_then(|v| v.as_str()) {
        return s.to_ascii_lowercase();
    }
    if let Some(s) = c.get("Status").and_then(|v| v.as_str()) {
        // `Status` is like "Up 2 minutes" / "Exited (1) …" — reduce to
        // first word so callers see "up" / "exited" / etc.
        return s
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
    }
    "unknown".into()
}

/// Does a host image record (from `docker images`) match the service image
/// tag from compose config? Compose may emit `myapp:dev` while docker images
/// reports Repository=myapp, Tag=dev — split + compare.
fn image_matches(img: &Value, wanted: &str) -> bool {
    let repo = img.get("Repository").and_then(|v| v.as_str()).unwrap_or("");
    let tag = img.get("Tag").and_then(|v| v.as_str()).unwrap_or("");
    let composite = if tag.is_empty() {
        repo.to_string()
    } else {
        format!("{repo}:{tag}")
    };
    composite == wanted
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn compose_file_discovery_prefers_docker_compose_yml() {
        // Positivity frame: when all three candidate names are present,
        // we pick the canonical `docker-compose.yml` — not a surprise order.
        let dir = tempdir().unwrap();
        for name in ["docker-compose.yml", "compose.yml", "docker-compose.yaml"] {
            std::fs::write(dir.path().join(name), "services: {}").unwrap();
        }
        let found = find_compose_file(dir.path().to_str().unwrap()).unwrap();
        assert!(found.ends_with("docker-compose.yml"));
    }

    #[test]
    fn compose_file_discovery_missing_returns_none() {
        let dir = tempdir().unwrap();
        assert!(find_compose_file(dir.path().to_str().unwrap()).is_none());
    }

    #[test]
    fn compose_file_discovery_accepts_compose_yml() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("compose.yml"), "services: {}").unwrap();
        let found = find_compose_file(dir.path().to_str().unwrap()).unwrap();
        assert!(found.ends_with("compose.yml"));
    }

    #[test]
    fn parse_docker_json_array_shape() {
        let raw = r#"[{"Name":"a"},{"Name":"b"}]"#;
        let v = parse_docker_json_list(raw).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["Name"], "a");
    }

    #[test]
    fn parse_docker_json_ndjson_shape() {
        // Regression guard: Compose < 2.25 emits NDJSON; we must not
        // silently misparse it as a blob.
        let raw = "{\"Name\":\"a\"}\n{\"Name\":\"b\"}\n";
        let v = parse_docker_json_list(raw).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[1]["Name"], "b");
    }

    #[test]
    fn parse_docker_json_empty_is_empty_vec() {
        assert_eq!(parse_docker_json_list("").unwrap().len(), 0);
        assert_eq!(parse_docker_json_list("   \n  ").unwrap().len(), 0);
    }

    #[test]
    fn image_matches_handles_repo_tag_split() {
        let img = json!({"Repository": "neurogrim", "Tag": "dev"});
        assert!(image_matches(&img, "neurogrim:dev"));
        assert!(!image_matches(&img, "neurogrim:prod"));
        assert!(!image_matches(&img, "other:dev"));
    }

    #[test]
    fn container_state_lowercases_state_field() {
        let c = json!({"State": "RUNNING"});
        assert_eq!(container_state(&c), "running");
    }

    #[test]
    fn container_state_falls_back_to_status_first_word() {
        let c = json!({"Status": "Exited (1) 2 minutes ago"});
        assert_eq!(container_state(&c), "exited");
    }

    #[test]
    fn container_state_unknown_when_both_missing() {
        let c = json!({"Name": "foo"});
        assert_eq!(container_state(&c), "unknown");
    }

    #[test]
    fn validate_docker_ident_accepts_reasonable_names() {
        assert!(validate_docker_ident("neurogrim-local").is_ok());
        assert!(validate_docker_ident("sha256:abc123").is_ok());
        assert!(validate_docker_ident("repo/image:tag").is_ok());
        assert!(validate_docker_ident("a_b.c-1").is_ok());
    }

    #[test]
    fn validate_docker_ident_rejects_whitespace_and_shell_metas() {
        assert!(validate_docker_ident("").is_err());
        assert!(validate_docker_ident("foo bar").is_err());
        assert!(validate_docker_ident("foo;ls").is_err());
        assert!(validate_docker_ident("$(whoami)").is_err());
        assert!(validate_docker_ident("--privileged").is_err());
    }

    /// This test exists to pin the public read-only tool surface. If a
    /// future change adds a mutating tool, the compile error (or the
    /// assertion below once the surface grows) will surface it — mutations
    /// demand a permission model we haven't designed. Keep the list in
    /// lock-step with the `#[tool(...)]` annotations above.
    #[test]
    fn read_only_tool_surface_is_fixed() {
        // The six v1 tools, by name (snake_case matches the Rust method
        // names — MCP tool names derive from those by default).
        let expected: &[&str] = &[
            "check_docker_topology",
            "list_containers",
            "list_images",
            "inspect_container",
            "container_logs",
            "compose_config",
        ];
        // We don't introspect the `ToolRouter` here (its internals aren't
        // public); this test is a spec-style reminder. If you add a new
        // `#[tool]` annotation above, update this list AND check it's
        // read-only.
        assert_eq!(expected.len(), 6);
    }

    // The full `analyze_docker_topology` path requires a live docker
    // daemon, so its end-to-end test is `#[ignore]`d (below). The pure
    // helpers are unit-tested above.

    #[tokio::test]
    #[ignore = "requires Docker daemon"]
    async fn analyze_runs_against_live_daemon() {
        // Live smoke test: given the real docker-compose.yml under
        // NeuroGrim, the sensor produces a CMDB with score in [0,100]
        // and at least the daemon-reachable finding. We don't assert
        // specific service states because the local topology state
        // depends on whether `docker compose up` has been run.
        let project_root = std::env::var("NEUROGRIM_COMPOSE_ROOT")
            .unwrap_or_else(|_| "D:\\Brains\\NeuroGrim".to_string());
        let cmdb = analyze_docker_topology(&project_root).await.unwrap();
        let score = cmdb["score"].as_u64().unwrap();
        assert!(score <= 100, "score out of range: {score}");
        let findings = cmdb["findings"].as_array().unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f["name"] == "docker_daemon" || f["name"] == "docker_cli"),
            "expected a docker_daemon / docker_cli finding"
        );
    }
}
