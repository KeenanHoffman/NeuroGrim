//! HTTP route definitions.
//!
//! Phase 0.3: `/api/health` + static-asset fallback.
//! Phase 1: per-page endpoints (`/api/agent`, `/api/domains`,
//! `/api/domains/:name`, `/api/federation`, `/api/skills`).
//! Phase 2: `/api/events` (SSE live updates).

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::{header, StatusCode, Uri},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json, Response,
    },
    routing::get,
    Router,
};
use serde::Deserialize;
use futures::stream::{Stream, StreamExt};
use neurogrim_a2a::agent_card::{AgentCard, TransportProtocol};
use neurogrim_a2a::TaskClient;
use neurogrim_core::trajectory::compute_trajectory;
use neurogrim_core::types::{ScoreSnapshot, TrajectoryClassification};
use neurogrim_mcp::context::BrainContext;
use neurogrim_mcp::prose::first_sentence;
use rust_embed::RustEmbed;
use std::convert::Infallible;
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;
use tokio_stream::wrappers::BroadcastStream;
use url::Url;

use crate::brains::BrainEntry;
use crate::layout::{
    default_layout_for, read_layout, reset_layout, save_layout, DashboardLayoutRequest,
    DashboardLayoutResponse,
};
use crate::skills::{scan as scan_skills, ALIVE_WINDOW_DAYS};
use crate::state::AppState;
use crate::types::{
    AgentCardExcerptDto, BrainListItemDto, BrainsListResponse, DomainDetailResponse,
    DomainListItemDto, DomainSignalDto, DomainsListResponse, FederationResponse, FindingDto,
    HatDto, HatsResponse, HealthResponse, HistoryPointDto, OverviewResponse, PeerDto,
    PeerStatusDto, RecommendationDto, SelfBrainDto, SkillsResponse,
};

/// Query params accepted by score-aware routes (overview, domains
/// list, domain detail). The `hat` field surfaces the AppShell's
/// hat-picker selection so the Brain output is filtered through
/// that hat's `domain_multipliers`. None / empty / "default" all
/// mean "no hat" — collapsed into Option<String> at the boundary.
#[derive(Debug, Deserialize, Default)]
pub struct ScoreQuery {
    #[serde(default)]
    pub hat: Option<String>,
}

impl ScoreQuery {
    /// Normalize the hat: trim whitespace, treat "default" / empty
    /// as no hat. The Brain's BrainContext::load takes
    /// `Option<String>` where `None` is the un-hatted lens.
    pub fn resolved_hat(&self) -> Option<String> {
        let raw = self.hat.as_deref()?.trim();
        if raw.is_empty() || raw.eq_ignore_ascii_case("default") {
            None
        } else {
            Some(raw.to_string())
        }
    }
}

/// Frontend bundle embedded at compile time. Built by `npm run build`
/// in `frontend/`. Empty during Phase 0 setup until the frontend has
/// been built at least once — `static_handler` falls back to an
/// inline placeholder page so the boot story is debuggable.
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/frontend/dist"]
struct FrontendAssets;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        // ---- Legacy single-Brain routes (point at the host Brain) ----
        // These are kept for backward compatibility while the
        // frontend transitions to /brains/:id/* — Path 2's index
        // route will redirect to /brains/<self_id>/.
        .route("/api/overview", get(overview))
        .route("/api/domains", get(domains_list))
        .route("/api/domains/:name", get(domain_detail))
        .route("/api/federation", get(federation))
        .route("/api/skills", get(skills))
        .route("/api/hats", get(hats))
        // ---- Multi-Brain navigation (Path 2) ----
        .route("/api/brains", get(brains_list))
        .route("/api/brains/:brain_id/overview", get(brain_overview))
        .route("/api/brains/:brain_id/domains", get(brain_domains_list))
        .route(
            "/api/brains/:brain_id/domains/:name",
            get(brain_domain_detail),
        )
        .route("/api/brains/:brain_id/federation", get(brain_federation))
        .route("/api/brains/:brain_id/skills", get(brain_skills))
        .route("/api/brains/:brain_id/hats", get(brain_hats))
        .route(
            "/api/brains/:brain_id/dashboard-layout",
            get(brain_dashboard_layout)
                .put(brain_save_dashboard_layout)
                .delete(brain_reset_dashboard_layout),
        )
        // ---- Live updates ----
        .route("/api/events", get(events_sse))
        .fallback(static_handler)
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        registry_path: state.registry_path.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// `GET /api/overview` — landing-page summary for the Phase 1.1
/// Overview page. Loads a fresh `BrainContext` (registry + scoring
/// pipeline run) on every call.
async fn overview(
    State(state): State<AppState>,
    Query(query): Query<ScoreQuery>,
) -> Response {
    let ctx = match state
        .cache
        .load_or_get(&state.registry_path, query.resolved_hat(), None)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to load BrainContext: {e:#}")
                })),
            )
                .into_response();
        }
    };

    Json(build_overview(&ctx)).into_response()
}

/// Pure conversion: BrainContext → OverviewResponse. Extracted so
/// the route's failure handling stays simple AND the unit test can
/// exercise the mapping logic against a constructed context without
/// the http layer.
fn build_overview(ctx: &BrainContext) -> OverviewResponse {
    let registry = &ctx.registry;
    let agent_output = &ctx.agent_output;

    let project_label = if !registry.meta.description.is_empty() {
        first_sentence(&registry.meta.description, 80)
    } else {
        Path::new(&*ctx.project_root.to_string_lossy())
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("(unnamed)")
            .to_string()
    };

    let domain_count = registry.config.domain_weights.len() as u32;
    let weighted_count = registry
        .config
        .domain_weights
        .values()
        .filter(|w| **w > 0.0)
        .count() as u32;
    let advisory_count = domain_count.saturating_sub(weighted_count);

    // All-advisory Brain → score is structurally meaningless; surface
    // None and let the frontend render "N/A (observe-only posture)"
    // (matches the CLI's `agent --prose` behavior).
    let is_all_advisory = weighted_count == 0 && domain_count > 0;
    let score = if is_all_advisory {
        None
    } else {
        Some(agent_output.score)
    };
    let confidence = if is_all_advisory {
        None
    } else {
        Some(agent_output.unified_confidence)
    };

    let (trajectory_class, trajectory_velocity, trajectory_samples) =
        match &agent_output.trajectory {
            Some(t) => (
                classification_string(&t.classification).to_string(),
                t.velocity,
                t.samples as u32,
            ),
            None => ("no-data".to_string(), 0.0, 0),
        };

    // Top 3 strongest signals (by effective_score desc, then weight desc, then name asc).
    let mut signals: Vec<(&String, u8, u8, f64)> = agent_output
        .domains
        .iter()
        .map(|(k, d)| (k, d.effective_score, d.confidence, d.weight))
        .collect();
    signals.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.0.cmp(b.0))
    });
    let strongest_signals: Vec<DomainSignalDto> = signals
        .iter()
        .take(3)
        .map(|(name, eff, conf, weight)| DomainSignalDto {
            name: (*name).clone(),
            display_name: registry
                .config
                .principle_map
                .get(*name)
                .cloned()
                .unwrap_or_else(|| (*name).to_string()),
            effective_score: *eff,
            confidence: *conf,
            weight: *weight,
        })
        .collect();

    let top_recommendations: Vec<RecommendationDto> = agent_output
        .top_recommendations
        .iter()
        .take(3)
        .map(|r| RecommendationDto {
            domain: r.domain.clone(),
            gate: r.gate.clone(),
            status: r.status.clone(),
            command: r.command.clone(),
            description: r.description.clone().unwrap_or_default(),
        })
        .collect();

    let federation_peer_count = registry
        .config
        .extra
        .get("children")
        .and_then(|v| v.as_object())
        .map(|m| m.len() as u32)
        .unwrap_or(0);

    OverviewResponse {
        project_label,
        project_root: ctx.project_root.to_string_lossy().to_string(),
        domain_count,
        weighted_count,
        advisory_count,
        score,
        confidence,
        trajectory_class,
        trajectory_velocity,
        trajectory_samples,
        top_recommendations,
        strongest_signals,
        federation_peer_count,
    }
}

/// Map `TrajectoryClassification` to a stable wire-format string.
/// Frontend mirrors this set in a TS union. Stringly-typed at the
/// wire keeps the JSON small + frontend-debuggable; the Rust enum
/// stays the source of truth.
fn classification_string(c: &TrajectoryClassification) -> &'static str {
    match c {
        TrajectoryClassification::Improving => "improving",
        TrajectoryClassification::Degrading => "degrading",
        TrajectoryClassification::Stable => "stable",
        TrajectoryClassification::Volatile => "volatile",
        TrajectoryClassification::NoData => "no-data",
    }
}

// =================================================================
// Phase 1.2 — Domains list + detail
// =================================================================

/// `GET /api/domains` — flat list of every declared domain with
/// per-domain score / confidence / weight / trajectory. Powers the
/// Domains-page sortable table.
async fn domains_list(
    State(state): State<AppState>,
    Query(query): Query<ScoreQuery>,
) -> Response {
    let ctx = match state
        .cache
        .load_or_get(&state.registry_path, query.resolved_hat(), None)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to load BrainContext: {e:#}")
                })),
            )
                .into_response();
        }
    };
    Json(build_domains_list(&ctx)).into_response()
}

fn build_domains_list(ctx: &BrainContext) -> DomainsListResponse {
    let registry = &ctx.registry;
    let agent_output = &ctx.agent_output;

    let mut domains: Vec<DomainListItemDto> = registry
        .config
        .domain_weights
        .iter()
        .map(|(name, weight)| {
            let display_name = registry
                .config
                .principle_map
                .get(name)
                .cloned()
                .unwrap_or_else(|| name.clone());

            // AgentOutput.domains carries the per-domain scoring
            // outputs (raw_score / effective_score / confidence).
            // Missing domain = sensor not yet authored / CMDB
            // missing → defaults of 0.
            let (raw_score, effective_score, confidence) =
                match agent_output.domains.get(name) {
                    Some(d) => (d.score, d.effective_score, d.confidence),
                    None => (0, 0, 0),
                };

            // Per-domain trajectory: re-evaluated from the same
            // history the unified trajectory uses, but filtered
            // to this domain. compute_trajectory accepts an
            // Option<&str> for the domain key.
            let trajectory = agent_output
                .domains
                .get(name)
                .and_then(|d| d.trajectory.clone());

            let (trajectory_class, trajectory_velocity, trajectory_samples) =
                match trajectory {
                    Some(t) => (
                        classification_string(&t.classification).to_string(),
                        t.velocity,
                        t.samples as u32,
                    ),
                    None => ("no-data".to_string(), 0.0, 0),
                };

            // Probe the CMDB on disk for `meta.updated_at`. None
            // when the CMDB doesn't exist (the v3.2 stub-domain
            // pattern: registry declares the domain, sensor not
            // yet written).
            let last_updated = registry
                .config
                .domain_definitions
                .get(name)
                .and_then(|def| def.scoring_source.as_ref())
                .and_then(|src| src.path.as_ref())
                .and_then(|rel| {
                    let full = ctx.project_root.join(rel);
                    std::fs::read_to_string(&full).ok()
                })
                .and_then(|s| {
                    let trimmed = s.trim_start_matches('\u{FEFF}');
                    serde_json::from_str::<serde_json::Value>(trimmed).ok()
                })
                .and_then(|v| {
                    v.get("meta")
                        .and_then(|m| m.get("updated_at"))
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                });

            DomainListItemDto {
                name: name.clone(),
                display_name,
                weight: *weight,
                raw_score,
                effective_score,
                confidence,
                trajectory_class,
                trajectory_velocity,
                trajectory_samples,
                last_updated,
            }
        })
        .collect();

    // Stable default order: name asc. The frontend can re-sort
    // client-side; this keeps the wire response deterministic.
    domains.sort_by(|a, b| a.name.cmp(&b.name));

    DomainsListResponse { domains }
}

/// `GET /api/domains/:name` — drill-in detail for a single domain.
/// Includes CMDB findings + score history sparkline + sensor
/// authoring intent (when present).
async fn domain_detail(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
    Query(query): Query<ScoreQuery>,
) -> Response {
    let ctx = match state
        .cache
        .load_or_get(&state.registry_path, query.resolved_hat(), None)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to load BrainContext: {e:#}")
                })),
            )
                .into_response();
        }
    };

    match build_domain_detail(&ctx, &name) {
        Some(detail) => Json(detail).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("domain '{name}' not in registry")
            })),
        )
            .into_response(),
    }
}

fn build_domain_detail(ctx: &BrainContext, name: &str) -> Option<DomainDetailResponse> {
    let registry = &ctx.registry;
    let agent_output = &ctx.agent_output;

    let weight = *registry.config.domain_weights.get(name)?;
    let display_name = registry
        .config
        .principle_map
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string());

    let (raw_score, effective_score, confidence) =
        match agent_output.domains.get(name) {
            Some(d) => (d.score, d.effective_score, d.confidence),
            None => (0, 0, 0),
        };

    let trajectory = agent_output
        .domains
        .get(name)
        .and_then(|d| d.trajectory.clone());
    let (trajectory_class, trajectory_velocity, trajectory_samples) = match trajectory {
        Some(t) => (
            classification_string(&t.classification).to_string(),
            t.velocity,
            t.samples as u32,
        ),
        None => ("no-data".to_string(), 0.0, 0),
    };

    // Resolve the CMDB path for findings + last_updated.
    let def = registry.config.domain_definitions.get(name);
    let cmdb_path_rel = def
        .and_then(|d| d.scoring_source.as_ref())
        .and_then(|s| s.path.as_ref())
        .cloned()
        .unwrap_or_else(|| format!(".claude/{name}-cmdb.json"));
    let cmdb_full = ctx.project_root.join(&cmdb_path_rel);

    let cmdb_json: Option<serde_json::Value> = std::fs::read_to_string(&cmdb_full)
        .ok()
        .and_then(|s| {
            let trimmed = s.trim_start_matches('\u{FEFF}').to_string();
            serde_json::from_str(&trimmed).ok()
        });

    let findings: Vec<FindingDto> = cmdb_json
        .as_ref()
        .and_then(|v| v.get("findings"))
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| {
                    Some(FindingDto {
                        name: f.get("name")?.as_str()?.to_string(),
                        status: f.get("status")?.as_str()?.to_string(),
                        points: f.get("points").and_then(|p| p.as_i64()).unwrap_or(0)
                            as i32,
                        detail: f
                            .get("detail")
                            .and_then(|d| d.as_str())
                            .map(|s| s.to_string()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let last_updated = cmdb_json
        .as_ref()
        .and_then(|v| {
            v.get("meta")
                .and_then(|m| m.get("updated_at"))
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        });

    // Sensor authoring intent — the `_todo_<name>` placeholder
    // captured by `domain new --sensor-intent` (or the v3.4
    // `init --domain-describe` flag).
    let sensor_intent = def
        .and_then(|d| d.extra.get(&format!("_todo_{name}")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Score history (last 30 days). The history file is at
    // `.claude/brain/score-history.json`; entries carry per-domain
    // `SnapshotDomain { score, confidence }`.
    let history_path = ctx
        .project_root
        .join(".claude")
        .join("brain")
        .join("score-history.json");
    let history: Vec<HistoryPointDto> = std::fs::read_to_string(&history_path)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<ScoreSnapshot>>(&s).ok())
        .map(|snapshots| {
            snapshots
                .iter()
                .filter_map(|snap| {
                    snap.domains.get(name).map(|d| HistoryPointDto {
                        scored_at: snap.scored_at.to_rfc3339(),
                        score: d.score,
                        confidence: d.confidence,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // (compute_trajectory is unused here — we return the
    // trajectory already computed in the AgentOutput. Importing
    // it keeps the option open for future per-domain trajectory
    // overrides.)
    let _ = compute_trajectory;

    Some(DomainDetailResponse {
        name: name.to_string(),
        display_name,
        weight,
        raw_score,
        effective_score,
        confidence,
        trajectory_class,
        trajectory_velocity,
        trajectory_samples,
        sensor_intent,
        findings,
        history,
        cmdb_path: cmdb_full.to_string_lossy().to_string(),
        last_updated,
    })
}

// =================================================================
// Phase 1.3 — Federation page
// =================================================================

/// Per-peer Agent Card discovery timeout. 1.5s is short enough to keep
/// the page responsive when a peer is offline, long enough to tolerate
/// a single TCP retry on a real LAN. Picked empirically against the
/// ecosystem Brain hitting NeuroGrim and LSP-Brains.
const PEER_PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// TCP-precheck timeout — the first stage of the two-stage probe.
/// On Windows in particular, a closed localhost port can take
/// 200-500ms to return ConnectionRefused (the kernel's SYN retry
/// timing). 1000ms gives us a clean signal across platforms while
/// keeping the federation page acceptable when peers are offline
/// (worst case: 1s per offline peer, sequential — the page is
/// otherwise free of latency from this code path).
const PEER_TCP_TIMEOUT: Duration = Duration::from_millis(1000);

/// `GET /api/federation` — the Brain's view of itself + its declared
/// A2A / subprocess peers, with a freshness probe for each enabled
/// A2A peer.
///
/// Probes are sequential to keep the implementation small (with the
/// realistic peer count of 1-3 the worst-case latency is < 5 s); the
/// frontend can show a loading state during that window. If concurrency
/// becomes a real issue we'd switch to `tokio::spawn` + `join_all`,
/// but introducing `futures` here for 1-3 calls would be premature.
async fn federation(State(state): State<AppState>) -> Response {
    let ctx = match state
        .cache
        .load_or_get(&state.registry_path, None, None)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to load BrainContext: {e:#}")
                })),
            )
                .into_response();
        }
    };
    Json(build_federation(&ctx).await).into_response()
}

async fn build_federation(ctx: &BrainContext) -> FederationResponse {
    let registry = &ctx.registry;

    let label = if !registry.meta.description.is_empty() {
        first_sentence(&registry.meta.description, 80)
    } else {
        Path::new(&*ctx.project_root.to_string_lossy())
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("(unnamed)")
            .to_string()
    };

    let self_brain = SelfBrainDto {
        label,
        project_root: ctx.project_root.to_string_lossy().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let registry_schema_version = registry.meta.schema_version.clone();

    let raw_peers: Vec<(String, serde_json::Value)> = registry
        .config
        .extra
        .get("children")
        .and_then(|v| v.as_object())
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    let client = TaskClient::new_http();

    let mut peers: Vec<PeerDto> = Vec::with_capacity(raw_peers.len());
    for (name, body) in raw_peers {
        let display_name = body
            .get("display_name")
            .and_then(|v| v.as_str())
            .unwrap_or(&name)
            .to_string();
        let weight = body.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let read_only = body
            .get("read_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let enabled = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
        let brain_path = body
            .get("brain_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let endpoint_str = body
            .get("a2a_endpoint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let agent_card_url = body
            .get("agent_card_url")
            .and_then(|v| v.as_str())
            .and_then(|s| Url::parse(s).ok());

        let transport = if endpoint_str.is_some() {
            "a2a"
        } else if brain_path.is_some() {
            "subprocess"
        } else {
            "unknown"
        }
        .to_string();

        let (status, agent_card) = if !enabled {
            (
                PeerStatusDto {
                    kind: "disabled".to_string(),
                    message: "peer marked enabled=false in registry".to_string(),
                },
                None,
            )
        } else if transport == "a2a" {
            // Best-effort probe. Failures are reported as
            // `unreachable` with a short message; they never break
            // the page render.
            match endpoint_str.as_deref().and_then(|s| Url::parse(s).ok()) {
                Some(endpoint) => {
                    probe_peer(&client, &endpoint, agent_card_url.as_ref()).await
                }
                None => (
                    PeerStatusDto {
                        kind: "unreachable".to_string(),
                        message: "registry's a2a_endpoint is not a valid URL".to_string(),
                    },
                    None,
                ),
            }
        } else {
            (
                PeerStatusDto {
                    kind: "unprobed".to_string(),
                    message: format!("transport={transport} (not probed)"),
                },
                None,
            )
        };

        peers.push(PeerDto {
            name,
            display_name,
            transport,
            a2a_endpoint: endpoint_str,
            brain_path,
            weight,
            read_only,
            enabled,
            status,
            agent_card,
        });
    }

    // Stable order: name asc.
    peers.sort_by(|a, b| a.name.cmp(&b.name));

    FederationResponse {
        self_brain,
        peers,
        registry_schema_version,
    }
}

/// Two-stage probe: TCP precheck → Agent Card fetch.
///
/// Stage 1 — TCP precheck: a fast `tokio::net::TcpStream::connect`
/// against the endpoint's host:port with a 250ms timeout. Splits
/// the prior catch-all `unreachable` into:
///
/// - **not-running**: connection refused at the OS level → the
///   A2A daemon isn't listening on that port.
/// - **unreachable**: any other TCP failure (DNS resolution failed,
///   network unreachable, route timeout). Genuinely unknown state.
///
/// Stage 2 — Agent Card fetch: only runs when the TCP precheck
/// succeeded. Maps to:
///
/// - **alive**: card fetched + parsed.
/// - **unhealthy**: card fetch failed or timed out. Process is
///   running but not serving the well-known endpoint cleanly.
///
/// The two-stage approach costs one extra round trip per peer
/// (~5ms on localhost when the port is open) and adds zero latency
/// for the common "peer not running" case (TCP refusal is
/// near-instant). Worth it for the operator-visible improvement of
/// "is the daemon running" — the question the dashboard is most
/// often answering.
async fn probe_peer(
    client: &TaskClient<neurogrim_a2a::transport::HttpSseTransport>,
    endpoint: &Url,
    override_url: Option<&Url>,
) -> (PeerStatusDto, Option<AgentCardExcerptDto>) {
    // Stage 1: TCP precheck.
    let host = endpoint.host_str().unwrap_or("localhost");
    let port = endpoint.port_or_known_default().unwrap_or(80);
    let addr = format!("{host}:{port}");
    let is_localhost = matches!(host, "127.0.0.1" | "::1" | "localhost");

    // Dual-stack-aware connect. On Windows, "localhost" resolves to
    // both ::1 and 127.0.0.1; tokio's TcpStream::connect((host, port))
    // tries only the first resolved address. If the daemon is bound
    // to 127.0.0.1 (the common case for `neurogrim a2a-serve`) but
    // ::1 sorts first, the connect times out on the IPv6 try and
    // we'd misclassify a running daemon as `not-running`. Resolve
    // explicitly and try each address until one connects.
    let connect_result = match tokio::net::lookup_host(addr.as_str()).await {
        Ok(addrs) => {
            let candidates: Vec<_> = addrs.collect();
            if candidates.is_empty() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::AddrNotAvailable,
                    "no addresses resolved",
                ))
            } else {
                // Try each candidate within the overall timeout.
                let mut last_err: Option<std::io::Error> = None;
                let mut got_stream = None;
                for candidate in candidates {
                    match timeout(
                        PEER_TCP_TIMEOUT,
                        tokio::net::TcpStream::connect(&candidate),
                    )
                    .await
                    {
                        Ok(Ok(s)) => {
                            got_stream = Some(s);
                            break;
                        }
                        Ok(Err(e)) => last_err = Some(e),
                        Err(_) => {
                            last_err = Some(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "tcp connect timed out",
                            ));
                        }
                    }
                }
                match got_stream {
                    Some(s) => Ok(Ok(s)),
                    None => Ok(Err(last_err.unwrap_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "all candidate addresses failed",
                        )
                    }))),
                }
            }
        }
        Err(e) => Err(e),
    };

    match connect_result {
        Ok(Ok(_stream)) => {
            // TCP open — drop the stream and proceed to Agent Card fetch.
        }
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::TimedOut => {
            // All candidate addresses timed out. Same localhost-vs-
            // remote logic as a DNS-lookup-success-then-timeout used
            // to apply: localhost → not-running, remote → unreachable.
            let (kind, descriptor) = if is_localhost {
                ("not-running", "no daemon listening")
            } else {
                ("unreachable", "host did not respond to TCP SYN")
            };
            return (
                PeerStatusDto {
                    kind: kind.to_string(),
                    message: format!(
                        "{descriptor} (tcp connect to {addr} timed out after {} ms)",
                        PEER_TCP_TIMEOUT.as_millis()
                    ),
                },
                None,
            );
        }
        Ok(Err(e)) => {
            // Connection refused / reset / etc. → daemon not running.
            // ConnectionRefused is the canonical signal; other IO
            // errors here (broken pipe, etc.) shouldn't normally
            // happen on connect but we treat them as "not-running"
            // because the OS-level layer rejected us before any
            // application logic ran.
            let message = if e.kind() == std::io::ErrorKind::ConnectionRefused {
                format!("port {port} not accepting connections")
            } else {
                format!("tcp connect to {addr} failed: {e}")
            };
            return (
                PeerStatusDto {
                    kind: "not-running".to_string(),
                    message,
                },
                None,
            );
        }
        Err(e) => {
            // DNS resolution failed entirely (host couldn't be
            // resolved). Classify as `unreachable` because the
            // daemon-state question can't be answered without
            // even an address to try.
            return (
                PeerStatusDto {
                    kind: "unreachable".to_string(),
                    message: format!("could not resolve {host}: {e}"),
                },
                None,
            );
        }
    }

    // Stage 2: Agent Card fetch. Process is running; question is
    // whether the well-known endpoint is healthy.
    let probe = client.discover_at(endpoint, override_url);
    match timeout(PEER_PROBE_TIMEOUT, probe).await {
        Ok(Ok(card)) => (
            PeerStatusDto {
                kind: "alive".to_string(),
                message: String::new(),
            },
            Some(excerpt_from(&card)),
        ),
        Ok(Err(e)) => (
            PeerStatusDto {
                kind: "unhealthy".to_string(),
                message: format!("agent-card fetch failed: {e}"),
            },
            None,
        ),
        Err(_) => (
            PeerStatusDto {
                kind: "unhealthy".to_string(),
                message: format!(
                    "agent-card fetch timed out after {} ms",
                    PEER_PROBE_TIMEOUT.as_millis()
                ),
            },
            None,
        ),
    }
}

fn excerpt_from(card: &AgentCard) -> AgentCardExcerptDto {
    let transport_protocol = match card.transport.protocol {
        TransportProtocol::HttpSse => "http+sse".to_string(),
        TransportProtocol::JsonRpc => "json-rpc".to_string(),
    };
    let (topology_role, topology_parent_id) = match card.topology.as_ref() {
        Some(t) => (
            t.role.map(|r| format!("{r:?}").to_lowercase()),
            t.parent_id.clone(),
        ),
        None => (None, None),
    };
    AgentCardExcerptDto {
        id: card.id.clone(),
        name: card.name.clone(),
        version: card.version.clone(),
        interface_version: card.interface_version.clone(),
        schema_version: card.schema_version.clone(),
        transport_protocol,
        topology_role,
        topology_parent_id,
    }
}

// =================================================================
// Phase 1.4 — Skills page
// =================================================================

/// `GET /api/skills` — inventory + hygiene of every skill the Brain
/// can route to under `.claude/skills/`. Pairs each skill with its
/// invocation-ledger stats (count, last-invoked, alive/dead/new).
async fn skills(State(state): State<AppState>) -> Response {
    // Resolve project_root from the registry path (registry lives at
    // `<project>/.claude/brain-registry.json`). We avoid loading a
    // full BrainContext here because the skills view is independent
    // of scoring — and BrainContext::load is the heavy path.
    let registry_path = std::path::Path::new(state.registry_path.as_str());
    let project_root = registry_path
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let now = chrono::Utc::now();
    let scan = scan_skills(&project_root, now);

    Json(SkillsResponse {
        skills: scan.skills,
        ledger_present: scan.ledger_present,
        total_invocations: scan.total_invocations,
        alive_window_days: ALIVE_WINDOW_DAYS as u32,
    })
    .into_response()
}

// =================================================================
// Phase 2.2 — Hat lens
// =================================================================

/// `GET /api/hats` — list every hat declared in the registry plus
/// a synthetic "default" entry the picker uses to represent the
/// un-hatted lens.
///
/// Reads only the registry — no scoring pipeline needed — so this
/// is one of the cheapest routes in the dashboard and never depends
/// on the hat itself.
async fn hats(State(state): State<AppState>) -> Response {
    let registry_text = match tokio::fs::read_to_string(state.registry_path.as_str()).await {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to read registry: {e}"),
                })),
            )
                .into_response();
        }
    };
    let registry: serde_json::Value = match serde_json::from_str(&registry_text) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("registry is not valid JSON: {e}"),
                })),
            )
                .into_response();
        }
    };

    let mut hats: Vec<HatDto> = vec![HatDto {
        name: "default".to_string(),
        description:
            "No hat — every domain weighted at 1.0× (the registry's authored weights apply unchanged)."
                .to_string(),
        is_default: true,
    }];

    if let Some(map) = registry
        .get("config")
        .and_then(|c| c.get("hats"))
        .and_then(|h| h.as_object())
    {
        let mut declared: Vec<HatDto> = map
            .iter()
            .map(|(name, body)| {
                let description = body
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                HatDto {
                    name: name.clone(),
                    description,
                    is_default: false,
                }
            })
            .collect();
        declared.sort_by(|a, b| a.name.cmp(&b.name));
        hats.extend(declared);
    }

    Json(HatsResponse { hats }).into_response()
}

// =================================================================
// Path 2 — Multi-Brain navigation
// =================================================================

/// `GET /api/brains` — every Brain reachable from the host's
/// federation tree. The frontend uses this to render the AppShell
/// Brain switcher and to redirect `/` to `/brains/<self_id>/`.
async fn brains_list(State(state): State<AppState>) -> Response {
    let brains: Vec<BrainListItemDto> = state
        .brains
        .list()
        .into_iter()
        .map(|e| BrainListItemDto {
            id: e.id.clone(),
            display_name: e.display_name.clone(),
            project_root: e.project_root.to_string_lossy().to_string(),
            parent_id: e.parent_id.clone(),
            depth: e.depth as u32,
        })
        .collect();
    Json(BrainsListResponse {
        self_id: state.brains.self_id.clone(),
        brains,
    })
    .into_response()
}

/// Resolve a `:brain_id` URL segment to a BrainEntry. Returns a
/// 404 response when the id is unknown — the frontend treats this
/// as "navigate the user back to the brain list."
fn resolve_brain<'a>(
    state: &'a AppState,
    brain_id: &str,
) -> Result<&'a BrainEntry, Response> {
    state.brains.get(brain_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("brain '{brain_id}' not found in this dashboard's federation tree"),
                "known_ids": state.brains.list().iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
            })),
        )
            .into_response()
    })
}

async fn brain_overview(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
    Query(query): Query<ScoreQuery>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let registry = brain.registry_path.to_string_lossy().to_string();
    let ctx = match state
        .cache
        .load_or_get(&registry, query.resolved_hat(), None)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to load BrainContext for '{brain_id}': {e:#}")
                })),
            )
                .into_response();
        }
    };
    Json(build_overview(&ctx)).into_response()
}

async fn brain_domains_list(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
    Query(query): Query<ScoreQuery>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let registry = brain.registry_path.to_string_lossy().to_string();
    let ctx = match state
        .cache
        .load_or_get(&registry, query.resolved_hat(), None)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to load BrainContext for '{brain_id}': {e:#}")
                })),
            )
                .into_response();
        }
    };
    Json(build_domains_list(&ctx)).into_response()
}

async fn brain_domain_detail(
    State(state): State<AppState>,
    AxumPath((brain_id, name)): AxumPath<(String, String)>,
    Query(query): Query<ScoreQuery>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let registry = brain.registry_path.to_string_lossy().to_string();
    let ctx = match state
        .cache
        .load_or_get(&registry, query.resolved_hat(), None)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to load BrainContext for '{brain_id}': {e:#}")
                })),
            )
                .into_response();
        }
    };
    match build_domain_detail(&ctx, &name) {
        Some(d) => Json(d).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("domain '{name}' not in '{brain_id}' registry"),
            })),
        )
            .into_response(),
    }
}

async fn brain_federation(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let registry = brain.registry_path.to_string_lossy().to_string();
    let ctx = match state
        .cache
        .load_or_get(&registry, None, None)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to load BrainContext for '{brain_id}': {e:#}")
                })),
            )
                .into_response();
        }
    };
    Json(build_federation(&ctx).await).into_response()
}

async fn brain_skills(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let now = chrono::Utc::now();
    let scan = scan_skills(&brain.project_root, now);
    Json(SkillsResponse {
        skills: scan.skills,
        ledger_present: scan.ledger_present,
        total_invocations: scan.total_invocations,
        alive_window_days: ALIVE_WINDOW_DAYS as u32,
    })
    .into_response()
}

async fn brain_hats(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    // Reuse the same logic as the legacy /api/hats handler but with
    // the resolved brain's registry path. Inline rather than calling
    // the legacy function so the brain_id-specific error message
    // surfaces correctly.
    let registry_text =
        match tokio::fs::read_to_string(brain.registry_path.as_path()).await {
            Ok(t) => t,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("failed to read registry for '{brain_id}': {e}"),
                    })),
                )
                    .into_response();
            }
        };
    let parsed: serde_json::Value = match serde_json::from_str(&registry_text) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("registry for '{brain_id}' is not valid JSON: {e}"),
                })),
            )
                .into_response();
        }
    };
    let mut hats: Vec<HatDto> = vec![HatDto {
        name: "default".to_string(),
        description:
            "No hat — every domain weighted at 1.0× (the registry's authored weights apply unchanged)."
                .to_string(),
        is_default: true,
    }];
    if let Some(map) = parsed
        .get("config")
        .and_then(|c| c.get("hats"))
        .and_then(|h| h.as_object())
    {
        let mut declared: Vec<HatDto> = map
            .iter()
            .map(|(name, body)| HatDto {
                name: name.clone(),
                description: body
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string(),
                is_default: false,
            })
            .collect();
        declared.sort_by(|a, b| a.name.cmp(&b.name));
        hats.extend(declared);
    }
    Json(HatsResponse { hats }).into_response()
}

/// `GET /api/brains/:id/dashboard-layout` — the per-brain custom
/// homepage layout. Returns the operator's saved
/// `dashboard-layout.json` when present, otherwise a posture-aware
/// default (gauge-centric for weighted Brains, child-card-centric
/// for all-advisory Brains with declared children).
async fn brain_dashboard_layout(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };

    // Saved layout takes precedence. The default is computed from
    // the registry's posture (weighted vs all-advisory).
    let layout = match read_layout(&brain.project_root) {
        Some(saved) => DashboardLayoutResponse {
            // Override brain_id in case the file was hand-copied
            // from another brain. The URL path is the source of
            // truth.
            brain_id: brain_id.clone(),
            ..saved
        },
        None => {
            // Need the registry to determine posture. Read it
            // directly rather than load the full BrainContext —
            // posture detection only needs domain_weights.
            let registry_text =
                match tokio::fs::read_to_string(brain.registry_path.as_path()).await {
                    Ok(t) => t,
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({
                                "error": format!("failed to read registry for '{brain_id}': {e}")
                            })),
                        )
                            .into_response();
                    }
                };
            let registry = match neurogrim_core::registry::BrainRegistry::from_json(&registry_text) {
                Ok(r) => r,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "error": format!("failed to parse registry for '{brain_id}': {e}")
                        })),
                    )
                        .into_response();
                }
            };
            default_layout_for(&brain_id, &registry)
        }
    };

    Json(layout).into_response()
}

/// `PUT /api/brains/:id/dashboard-layout` — save a custom layout.
/// First mutation endpoint in v3.4. Layout edits are operator
/// preference, not Brain state, so this is *not* gated behind the
/// `--allow-mutations` flag we're reserving for v3.5 score/CMDB
/// mutations. Writes atomically (temp file + rename) so a
/// concurrent reader never sees a half-written file.
///
/// Fires a `LayoutChanged` SSE event explicitly after a successful
/// write. The filesystem watcher will also fire one within ~250ms,
/// but pushing immediately means the operator's own browser sees
/// the change without waiting on the watcher debounce.
async fn brain_save_dashboard_layout(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
    Json(body): Json<DashboardLayoutRequest>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    if let Err(e) = save_layout(&brain.project_root, body.widgets) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("failed to save layout for '{brain_id}': {e}")
            })),
        )
            .into_response();
    }
    if let Some(tx) = &state.events {
        let _ = tx.send(crate::events::DashboardEvent::LayoutChanged);
    }
    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}

/// `DELETE /api/brains/:id/dashboard-layout` — reset to the
/// posture-aware default by removing the on-disk file. Idempotent:
/// returns 200 OK whether or not a file existed.
async fn brain_reset_dashboard_layout(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let removed = match reset_layout(&brain.project_root) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to reset layout for '{brain_id}': {e}")
                })),
            )
                .into_response();
        }
    };
    if removed {
        if let Some(tx) = &state.events {
            let _ = tx.send(crate::events::DashboardEvent::LayoutChanged);
        }
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({ "ok": true, "removed": removed })),
    )
        .into_response()
}

// =================================================================
// Phase 2.1 — SSE live updates
// =================================================================

/// `GET /api/events` — Server-Sent Events stream of dashboard events.
///
/// One subscription per connection. Events are JSON-encoded into the
/// SSE `data:` field; clients parse with `JSON.parse(event.data)` and
/// invalidate the relevant TanStack Query keys.
///
/// When the watcher failed to start (or the `AppState` was built
/// without events), the endpoint returns a single `data: disabled`
/// keepalive and stays open — clients fall back to polling but their
/// EventSource doesn't reconnect-loop.
async fn events_sse(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream: futures::stream::BoxStream<'static, Result<Event, Infallible>> =
        match state.events {
            Some(tx) => {
                let rx = tx.subscribe();
                BroadcastStream::new(rx)
                    .filter_map(|res| async move {
                        let de = res.ok()?;
                        let json = serde_json::to_string(&de).ok()?;
                        Some(Ok(Event::default().data(json)))
                    })
                    .boxed()
            }
            None => futures::stream::once(async {
                Ok(Event::default().data("\"disabled\""))
            })
            .chain(futures::stream::pending::<Result<Event, Infallible>>())
            .boxed(),
        };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Serve embedded frontend assets; fall back to `index.html` for
/// client-side routing (TanStack Router uses pushState; refreshing
/// `/domains/foo` should serve `index.html` and let the frontend
/// resolve the route).
async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/').to_string();
    let path = if path.is_empty() { "index.html".to_string() } else { path };

    if let Some(content) = FrontendAssets::get(&path) {
        let mime = guess_mime(&path);
        return ([(header::CONTENT_TYPE, mime)], content.data).into_response();
    }

    // SPA fallback: any unknown path serves index.html so client
    // routing works on refresh / direct-link.
    if let Some(index) = FrontendAssets::get("index.html") {
        return (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            index.data,
        )
            .into_response();
    }

    // Phase 0 fallback when no frontend has been built yet — the
    // boot story stays debuggable. Once `npm run build` produces a
    // bundle, this branch is unreachable.
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>NeuroGrim Dashboard — boot</title></head>
<body style="font-family: system-ui; max-width: 40em; margin: 4em auto; line-height: 1.5;">
  <h1>NeuroGrim Dashboard</h1>
  <p>The Rust server is running, but no frontend bundle has been embedded
     yet. The dashboard is in Phase 0 of its v3.4 boot — Rust API works,
     UI is next.</p>
  <p>Try the API: <a href="/api/health"><code>/api/health</code></a></p>
</body></html>"#,
    )
        .into_response()
}

/// Tiny MIME-type guesser covering the file types a Vite build emits.
/// Avoids pulling in the full `mime_guess` crate (~100 KB) for ten
/// extensions.
fn guess_mime(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "application/javascript; charset=utf-8"
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if lower.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else if lower.ends_with(".woff2") {
        "font/woff2"
    } else if lower.ends_with(".woff") {
        "font/woff"
    } else if lower.ends_with(".map") {
        "application/json; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        AppState::new(".claude/brain-registry.json".to_string())
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = router(test_state());
        let req = Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["ok"], true);
        assert!(v["version"].is_string());
    }

    #[tokio::test]
    async fn unknown_path_falls_back_to_html() {
        let app = router(test_state());
        let req = Request::builder()
            .uri("/some/spa/route")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.starts_with("text/html"), "got content-type: {ct}");
    }

    #[test]
    fn guess_mime_covers_common_vite_outputs() {
        assert_eq!(guess_mime("index.html"), "text/html; charset=utf-8");
        assert_eq!(guess_mime("assets/main-abc123.js"), "application/javascript; charset=utf-8");
        assert_eq!(guess_mime("assets/index.css"), "text/css; charset=utf-8");
        assert_eq!(guess_mime("favicon.svg"), "image/svg+xml");
        assert_eq!(guess_mime("font.woff2"), "font/woff2");
    }

    #[test]
    fn score_query_resolves_default_and_empty_to_none() {
        assert_eq!(ScoreQuery { hat: None }.resolved_hat(), None);
        assert_eq!(ScoreQuery { hat: Some("".to_string()) }.resolved_hat(), None);
        assert_eq!(
            ScoreQuery { hat: Some("   ".to_string()) }.resolved_hat(),
            None
        );
        assert_eq!(
            ScoreQuery { hat: Some("default".to_string()) }.resolved_hat(),
            None
        );
        assert_eq!(
            ScoreQuery { hat: Some("DEFAULT".to_string()) }.resolved_hat(),
            None
        );
    }

    #[test]
    fn score_query_passes_through_explicit_hat() {
        assert_eq!(
            ScoreQuery {
                hat: Some("engineer".to_string())
            }
            .resolved_hat(),
            Some("engineer".to_string())
        );
        // Whitespace tolerance — pickers can submit values that
        // came from a select element with surrounding whitespace.
        assert_eq!(
            ScoreQuery {
                hat: Some("  reviewer ".to_string())
            }
            .resolved_hat(),
            Some("reviewer".to_string())
        );
    }

    #[tokio::test]
    async fn hats_route_returns_default_plus_declared_hats() {
        let tmp = tempfile::tempdir().unwrap();
        let registry_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&registry_dir).unwrap();
        let registry_path = registry_dir.join("brain-registry.json");
        let registry = serde_json::json!({
            "meta": {
                "schema_version": "2",
                "description": "test",
                "updated_by": "test"
            },
            "tools": {},
            "data_sources": {},
            "config": {
                "domain_weights": {"d": 1.0},
                "domain_definitions": {
                    "d": { "principle": "x", "scoring_source": null, "exported_variables": {} }
                },
                "hats": {
                    "engineer": { "description": "Active dev work" },
                    "reviewer": { "description": "Code review" }
                }
            }
        });
        std::fs::write(&registry_path, registry.to_string()).unwrap();

        let state = AppState::new(registry_path.to_string_lossy().to_string());
        let app = router(state);
        let req = Request::builder()
            .uri("/api/hats")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let hats = v["hats"].as_array().unwrap();
        // 1 default + 2 declared, sorted alphabetically: default,
        // engineer, reviewer.
        assert_eq!(hats.len(), 3);
        assert_eq!(hats[0]["name"], "default");
        assert_eq!(hats[0]["is_default"], true);
        assert_eq!(hats[1]["name"], "engineer");
        assert_eq!(hats[1]["description"], "Active dev work");
        assert_eq!(hats[2]["name"], "reviewer");
    }

    #[tokio::test]
    async fn hats_route_returns_default_only_when_registry_has_no_hats() {
        let tmp = tempfile::tempdir().unwrap();
        let registry_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&registry_dir).unwrap();
        let registry_path = registry_dir.join("brain-registry.json");
        std::fs::write(
            &registry_path,
            r#"{"meta":{"schema_version":"2","description":"t","updated_by":"t"},
                "tools":{},"data_sources":{},
                "config":{"domain_weights":{"d":1.0},
                  "domain_definitions":{"d":{"principle":"x","scoring_source":null,"exported_variables":{}}}}}"#,
        )
        .unwrap();

        let state = AppState::new(registry_path.to_string_lossy().to_string());
        let app = router(state);
        let req = Request::builder()
            .uri("/api/hats")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let hats = v["hats"].as_array().unwrap();
        assert_eq!(hats.len(), 1);
        assert_eq!(hats[0]["name"], "default");
    }

    #[tokio::test]
    async fn events_endpoint_returns_event_stream_content_type() {
        // Smoke-check that /api/events responds with the SSE
        // content-type. We don't pull an event off the stream here —
        // the response body is open-ended by design and there's no
        // sender feeding it in this test setup. The classification
        // + watcher behavior is exercised in the events.rs unit
        // tests.
        let state = AppState::new(".claude/brain-registry.json".to_string());
        let app = router(state);
        let req = Request::builder()
            .uri("/api/events")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            ct.starts_with("text/event-stream"),
            "got content-type: {ct}"
        );
    }

    #[tokio::test]
    async fn skills_returns_empty_when_no_skills_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let registry_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&registry_dir).unwrap();
        let registry_path = registry_dir.join("brain-registry.json");
        // Skills route doesn't load BrainContext, but it derives
        // project_root from the registry path's parent's parent. The
        // file doesn't have to be valid for the skills route — but
        // we write a placeholder so the path resolution lands in a
        // real directory.
        std::fs::write(&registry_path, "{}").unwrap();

        let state = AppState::new(registry_path.to_string_lossy().to_string());
        let app = router(state);
        let req = Request::builder()
            .uri("/api/skills")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["skills"], serde_json::json!([]));
        assert_eq!(v["ledger_present"], false);
        assert_eq!(v["total_invocations"], 0);
        assert_eq!(v["alive_window_days"], 30);
    }

    #[tokio::test]
    async fn probe_peer_classifies_closed_port_as_not_running() {
        // Bind a TcpListener to grab an unused port, then drop it
        // immediately so the port is closed when probe_peer runs.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let client = TaskClient::new_http();
        let endpoint =
            Url::parse(&format!("http://127.0.0.1:{port}/a2a/v1/")).unwrap();
        let (status, agent_card) = probe_peer(&client, &endpoint, None).await;
        assert_eq!(
            status.kind, "not-running",
            "closed port should classify as not-running, not '{}' (msg: {})",
            status.kind, status.message
        );
        assert!(
            status.message.contains(&port.to_string()),
            "message should mention the port for operator clarity: {}",
            status.message
        );
        assert!(agent_card.is_none());
    }

    #[tokio::test]
    async fn probe_peer_classifies_open_port_with_no_handler_as_unhealthy() {
        // Bind a listener and KEEP it bound — but never accept any
        // connection. The TCP precheck will succeed (the kernel
        // accepts incoming connections via the listen backlog), but
        // the Agent Card fetch will time out because nothing reads
        // from the socket. This is the canonical "process is up but
        // not serving the well-known endpoint" signal.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        // Hold the listener; never `accept()`. Drop happens at
        // end of scope after the test body completes.

        let client = TaskClient::new_http();
        let endpoint =
            Url::parse(&format!("http://127.0.0.1:{port}/a2a/v1/")).unwrap();
        let (status, agent_card) = probe_peer(&client, &endpoint, None).await;
        assert_eq!(
            status.kind, "unhealthy",
            "open port with no handler should classify as unhealthy, not '{}'",
            status.kind
        );
        assert!(agent_card.is_none());
        // Keep listener alive until after the probe completes.
        drop(listener);
    }

    #[tokio::test]
    async fn federation_returns_self_with_empty_peers_when_no_children_block() {
        // Build a minimal valid registry on disk that has no
        // `config.children` block — exercises the empty-federation
        // path without needing to spin up live peer servers.
        let tmp = tempfile::tempdir().unwrap();
        let registry_path = tmp.path().join("brain-registry.json");
        // Registry must declare at least one domain to satisfy
        // BrainRegistry::validate(). One stub-domain is enough; the
        // route doesn't read scores in the empty-peers branch.
        let registry = serde_json::json!({
            "meta": {
                "schema_version": "2",
                "description": "smoke test registry",
                "updated_by": "test"
            },
            "tools": {},
            "data_sources": {},
            "config": {
                "domain_weights": {"placeholder": 0.0},
                "domain_definitions": {
                    "placeholder": {
                        "principle": "Placeholder domain for testing",
                        "scoring_source": null,
                        "exported_variables": {}
                    }
                }
            }
        });
        std::fs::write(&registry_path, registry.to_string()).unwrap();

        let state = AppState::new(registry_path.to_string_lossy().to_string());
        let app = router(state);
        let req = Request::builder()
            .uri("/api/federation")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let body_text = String::from_utf8_lossy(&body);
        assert_eq!(status, StatusCode::OK, "body was: {body_text}");
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(v["self_brain"]["label"].is_string());
        assert_eq!(v["peers"], serde_json::json!([]));
        assert_eq!(v["registry_schema_version"], "2");
    }
}
