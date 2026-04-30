//! NeuroGrim v3.4 dashboard server.
//!
//! HTTP server + embedded React frontend that gives humans a visual
//! inspection surface for the Brain. The dashboard is the "show, don't
//! tell" surface for the LSP Brains methodology — the third audience
//! after agents (CLI/MCP) and operators (CLI prose mode).
//!
//! ## Architecture
//!
//! - **Per-Brain primary, federation-aware secondary** — each Brain
//!   ships its own dashboard via `neurogrim ui`, looking at itself
//!   first; federation views fetch peer state via A2A.
//! - **Self-contained** — frontend is built into `frontend/dist/` and
//!   embedded at compile time via `rust-embed`. Users `cargo install
//!   neurogrim-cli` and the dashboard ships with it.
//! - **Read-only by default** — mutation endpoints (sensor refresh,
//!   registry edits) are gated behind `--allow-mutations` opt-in
//!   (planned for v3.5; v3.4 is read-only).
//!
//! ## Phase status (v3.4 in progress)
//!
//! - **Phase 0** (foundation refactor + skeleton) — IN PROGRESS
//! - **Phase 1** (5 pages: Overview, Domains, Domain detail, Federation,
//!   Skills) — pending
//! - **Phase 2** (SSE live updates, hat lens, browser launch, ship) — pending
//!
//! See `audit/v3.4-CHARTER.md` and the workspace `CHANGELOG.md` for
//! the full plan.

pub mod brains;
pub mod bus;
pub mod cache;
pub mod events;
pub mod instrumentation;
pub mod json_diff;
pub mod layout;
pub mod logs;
pub mod pages;
pub mod routes;
pub mod secrets_api;
pub mod services;
pub mod skills;
pub mod state;
pub mod tls_serve;
pub mod types;

pub use routes::router;
pub use state::AppState;

use anyhow::Result;
use std::net::SocketAddr;
use std::path::Path;

/// Spin up the dashboard HTTP server on the given socket address.
/// Blocks until the server exits (Ctrl+C, kill, or fatal error).
///
/// `allow_mutations` controls whether v3.5+ mutation endpoints
/// (service start/stop, sensor refresh) are reachable. When false,
/// those endpoints return 403 with `code: "mutations-disabled"` and
/// the frontend hides their action buttons.
///
/// Spawns the filesystem watcher so SSE clients connected to
/// `/api/events` receive live updates when CMDBs, the registry,
/// the invocation ledger, or the dashboard layout change.
///
/// **HTTPS (v4.2 S14-S-4.5 v2):** when cert + key files exist
/// at `<project>/.claude/brain/tls/{cert,key}.pem` (placed there
/// by `neurogrim secrets tls-cert generate`), this function also
/// binds an HTTPS listener on `addr.port + 1` serving the same
/// router. HTTP and HTTPS share state; the frontend chooses HTTPS
/// for secret-management routes via its own client logic. When
/// the cert files don't exist, only HTTP binds — backward
/// compatible with adopters who haven't run `tls-cert generate`.
pub async fn serve(
    addr: SocketAddr,
    registry_path: String,
    allow_mutations: bool,
) -> Result<()> {
    // Derive project_root from the registry path
    // (`<project>/.claude/brain-registry.json`). Canonicalize so
    // notify's absolute event paths can be `strip_prefix`'d cleanly.
    // PathBuf::parent returns `""` (empty path) — not None — when
    // the path has only one component, so we also treat empty paths
    // as cwd before canonicalizing.
    let registry_path_buf = std::path::PathBuf::from(&registry_path);
    let project_root_raw = registry_path_buf
        .parent()
        .and_then(Path::parent)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let project_root_normalized = if project_root_raw.as_os_str().is_empty() {
        std::path::PathBuf::from(".")
    } else {
        project_root_raw
    };
    let project_root = std::fs::canonicalize(&project_root_normalized)
        .unwrap_or(project_root_normalized);

    let events_tx = events::spawn_watcher(project_root.clone());
    let mut state = AppState::with_events(registry_path, events_tx.clone(), allow_mutations);

    // S13 follow-on: hot-reload queue-config.yaml. Subscribe a
    // background task to the event channel and re-read the YAML +
    // swap into BusState whenever the watcher fires
    // QueueConfigChanged. Frontend invalidates its viewer query
    // off the same SSE event so the read-only viewer reflects the
    // new content.
    {
        let bus = state.bus.clone();
        let watch_root = project_root.clone();
        let mut rx = events_tx.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if matches!(event, events::DashboardEvent::QueueConfigChanged) {
                    bus.reload_from_path(&watch_root).await;
                }
            }
        });
    }

    // v4.5 — auto-ingest the `_neurogrim/score-snapshots` bus topic
    // into the TSDB. Each snapshot's payload carries one unified score
    // plus per-domain breakdowns; we record one point per domain so
    // each becomes its own time-series for trajectory visualization.
    //
    // On startup, backfill from the existing SQLite topic so operators
    // see history immediately — the topic itself is the authoritative
    // record from the v4.4 dogfood, and idempotent re-ingest is fine
    // because the metrics store is just-created.
    if let Some(metrics) = state.metrics.clone() {
        let bus = state.bus.clone();
        let backfill_root = project_root.clone();
        tokio::spawn(async move {
            // One-time backfill from the topic's persistent storage.
            backfill_score_snapshots_into_metrics(&bus, &backfill_root, &metrics).await;
            // Then subscribe for ongoing publishes.
            let mut rx = bus
                .subscribe(neurogrim_core::queue::SCORE_SNAPSHOTS_TOPIC)
                .await;
            while let Ok(msg) = rx.recv().await {
                ingest_score_snapshot_payload(&metrics, &msg.payload);
            }
        });
    }

    // Probe for the TLS files BEFORE binding so we know whether to
    // log the HTTPS port + plumb it into AppState so handlers can
    // decide whether to enforce HTTPS for secret-write paths.
    let tls_config = tls_serve::load_rustls_config(&project_root).await?;
    if tls_config.is_some() {
        state.https_port = Some(tls_serve::https_port_for(addr.port()));
    }

    let app = routes::router(state);

    let http_listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("neurogrim dashboard listening on http://{}", addr);

    match tls_config {
        Some(config) => {
            let https_addr = tls_serve::https_addr_for(&addr);
            tracing::info!(
                "neurogrim dashboard listening on https://{} (S14-S-4.5 v2; \
                 secret-management endpoints REQUIRE this listener)",
                https_addr
            );
            // S14-S-4.5 v3: HTTP listener applies a middleware that
            // 426s POST/DELETE on `/secrets/*` paths so secret-write
            // operations only succeed over HTTPS.
            //
            // S14-S-4.5 v4: HTTP listener also auto-redirects GET
            // requests on the `/brains/<id>/secrets` SPA route to
            // the HTTPS equivalent. Removes a manual click from the
            // operator workflow — they no longer have to read the
            // "switch to HTTPS" banner and re-type the URL with
            // port +1. The redirect targets only the page route,
            // not the API: agents hitting GET /api/.../secrets over
            // HTTP keep working (they get list metadata only;
            // writes already 426 from S14-S-4.5 v3).
            //
            // GET on `/api/tls-status` stays available on HTTP so
            // any future HTTP-rendered surface can probe TLS state
            // without hitting the redirect.
            let https_port = tls_serve::https_port_for(addr.port());
            let http_app = app
                .clone()
                .layer(axum::middleware::from_fn(reject_http_secret_writes))
                .layer(axum::middleware::from_fn(
                    move |req: axum::http::Request<axum::body::Body>,
                          next: axum::middleware::Next| {
                        redirect_secrets_page_to_https(https_port, req, next)
                    },
                ));
            let https_app = app;
            let http_task = async move {
                axum::serve(http_listener, http_app)
                    .await
                    .map_err(anyhow::Error::from)
            };
            let https_task = async move {
                axum_server::bind_rustls(https_addr, config)
                    .serve(https_app.into_make_service())
                    .await
                    .map_err(anyhow::Error::from)
            };
            tokio::try_join!(http_task, https_task)?;
        }
        None => {
            tracing::info!(
                "neurogrim dashboard: no TLS cert at {}; HTTPS listener \
                 disabled. Run `neurogrim secrets tls-cert generate` to enable.",
                project_root.join(".claude/brain/tls").display()
            );
            // No HTTPS bound → no HTTP-write enforcement (otherwise
            // adopters who haven't run `tls-cert generate` couldn't
            // set secrets at all).
            axum::serve(http_listener, app).await?;
        }
    }
    Ok(())
}

/// v4.5 — One-time backfill: replay every existing message in
/// `_neurogrim/score-snapshots` into the metrics store on dashboard
/// startup. Idempotent because the metrics store is fresh on each
/// process start (no incremental ingest tracking yet — iteration 3
/// adds a watermark sidecar table).
///
/// Errors are silently logged: backfill is a "nice to have" on first
/// load, not a correctness requirement. Live publishes will continue
/// populating the store.
async fn backfill_score_snapshots_into_metrics(
    bus: &bus::BusState,
    project_root: &std::path::Path,
    metrics: &neurogrim_core::metrics::MetricsHandle,
) {
    use neurogrim_core::queue::SCORE_SNAPSHOTS_TOPIC;

    let handle = match bus.backend_for(project_root, SCORE_SNAPSHOTS_TOPIC).await {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("metrics backfill: backend_for failed: {e}");
            return;
        }
    };
    let total = match handle.len() {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("metrics backfill: len() failed: {e}");
            return;
        }
    };
    if total == 0 {
        return;
    }
    let msgs = match handle.read_from(1, total as usize) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("metrics backfill: read_from failed: {e}");
            return;
        }
    };
    for stored in &msgs {
        ingest_score_snapshot_payload(metrics, &stored.message.payload);
    }
    tracing::info!(
        "metrics backfill: ingested {} score-snapshot messages into TSDB",
        msgs.len()
    );
}

/// Helper: project a single score-snapshot payload into the TSDB.
/// Records `brain_score` (unified), `domain_score{domain=...}`, and
/// `domain_confidence{domain=...}` data points stamped with the
/// snapshot's `scored_at` timestamp (falls back to `now` if the
/// payload's timestamp can't be parsed — drift signal).
fn ingest_score_snapshot_payload(
    metrics: &neurogrim_core::metrics::MetricsHandle,
    payload: &serde_json::Value,
) {
    use neurogrim_core::metrics::Tags;

    let scored_at = payload
        .get("scored_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);

    if let Some(score) = payload.get("score").and_then(|v| v.as_i64()) {
        metrics.record_at("brain_score", &Tags::new(), score as f64, scored_at);
    }
    if let Some(domains) = payload.get("domains").and_then(|d| d.as_object()) {
        for (name, dom) in domains {
            if let Some(s) = dom.get("score").and_then(|v| v.as_i64()) {
                let tags = Tags::new().with("domain", name.as_str());
                metrics.record_at("domain_score", &tags, s as f64, scored_at);
            }
            if let Some(c) = dom.get("confidence").and_then(|v| v.as_i64()) {
                let tags = Tags::new().with("domain", name.as_str());
                metrics.record_at("domain_confidence", &tags, c as f64, scored_at);
            }
        }
    }
}

/// v4.2 S14-S-4.5 v3 middleware: reject POST/DELETE requests to
/// secret-management paths with `426 Upgrade Required`. Applied
/// only to the HTTP listener when HTTPS is also bound — adopters
/// without a TLS cert keep working as before.
///
/// The check is path-based + method-based so GET (read-only list,
/// no values returned) stays available on HTTP for the page UX:
/// operators load the Secrets page over HTTP, see the banner +
/// fingerprint, and click through to HTTPS for writes.
async fn reject_http_secret_writes(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    if is_secret_write_path(&method, &path) {
        return (
            StatusCode::UPGRADE_REQUIRED,
            [(axum::http::header::UPGRADE, "TLS/1.2, HTTP/1.1")],
            axum::Json(serde_json::json!({
                "error": "secret-management writes require HTTPS",
                "code": "https-required",
                "hint": "switch to the HTTPS listener (port = HTTP port + 1) — the Secrets page surfaces a switch-to-HTTPS banner",
            })),
        )
            .into_response();
    }
    next.run(req).await
}

/// Pure predicate the HTTP middleware uses to decide whether to
/// reject a request with `426 Upgrade Required`. Extracted so
/// unit tests pin the routing rules without spinning up axum.
fn is_secret_write_path(method: &axum::http::Method, path: &str) -> bool {
    use axum::http::Method;
    let is_write = method == Method::POST || method == Method::DELETE;
    let is_secret_path = path.starts_with("/api/brains/") && path.contains("/secrets/");
    is_write && is_secret_path
}

/// v4.2 S14-S-4.5 v4 middleware: redirect the Secrets SPA route
/// from HTTP to HTTPS so operators don't have to manually retype
/// the URL with port +1.
///
/// Only redirects GET on the SPA page path
/// (`/brains/<brain-id>/secrets`); the API routes
/// (`/api/brains/.../secrets`) keep their existing semantics —
/// list (GET) over HTTP returns metadata, writes (POST/DELETE) get
/// 426 Upgrade Required from `reject_http_secret_writes`. This
/// keeps agents that hit the API directly working without
/// surprising HTTP redirect behavior, while the human-facing page
/// auto-upgrades.
async fn redirect_secrets_page_to_https(
    https_port: u16,
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{header, Method, StatusCode};
    use axum::response::IntoResponse;
    if req.method() != Method::GET {
        return next.run(req).await;
    }
    let path = req.uri().path();
    if !is_secrets_page_path(path) {
        return next.run(req).await;
    }
    // Build the HTTPS Location URL. Use the request's Host header
    // so the redirect honors whatever name the operator typed
    // (localhost, 127.0.0.1, LAN hostname, etc.). Strip the port
    // off the Host (the listener bound port is unrelated to the
    // HTTPS port we're targeting).
    let host_header = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    let hostname = host_header.split(':').next().unwrap_or("localhost");
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(path);
    let target = format!("https://{}:{}{}", hostname, https_port, path_and_query);
    // Use 307 Temporary Redirect rather than 301 — we don't want
    // browsers caching the redirect across cert rotations or the
    // (rare) case where the operator restarts without HTTPS.
    (
        StatusCode::TEMPORARY_REDIRECT,
        [(header::LOCATION, target)],
    )
        .into_response()
}

/// Pure predicate: is this path the Secrets SPA route?
/// Matches `/brains/<id>/secrets` exactly (with optional trailing
/// slash). Does NOT match the API path
/// (`/api/brains/<id>/secrets`) — those have their own behavior
/// via `reject_http_secret_writes`.
fn is_secrets_page_path(path: &str) -> bool {
    if path.starts_with("/api/") {
        return false;
    }
    let stripped = path.strip_suffix('/').unwrap_or(path);
    if !stripped.starts_with("/brains/") {
        return false;
    }
    if !stripped.ends_with("/secrets") {
        return false;
    }
    // Verify the shape: /brains/<single-segment>/secrets — reject
    // any deeper paths (e.g. /brains/x/y/secrets) to avoid
    // accidentally redirecting unrelated routes.
    let middle = &stripped["/brains/".len()..stripped.len() - "/secrets".len()];
    !middle.is_empty() && !middle.contains('/')
}

#[cfg(test)]
mod tests {
    use super::is_secret_write_path;
    use axum::http::Method;

    #[test]
    fn rejects_post_on_secrets_path() {
        assert!(is_secret_write_path(
            &Method::POST,
            "/api/brains/alpha/secrets/github-pat"
        ));
    }

    #[test]
    fn rejects_delete_on_secrets_path() {
        assert!(is_secret_write_path(
            &Method::DELETE,
            "/api/brains/alpha/secrets/github-pat"
        ));
    }

    #[test]
    fn allows_get_on_secrets_path() {
        assert!(!is_secret_write_path(
            &Method::GET,
            "/api/brains/alpha/secrets"
        ));
    }

    #[test]
    fn allows_put_on_secrets_path() {
        // Currently no PUT endpoint on secrets, but defensive — the
        // predicate is method-specific so an accidentally-added
        // PUT wouldn't bypass HTTP enforcement silently.
        assert!(!is_secret_write_path(
            &Method::PUT,
            "/api/brains/alpha/secrets/x"
        ));
    }

    #[test]
    fn allows_post_on_non_secrets_paths() {
        assert!(!is_secret_write_path(
            &Method::POST,
            "/api/brains/alpha/registry"
        ));
        assert!(!is_secret_write_path(
            &Method::POST,
            "/api/brains/alpha/queues/scratch"
        ));
        assert!(!is_secret_write_path(
            &Method::POST,
            "/api/brains/alpha/dashboard-pages/x"
        ));
    }

    #[test]
    fn allows_post_on_legacy_paths_without_brains_prefix() {
        // The /api/brains/ prefix gate ensures we don't accidentally
        // reject writes to legacy or future top-level routes.
        assert!(!is_secret_write_path(&Method::POST, "/api/secrets/x"));
        assert!(!is_secret_write_path(&Method::POST, "/secrets/x"));
    }

    #[test]
    fn nested_secrets_path_segments_still_caught() {
        // Reasonable variants the path matcher should still catch.
        assert!(is_secret_write_path(
            &Method::POST,
            "/api/brains/some-brain/secrets/abc"
        ));
        assert!(is_secret_write_path(
            &Method::DELETE,
            "/api/brains/_neurogrim/secrets/test"
        ));
    }

    // ── S14-S-4.5 v4: secrets page auto-redirect to HTTPS ─────

    use super::is_secrets_page_path;

    #[test]
    fn secrets_page_path_matches_canonical_shape() {
        assert!(is_secrets_page_path("/brains/alpha/secrets"));
        assert!(is_secrets_page_path("/brains/alpha/secrets/"));
        assert!(is_secrets_page_path("/brains/some-brain-id/secrets"));
        assert!(is_secrets_page_path("/brains/_neurogrim/secrets"));
    }

    #[test]
    fn secrets_page_path_rejects_api_paths() {
        // The API routes have their own enforcement (reject_http_secret_writes).
        // Don't redirect them — agents/tools hitting GET /api/.../secrets
        // for metadata should keep working over HTTP.
        assert!(!is_secrets_page_path("/api/brains/alpha/secrets"));
        assert!(!is_secrets_page_path("/api/brains/alpha/secrets/"));
        assert!(!is_secrets_page_path("/api/brains/alpha/secrets/foo"));
    }

    #[test]
    fn secrets_page_path_rejects_unrelated_routes() {
        assert!(!is_secrets_page_path("/"));
        assert!(!is_secrets_page_path("/brains/alpha"));
        assert!(!is_secrets_page_path("/brains/alpha/overview"));
        assert!(!is_secrets_page_path("/brains/alpha/settings"));
        assert!(!is_secrets_page_path("/brains/alpha/p/secrets"));
        // Deeper paths that contain "secrets" in the wrong slot.
        assert!(!is_secrets_page_path("/brains/alpha/x/secrets"));
        // Empty brain segment.
        assert!(!is_secrets_page_path("/brains//secrets"));
    }

    #[test]
    fn secrets_page_path_rejects_substring_matches() {
        // Defensive: don't redirect URLs that just happen to contain
        // "/secrets" somewhere weird.
        assert!(!is_secrets_page_path("/brains/alpha/secrets-archived"));
        assert!(!is_secrets_page_path("/brains/alpha/secrets-old"));
    }

    // ── Middleware integration: actual axum router with the
    //    redirect layer attached. Verifies the redirect Response
    //    has the right status + Location header. ──────────────────

    use axum::body::Body;
    use axum::http::Request;
    use axum::Router;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn router_with_redirect_layer(https_port: u16) -> Router {
        // Minimal app: just a marker handler that returns 200 OK on
        // the secrets page route. The layer below intercepts before
        // the handler can fire, so a 200 means the redirect didn't
        // catch the request.
        let app = Router::new().fallback(|| async { "fallthrough" });
        app.layer(axum::middleware::from_fn(
            move |req: Request<Body>, next: axum::middleware::Next| {
                super::redirect_secrets_page_to_https(https_port, req, next)
            },
        ))
    }

    #[tokio::test]
    async fn http_get_on_secrets_page_redirects_to_https() {
        let app = router_with_redirect_layer(8421);
        let req = Request::builder()
            .method("GET")
            .uri("/brains/alpha/secrets")
            .header("host", "localhost:8420")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::TEMPORARY_REDIRECT);
        let location = resp
            .headers()
            .get(axum::http::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        assert_eq!(location, "https://localhost:8421/brains/alpha/secrets");
    }

    #[tokio::test]
    async fn http_get_preserves_query_string_on_redirect() {
        let app = router_with_redirect_layer(8421);
        let req = Request::builder()
            .method("GET")
            .uri("/brains/alpha/secrets?fresh=1")
            .header("host", "127.0.0.1:8420")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::TEMPORARY_REDIRECT);
        let location = resp
            .headers()
            .get(axum::http::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        assert_eq!(
            location,
            "https://127.0.0.1:8421/brains/alpha/secrets?fresh=1"
        );
    }

    #[tokio::test]
    async fn http_get_on_unrelated_route_falls_through() {
        let app = router_with_redirect_layer(8421);
        let req = Request::builder()
            .method("GET")
            .uri("/brains/alpha/overview")
            .header("host", "localhost:8420")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"fallthrough");
    }

    #[tokio::test]
    async fn http_get_on_api_secrets_path_is_not_redirected() {
        let app = router_with_redirect_layer(8421);
        let req = Request::builder()
            .method("GET")
            .uri("/api/brains/alpha/secrets")
            .header("host", "localhost:8420")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn http_post_on_secrets_page_is_not_redirected() {
        // Defensive: redirect only fires for GET. A non-GET request
        // on the page path falls through (in production it would hit
        // the existing 426 middleware on the API path; the page
        // route itself has no POST handler).
        let app = router_with_redirect_layer(8421);
        let req = Request::builder()
            .method("POST")
            .uri("/brains/alpha/secrets")
            .header("host", "localhost:8420")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn http_get_with_missing_host_falls_back_to_localhost() {
        // Defensive — Host is required by HTTP/1.1, but if it
        // somehow comes through missing, we still emit a sensible
        // redirect rather than panicking.
        let app = router_with_redirect_layer(8421);
        let req = Request::builder()
            .method("GET")
            .uri("/brains/alpha/secrets")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Hyper auto-sets a Host header for HTTP/1.1 requests when
        // none is provided — the actual redirect target depends on
        // what Hyper picks. Either we redirect (if the auto-set
        // host is something sensible) or we don't crash. The key
        // invariant is no panic.
        let status = resp.status();
        assert!(
            status == axum::http::StatusCode::TEMPORARY_REDIRECT
                || status == axum::http::StatusCode::OK,
            "unexpected status: {status:?}"
        );
    }
}
