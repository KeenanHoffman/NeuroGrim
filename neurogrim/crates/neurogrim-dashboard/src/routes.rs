//! HTTP route definitions.
//!
//! Phase 0.3: `/api/health` + static-asset fallback.
//! Phase 1: per-page endpoints (`/api/agent`, `/api/domains`,
//! `/api/domains/:name`, `/api/federation`, `/api/skills`).
//! Phase 2: `/api/events` (SSE live updates).

use axum::{
    extract::{Path as AxumPath, State},
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use neurogrim_a2a::agent_card::{AgentCard, TransportProtocol};
use neurogrim_a2a::TaskClient;
use neurogrim_core::trajectory::compute_trajectory;
use neurogrim_core::types::{ScoreSnapshot, TrajectoryClassification};
use neurogrim_mcp::context::BrainContext;
use neurogrim_mcp::prose::first_sentence;
use rust_embed::RustEmbed;
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;
use url::Url;

use crate::state::AppState;
use crate::types::{
    AgentCardExcerptDto, DomainDetailResponse, DomainListItemDto, DomainSignalDto,
    DomainsListResponse, FederationResponse, FindingDto, HealthResponse, HistoryPointDto,
    OverviewResponse, PeerDto, PeerStatusDto, RecommendationDto, SelfBrainDto,
};

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
        .route("/api/overview", get(overview))
        .route("/api/domains", get(domains_list))
        .route("/api/domains/:name", get(domain_detail))
        .route("/api/federation", get(federation))
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
/// pipeline run) on every call; Phase 2.1 will add caching with
/// SSE-driven invalidation.
async fn overview(State(state): State<AppState>) -> Response {
    let ctx = match BrainContext::load(&state.registry_path, None, None).await {
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
async fn domains_list(State(state): State<AppState>) -> Response {
    let ctx = match BrainContext::load(&state.registry_path, None, None).await {
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
) -> Response {
    let ctx = match BrainContext::load(&state.registry_path, None, None).await {
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
    let ctx = match BrainContext::load(&state.registry_path, None, None).await {
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

/// Run a single Agent Card probe with the standard timeout. Maps any
/// failure to a `unreachable` status with a short operator-facing
/// message; never panics.
async fn probe_peer(
    client: &TaskClient<neurogrim_a2a::transport::HttpSseTransport>,
    endpoint: &Url,
    override_url: Option<&Url>,
) -> (PeerStatusDto, Option<AgentCardExcerptDto>) {
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
                kind: "unreachable".to_string(),
                message: format!("{e}"),
            },
            None,
        ),
        Err(_) => (
            PeerStatusDto {
                kind: "unreachable".to_string(),
                message: format!("timeout after {} ms", PEER_PROBE_TIMEOUT.as_millis()),
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
