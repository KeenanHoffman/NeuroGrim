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
    let mut state = AppState::with_events(registry_path, events_tx, allow_mutations);

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
            // operations only succeed over HTTPS. The Secrets page
            // surfaces a "switch to HTTPS" banner before the operator
            // even tries; this is defense-in-depth for adopters who
            // bypass the page or hit the endpoints directly.
            //
            // GET on `/secrets` and `/api/tls-status` stay available
            // on HTTP so the page can render + show the banner without
            // a chicken-and-egg.
            let http_app = app
                .clone()
                .layer(axum::middleware::from_fn(reject_http_secret_writes));
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
}
