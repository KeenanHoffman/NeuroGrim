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
use crate::events::DashboardEvent;
use crate::layout::{
    default_layout_for, read_layout, reset_layout, save_layout, DashboardLayoutRequest,
    DashboardLayoutResponse,
};
use crate::services::{
    broadcast_event as broadcast_service_event, ServiceErrorDto, ServiceHandle,
    ServicesListResponse, StartPeerResponse, StopPeerResponse,
};
use crate::skills::{scan as scan_skills, ALIVE_WINDOW_DAYS};
use crate::state::AppState;
use crate::types::{
    AgentCardExcerptDto, ApprovalRequestView, ApprovalResolutionView, ApprovalsPageResponse,
    BrainListItemDto, BrainsListResponse, ConfigFileResponse, CreateCustomPageRequest,
    CustomPageLayoutRequest, CustomPageMutationResponse, DomainDetailResponse,
    DomainListItemDto, DomainSignalDto, DomainsListResponse, FederationResponse, FindingDto,
    HatDto, HatsResponse, HealthResponse, HistoryPointDto, OverviewResponse, PeerDto,
    PeerStatusDto, PublishGateLedgerView, PublishGateView, PublishGatesPageResponse,
    ExplainTopicResponse, QueueMessageDto, QueuePublishRequest, QueuePublishResponse,
    QueueReadResponse, QueueTopicStatsDto, QueuesListResponse, RecommendationDto,
    RegistryResponse, RegistryUpdateRequest, ResolveApprovalRequest, ResolveApprovalResponse,
    SelfBrainDto, SkillsResponse, TlsStatusResponse,
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

// ── Tiny shared helpers ─────────────────────────────────────────────────

/// Build a `500 Internal Server Error` response with the standard
/// `{"error": "<msg>"}` body. Collapses the otherwise-repeated
/// `(StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": ...})))
/// .into_response()` shape used across handlers.
fn internal_error(msg: impl Into<String>) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": msg.into() })),
    )
        .into_response()
}

/// Read a JSON file, stripping any UTF-8 BOM, into a `serde_json::Value`.
/// Returns `None` on read or parse failure (non-fatal — callers project
/// optional fields out of the value with their own `.and_then` chains).
fn read_json_value(path: &Path) -> Option<serde_json::Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(s.trim_start_matches('\u{FEFF}')).ok())
}

/// Load the host Brain's context and run an async build closure.
/// Collapses the cache-error scaffolding shared by every legacy
/// single-Brain endpoint (`overview`, `domains_list`,
/// `domain_detail`, `federation`).
///
/// The closure takes the loaded context as `Arc<BrainContext>` (by
/// value) so it can hold it across `.await` points without lifetime
/// gymnastics. Inside the closure, deref the Arc with `&ctx` —
/// auto-deref-coercion gives `&BrainContext` for the build_* fns.
/// Sync build paths still need `async move { ... }` to satisfy the
/// `Future` bound.
async fn with_host_context<F, Fut, R>(
    state: &AppState,
    hat: Option<String>,
    build: F,
) -> Response
where
    F: FnOnce(std::sync::Arc<BrainContext>) -> Fut,
    Fut: std::future::Future<Output = R>,
    R: IntoResponse,
{
    // V5-FOUND-1 Phase 3 step 5: dashboard route convergence span.
    // Span name `dashboard.route` is mapped to EventKind::DashboardRoute.
    // Schema-allowed extras for kind=dashboard_route are
    // {route_name, status_code} — never the request body, never the
    // response body. route_name is left Empty in v1; per-route
    // granularity (`overview`, `domains_list`, etc.) is a v5.5
    // follow-on once we have signal that route-level timing matters.
    let dashboard_span = tracing::info_span!(
        "dashboard.route",
        route_name = tracing::field::Empty,
        status_code = tracing::field::Empty,
    );
    let _entered = dashboard_span.enter();

    let response = match state
        .cache
        .load_or_get(&state.registry_path, hat, None)
        .await
    {
        Ok(ctx) => build(ctx).await.into_response(),
        Err(e) => internal_error(format!("failed to load BrainContext: {e:#}")),
    };
    dashboard_span.record("status_code", response.status().as_u16() as i64);
    response
}

/// Resolve a Brain by id, load its context, and run an async build
/// closure. Federation-aware analogue of `with_host_context`.
async fn with_brain_context<F, Fut, R>(
    state: &AppState,
    brain_id: &str,
    hat: Option<String>,
    build: F,
) -> Response
where
    F: FnOnce(std::sync::Arc<BrainContext>) -> Fut,
    Fut: std::future::Future<Output = R>,
    R: IntoResponse,
{
    // V5-FOUND-1 Phase 3 step 5: dashboard route convergence span
    // (federation-aware analogue of with_host_context's
    // instrumentation). Same extras posture: status_code captured;
    // route_name + brain_id deferred to per-handler v5.5.
    let dashboard_span = tracing::info_span!(
        "dashboard.route",
        route_name = tracing::field::Empty,
        status_code = tracing::field::Empty,
    );
    let _entered = dashboard_span.enter();

    let brain = match resolve_brain(state, brain_id) {
        Ok(b) => b,
        Err(r) => {
            dashboard_span.record("status_code", r.status().as_u16() as i64);
            return r;
        }
    };
    let registry = brain.registry_path.to_string_lossy().to_string();
    let response = match state.cache.load_or_get(&registry, hat, None).await {
        Ok(ctx) => build(ctx).await.into_response(),
        Err(e) => internal_error(format!(
            "failed to load BrainContext for '{brain_id}': {e:#}"
        )),
    };
    dashboard_span.record("status_code", response.status().as_u16() as i64);
    response
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
        // v4.2 S14-S-4.5 v3 — TLS status for the Secrets page's
        // "switch to HTTPS" banner + browser TOFU pinning.
        // Public values; safe on both HTTP and HTTPS listeners.
        .route("/api/tls-status", get(tls_status))
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
        // ---- v3.5.0 service lifecycle (gated by --allow-mutations) ----
        .route(
            "/api/brains/:brain_id/peers/:peer/start",
            axum::routing::post(brain_peer_start),
        )
        .route(
            "/api/brains/:brain_id/peers/:peer/stop",
            axum::routing::post(brain_peer_stop),
        )
        // v4.3 S15-C-2 expansion: trailing lines from the peer's
        // spawned-process log file. Read-only; bounded by
        // PEER_LOG_TAIL_BYTES so a huge log doesn't OOM the response.
        .route(
            "/api/brains/:brain_id/peers/:peer/log",
            get(brain_peer_log),
        )
        // Read-only: list services tracked by this dashboard.
        .route("/api/brains/:brain_id/services", get(brain_services))
        // Read-only: surface the project's port allocation for the
        // v3.5.0 ports-panel widget.
        .route("/api/brains/:brain_id/ports", get(brain_ports))
        // v4.0 S12-G-6: read-only publish-gates page. Joins the
        // manifest with the ledger so the page renders current state
        // per gate + a recent-activity timeline in one fetch.
        .route(
            "/api/brains/:brain_id/publish-gates",
            get(brain_publish_gates),
        )
        // v4.1 S13-B-2: agent coordination bus. Topic-aware
        // wildcard suffix (`/*topic`) so path segments can carry
        // slashes (e.g. `_neurogrim/approvals` topic). The list
        // endpoint at `/queues` (no trailing path) is distinct.
        .route("/api/brains/:brain_id/queues", get(brain_queues_list))
        .route(
            "/api/brains/:brain_id/queues/*rest",
            get(brain_queue_read_or_events).post(brain_queue_publish),
        )
        // v4.1 S13-B-6: autonomy approvals page. Joins the pending
        // requests on `_neurogrim/approvals` with the recently-
        // resolved entries on `_neurogrim/approval-resolutions`.
        .route(
            "/api/brains/:brain_id/approvals",
            get(brain_approvals_list),
        )
        // v4.3 S15-C-5: read-only config-file viewer for the
        // Settings page. Hardcoded allowlist (culture.yaml,
        // queue-config.yaml) keeps the surface tight.
        .route(
            "/api/brains/:brain_id/config-file/:name",
            get(brain_config_file),
        )
        // v4.3 S15-C-4 v1: registry editor. GET returns the full
        // registry JSON + ETag; PUT validates + atomically writes
        // the replacement (rejects with 409 on ETag mismatch).
        // Gated by --allow-mutations.
        .route(
            "/api/brains/:brain_id/registry",
            get(brain_registry_get).put(brain_registry_put),
        )
        // v4.3 S15-C-6 v1: custom-pages CRUD. GET returns the full
        // multi-page config (with v3.4 backward-compat read).
        // POST creates a new custom page (kebab-case validated;
        // collisions rejected). DELETE removes a custom page.
        // Both gated by --allow-mutations.
        .route(
            "/api/brains/:brain_id/dashboard-pages",
            get(brain_dashboard_pages_get),
        )
        .route(
            "/api/brains/:brain_id/dashboard-pages/:name",
            axum::routing::post(brain_dashboard_pages_create)
                .delete(brain_dashboard_pages_delete),
        )
        // v4.3 S15-C-6 v2: per-custom-page widget layout save. Lets
        // operators compose their own dashboards through the UI rather
        // than hand-editing dashboard-pages.json. Body matches the
        // v3.4 dashboard-layout PUT (a list of WidgetSpec); the URL
        // scopes the write to a single page in the v2 config.
        .route(
            "/api/brains/:brain_id/dashboard-pages/:name/layout",
            axum::routing::put(brain_dashboard_pages_save_layout),
        )
        // v4.3 S15-C-8 v1: explain topic content for the inline-help
        // HelpIcon. Brain-agnostic (the topics ship with the binary,
        // not per-brain), so the route doesn't carry a brain_id.
        .route("/api/explain/:topic", get(explain_topic))
        // v4.3 S15-C-2 v2: invocation-ledger source for the Logs
        // page. Returns the most recent N entries newest-first;
        // accepts `?limit=N` (default 50, max 500).
        .route(
            "/api/brains/:brain_id/logs/invocation-ledger",
            get(brain_logs_invocation_ledger),
        )
        // v4.3 S15-C-3 expansion: score-history source for the Logs
        // page. Same shape as invocation-ledger — newest-first, with
        // per-snapshot delta computed against the chronologically-
        // prior entry.
        .route(
            "/api/brains/:brain_id/logs/score-history",
            get(brain_logs_score_history),
        )
        // v4.3 S15-C-3 expansion follow-on: services.jsonl source
        // for the Logs page (the 6th and final source). The on-disk
        // ledger is appended by the start/stop/readiness-watcher
        // handlers; this endpoint reads it.
        .route(
            "/api/brains/:brain_id/logs/services",
            get(brain_logs_services),
        )
        // v4.2 S14-S-6 v1: secrets-management UI surface. List
        // is read-only; set + delete gated behind
        // --allow-mutations. Values flow over the HTTPS listener
        // (S14-S-4.5 v2) when cert files are present; values
        // are NEVER returned by any endpoint.
        .route(
            "/api/brains/:brain_id/secrets",
            get(brain_secrets_list),
        )
        .route(
            "/api/brains/:brain_id/secrets/:secret_id",
            axum::routing::post(brain_secrets_set)
                .delete(brain_secrets_delete),
        )
        .route(
            "/api/brains/:brain_id/approvals/:action_id/resolve",
            axum::routing::post(brain_approvals_resolve),
        )
        // ---- v4.5 Plumbing page (TSDB observability) ----
        .route(
            "/api/brains/:brain_id/plumbing/overview",
            get(plumbing_overview),
        )
        .route(
            "/api/brains/:brain_id/plumbing/metrics/series",
            get(plumbing_list_series),
        )
        .route(
            "/api/brains/:brain_id/plumbing/metrics/:name",
            get(plumbing_query_series),
        )
        // ---- Live updates ----
        .route("/api/events", get(events_sse))
        .fallback(static_handler)
        // v4.5 — universal request-duration instrumentation. Records
        // `request_duration_ms{path, status}` for every handler.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::instrumentation::record_request_duration,
        ))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        registry_path: state.registry_path.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        mutations_allowed: state.mutations_allowed,
    })
}

/// `GET /api/tls-status` — v4.2 S14-S-4.5 v3.
///
/// Returns whether HTTPS is bound on this dashboard + the cert
/// fingerprint operators pin in localStorage. Both listeners
/// serve this endpoint (the values are public). Frontend reads
/// it on the Secrets page to drive the "switch to HTTPS" banner
/// and the TOFU comparison.
async fn tls_status(State(state): State<AppState>) -> Json<TlsStatusResponse> {
    // The dashboard tracks HTTPS availability + port at startup
    // (the constructor probes for cert files). The fingerprint
    // is recomputed on each call so a `tls-cert rotate` between
    // requests is visible to the frontend without a server
    // restart.
    let project_root = derive_project_root_from_registry(&state.registry_path);
    let fingerprint = crate::tls_serve::cert_fingerprint(&project_root);
    Json(TlsStatusResponse {
        https_available: state.https_port.is_some(),
        https_port: state.https_port,
        fingerprint_sha256: fingerprint,
    })
}

/// Derive the project root from the registry path string. Same
/// rule as `lib.rs::serve()` — registry path is
/// `<project>/.claude/brain-registry.json`, parent.parent gives
/// the project root.
fn derive_project_root_from_registry(registry_path: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(registry_path);
    p.parent()
        .and_then(std::path::Path::parent)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

/// `GET /api/overview` — landing-page summary for the Phase 1.1
/// Overview page. Loads a fresh `BrainContext` (registry + scoring
/// pipeline run) on every call.
async fn overview(
    State(state): State<AppState>,
    Query(query): Query<ScoreQuery>,
) -> Response {
    with_host_context(&state, query.resolved_hat(), |ctx| async move {
        Json(build_overview(&ctx))
    })
    .await
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
    with_host_context(&state, query.resolved_hat(), |ctx| async move {
        Json(build_domains_list(&ctx))
    })
    .await
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
                .and_then(|rel| read_json_value(&ctx.project_root.join(rel)))
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
    with_host_context(&state, query.resolved_hat(), |ctx| async move {
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
    })
    .await
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

    let cmdb_json: Option<serde_json::Value> = read_json_value(&cmdb_full);

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
    let metrics = state.metrics.clone();
    with_host_context(&state, None, |ctx| async move {
        Json(build_federation(&ctx, metrics.as_ref()).await)
    })
    .await
}

async fn build_federation(
    ctx: &BrainContext,
    metrics: Option<&neurogrim_core::metrics::MetricsHandle>,
) -> FederationResponse {
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
            // the page render. Records `peer_probe_ms{peer, outcome}`
            // so federation reachability becomes a time-series.
            match endpoint_str.as_deref().and_then(|s| Url::parse(s).ok()) {
                Some(endpoint) => {
                    let probe_start = std::time::Instant::now();
                    let result =
                        probe_peer(&client, &endpoint, agent_card_url.as_ref()).await;
                    if let Some(m) = metrics {
                        let duration_ms = probe_start.elapsed().as_secs_f64() * 1000.0;
                        let tags = neurogrim_core::metrics::Tags::new()
                            .with("peer", name.as_str())
                            .with("outcome", result.0.kind.as_str());
                        m.record("peer_probe_ms", &tags, duration_ms);
                    }
                    result
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
    with_brain_context(&state, &brain_id, query.resolved_hat(), |ctx| async move {
        Json(build_overview(&ctx))
    })
    .await
}

async fn brain_domains_list(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
    Query(query): Query<ScoreQuery>,
) -> Response {
    with_brain_context(&state, &brain_id, query.resolved_hat(), |ctx| async move {
        Json(build_domains_list(&ctx))
    })
    .await
}

async fn brain_domain_detail(
    State(state): State<AppState>,
    AxumPath((brain_id, name)): AxumPath<(String, String)>,
    Query(query): Query<ScoreQuery>,
) -> Response {
    let brain_id_for_msg = brain_id.clone();
    with_brain_context(&state, &brain_id, query.resolved_hat(), |ctx| async move {
        match build_domain_detail(&ctx, &name) {
            Some(d) => Json(d).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("domain '{name}' not in '{brain_id_for_msg}' registry"),
                })),
            )
                .into_response(),
        }
    })
    .await
}

async fn brain_federation(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let metrics = state.metrics.clone();
    with_brain_context(&state, &brain_id, None, |ctx| async move {
        Json(build_federation(&ctx, metrics.as_ref()).await)
    })
    .await
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
    // S15-C-7 v2: snapshot before/after to compute the keypath
    // diff. Read the existing layout (None when this is a first
    // save), serialize both shapes to Value, then save. Capturing
    // before the save keeps the diff snapshot consistent with what
    // got written.
    let before_value = read_layout(&brain.project_root)
        .map(|saved| serde_json::to_value(saved.widgets).unwrap_or(serde_json::Value::Null))
        .unwrap_or(serde_json::Value::Null);
    let after_value =
        serde_json::to_value(&body.widgets).unwrap_or(serde_json::Value::Null);
    let widget_count = body.widgets.len();
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
    let diff = crate::json_diff::diff(&before_value, &after_value);
    emit_config_change_with_diff(
        &state,
        &brain_id,
        "layout_change",
        format!("dashboard-layout saved with {widget_count} widget(s)"),
        &brain.project_root,
        if diff.is_empty() { None } else { Some(diff) },
    )
    .await;
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
        // S15-C-7: emit on the config-changes queue.
        emit_config_change(
            &state,
            &brain_id,
            "layout_change",
            "dashboard-layout reset to posture-aware default",
            &brain.project_root,
        )
        .await;
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({ "ok": true, "removed": removed })),
    )
        .into_response()
}

// =================================================================
// v3.5.0 — service lifecycle + ports panel
// =================================================================

/// Returns 403 when mutations are disabled. Mutation handlers call
/// this at the top so the gate is applied uniformly + the error
/// shape matches the documented contract.
fn require_mutations(state: &AppState) -> Result<(), Response> {
    if state.mutations_allowed {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(ServiceErrorDto::new(
                "mutations-disabled",
                "dashboard started without --allow-mutations; restart with the flag to enable",
            )),
        )
            .into_response())
    }
}

/// Resolved info about a peer declared in some Brain's
/// `config.children`. The `brain_path` is absolutized against the
/// parent's project_root so callers can hand it to `tokio::process::
/// Command` without further canonicalization.
struct PeerLookup {
    peer_name: String,
    brain_path: std::path::PathBuf,
}

/// Walk the parent's registry's `config.children` to find a peer
/// declaration matching `peer_name`, then resolve its brain_path
/// against the parent's project_root.
async fn resolve_peer(
    state: &AppState,
    brain_id: &str,
    peer_name: &str,
) -> Result<PeerLookup, Response> {
    let brain = resolve_brain(state, brain_id)?;
    let registry_path = brain.registry_path.to_string_lossy().to_string();
    let ctx = state
        .cache
        .load_or_get(&registry_path, None, None)
        .await
        .map_err(|e| {
            internal_error(format!(
                "failed to load BrainContext for '{brain_id}': {e:#}"
            ))
        })?;

    let children = ctx
        .registry
        .config
        .extra
        .get("children")
        .and_then(|v| v.as_object())
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ServiceErrorDto::new(
                    "peer-not-found",
                    format!("brain '{brain_id}' has no config.children block"),
                )),
            )
                .into_response()
        })?;
    let body = children.get(peer_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ServiceErrorDto::new(
                "peer-not-found",
                format!(
                    "peer '{peer_name}' not found in '{brain_id}'s config.children"
                ),
            )),
        )
            .into_response()
    })?;
    let brain_path_str = body.get("brain_path").and_then(|v| v.as_str()).ok_or_else(|| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ServiceErrorDto::new(
                "spawn-failed",
                format!("peer '{peer_name}' has no brain_path field"),
            )),
        )
            .into_response()
    })?;
    let candidate = std::path::PathBuf::from(brain_path_str);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        brain.project_root.join(&candidate)
    };
    Ok(PeerLookup {
        peer_name: peer_name.to_string(),
        brain_path: absolute,
    })
}

/// `POST /api/brains/:id/peers/:peer/start` — spawn the peer's A2A
/// service as a child process of the dashboard. Returns 202 Accepted
/// with the optimistic state; the readiness watcher emits
/// ServiceStarted (or ServiceFailed) within ~5s.
async fn brain_peer_start(
    State(state): State<AppState>,
    AxumPath((brain_id, peer_name)): AxumPath<(String, String)>,
) -> Response {
    if let Err(r) = require_mutations(&state) {
        return r;
    }
    // Capture the parent brain's project_root so we can append
    // service-lifecycle events to its services.jsonl ledger. Cheap
    // call (BrainTree is preloaded at AppState construction).
    let parent_project_root = match resolve_brain(&state, &brain_id) {
        Ok(b) => b.project_root.clone(),
        Err(r) => return r,
    };
    let peer = match resolve_peer(&state, &brain_id, &peer_name).await {
        Ok(p) => p,
        Err(r) => return r,
    };

    if state.service_registry.contains(&peer.peer_name).await {
        return (
            StatusCode::CONFLICT,
            Json(ServiceErrorDto::new(
                "already-running",
                format!("peer '{peer_name}' is already running under this dashboard"),
            )),
        )
            .into_response();
    }

    // Resolve the target port: read peer's ports.json, or allocate
    // fresh against the peer's project root.
    let port = match neurogrim_core::ports::read_ports(&peer.brain_path) {
        Some(cfg) => cfg.a2a_port,
        None => {
            let alloc = neurogrim_core::ports::PortAllocator::default();
            match neurogrim_core::ports::allocate(&peer.brain_path, &alloc) {
                Ok((cfg, _fresh)) => cfg.a2a_port,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ServiceErrorDto::new(
                            "spawn-failed",
                            format!(
                                "failed to allocate a2a port for peer '{peer_name}': {e}"
                            ),
                        )),
                    )
                        .into_response();
                }
            }
        }
    };

    // OS bind feasibility precheck — quick filter before we spawn
    // and watch a child fail at bind. TOCTOU-vulnerable on purpose;
    // the readiness watcher catches the race.
    if !neurogrim_core::ports::try_bind(port) {
        crate::services::log_service_failed(
            &parent_project_root,
            &peer.peer_name,
            &format!("port-conflict: port {port} already bound"),
            None,
        );
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ServiceErrorDto::new(
                "port-conflict",
                format!(
                    "port {port} is already bound by another process; \
                     stop the conflicting service or run \
                     `neurogrim federation rewire --child {peer_name}` \
                     after re-allocating ports"
                ),
            )),
        )
            .into_response();
    }

    // Set up the per-service log file under .claude/brain/logs/.
    let log_dir = peer.brain_path.join(".claude").join("brain").join("logs");
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ServiceErrorDto::new(
                "spawn-failed",
                format!("failed to create log directory {}: {e}", log_dir.display()),
            )),
        )
            .into_response();
    }
    let log_path = log_dir.join(format!("{peer_name}.log"));
    let log_file = match std::fs::File::create(&log_path) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ServiceErrorDto::new(
                    "spawn-failed",
                    format!("failed to open log file {}: {e}", log_path.display()),
                )),
            )
                .into_response();
        }
    };
    // tokio::process needs separate File handles for stdout vs stderr.
    let log_file_err = match log_file.try_clone() {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ServiceErrorDto::new(
                    "spawn-failed",
                    format!("failed to clone log handle: {e}"),
                )),
            )
                .into_response();
        }
    };

    // Resolve the binary to spawn. `current_exe()` returns the binary
    // the operator ran (works for `cargo install` global, dev builds,
    // and WSL/Windows boundary) — never spell it as `"neurogrim"`.
    let binary_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ServiceErrorDto::new(
                    "spawn-failed",
                    format!("failed to resolve current_exe: {e}"),
                )),
            )
                .into_response();
        }
    };

    let mut cmd = tokio::process::Command::new(&binary_path);
    cmd.arg("a2a-serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--bind")
        .arg("127.0.0.1")
        .arg("--project-root")
        .arg(&peer.brain_path)
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(log_file_err))
        .stdin(std::process::Stdio::null());
    // kill_on_drop intentionally NOT set — orphans survive
    // dashboard restart per v3.5.0 user contract. See
    // services.rs module docstring for rationale.

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ServiceErrorDto::new(
                    "spawn-failed",
                    format!(
                        "failed to spawn '{}' a2a-serve for peer '{peer_name}': {e}",
                        binary_path.display()
                    ),
                )),
            )
                .into_response();
        }
    };

    let pid = child.id().unwrap_or(0);
    let started_at = chrono::Utc::now();

    // Wrap the child in Arc<Mutex<…>> once and clone the Arc for
    // the registry vs. the readiness watcher. Both end up pointing
    // at the same Child; the watcher can `try_wait` while the
    // registry retains the handle for the eventual stop.
    let child_arc = std::sync::Arc::new(tokio::sync::Mutex::new(child));

    let handle = ServiceHandle {
        peer_name: peer.peer_name.clone(),
        pid,
        port,
        started_at,
        log_path: log_path.clone(),
        child: child_arc.clone(),
    };
    state.service_registry.insert(handle).await;

    broadcast_service_event(
        &state.events,
        DashboardEvent::ServiceStarting {
            peer_name: peer.peer_name.clone(),
            pid,
            port,
        },
    );

    // Spawn the readiness watcher: poll try_bind until the spawned
    // child's bind is observable (port becomes unavailable to us),
    // up to ~5s. Then broadcast ServiceStarted. On timeout or early
    // child exit, broadcast ServiceFailed and reap the child.
    let registry_clone = state.service_registry.clone();
    let events_clone = state.events.clone();
    let peer_name_clone = peer.peer_name.clone();
    let parent_root_clone = parent_project_root.clone();
    tokio::spawn(async move {
        run_readiness_watcher(
            registry_clone,
            events_clone,
            peer_name_clone,
            pid,
            port,
            child_arc,
            parent_root_clone,
        )
        .await;
    });

    (
        StatusCode::ACCEPTED,
        Json(StartPeerResponse {
            state: "starting".to_string(),
            peer_name: peer.peer_name,
            pid,
            port,
        }),
    )
        .into_response()
}

/// Background task: watches a freshly-spawned service for readiness.
/// Polls `try_bind` every 250ms — first time the port becomes
/// unavailable to us AND the child is still alive, broadcasts
/// `ServiceStarted`. On 5s timeout or early child exit, broadcasts
/// `ServiceFailed` and removes the entry from the registry.
///
/// `child` is a clone of the Arc that the registry holds, so this
/// watcher can `try_wait` without going through the registry's lock.
// `parent_project_root` is the parent brain's project_root, used to
// append terminal service-lifecycle events to its services.jsonl
// ledger.
async fn run_readiness_watcher(
    registry: std::sync::Arc<crate::services::ServiceRegistry>,
    events: Option<tokio::sync::broadcast::Sender<DashboardEvent>>,
    peer_name: String,
    pid: u32,
    port: u16,
    child: std::sync::Arc<tokio::sync::Mutex<tokio::process::Child>>,
    parent_project_root: std::path::PathBuf,
) {
    const POLL_INTERVAL: Duration = Duration::from_millis(250);
    const READINESS_TIMEOUT: Duration = Duration::from_secs(5);
    let started = std::time::Instant::now();

    loop {
        // First: did the spawned child exit early? If yes, the
        // bind() failed inside the child and we'll never see the
        // port become unavailable.
        let child_exited = {
            let mut guard = child.lock().await;
            matches!(guard.try_wait(), Ok(Some(_)))
        };
        if child_exited {
            // Drop the registry entry (cleanup is idempotent).
            registry.remove(&peer_name).await;
            let reason = format!(
                "spawned process for peer '{peer_name}' exited during startup; \
                 check the log under <peer_root>/.claude/brain/logs/{peer_name}.log"
            );
            crate::services::log_service_failed(
                &parent_project_root,
                &peer_name,
                &reason,
                Some(pid),
            );
            broadcast_service_event(
                &events,
                DashboardEvent::ServiceFailed {
                    peer_name: peer_name.clone(),
                    reason,
                },
            );
            return;
        }

        // Then: has the port become bound? `try_bind` returns false
        // when something is listening on it — most likely our child.
        if !neurogrim_core::ports::try_bind(port) {
            crate::services::log_service_started(
                &parent_project_root,
                &peer_name,
                pid,
                port,
            );
            broadcast_service_event(
                &events,
                DashboardEvent::ServiceStarted {
                    peer_name: peer_name.clone(),
                    pid,
                    port,
                },
            );
            return;
        }

        // Timeout check.
        if started.elapsed() >= READINESS_TIMEOUT {
            // Reap the child + remove from registry.
            if let Some(handle) = registry.remove(&peer_name).await {
                let _ = handle.child.lock().await.start_kill();
            }
            let reason = format!(
                "readiness timeout: peer '{peer_name}' did not bind port {port} \
                 within {}s",
                READINESS_TIMEOUT.as_secs()
            );
            crate::services::log_service_failed(
                &parent_project_root,
                &peer_name,
                &reason,
                Some(pid),
            );
            broadcast_service_event(
                &events,
                DashboardEvent::ServiceFailed {
                    peer_name: peer_name.clone(),
                    reason,
                },
            );
            return;
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// `POST /api/brains/:id/peers/:peer/stop` — kill the tracked child.
async fn brain_peer_stop(
    State(state): State<AppState>,
    AxumPath((brain_id, peer_name)): AxumPath<(String, String)>,
) -> Response {
    if let Err(r) = require_mutations(&state) {
        return r;
    }
    // Capture the parent brain's project_root for the services.jsonl
    // append before peer resolution so a failure to look up the peer
    // (already-removed, registry edited mid-flight) doesn't lose the
    // log line we'd want to emit on terminal-state transitions.
    let parent_project_root = match resolve_brain(&state, &brain_id) {
        Ok(b) => b.project_root.clone(),
        Err(r) => return r,
    };
    // resolve_brain validates the brain_id; unknown brain → 404.
    if let Err(r) = resolve_peer(&state, &brain_id, &peer_name).await {
        return r;
    }

    let handle = match state.service_registry.remove(&peer_name).await {
        Some(h) => h,
        None => {
            return (
                StatusCode::CONFLICT,
                Json(ServiceErrorDto::new(
                    "not-running",
                    format!(
                        "peer '{peer_name}' is not currently tracked by this dashboard \
                         (it may be running from a previous dashboard run; kill via OS)"
                    ),
                )),
            )
                .into_response();
        }
    };

    let pid = handle.pid;
    // Kill via the tokio Child. SIGKILL on Unix, TerminateProcess
    // on Windows. `a2a-serve` has no graceful-shutdown work, so
    // unconditional kill is fine.
    if let Err(e) = handle.child.lock().await.kill().await {
        let reason = format!("failed to kill child pid={pid}: {e}");
        crate::services::log_service_failed(
            &parent_project_root,
            &peer_name,
            &reason,
            Some(pid),
        );
        broadcast_service_event(
            &state.events,
            DashboardEvent::ServiceFailed {
                peer_name: peer_name.clone(),
                reason,
            },
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ServiceErrorDto::new(
                "spawn-failed",
                format!("failed to kill child pid={pid}: {e}"),
            )),
        )
            .into_response();
    }

    crate::services::log_service_stopped(&parent_project_root, &peer_name, pid);
    broadcast_service_event(
        &state.events,
        DashboardEvent::ServiceStopped {
            peer_name: peer_name.clone(),
            pid,
        },
    );

    (
        StatusCode::OK,
        Json(StopPeerResponse {
            state: "stopped".to_string(),
            peer_name,
        }),
    )
        .into_response()
}

/// `GET /api/brains/:id/services` — read-only list of services
/// tracked by this dashboard instance.
async fn brain_services(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    if let Err(r) = resolve_brain(&state, &brain_id) {
        return r;
    }
    Json(ServicesListResponse {
        services: state.service_registry.list().await,
    })
    .into_response()
}

/// `GET /api/brains/:id/ports` — read-only view of the project's
/// port allocation. Powers the v3.5.0 ports-panel widget.
async fn brain_ports(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let cfg = neurogrim_core::ports::read_ports(&brain.project_root);

    // Synthesize a wire-friendly payload. We don't define a typed
    // DTO for this in types.rs — the shape is small and the
    // frontend's `ports-panel` widget reads it directly.
    let payload = match cfg {
        Some(c) => {
            let dashboard_bound = !neurogrim_core::ports::try_bind(c.dashboard_port);
            let a2a_bound = !neurogrim_core::ports::try_bind(c.a2a_port);
            serde_json::json!({
                "schema_version": c.schema_version,
                "dashboard_port": c.dashboard_port,
                "a2a_port": c.a2a_port,
                "created_at": c.created_at.to_rfc3339(),
                "generated_by": c.generated_by,
                "dashboard_port_bound": dashboard_bound,
                "a2a_port_bound": a2a_bound,
                "ports_file": neurogrim_core::ports::ports_file_path(&brain.project_root)
                    .to_string_lossy(),
                "missing": false,
            })
        }
        None => serde_json::json!({
            "missing": true,
            "ports_file": neurogrim_core::ports::ports_file_path(&brain.project_root)
                .to_string_lossy(),
        }),
    };
    Json(payload).into_response()
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
// ── v4.5 Plumbing page handlers ─────────────────────────────────────────

/// `GET /api/brains/:id/plumbing/overview` — top-level summary
/// suitable for the Plumbing page header. Returns lightweight
/// counters from each substrate: TSDB cardinality, queue topic
/// count, and per-substrate disk size where cheap to compute.
async fn plumbing_overview(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let metrics_total = state
        .metrics
        .as_ref()
        .and_then(|m| m.total_points().ok())
        .unwrap_or(0);
    let metrics_size = state
        .metrics
        .as_ref()
        .and_then(|m| m.size_bytes().ok())
        .unwrap_or(0);
    let metrics_series_count = state
        .metrics
        .as_ref()
        .and_then(|m| m.list_series().ok())
        .map(|v| v.len() as u64)
        .unwrap_or(0);
    let queues_dir = brain
        .project_root
        .join(".claude")
        .join("brain")
        .join("queues");
    let queues_size = directory_size_bytes(&queues_dir).unwrap_or(0);

    Json(serde_json::json!({
        "metrics": {
            "enabled": state.metrics.is_some(),
            "series_count": metrics_series_count,
            "total_points": metrics_total,
            "size_bytes": metrics_size,
        },
        "queues": {
            "size_bytes": queues_size,
        },
    }))
    .into_response()
}

/// `GET /api/brains/:id/plumbing/metrics/series` — list every
/// recorded metric series with summary stats. Powers the Plumbing
/// page's Metrics tab landing view.
async fn plumbing_list_series(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let _brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let series = match state.metrics.as_ref() {
        Some(m) => match m.list_series() {
            Ok(v) => v,
            Err(e) => return internal_error(format!("metrics list_series: {e}")),
        },
        None => vec![],
    };
    Json(serde_json::json!({ "series": series })).into_response()
}

#[derive(Debug, Deserialize)]
pub struct MetricsQueryParams {
    /// Lookback window in seconds. Defaults to 30 days.
    #[serde(default = "default_window_seconds")]
    pub window_s: i64,
    /// Aggregation: `avg | sum | min | max | count | last | none`.
    /// Defaults to `none` (raw points).
    #[serde(default)]
    pub agg: Option<String>,
    /// Bucket size in seconds for aggregation. Defaults to 1 day
    /// when agg is set; ignored when agg is `none`.
    pub bucket_s: Option<i64>,
    /// Optional tag filter `domain=test-health` etc. Repeatable.
    /// Encoded as `tag=k:v` in the query string for simplicity.
    #[serde(default)]
    pub tag: Vec<String>,
    /// Hard cap on returned points. Default 5000.
    #[serde(default = "default_metrics_limit")]
    pub limit: usize,
}

fn default_window_seconds() -> i64 {
    60 * 60 * 24 * 30
}

fn default_metrics_limit() -> usize {
    5000
}

/// `GET /api/brains/:id/plumbing/metrics/:name?window_s=...&agg=...&bucket_s=...&tag=k:v`
async fn plumbing_query_series(
    State(state): State<AppState>,
    AxumPath((brain_id, name)): AxumPath<(String, String)>,
    Query(params): Query<MetricsQueryParams>,
) -> Response {
    use neurogrim_core::metrics::{Aggregate, Query as MQuery};

    let _brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let metrics = match state.metrics.as_ref() {
        Some(m) => m,
        None => {
            return Json(serde_json::json!({
                "name": name,
                "points": [],
                "warning": "metrics store not initialized for this dashboard session",
            }))
            .into_response()
        }
    };

    let since = chrono::Utc::now() - chrono::Duration::seconds(params.window_s.max(60));
    let mut q = MQuery::new(name.clone(), since).limit(params.limit.min(50_000).max(1));
    for entry in &params.tag {
        if let Some((k, v)) = entry.split_once(':') {
            q = q.filter(k, v);
        }
    }
    if let Some(agg_str) = params.agg.as_deref() {
        let agg = match agg_str {
            "avg" => Some(Aggregate::Avg),
            "sum" => Some(Aggregate::Sum),
            "min" => Some(Aggregate::Min),
            "max" => Some(Aggregate::Max),
            "count" => Some(Aggregate::Count),
            "last" => Some(Aggregate::Last),
            "none" | "" => None,
            _ => None,
        };
        if let Some(a) = agg {
            let bucket = params.bucket_s.unwrap_or(60 * 60 * 24).max(1);
            q = q.aggregate(a, chrono::Duration::seconds(bucket));
        }
    }

    let points = match metrics.query(&q) {
        Ok(p) => p,
        Err(e) => return internal_error(format!("metrics query: {e}")),
    };
    Json(serde_json::json!({
        "name": name,
        "since": since.to_rfc3339(),
        "point_count": points.len(),
        "points": points,
    }))
    .into_response()
}

/// Recursive directory size sum. Best-effort — silently treats any
/// metadata error as 0. Used by the Plumbing overview to report
/// approximate substrate disk consumption without a stat-walk loop
/// at every call site.
fn directory_size_bytes(dir: &std::path::Path) -> Option<u64> {
    let mut total: u64 = 0;
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            if let Some(n) = directory_size_bytes(&entry.path()) {
                total = total.saturating_add(n);
            }
        } else {
            total = total.saturating_add(meta.len());
        }
    }
    Some(total)
}

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

// ── S13-B-2: coordination bus handlers ──────────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct QueueReadQuery {
    /// Offset (0-indexed line number) to resume from. Defaults to 0
    /// — the consumer reads from the start of the topic. Consumers
    /// persist this themselves; the bus is stateless w.r.t. who's
    /// read what.
    #[serde(default)]
    pub since: u64,
    /// Cap on the number of messages returned per call. Defaults to
    /// 100; capped server-side at 1000 to prevent runaway responses.
    #[serde(default)]
    pub limit: Option<u32>,
}

const DEFAULT_QUEUE_READ_LIMIT: u32 = 100;
const MAX_QUEUE_READ_LIMIT: u32 = 1000;

/// `GET /api/brains/:brain_id/queues` — list topics + stats.
async fn brain_queues_list(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    // S13-B-3 v2: enumerate via the file walker (which now sees
    // both .jsonl and .sqlite extensions) and resolve each topic's
    // backend through the bus so stats reflect the right counts /
    // size. Falls back to JSONL stats on backend init failure so
    // partial misconfig still renders.
    let topics = crate::bus::list_topics(&brain.project_root);
    let mut stats: Vec<QueueTopicStatsDto> = Vec::with_capacity(topics.len());
    for t in &topics {
        let s = match state.bus.backend_for(&brain.project_root, t).await {
            Ok(handle) => crate::bus::TopicStats::for_topic(&handle, &brain.project_root, t),
            Err(_) => {
                let path = crate::bus::topic_path(&brain.project_root, t);
                crate::bus::TopicStats::from_path(t, &path)
            }
        };
        stats.push(QueueTopicStatsDto {
            topic: s.topic,
            message_count: s.message_count.min(u32::MAX as usize) as u32,
            size_bytes: s.size_bytes,
            oldest: s.oldest,
            newest: s.newest,
        });
    }
    Json(QueuesListResponse { topics: stats }).into_response()
}

/// `GET /api/brains/:brain_id/queues/*rest`. Dispatches by suffix:
/// - `rest` ending in `/events` → SSE subscription on the topic
///   (everything before the suffix).
/// - Otherwise → read messages from `since` cursor up to `limit`.
async fn brain_queue_read_or_events(
    State(state): State<AppState>,
    AxumPath((brain_id, rest)): AxumPath<(String, String)>,
    Query(q): Query<QueueReadQuery>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    if let Some(topic) = rest.strip_suffix("/events") {
        return brain_queue_sse(state.bus.clone(), topic.to_string()).await;
    }
    if !neurogrim_core::queue::Topic::is_valid(&rest) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid topic name", "topic": rest})),
        )
            .into_response();
    }
    // S13-B-3 v2: dispatch through the bus's backend handle so
    // SQLite-configured topics serve correctly. The bus lazy-creates
    // the right backend on first call.
    let handle = match state.bus.backend_for(&brain.project_root, &rest).await {
        Ok(h) => h,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("backend init failed: {e}"),
                    "topic": rest,
                })),
            )
                .into_response();
        }
    };
    let limit = q
        .limit
        .unwrap_or(DEFAULT_QUEUE_READ_LIMIT)
        .min(MAX_QUEUE_READ_LIMIT) as usize;
    let stored = match handle.read_from(q.since, limit) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("read failed: {e}"),
                    "topic": rest,
                })),
            )
                .into_response();
        }
    };
    let messages: Vec<QueueMessageDto> =
        stored.iter().map(|sm| qm_to_dto(&sm.message)).collect();
    // For ack-aware backends (SQLite), the next offset is one past
    // the highest offset returned. For JSONL, offsets are
    // line-numbered and contiguous, so this simplifies to
    // `since + messages.len()` — but the explicit form is correct
    // for both.
    let next_offset = stored
        .last()
        .map(|sm| sm.offset + 1)
        .unwrap_or(q.since);
    Json(QueueReadResponse {
        topic: rest,
        messages,
        next_offset,
    })
    .into_response()
}

/// SSE stream of new messages on `topic`. One subscription per
/// connection. Subscribers join AFTER any backlog is consumed via
/// the read endpoint — this stream only carries newly-published
/// messages.
async fn brain_queue_sse(bus: crate::bus::BusState, topic: String) -> Response {
    if !neurogrim_core::queue::Topic::is_valid(&topic) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid topic name", "topic": topic})),
        )
            .into_response();
    }
    let rx = bus.subscribe(&topic).await;
    let stream: futures::stream::BoxStream<'static, Result<Event, Infallible>> =
        BroadcastStream::new(rx)
            .filter_map(|res| async move {
                let qm = res.ok()?;
                let dto = qm_to_dto(&qm);
                let json = serde_json::to_string(&dto).ok()?;
                Some(Ok(Event::default().data(json)))
            })
            .boxed();
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

/// `POST /api/brains/:brain_id/queues/*rest` — publish to topic.
/// Gated by `--allow-mutations` (returns 403 when disabled). Body
/// is [`QueuePublishRequest`]; bus generates id + produced_at.
async fn brain_queue_publish(
    State(state): State<AppState>,
    AxumPath((brain_id, rest)): AxumPath<(String, String)>,
    Json(body): Json<QueuePublishRequest>,
) -> Response {
    if !state.mutations_allowed {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "mutations-disabled",
                "code": "mutations-disabled",
                "hint": "start the dashboard with --allow-mutations to enable bus publishes",
            })),
        )
            .into_response();
    }
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    if !neurogrim_core::queue::Topic::is_valid(&rest) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid topic name", "topic": rest})),
        )
            .into_response();
    }
    let mut msg = neurogrim_core::queue::QueueMessage::new(rest.clone(), body.payload);
    if let Some(p) = body.priority {
        match p.as_str() {
            "low" => msg.priority = neurogrim_core::queue::Priority::Low,
            "normal" => msg.priority = neurogrim_core::queue::Priority::Normal,
            "high" => msg.priority = neurogrim_core::queue::Priority::High,
            other => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid priority",
                        "expected": ["low", "normal", "high"],
                        "got": other,
                    })),
                )
                    .into_response();
            }
        }
    }
    if let Some(ttl_ms) = body.expires_in_ms {
        let when = msg.produced_at + chrono::Duration::milliseconds(ttl_ms as i64);
        msg = msg.with_expires_at(when);
    }
    let written = match state.bus.publish(&brain.project_root, msg).await {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("publish failed: {e}")})),
            )
                .into_response();
        }
    };
    crate::instrumentation::record_bus_publish(&state, &written.topic, "dashboard");
    Json(QueuePublishResponse {
        id: written.id.to_string(),
        topic: written.topic,
        produced_at: written.produced_at.to_rfc3339(),
    })
    .into_response()
}

fn qm_to_dto(m: &neurogrim_core::queue::QueueMessage) -> QueueMessageDto {
    let priority = match m.priority {
        neurogrim_core::queue::Priority::Low => "low",
        neurogrim_core::queue::Priority::Normal => "normal",
        neurogrim_core::queue::Priority::High => "high",
    };
    QueueMessageDto {
        id: m.id.to_string(),
        topic: m.topic.clone(),
        payload: m.payload.clone(),
        produced_at: m.produced_at.to_rfc3339(),
        priority: priority.to_string(),
        expires_at: m.expires_at.map(|x| x.to_rfc3339()),
    }
}

// ── S15-C-7: edit-via-bus emitter ──────────────────────────────────────

/// Reserved system topic that every UI mutation publishes to. Agents
/// (and the dashboard's own widgets in future stories) subscribe to
/// observe operator activity in real-time.
pub const CONFIG_CHANGES_TOPIC: &str = "_neurogrim/config-changes";

/// Emit a `_neurogrim/config-changes` event. Best-effort — if the bus
/// publish fails (zero subscribers, disk write transient error), the
/// caller's mutation still succeeds. Failure to record observation is
/// strictly less bad than failure to apply the mutation.
///
/// **v1 payload shape (deliberately minimal):**
///
/// ```json
/// {
///   "action_type": "<one of `layout_change`, `registry_edit`, ...>",
///   "operator": "<from $NEUROGRIM_OPERATOR; null when unset>",
///   "timestamp": "<RFC3339>",
///   "brain_id": "<…>",
///   "summary": "<one-line operator-facing description>"
/// }
/// ```
///
/// **v1 (this signature):** WHAT (action_type) + WHO (operator) +
/// WHEN (timestamp) + free-text summary. Adopters subscribed to the
/// queue can read the file from disk to see the new state.
///
/// **v2:** [`emit_config_change_with_diff`] adds an optional
/// `diff: [{path, op, before, after}]` field so subscribers react
/// surgically without re-fetching. Call sites that have natural
/// before/after snapshots (registry edits, layout edits) compute
/// the diff via [`crate::json_diff::diff`] and use the v2 helper;
/// call sites without natural snapshots (page CRUD, approval
/// resolution) keep using this v1 helper.
pub async fn emit_config_change(
    state: &AppState,
    brain_id: &str,
    action_type: &str,
    summary: impl Into<String>,
    project_root: &std::path::Path,
) {
    emit_config_change_with_diff(state, brain_id, action_type, summary, project_root, None).await
}

/// S15-C-7 v2: emit on the config-changes queue with an optional
/// keypath diff. The diff lets agents subscribed to the topic react
/// to specific field changes (e.g. autonomy level dropped, federation
/// child added) without re-fetching the whole document.
///
/// Pass `None` for `diff` when the change has no natural before/after
/// shape (e.g. page deletion, approval resolution); the payload will
/// match the v1 shape and existing subscribers stay compatible.
pub async fn emit_config_change_with_diff(
    state: &AppState,
    brain_id: &str,
    action_type: &str,
    summary: impl Into<String>,
    project_root: &std::path::Path,
    diff: Option<Vec<crate::json_diff::KeypathChange>>,
) {
    let mut payload = serde_json::json!({
        "action_type": action_type,
        "operator": std::env::var("NEUROGRIM_OPERATOR").ok(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "brain_id": brain_id,
        "summary": summary.into(),
    });
    if let Some(changes) = diff {
        if !changes.is_empty() {
            // v2: embed as `diff: [...]`. Empty vec is functionally
            // identical to v1; skip emitting it to keep payloads
            // tight for change-free saves (rare but possible — a
            // double-Save with no changes between).
            if let Some(obj) = payload.as_object_mut() {
                obj.insert(
                    "diff".to_string(),
                    serde_json::to_value(&changes).unwrap_or(serde_json::Value::Null),
                );
            }
        }
    }
    let msg = neurogrim_core::queue::QueueMessage::new(CONFIG_CHANGES_TOPIC, payload);
    if let Err(e) = state.bus.publish(project_root, msg).await {
        // Best-effort — log but don't propagate.
        tracing::warn!(
            "edit-via-bus emit failed for action_type={action_type}: {e}"
        );
    } else {
        crate::instrumentation::record_bus_publish(state, CONFIG_CHANGES_TOPIC, "dashboard");
    }
}

// ── S15-C-5: config-file read-only viewer ───────────────────────────────

/// `GET /api/brains/:brain_id/config-file/:name` — return the
/// raw text of a known config file. Hardcoded allowlist:
///
/// - `culture.yaml` → `<root>/.claude/culture.yaml`
/// - `queue-config.yaml` → `<root>/.claude/brain/queue-config.yaml`
///
/// Other names return 400. Read-only by design — Settings UI
/// edits land via separate per-config endpoints (S15-C-4 + S15-C-5
/// expansion in session 2).
async fn brain_config_file(
    State(state): State<AppState>,
    AxumPath((brain_id, name)): AxumPath<(String, String)>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let relative = match name.as_str() {
        "culture.yaml" => Path::new(".claude").join("culture.yaml"),
        "queue-config.yaml" => Path::new(".claude").join("brain").join("queue-config.yaml"),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "unknown config-file name",
                    "allowed": ["culture.yaml", "queue-config.yaml"],
                    "got": name,
                })),
            )
                .into_response();
        }
    };
    let path = brain.project_root.join(&relative);
    let path_display = path.display().to_string();
    match std::fs::read_to_string(&path) {
        Ok(text) => Json(ConfigFileResponse {
            name,
            present: true,
            path: path_display,
            text: Some(text),
            error: None,
        })
        .into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Json(ConfigFileResponse {
            name,
            present: false,
            path: path_display,
            text: None,
            error: None,
        })
        .into_response(),
        Err(e) => Json(ConfigFileResponse {
            name,
            present: false,
            path: path_display,
            text: None,
            error: Some(format!("{e}")),
        })
        .into_response(),
    }
}

// ── S15-C-8 v1: explain topic content ───────────────────────────────────

/// `GET /api/explain/:topic` — return the markdown text of a
/// bundled `neurogrim explain` topic. Used by the inline-help
/// HelpIcon to render relevant content in a modal.
async fn explain_topic(AxumPath(topic): AxumPath<String>) -> Response {
    match neurogrim_mcp::explain::lookup(&topic) {
        Some(content) => Json(ExplainTopicResponse {
            name: topic,
            content: content.to_string(),
        })
        .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "unknown topic",
                "got": topic,
                "hint": "list of valid topics: run `neurogrim explain` (no args)",
            })),
        )
            .into_response(),
    }
}

// ── S15-C-2 v2: invocation-ledger source for the Logs page ─────────────

#[derive(Debug, Deserialize)]
struct LogsLimitQuery {
    /// Max entries to return. Clamped server-side to
    /// `[1, logs::MAX_LIMIT]`.
    limit: Option<usize>,
}

/// `GET /api/brains/:brain_id/logs/invocation-ledger?limit=N` —
/// recent invocation-ledger rows for the Logs page timeline.
///
/// Returns newest-first. Defaults to 50 entries. The Logs page
/// uses this as one of its aggregated sources alongside publish-
/// gates and approvals.
async fn brain_logs_invocation_ledger(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
    Query(q): Query<LogsLimitQuery>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let limit = q.limit.unwrap_or(crate::logs::DEFAULT_LIMIT);
    let resp = crate::logs::read_invocation_ledger(&brain.project_root, limit);
    Json(resp).into_response()
}

/// `GET /api/brains/:brain_id/logs/score-history?limit=N` —
/// recent score-history snapshots for the Logs page timeline,
/// each annotated with the delta against its chronologically-prior
/// snapshot. S15-C-3 expansion.
///
/// Returns newest-first. Defaults to 50 entries. Only the unified
/// score is surfaced (per-domain detail lives in the Domains pages)
/// so a Brain scored 50× per day doesn't drown the timeline.
async fn brain_logs_score_history(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
    Query(q): Query<LogsLimitQuery>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let limit = q.limit.unwrap_or(crate::logs::DEFAULT_LIMIT);
    let resp = crate::logs::read_score_history(&brain.project_root, limit);
    Json(resp).into_response()
}

/// `GET /api/brains/:brain_id/logs/services?limit=N` — recent
/// service-lifecycle events from `<project>/.claude/brain/services.jsonl`.
/// S15-C-3 expansion follow-on: closes the sixth Logs source.
async fn brain_logs_services(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
    Query(q): Query<LogsLimitQuery>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let limit = q.limit.unwrap_or(crate::logs::DEFAULT_LIMIT);
    let resp = crate::logs::read_services_log(&brain.project_root, limit);
    Json(resp).into_response()
}

/// Query params for the per-peer log endpoint.
#[derive(Debug, Deserialize)]
struct PeerLogQuery {
    /// Number of trailing lines to return. Clamped server-side to
    /// `[1, logs::PEER_LOG_MAX_LINES]`. Defaults to
    /// `logs::PEER_LOG_DEFAULT_LINES`.
    lines: Option<usize>,
}

/// `GET /api/brains/:brain_id/peers/:peer_name/log?lines=N` —
/// trailing lines from the peer's spawned-process log file.
/// S15-C-2 expansion: gives operators in-dashboard debugging
/// without dropping to a terminal for `tail -f`.
///
/// The log lives at the conventional path:
///   `<peer_brain_path>/.claude/brain/logs/<peer_name>.log`
/// (where `<peer_brain_path>` is resolved from the parent brain's
/// `config.children.<peer_name>.brain_path`). Works for any peer
/// declared in the registry, whether currently tracked by the
/// in-memory service registry or not — operators can read logs
/// from a peer that started in a previous dashboard session.
///
/// The reader is bounded by [`crate::logs::PEER_LOG_TAIL_BYTES`]
/// so a multi-GB log doesn't OOM the dashboard. The response's
/// `truncated` field tells the UI whether older content exists.
async fn brain_peer_log(
    State(state): State<AppState>,
    AxumPath((brain_id, peer_name)): AxumPath<(String, String)>,
    Query(q): Query<PeerLogQuery>,
) -> Response {
    let peer = match resolve_peer(&state, &brain_id, &peer_name).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let log_path = peer
        .brain_path
        .join(".claude")
        .join("brain")
        .join("logs")
        .join(format!("{}.log", peer.peer_name));
    let lines = q.lines.unwrap_or(crate::logs::PEER_LOG_DEFAULT_LINES);
    let resp = crate::logs::read_peer_log_tail(&log_path, lines);
    Json(resp).into_response()
}

// ── S14-S-6 v1: secrets-management UI surface ──────────────────────────

/// `GET /api/brains/:brain_id/secrets` — list declared secrets
/// + their backend-stored status. Read-only; values are NEVER
/// included in the response.
async fn brain_secrets_list(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let resp = crate::secrets_api::build_secrets_list(&brain_id, &brain.project_root);
    Json(resp).into_response()
}

/// `POST /api/brains/:brain_id/secrets/:secret_id` — set or rotate
/// a secret's value. Body: `{"value": "<plaintext>"}`. The server
/// consumes the value into a zeroizing `SecretValue` immediately
/// + writes to the OS-native backend. Gated behind
/// `--allow-mutations`. Value is NEVER echoed in the response.
async fn brain_secrets_set(
    State(state): State<AppState>,
    AxumPath((brain_id, secret_id)): AxumPath<(String, String)>,
    Json(body): Json<crate::secrets_api::SetSecretRequest>,
) -> Response {
    if !state.mutations_allowed {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "mutations are disabled",
                "code": "mutations-disabled",
                "hint": "start the dashboard with --allow-mutations to enable secret writes"
            })),
        )
            .into_response();
    }
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    if let Err(msg) = crate::secrets_api::ensure_declared(&brain.project_root, &secret_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": msg,
                "code": "undeclared-secret",
            })),
        )
            .into_response();
    }
    if body.value.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "value must not be empty",
                "code": "empty-value",
            })),
        )
            .into_response();
    }
    // `body.value` is moved into `set_secret`, which moves it
    // into a zeroizing `SecretValue`. After this line the
    // plaintext exists only inside the SecretValue (and its
    // ciphertext form once written to the backend).
    match crate::secrets_api::set_secret(&brain_id, &secret_id, body.value) {
        Ok(updated_at) => Json(crate::secrets_api::SetSecretResponse {
            brain_id,
            secret_id,
            updated_at,
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("secret write failed: {e}"),
                "code": "backend-error",
            })),
        )
            .into_response(),
    }
}

/// `DELETE /api/brains/:brain_id/secrets/:secret_id` — remove a
/// stored secret value. Gated behind `--allow-mutations`. The
/// manifest entry stays in `secret-refs.yaml`; only the backend
/// value is removed. Idempotent.
async fn brain_secrets_delete(
    State(state): State<AppState>,
    AxumPath((brain_id, secret_id)): AxumPath<(String, String)>,
) -> Response {
    if !state.mutations_allowed {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "mutations are disabled",
                "code": "mutations-disabled",
            })),
        )
            .into_response();
    }
    let _brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    match crate::secrets_api::delete_secret(&brain_id, &secret_id) {
        Ok(removed) => Json(crate::secrets_api::DeleteSecretResponse {
            brain_id,
            secret_id,
            removed,
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("secret delete failed: {e}"),
                "code": "backend-error",
            })),
        )
            .into_response(),
    }
}

// ── S15-C-6 v1: custom-pages CRUD ───────────────────────────────────────

/// `GET /api/brains/:brain_id/dashboard-pages` — return the v2
/// multi-page config (with v3.4 backward-compat read).
async fn brain_dashboard_pages_get(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let cfg = crate::pages::read_dashboard_pages(&brain.project_root, &brain_id);
    Json(cfg).into_response()
}

/// `POST /api/brains/:brain_id/dashboard-pages/:name` — create a new
/// custom page. Validates the name + collisions; appends to
/// `page_order` so the sidebar surfaces it; persists atomically.
async fn brain_dashboard_pages_create(
    State(state): State<AppState>,
    AxumPath((brain_id, name)): AxumPath<(String, String)>,
    Json(_body): Json<CreateCustomPageRequest>,
) -> Response {
    if !state.mutations_allowed {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "mutations-disabled",
                "code": "mutations-disabled",
                "hint": "start the dashboard with --allow-mutations to enable custom-page CRUD",
            })),
        )
            .into_response();
    }
    if !crate::pages::is_valid_custom_page_name(&name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid page name",
                "code": "invalid-name",
                "hint": "kebab-case starting with a letter, max 64 chars, not a reserved built-in id",
                "got": name,
            })),
        )
            .into_response();
    }
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let mut cfg = crate::pages::read_dashboard_pages(&brain.project_root, &brain_id);
    if cfg.pages.contains_key(&name) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "page already exists",
                "code": "name-collision",
                "got": name,
            })),
        )
            .into_response();
    }
    cfg.pages.insert(name.clone(), Vec::new());
    if !cfg.page_order.contains(&name) {
        cfg.page_order.push(name.clone());
    }
    if let Err(e) = crate::pages::save_dashboard_pages(&brain.project_root, &cfg) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("save failed: {e}")
            })),
        )
            .into_response();
    }
    // S15-C-7: emit on the config-changes queue.
    emit_config_change(
        &state,
        &brain_id,
        "page_added",
        format!("custom page '{}' created", name),
        &brain.project_root,
    )
    .await;
    Json(CustomPageMutationResponse { ok: true, name }).into_response()
}

/// `DELETE /api/brains/:brain_id/dashboard-pages/:name` — remove a
/// custom page. Built-ins can't be deleted (rejected with 400).
/// Idempotent for unknown names — returns 200 OK.
async fn brain_dashboard_pages_delete(
    State(state): State<AppState>,
    AxumPath((brain_id, name)): AxumPath<(String, String)>,
) -> Response {
    if !state.mutations_allowed {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "mutations-disabled",
                "code": "mutations-disabled",
            })),
        )
            .into_response();
    }
    if crate::pages::DashboardPagesConfig::is_builtin(&name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "cannot delete built-in pages",
                "code": "builtin-protected",
                "got": name,
            })),
        )
            .into_response();
    }
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let mut cfg = crate::pages::read_dashboard_pages(&brain.project_root, &brain_id);
    let removed = cfg.pages.remove(&name).is_some();
    cfg.page_order.retain(|n| n != &name);
    if removed {
        if let Err(e) = crate::pages::save_dashboard_pages(&brain.project_root, &cfg) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("save failed: {e}")
                })),
            )
                .into_response();
        }
        // S15-C-7: emit on the config-changes queue.
        emit_config_change(
            &state,
            &brain_id,
            "page_removed",
            format!("custom page '{}' removed", name),
            &brain.project_root,
        )
        .await;
    }
    Json(CustomPageMutationResponse { ok: true, name }).into_response()
}

/// `PUT /api/brains/:brain_id/dashboard-pages/:name/layout` — save
/// the widget list for a single custom page. S15-C-6 v2.
///
/// **Why a per-page endpoint vs. extending the catch-all
/// dashboard-pages PUT:** scoping to a single page keeps the wire
/// payload small (one widget array, not the whole multi-page
/// config), and lets the page-aware `LayoutChanged` event invalidate
/// only the affected page in TanStack Query — pages stay independent
/// even when several are open in tabs.
///
/// Validations:
/// - Page name must already exist in `dashboard-pages.json`. Use
///   `POST /api/brains/:id/dashboard-pages/:name` to create.
/// - Built-in pages aren't editable here (Overview's layout has its
///   own dedicated `/dashboard-layout` endpoint; Services / Logs /
///   Settings / Approvals / Publish gates are React components, not
///   widget layouts).
/// - Atomic write via `pages::save_dashboard_pages`.
///
/// Gated by `--allow-mutations`.
async fn brain_dashboard_pages_save_layout(
    State(state): State<AppState>,
    AxumPath((brain_id, name)): AxumPath<(String, String)>,
    Json(body): Json<CustomPageLayoutRequest>,
) -> Response {
    if !state.mutations_allowed {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "mutations-disabled",
                "code": "mutations-disabled",
                "hint": "start the dashboard with --allow-mutations to enable custom-page editing",
            })),
        )
            .into_response();
    }
    if crate::pages::DashboardPagesConfig::is_builtin(&name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "built-in page layouts are not editable here",
                "code": "builtin-protected",
                "hint": "Overview uses /api/brains/:id/dashboard-layout; Services / Logs / Settings / Approvals / Publish gates are React components, not widget layouts",
                "got": name,
            })),
        )
            .into_response();
    }
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let mut cfg = crate::pages::read_dashboard_pages(&brain.project_root, &brain_id);
    if !cfg.pages.contains_key(&name) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "no such custom page",
                "code": "page-not-found",
                "hint": "POST /api/brains/:id/dashboard-pages/:name first to create the page",
                "got": name,
            })),
        )
            .into_response();
    }
    // S15-C-7 v2: snapshot before/after. The before-state is the
    // page's existing widget list (empty array on a fresh page);
    // the after-state is the request body. Diff scoped to the
    // page's widget list rather than the whole multi-page config —
    // matches the URL scope and keeps subscribers focused on what
    // the operator actually edited.
    let before_value = cfg
        .pages
        .get(&name)
        .map(|widgets| serde_json::to_value(widgets).unwrap_or(serde_json::Value::Null))
        .unwrap_or(serde_json::Value::Null);
    let after_value =
        serde_json::to_value(&body.widgets).unwrap_or(serde_json::Value::Null);
    let widget_count = body.widgets.len();
    cfg.pages.insert(name.clone(), body.widgets);
    if let Err(e) = crate::pages::save_dashboard_pages(&brain.project_root, &cfg) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("save failed: {e}")
            })),
        )
            .into_response();
    }
    if let Some(tx) = &state.events {
        let _ = tx.send(crate::events::DashboardEvent::LayoutChanged);
    }
    let diff = crate::json_diff::diff(&before_value, &after_value);
    emit_config_change_with_diff(
        &state,
        &brain_id,
        "layout_change",
        format!(
            "custom page '{}' layout saved with {} widget(s)",
            name, widget_count
        ),
        &brain.project_root,
        if diff.is_empty() { None } else { Some(diff) },
    )
    .await;
    Json(serde_json::json!({
        "ok": true,
        "name": name,
        "widget_count": widget_count,
    }))
    .into_response()
}

// ── S15-C-4 v1: registry editor ─────────────────────────────────────────

/// `GET /api/brains/:brain_id/registry` — return the parsed
/// registry JSON + ETag fingerprint. Read-only.
async fn brain_registry_get(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let registry_path = brain.registry_path.as_path();
    let raw = match std::fs::read_to_string(registry_path) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("read {}: {e}", registry_path.display())
                })),
            )
                .into_response();
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("parse {}: {e}", registry_path.display())
                })),
            )
                .into_response();
        }
    };
    let etag = compute_etag(&raw);
    Json(RegistryResponse {
        brain_id,
        path: registry_path.display().to_string(),
        etag,
        registry: parsed,
    })
    .into_response()
}

/// `PUT /api/brains/:brain_id/registry` — replace the registry
/// with the request body. Three-step validation:
///
/// 1. **ETag check** — reject with 409 Conflict if `expected_etag`
///    doesn't match the current file's fingerprint (someone else
///    edited the file in the interim).
/// 2. **Schema validation** — `BrainRegistry::from_json` parses the
///    new JSON; `registry.validate()` checks invariants. Rejects
///    with 400 Bad Request on either failure.
/// 3. **Atomic write** — temp file + rename so concurrent readers
///    see either the old or the new file, never a partial write.
///
/// Gated by `--allow-mutations`.
async fn brain_registry_put(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
    Json(body): Json<RegistryUpdateRequest>,
) -> Response {
    if !state.mutations_allowed {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "mutations-disabled",
                "code": "mutations-disabled",
                "hint": "start the dashboard with --allow-mutations to enable registry editing",
            })),
        )
            .into_response();
    }
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let registry_path = brain.registry_path.as_path();
    let current_raw = match std::fs::read_to_string(registry_path) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("read {}: {e}", registry_path.display())
                })),
            )
                .into_response();
        }
    };
    let current_etag = compute_etag(&current_raw);
    if current_etag != body.expected_etag {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "etag mismatch — registry was edited externally between your read and save",
                "code": "etag-conflict",
                "current_etag": current_etag,
                "expected_etag": body.expected_etag,
                "hint": "reload the page; manual 3-way merge UI ships with C-4 v2",
            })),
        )
            .into_response();
    }
    let new_text = match serde_json::to_string_pretty(&body.registry) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("serialize: {e}")
                })),
            )
                .into_response();
        }
    };
    // Schema validation via existing core path.
    let registry = match neurogrim_core::registry::BrainRegistry::from_json(&new_text) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("parse: {e}"),
                    "code": "parse-failed",
                })),
            )
                .into_response();
        }
    };
    if let Err(e) = registry.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("registry validation failed: {e}"),
                "code": "validate-failed",
            })),
        )
            .into_response();
    }
    // Atomic write: temp + rename.
    let tmp_path = registry_path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, &new_text) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("write temp: {e}")
            })),
        )
            .into_response();
    }
    if let Err(e) = std::fs::rename(&tmp_path, registry_path) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("rename: {e}")
            })),
        )
            .into_response();
    }
    if let Some(tx) = &state.events {
        let _ = tx.send(crate::events::DashboardEvent::RegistryChanged);
    }
    let new_etag = compute_etag(&new_text);
    // S15-C-7 v2: compute keypath diff so subscribers see exactly
    // what changed rather than just "registry was edited". The
    // before-state was already read at ETag-check time
    // (`current_raw`); parse it once for the diff. Parse failure
    // here is non-fatal (the new state still saved successfully) —
    // we fall back to the v1 summary-only payload.
    let diff = serde_json::from_str::<serde_json::Value>(&current_raw)
        .ok()
        .map(|before_value| crate::json_diff::diff(&before_value, &body.registry));
    emit_config_change_with_diff(
        &state,
        &brain_id,
        "registry_edit",
        match &diff {
            Some(changes) if !changes.is_empty() => format!(
                "brain-registry.json: {} keypath change(s)",
                changes.len()
            ),
            _ => "brain-registry.json saved via dashboard editor".to_string(),
        },
        &brain.project_root,
        diff,
    )
    .await;
    Json(serde_json::json!({
        "ok": true,
        "etag": new_etag,
    }))
    .into_response()
}

/// SHA-256 of `text`, hex-encoded. Used as the registry editor's
/// ETag fingerprint. Recomputed on every read; the frontend echoes
/// it back on save to catch concurrent edits.
fn compute_etag(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

// ── S13-B-6: autonomy approvals handlers ────────────────────────────────

/// `GET /api/brains/:brain_id/approvals` — pending approvals + recent
/// resolutions joined by action_id.
async fn brain_approvals_list(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let approvals_path = approvals_topic_path(&brain.project_root);
    let resolutions_path = resolutions_topic_path(&brain.project_root);

    // Resolutions come first — we need a set of resolved action_ids
    // to filter the approvals list.
    let resolutions = read_resolutions(&resolutions_path);
    let resolved_ids: std::collections::HashSet<&str> =
        resolutions.iter().map(|r| r.action_id.as_str()).collect();

    let pending = read_pending_approvals(&approvals_path, &resolved_ids);

    Json(ApprovalsPageResponse {
        pending,
        recent_resolutions: resolutions,
    })
    .into_response()
}

/// `POST /api/brains/:brain_id/approvals/:action_id/resolve` — operator
/// click flow. Body: `{decision: "approve" | "deny"}`. Stamps the
/// resolution with the dashboard server's `$NEUROGRIM_OPERATOR` env
/// (resolved at startup; falls back to "unknown" only when explicitly
/// permitted by mutations-allowed). Gated by `--allow-mutations`.
async fn brain_approvals_resolve(
    State(state): State<AppState>,
    AxumPath((brain_id, action_id)): AxumPath<(String, String)>,
    Json(body): Json<ResolveApprovalRequest>,
) -> Response {
    if !state.mutations_allowed {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "mutations-disabled",
                "code": "mutations-disabled",
                "hint": "start the dashboard with --allow-mutations to enable approval resolution",
            })),
        )
            .into_response();
    }
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let decision = body.decision.trim().to_ascii_lowercase();
    if decision != "approve" && decision != "deny" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid decision",
                "expected": ["approve", "deny"],
                "got": body.decision,
            })),
        )
            .into_response();
    }
    let operator = std::env::var("NEUROGRIM_OPERATOR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let decided_at = chrono::Utc::now().to_rfc3339();

    let resolution_path = resolutions_topic_path(&brain.project_root);
    let payload = serde_json::json!({
        "action_id": action_id,
        "decision": decision,
        "operator": operator,
        "decided_at": decided_at,
    });
    let topic = neurogrim_mcp::autonomy::APPROVAL_RESOLUTIONS_TOPIC;
    let msg = neurogrim_core::queue::QueueMessage::new(topic, payload);
    if let Err(e) = neurogrim_core::queue::append(&resolution_path, &msg) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("write failed: {e}")})),
        )
            .into_response();
    }

    // S15-C-7: emit on the config-changes queue.
    emit_config_change(
        &state,
        &brain_id,
        "approval_resolved",
        format!("approval {action_id} resolved as {decision}"),
        &brain.project_root,
    )
    .await;

    Json(ResolveApprovalResponse {
        action_id,
        decision,
        operator,
        decided_at,
    })
    .into_response()
}

fn approvals_topic_path(project_root: &Path) -> std::path::PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("queues")
        .join("_neurogrim")
        .join("approvals.jsonl")
}

fn resolutions_topic_path(project_root: &Path) -> std::path::PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("queues")
        .join("_neurogrim")
        .join("approval-resolutions.jsonl")
}

fn read_resolutions(path: &Path) -> Vec<ApprovalResolutionView> {
    let reader = match neurogrim_core::queue::JsonlQueueReader::open(path) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<ApprovalResolutionView> = reader
        .into_messages()
        .into_iter()
        .filter_map(|m| {
            let action_id = m.payload.get("action_id")?.as_str()?.to_string();
            let decision = m
                .payload
                .get("decision")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let operator = m
                .payload
                .get("operator")
                .and_then(|v| v.as_str())
                .map(String::from);
            let decided_at = m
                .payload
                .get("decided_at")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| m.produced_at.to_rfc3339());
            Some(ApprovalResolutionView {
                action_id,
                decision,
                operator,
                decided_at,
            })
        })
        .collect();
    // Newest-first; cap at 50.
    out.reverse();
    out.truncate(50);
    out
}

fn read_pending_approvals(
    path: &Path,
    resolved_ids: &std::collections::HashSet<&str>,
) -> Vec<ApprovalRequestView> {
    let reader = match neurogrim_core::queue::JsonlQueueReader::open(path) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<ApprovalRequestView> = reader
        .into_messages()
        .into_iter()
        .filter_map(|m| {
            let action_id = m.payload.get("action_id")?.as_str()?.to_string();
            if resolved_ids.contains(action_id.as_str()) {
                return None;
            }
            let tool = m
                .payload
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("(unknown)")
                .to_string();
            let action_type = m
                .payload
                .get("action_type")
                .and_then(|v| v.as_str())
                .unwrap_or("(unknown)")
                .to_string();
            Some(ApprovalRequestView {
                action_id,
                tool,
                action_type,
                requested_at: m.produced_at.to_rfc3339(),
            })
        })
        .collect();
    out.reverse(); // newest-first
    out
}

// ── S12-G-6: publish-gates page handler ─────────────────────────────────

/// `GET /api/brains/:brain_id/publish-gates` — read-only view of the
/// brain's publish-gates manifest joined with its ledger. Backs the
/// `/brains/:id/publish-gates` dashboard page (S12-G-6).
///
/// Three response shapes:
/// - **No manifest**: `manifest_present: false`, gates empty,
///   recent_ledger may still be non-empty (a deleted manifest doesn't
///   wipe historical ledger entries).
/// - **Malformed manifest**: `manifest_present: true`,
///   `manifest_error: Some(...)`. Page surfaces a banner.
/// - **Valid manifest**: `manifest_present: true`,
///   `manifest_error: None`, gates populated by joining each gate
///   with its most recent ledger entry.
async fn brain_publish_gates(
    State(state): State<AppState>,
    AxumPath(brain_id): AxumPath<String>,
) -> Response {
    let brain = match resolve_brain(&state, &brain_id) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let manifest_path = brain
        .project_root
        .join(".claude")
        .join("brain")
        .join("publish-gates.yaml");
    let ledger_path = brain
        .project_root
        .join(".claude")
        .join("brain")
        .join("publish-gate-ledger.jsonl");

    // Read the ledger first (it's needed regardless of manifest
    // state — the page can show historical activity even if the
    // manifest was deleted).
    let recent_ledger = read_publish_gate_ledger(&ledger_path);

    // Try to load the manifest. Three branches.
    let (manifest_present, manifest_error, gates) =
        match neurogrim_mcp::publish_gates::load_publish_gates(&manifest_path) {
            Ok(cfg) => {
                // Build a current-state view per gate by joining with
                // the ledger.
                let gates: Vec<PublishGateView> = cfg
                    .gates
                    .iter()
                    .map(|g| {
                        let latest =
                            recent_ledger.iter().find(|e| e.gate_id == g.id);
                        PublishGateView {
                            id: g.id.clone(),
                            gate_type: gate_type_str(g.gate_type).to_string(),
                            description: g.description.clone(),
                            blocking: g.blocking.unwrap_or(true),
                            timeout_seconds: g.timeout_seconds,
                            current_status: latest
                                .map(|e| e.status.clone())
                                .unwrap_or_else(|| "no_runs".to_string()),
                            last_run_at: latest.map(|e| e.started_at.clone()),
                            last_run_id: latest.map(|e| e.run_id.clone()),
                            operator: latest.and_then(|e| e.operator.clone()),
                        }
                    })
                    .collect();
                (true, None, gates)
            }
            Err(neurogrim_mcp::publish_gates::PublishGatesError::NotFound) => {
                (false, None, Vec::new())
            }
            Err(other) => (true, Some(format!("{other}")), Vec::new()),
        };

    Json(PublishGatesPageResponse {
        manifest_present,
        manifest_error,
        gates,
        recent_ledger,
    })
    .into_response()
}

/// Map `GateType` enum → wire string. Mirrors the publish_gate.rs
/// helper of the same name; we can't share that one because it lives
/// in the cli crate (and the dashboard shouldn't depend on cli).
fn gate_type_str(gt: neurogrim_mcp::publish_gates::GateType) -> &'static str {
    match gt {
        neurogrim_mcp::publish_gates::GateType::Automated => "automated",
        neurogrim_mcp::publish_gates::GateType::Manual => "manual",
        neurogrim_mcp::publish_gates::GateType::E2e => "e2e",
    }
}

/// Read the ledger and return the most recent N entries, newest
/// first. Cap at 50 to keep the API response tight; future stories
/// can paginate.
fn read_publish_gate_ledger(path: &Path) -> Vec<PublishGateLedgerView> {
    const MAX_ENTRIES: usize = 50;
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    // Lines are appended chronologically; iterate forwards, keep the
    // last MAX_ENTRIES, then reverse to put newest first.
    let mut all: Vec<PublishGateLedgerView> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).ok()?;
            Some(PublishGateLedgerView {
                run_id: v.get("run_id")?.as_str()?.to_string(),
                gate_id: v.get("gate_id")?.as_str()?.to_string(),
                gate_type: v.get("gate_type")?.as_str()?.to_string(),
                mode: v.get("mode")?.as_str()?.to_string(),
                started_at: v.get("started_at")?.as_str()?.to_string(),
                completed_at: v
                    .get("completed_at")
                    .and_then(|x| x.as_str())
                    .map(String::from),
                status: v.get("status")?.as_str()?.to_string(),
                blocking: v.get("blocking")?.as_bool().unwrap_or(true),
                operator: v
                    .get("operator")
                    .and_then(|x| x.as_str())
                    .map(String::from),
                exit_code: v.get("exit_code").and_then(|x| x.as_i64()).map(|n| n as i32),
                error_detail: v
                    .get("error_detail")
                    .and_then(|x| x.as_str())
                    .map(String::from),
            })
        })
        .collect();
    if all.len() > MAX_ENTRIES {
        let drop = all.len() - MAX_ENTRIES;
        all.drain(..drop);
    }
    all.reverse();
    all
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

    /// Helper: build a minimal AppState backed by a registry on
    /// disk that has a stable `meta.project` so the BrainTree's
    /// derived id is predictable across tests.
    fn make_state_with_brain(
        tmp: &tempfile::TempDir,
        project: &str,
        children: serde_json::Value,
    ) -> (AppState, String) {
        let claude_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let registry_path = claude_dir.join("brain-registry.json");
        let mut config = serde_json::json!({
            "domain_weights": {"placeholder": 0.0},
            "domain_definitions": {
                "placeholder": {
                    "principle": "Placeholder domain for testing",
                    "scoring_source": null,
                    "exported_variables": {}
                }
            }
        });
        if !children.is_null() {
            config["children"] = children;
        }
        let registry = serde_json::json!({
            "meta": {
                "schema_version": "2.1",
                "description": "v3.5 routes test",
                "updated_by": "test",
                "project": project
            },
            "tools": {},
            "data_sources": {},
            "config": config
        });
        std::fs::write(&registry_path, registry.to_string()).unwrap();
        let state = AppState::new(registry_path.to_string_lossy().to_string());
        (state, project.to_string())
    }

    #[tokio::test]
    async fn start_endpoint_returns_403_when_mutations_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, brain_id) = make_state_with_brain(&tmp, "alpha", serde_json::Value::Null);
        // mutations_allowed defaults to false from AppState::new.
        let app = router(state);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/brains/{brain_id}/peers/anything/start"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["code"], "mutations-disabled");
        assert!(v["error"].as_str().unwrap().contains("--allow-mutations"));
    }

    #[tokio::test]
    async fn services_endpoint_returns_empty_list_when_no_services_running() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, brain_id) = make_state_with_brain(&tmp, "alpha", serde_json::Value::Null);
        let app = router(state);
        let req = Request::builder()
            .uri(format!("/api/brains/{brain_id}/services"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["services"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn ports_endpoint_returns_missing_when_no_ports_json() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, brain_id) = make_state_with_brain(&tmp, "alpha", serde_json::Value::Null);
        let app = router(state);
        let req = Request::builder()
            .uri(format!("/api/brains/{brain_id}/ports"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["missing"], true);
        assert!(v["ports_file"].as_str().unwrap().ends_with("ports.json"));
    }

    #[tokio::test]
    async fn ports_endpoint_returns_persisted_config_when_ports_json_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, brain_id) = make_state_with_brain(&tmp, "alpha", serde_json::Value::Null);
        // Pre-populate ports.json so the endpoint sees it.
        let cfg = neurogrim_core::ports::PortConfig {
            schema_version: "1".into(),
            dashboard_port: 51234,
            a2a_port: 51235,
            created_at: chrono::Utc::now(),
            generated_by: "test".into(),
        };
        // The BrainEntry's project_root for the host is the canonical
        // form of the tempdir; use the same logic for save_ports.
        let project_root = std::fs::canonicalize(tmp.path()).unwrap_or_else(|_| tmp.path().to_path_buf());
        neurogrim_core::ports::save_ports(&project_root, &cfg).unwrap();

        let app = router(state);
        let req = Request::builder()
            .uri(format!("/api/brains/{brain_id}/ports"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["missing"], false);
        assert_eq!(v["dashboard_port"], 51234);
        assert_eq!(v["a2a_port"], 51235);
    }

    #[tokio::test]
    async fn health_response_carries_mutations_allowed_field() {
        let mut state = test_state();
        state.mutations_allowed = true;
        let app = router(state);
        let req = Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["mutations_allowed"], true);
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

    // ── S15-C-6 v2: per-custom-page widget layout PUT ───────────────

    /// Helper: build a state with `mutations_allowed = true` and a
    /// pre-existing custom page in dashboard-pages.json.
    fn make_state_with_custom_page(
        tmp: &tempfile::TempDir,
        page_name: &str,
    ) -> (AppState, String) {
        let (mut state, brain_id) =
            make_state_with_brain(tmp, "alpha", serde_json::Value::Null);
        state.mutations_allowed = true;
        // Seed dashboard-pages.json with the named custom page.
        let mut cfg = crate::pages::DashboardPagesConfig::default_for(&brain_id);
        cfg.pages.insert(page_name.to_string(), Vec::new());
        if !cfg.page_order.contains(&page_name.to_string()) {
            cfg.page_order.push(page_name.to_string());
        }
        crate::pages::save_dashboard_pages(tmp.path(), &cfg).unwrap();
        (state, brain_id)
    }

    #[tokio::test]
    async fn save_layout_returns_403_when_mutations_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, brain_id) = make_state_with_brain(&tmp, "alpha", serde_json::Value::Null);
        // mutations_allowed defaults to false from AppState::new.
        let app = router(state);
        let body = serde_json::json!({ "widgets": [] });
        let req = Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/brains/{brain_id}/dashboard-pages/my-page/layout"
            ))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["code"], "mutations-disabled");
    }

    #[tokio::test]
    async fn save_layout_rejects_builtin_page_names() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut state, brain_id) =
            make_state_with_brain(&tmp, "alpha", serde_json::Value::Null);
        state.mutations_allowed = true;
        let app = router(state);
        let body = serde_json::json!({ "widgets": [] });
        let req = Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/brains/{brain_id}/dashboard-pages/overview/layout"
            ))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["code"], "builtin-protected");
        assert_eq!(v["got"], "overview");
    }

    #[tokio::test]
    async fn save_layout_returns_404_when_page_not_declared() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut state, brain_id) =
            make_state_with_brain(&tmp, "alpha", serde_json::Value::Null);
        state.mutations_allowed = true;
        // Note: NO dashboard-pages.json on disk; the page doesn't exist.
        let app = router(state);
        let body = serde_json::json!({ "widgets": [] });
        let req = Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/brains/{brain_id}/dashboard-pages/never-declared/layout"
            ))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["code"], "page-not-found");
    }

    #[tokio::test]
    async fn save_layout_round_trips_widgets_through_dashboard_pages_json() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, brain_id) = make_state_with_custom_page(&tmp, "my-page");
        let app = router(state);
        let body = serde_json::json!({
            "widgets": [
                {
                    "id": "w1",
                    "widget_type": "score-gauge",
                    "size": "third",
                    "title": null,
                    "config": {}
                },
                {
                    "id": "w2",
                    "widget_type": "markdown-note",
                    "size": "full",
                    "title": "Notes",
                    "config": { "content": "hello world" }
                }
            ]
        });
        let req = Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/brains/{brain_id}/dashboard-pages/my-page/layout"
            ))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body_text = String::from_utf8_lossy(&body_bytes);
        assert_eq!(status, StatusCode::OK, "body was: {body_text}");
        let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["name"], "my-page");
        assert_eq!(v["widget_count"], 2);

        // Re-read dashboard-pages.json and confirm the widgets persisted.
        let cfg = crate::pages::read_dashboard_pages(tmp.path(), &brain_id);
        let widgets = cfg.pages.get("my-page").expect("page persisted");
        assert_eq!(widgets.len(), 2);
        assert_eq!(widgets[0].id, "w1");
        assert_eq!(widgets[1].id, "w2");
        assert_eq!(widgets[1].widget_type, "markdown-note");
    }

    /// Helper: read every line of the `_neurogrim/config-changes`
    /// topic JSONL backing file in the given project root. Returns
    /// parsed JSON values in append order. Returns an empty vec
    /// when the topic file doesn't exist (no events emitted yet).
    fn read_config_changes_topic(
        project_root: &std::path::Path,
    ) -> Vec<serde_json::Value> {
        let path = crate::bus::topic_path(project_root, CONFIG_CHANGES_TOPIC);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Vec::new();
        };
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    #[tokio::test]
    async fn registry_put_emits_config_change_with_keypath_diff() {
        // S15-C-7 v2: registry edits (the v4.x reframe surface)
        // emit on the config-changes queue with a keypath diff so
        // agents subscribed to operator activity see *what
        // changed*, not just "registry was edited".
        let tmp = tempfile::tempdir().unwrap();
        let (mut state, brain_id) =
            make_state_with_brain(&tmp, "alpha", serde_json::Value::Null);
        state.mutations_allowed = true;

        // The seed registry from `make_state_with_brain` has a
        // single domain with weight 0.0; bump it to 1.0 to exercise
        // the diff path. Read the on-disk file to compute the
        // ETag the PUT will need to match.
        let registry_path = tmp.path().join(".claude").join("brain-registry.json");
        let current_text = std::fs::read_to_string(&registry_path).unwrap();
        let etag = compute_etag(&current_text);
        let mut current_value: serde_json::Value =
            serde_json::from_str(&current_text).unwrap();
        current_value["config"]["domain_weights"]["placeholder"] =
            serde_json::json!(1.0);

        let app = router(state);
        let body = serde_json::json!({
            "expected_etag": etag,
            "registry": current_value,
        });
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/api/brains/{brain_id}/registry"))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body_text = String::from_utf8_lossy(&body_bytes);
        assert_eq!(status, StatusCode::OK, "body was: {body_text}");

        // The queue payload should carry a `diff: [...]` field
        // pointing at the changed weight path.
        let entries = read_config_changes_topic(tmp.path());
        assert_eq!(entries.len(), 1, "expected one event, got {entries:?}");
        let payload = &entries[0]["payload"];
        assert_eq!(payload["action_type"], "registry_edit");
        let diff = payload["diff"]
            .as_array()
            .expect("diff field present and is an array");
        // Find the weight change — there might be other paths if the
        // serialization round-trip surfaces extra fields.
        let weight_change = diff
            .iter()
            .find(|c| c["path"] == "config.domain_weights.placeholder")
            .expect("expected a keypath change at config.domain_weights.placeholder");
        assert_eq!(weight_change["op"], "replace");
        assert_eq!(weight_change["before"], 0.0);
        assert_eq!(weight_change["after"], 1.0);
        // Summary surfaces the keypath count rather than the
        // generic v1 "saved via dashboard editor" copy.
        assert!(
            payload["summary"]
                .as_str()
                .unwrap_or("")
                .contains("keypath change"),
            "summary was: {}",
            payload["summary"]
        );
    }

    #[tokio::test]
    async fn peer_log_endpoint_returns_tail_lines_from_disk() {
        // S15-C-2 expansion integration: write a peer log to disk
        // and confirm the endpoint reads it via the resolve_peer +
        // log-path-convention chain.
        let tmp = tempfile::tempdir().unwrap();
        // Set up a parent brain with a child peer.
        let peer_root = tmp.path().join("peer-alpha");
        std::fs::create_dir_all(&peer_root).unwrap();
        let (state, brain_id) = make_state_with_brain(
            &tmp,
            "parent-brain",
            serde_json::json!({
                "alpha": {
                    "brain_path": peer_root.to_string_lossy(),
                    "a2a_endpoint": "http://localhost:8421/a2a/v1/",
                    "interface_version": "1",
                    "weight": 1.0,
                    "enabled": true,
                }
            }),
        );
        // Write the peer log at the conventional path.
        let log_dir = peer_root.join(".claude").join("brain").join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        let log_path = log_dir.join("alpha.log");
        std::fs::write(
            &log_path,
            "boot\nready\nincoming probe\nshutdown signal\n",
        )
        .unwrap();

        let app = router(state);
        let req = Request::builder()
            .uri(format!(
                "/api/brains/{brain_id}/peers/alpha/log?lines=200"
            ))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body_text = String::from_utf8_lossy(&body_bytes);
        assert_eq!(status, StatusCode::OK, "body was: {body_text}");
        let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(v["present"], true);
        assert_eq!(v["truncated"], false);
        let lines = v["lines"].as_array().expect("lines is array");
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "boot");
        assert_eq!(lines[3], "shutdown signal");
    }

    #[tokio::test]
    async fn peer_log_endpoint_returns_404_for_unknown_peer() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, brain_id) =
            make_state_with_brain(&tmp, "alpha", serde_json::Value::Null);
        let app = router(state);
        let req = Request::builder()
            .uri(format!("/api/brains/{brain_id}/peers/never-declared/log"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(v["code"], "peer-not-found");
    }

    #[tokio::test]
    async fn peer_log_endpoint_returns_absent_when_file_missing() {
        // Peer is declared but never started → no log file. Endpoint
        // should return 200 with present=false rather than 500.
        let tmp = tempfile::tempdir().unwrap();
        let peer_root = tmp.path().join("peer-alpha");
        std::fs::create_dir_all(&peer_root).unwrap();
        let (state, brain_id) = make_state_with_brain(
            &tmp,
            "parent-brain",
            serde_json::json!({
                "alpha": {
                    "brain_path": peer_root.to_string_lossy(),
                    "a2a_endpoint": "http://localhost:8421/a2a/v1/",
                    "interface_version": "1",
                    "weight": 1.0,
                    "enabled": true,
                }
            }),
        );
        let app = router(state);
        let req = Request::builder()
            .uri(format!("/api/brains/{brain_id}/peers/alpha/log"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(v["present"], false);
        let lines = v["lines"].as_array().expect("lines is array");
        assert!(lines.is_empty());
    }

    #[tokio::test]
    async fn save_layout_emits_config_change_with_diff_payload() {
        // S15-C-7 v2: layout saves should emit on the config-changes
        // queue with a `diff: [...]` field showing the keypath
        // changes between old + new widget lists.
        let tmp = tempfile::tempdir().unwrap();
        let (state, brain_id) = make_state_with_custom_page(&tmp, "my-page");
        // Seed an existing widget so the diff has a non-trivial
        // before/after.
        let mut cfg = crate::pages::read_dashboard_pages(tmp.path(), &brain_id);
        cfg.pages.insert(
            "my-page".to_string(),
            vec![crate::layout::WidgetSpec {
                id: "old".to_string(),
                widget_type: "score-gauge".to_string(),
                size: "third".to_string(),
                title: None,
                config: serde_json::json!({}),
            }],
        );
        crate::pages::save_dashboard_pages(tmp.path(), &cfg).unwrap();

        let app = router(state);
        let body = serde_json::json!({
            "widgets": [
                {
                    "id": "old",
                    "widget_type": "score-gauge",
                    "size": "full",
                    "title": null,
                    "config": {}
                }
            ]
        });
        let req = Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/brains/{brain_id}/dashboard-pages/my-page/layout"
            ))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Inspect the queue payload — it should carry a `diff` field
        // pointing at the size change.
        let entries = read_config_changes_topic(tmp.path());
        assert_eq!(
            entries.len(),
            1,
            "expected exactly one config-changes event, got {entries:?}"
        );
        let payload = &entries[0]["payload"];
        assert_eq!(payload["action_type"], "layout_change");
        let diff = payload["diff"]
            .as_array()
            .expect("diff field present and is an array");
        assert_eq!(diff.len(), 1, "expected one keypath change, got {diff:?}");
        assert_eq!(diff[0]["path"], "[0].size");
        assert_eq!(diff[0]["op"], "replace");
        assert_eq!(diff[0]["before"], "third");
        assert_eq!(diff[0]["after"], "full");
    }

    #[tokio::test]
    async fn save_layout_overwrites_prior_widget_list() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, brain_id) = make_state_with_custom_page(&tmp, "my-page");
        // Seed a widget so the overwrite is visible.
        let mut cfg = crate::pages::read_dashboard_pages(tmp.path(), &brain_id);
        cfg.pages.insert(
            "my-page".to_string(),
            vec![crate::layout::WidgetSpec {
                id: "old".to_string(),
                widget_type: "identity".to_string(),
                size: "full".to_string(),
                title: None,
                config: serde_json::json!({}),
            }],
        );
        crate::pages::save_dashboard_pages(tmp.path(), &cfg).unwrap();

        let app = router(state);
        let body = serde_json::json!({ "widgets": [] });
        let req = Request::builder()
            .method("PUT")
            .uri(format!(
                "/api/brains/{brain_id}/dashboard-pages/my-page/layout"
            ))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let after = crate::pages::read_dashboard_pages(tmp.path(), &brain_id);
        assert_eq!(after.pages.get("my-page").map(|v| v.len()), Some(0));
    }
}
